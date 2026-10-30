use crate::control_names::control_name;
use crate::hidpp::{
    self, BacklightInfo, Battery, Control, DisabledKeysInfo, FirmwareInfo, HostsInfo,
    MouseButtonInfo, OnboardDpi, Transport,
};
use anyhow::{Context, Result};
use hidapi::HidApi;
use serde::Serialize;
use std::thread::sleep;
use std::time::Duration;

#[derive(Clone, Debug, Serialize)]
pub struct DeviceDescriptor {
    pub path: String,
    pub index: u8,
    pub name: String,
    pub product_id: String,
    pub unit_id: String,
    pub transport: String,
    pub features: Vec<FeatureView>,
}

#[derive(Clone, Debug, Serialize)]
pub struct FeatureView {
    pub index: u8,
    pub id: u16,
    pub name: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub device: DeviceDescriptor,
    pub battery: Option<BatteryView>,
    pub firmware: Vec<FirmwareView>,
    pub controls: Vec<ControlView>,
    pub report_rate_ms: Option<u8>,
    pub polling_rate_hz: Option<u16>,
    pub supported_polling_rates: Vec<u16>,
    pub mode_status: Option<u8>,
    pub dpi: Option<OnboardDpi>,
    pub backlight: Option<BacklightInfo>,
    pub fn_inverted: Option<bool>,
    pub hosts: Option<HostsInfo>,
    pub disabled_keys: Option<DisabledKeysInfo>,
    pub mouse_buttons: Option<Vec<MouseButtonInfo>>,
    pub serial: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct BatteryView {
    pub level: Option<u8>,
    pub status: String,
    pub voltage_mv: Option<u16>,
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct FirmwareView {
    pub kind: u8,
    pub name: String,
    pub version: String,
    pub text: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct ControlView {
    pub index: u8,
    pub cid: u16,
    pub name: String,
    pub task: u16,
    pub flags: u16,
    pub flags_label: String,
    pub pos: u8,
    pub group: u8,
    pub group_mask: u8,
    pub mapped_to: u16,
    pub mapped_to_name: String,
    pub mapping_flags: u16,
}

pub fn discover(api: &HidApi) -> Result<Vec<DeviceDescriptor>> {
    let fresh_api = HidApi::new().ok();
    let list_api = fresh_api.as_ref().unwrap_or(api);
    let mut paths: Vec<String> = list_api
        .device_list()
        .filter(|info| {
            info.vendor_id() == hidpp::LOGITECH_VID
                && ((info.usage_page() == hidpp::HIDPP_USAGE_PAGE
                    && info.usage() == hidpp::HIDPP_LONG_USAGE)
                    || (info.usage_page() == hidpp::HIDPP_BLE_USAGE_PAGE
                        && info.usage() == hidpp::HIDPP_BLE_LONG_USAGE))
        })
        .map(|info| info.path().to_string_lossy().into_owned())
        .collect();
    paths.sort();
    paths.dedup();

    let mut devices = std::thread::scope(|scope| {
        let handles: Vec<_> = paths
            .iter()
            .map(|path| {
                let path = path.as_str();
                scope.spawn(move || scan_path(path))
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|handle| handle.join().unwrap_or_default())
            .collect::<Vec<_>>()
    });
    // Deduplicate any repeated responses on same unit_id (e.g. bluetooth virtual slots)
    devices.sort_by(|a, b| a.product_id.cmp(&b.product_id).then_with(|| a.unit_id.cmp(&b.unit_id)));
    devices.dedup_by(|a, b| a.product_id == b.product_id && a.unit_id == b.unit_id && !a.unit_id.is_empty());
    devices.sort_by_key(|a| a.name.to_lowercase());
    Ok(devices)
}

fn scan_path(path: &str) -> Vec<DeviceDescriptor> {
    let Ok(api) = HidApi::new() else {
        return Vec::new();
    };
    let Ok(mut transport) = Transport::open(&api, path, 1) else {
        return Vec::new();
    };
    let mut devices = Vec::new();
    for index in 1..=6u8 {
        transport.set_device_index(index);
        if let Ok(Some(mut device)) = probe(&transport, index) {
            device.path = path.to_owned();
            devices.push(device);
        }
    }
    if devices.is_empty() {
        transport.set_device_index(0xff);
        if let Ok(Some(mut device)) = probe(&transport, 0xff) {
            device.path = path.to_owned();
            devices.push(device);
        }
    }
    devices
}

fn probe(transport: &Transport, index: u8) -> Result<Option<DeviceDescriptor>> {
    let has_dev_info = transport
        .get_feature_with_timeout(0x0003, Duration::from_millis(450))?
        .is_some();
    if !has_dev_info
        && transport
            .get_feature_with_timeout(0x0001, Duration::from_millis(300))?
            .is_none()
    {
        return Ok(None);
    }
    let name = transport
        .name()
        .ok()
        .flatten()
        .unwrap_or_else(|| format!("Logitech device {}", index));
    let info = transport.device_info().ok().flatten();
    let mut features = transport.feature_set().unwrap_or_default();
    if features.is_empty() {
        sleep(Duration::from_millis(50));
        features = transport.feature_set().unwrap_or_default();
    }
    Ok(Some(DeviceDescriptor {
        path: String::new(),
        index,
        name,
        product_id: info
            .as_ref()
            .map(|info| format!("{:04X}", info.product_id))
            .unwrap_or_else(|| "UNKNOWN".into()),
        unit_id: info
            .as_ref()
            .map(|info| info.unit_id.clone())
            .unwrap_or_default(),
        transport: info
            .as_ref()
            .map(|info| info.transport.clone())
            .unwrap_or_default(),
        features: features
            .into_iter()
            .map(|(index, id)| FeatureView {
                index,
                id,
                name: feature_name(id).to_string(),
            })
            .collect(),
    }))
}

pub fn open(api: &HidApi, device: &DeviceDescriptor) -> Result<Transport> {
    Transport::open(api, &device.path, device.index)
        .with_context(|| format!("opening {}", device.name))
}

pub fn read_battery(api: &HidApi, device: &DeviceDescriptor) -> Result<Option<BatteryView>> {
    let transport = open(api, device)?;
    Ok(transport.battery().ok().flatten().map(battery_view))
}

pub fn snapshot(api: &HidApi, device: &DeviceDescriptor) -> Result<Snapshot> {
    let transport = open(api, device)?;
    let has = |id: u16| device.features.iter().any(|f| f.id == id);

    let battery = if has(0x1000) || has(0x1001) || has(0x1004) {
        transport.battery().ok().flatten().map(battery_view)
    } else {
        None
    };

    let firmware = if has(0x0003) {
        transport
            .firmware()
            .unwrap_or_default()
            .into_iter()
            .filter(|firmware| !firmware.name.is_empty())
            .map(firmware_view)
            .collect()
    } else {
        Vec::new()
    };

    let controls = if has(0x1B04) {
        transport
            .controls()
            .unwrap_or_default()
            .into_iter()
            .map(control_view)
            .collect()
    } else {
        Vec::new()
    };

    let report_rate_ms = if has(0x8060) {
        transport.report_rate_ms().ok().flatten()
    } else {
        None
    };

    let supported_polling_rates = if has(0x8060) {
        transport.supported_report_rates().unwrap_or_default()
    } else {
        Vec::new()
    };

    let polling_rate_hz = report_rate_ms.map(|ms| match ms {
        1 => 1000,
        2 => 500,
        4 => 250,
        8 => 125,
        other => 1000 / (other as u16).max(1),
    });

    let mode_status = if has(0x8090) || has(0x8071) {
        transport.mode_status().ok().flatten()
    } else {
        None
    };

    let dpi = if has(0x8100) || has(0x2201) || has(0x2110) || has(0x2111) {
        transport.onboard_dpi().ok().flatten()
    } else {
        None
    };

    let mut backlight = if has(0x1982) {
        transport.backlight().ok().flatten()
    } else {
        None
    };

    if let Some(b) = &mut backlight {
        if let Ok(cfg) = crate::config::load() {
            if let Some(s) = cfg.device_settings.iter().find(|s| s.product_id == device.product_id && s.unit_id == device.unit_id) {
                if let Some(eff) = s.backlight_effect {
                    b.current_effect = eff;
                }
                if let Some(dur) = s.backlight_duration {
                    b.dpow = dur;
                }
            }
        }
    }

    let fn_inverted = if has(0x40A3) {
        transport.fn_inversion().ok().flatten()
    } else {
        None
    };

    let hosts = if has(0x1815) {
        transport.hosts().ok().flatten()
    } else {
        None
    };

    let disabled_keys = if has(0x4521) {
        transport.disabled_keys().ok().flatten()
    } else {
        None
    };

    let mouse_buttons = if has(0x8100) {
        transport.mouse_buttons().ok().flatten()
    } else {
        None
    };

    Ok(Snapshot {
        device: device.clone(),
        battery,
        firmware,
        controls,
        report_rate_ms,
        polling_rate_hz,
        supported_polling_rates,
        mode_status,
        dpi,
        backlight,
        fn_inverted,
        hosts,
        disabled_keys,
        mouse_buttons,
        serial: transport.serial().ok().flatten(),
    })
}

pub fn set_mapping(api: &HidApi, device: &DeviceDescriptor, from: u16, to: u16) -> Result<bool> {
    let transport = open(api, device)?;
    transport.set_mapping(from, to)
}

pub fn set_polling_rate(api: &HidApi, device: &DeviceDescriptor, rate_hz: u16) -> Result<bool> {
    let transport = open(api, device)?;
    let rate_ms = match rate_hz {
        1000 => 1,
        500 => 2,
        250 => 4,
        125 => 8,
        _ => (1000 / rate_hz).max(1) as u8,
    };
    transport.set_report_rate_ms(rate_ms)
}

pub fn set_power_mode(api: &HidApi, device: &DeviceDescriptor, mode: u8) -> Result<bool> {
    let transport = open(api, device)?;
    transport.set_mode_status(mode)
}

pub fn set_dpi_stage(api: &HidApi, device: &DeviceDescriptor, stage_index: u8) -> Result<bool> {
    let transport = open(api, device)?;
    transport.set_onboard_dpi_index(stage_index)
}

pub fn set_mouse_button(
    api: &HidApi,
    device: &DeviceDescriptor,
    button_index: u8,
    raw_binding: [u8; 4],
) -> Result<bool> {
    let transport = open(api, device)?;
    transport.set_mouse_button(button_index, raw_binding)
}

pub fn set_backlight(
    api: &HidApi,
    device: &DeviceDescriptor,
    level: u8,
    manual: bool,
    enabled: bool,
) -> Result<bool> {
    let transport = open(api, device)?;
    transport.set_backlight(level, manual, enabled)
}

pub fn set_backlight_full(
    api: &HidApi,
    device: &DeviceDescriptor,
    level: u8,
    manual: bool,
    enabled: bool,
    effect: u8,
    duration: u16,
) -> Result<bool> {
    let transport = open(api, device)?;
    transport.set_backlight_full(level, manual, enabled, effect, duration)
}

pub fn set_backlight_effect(api: &HidApi, device: &DeviceDescriptor, effect: u8) -> Result<bool> {
    let transport = open(api, device)?;
    transport.set_backlight_effect(effect)
}

pub fn set_backlight_duration(
    api: &HidApi,
    device: &DeviceDescriptor,
    duration: u16,
) -> Result<bool> {
    let transport = open(api, device)?;
    transport.set_backlight_duration(duration)
}

pub fn set_fn_inversion(api: &HidApi, device: &DeviceDescriptor, inverted: bool) -> Result<bool> {
    let transport = open(api, device)?;
    transport.set_fn_inversion(inverted)
}

pub fn set_disabled_keys(api: &HidApi, device: &DeviceDescriptor, mask: u8) -> Result<bool> {
    let transport = open(api, device)?;
    transport.set_disabled_keys(mask)
}

pub fn switch_host(api: &HidApi, device: &DeviceDescriptor, host: u8) -> Result<bool> {
    let transport = open(api, device)?;
    transport.switch_host(host)
}

fn battery_view(battery: Battery) -> BatteryView {
    let status = battery.status.label();
    BatteryView {
        level: battery.level,
        status: status.clone(),
        voltage_mv: battery.voltage_mv,
        text: format_battery(battery.level, &status),
    }
}

fn firmware_view(info: FirmwareInfo) -> FirmwareView {
    let text = if info.name.is_empty() {
        format!("kind {}", info.kind)
    } else if info.version.is_empty() {
        info.name.clone()
    } else {
        format!("{} {}", info.name, info.version)
    };
    FirmwareView {
        kind: info.kind,
        name: info.name,
        version: info.version,
        text,
    }
}

fn control_view(control: Control) -> ControlView {
    ControlView {
        index: control.index,
        cid: control.cid,
        name: control_name(control.cid),
        task: control.task,
        flags: control.flags,
        flags_label: control.flags_label(),
        pos: control.pos,
        group: control.group,
        group_mask: control.group_mask,
        mapped_to: control.mapped_to,
        mapped_to_name: control_name(control.mapped_to),
        mapping_flags: control.mapping_flags,
    }
}

fn format_battery(level: Option<u8>, status: &str) -> String {
    match level {
        Some(level) => format!("{level}% ({status})"),
        None => status.to_string(),
    }
}

pub fn feature_name(id: u16) -> &'static str {
    match id {
        0x0000 => "Root",
        0x0001 => "FeatureSet",
        0x0003 => "DeviceInformation",
        0x0005 => "DeviceName",
        0x0007 => "DeviceFriendlyName",
        0x0020 => "ConfigChange",
        0x00c2 => "DFUControlSigned",
        0x00c3 => "DFUControl",
        0x1000 => "BatteryStatus",
        0x1001 => "BatteryVoltage",
        0x1004 => "UnifiedBattery",
        0x1010 => "ChargingControl",
        0x1300 => "LedControl",
        0x1602 => "Unknown1602",
        0x1801 => "Unknown1801",
        0x1802 => "DeviceReset",
        0x1803 => "Unknown1803",
        0x1805 => "OobState",
        0x1806 => "ConfigDeviceProps",
        0x1811 => "Unknown1811",
        0x1814 => "ChangeHost",
        0x1815 => "HostsInfo",
        0x1816 => "BleProPrePairing",
        0x1830 => "Unknown1830",
        0x1861 => "Unknown1861",
        0x1890 => "Unknown1890",
        0x1891 => "Unknown1891",
        0x18a1 => "Unknown18A1",
        0x18b1 => "Unknown18B1",
        0x1982 => "Backlight2",
        0x1a20 => "Unknown1A20",
        0x1b04 => "ReprogControlsV4",
        0x1d4b => "WirelessDeviceStatus",
        0x1df3 => "Unknown1DF3",
        0x1e00 => "EnableHiddenFeatures",
        0x1e02 => "Unknown1E02",
        0x1e22 => "Unknown1E22",
        0x1eb0 => "Unknown1EB0",
        0x40a3 => "K375sFnInversion",
        0x4220 => "LockKeyState",
        0x4521 => "KeyboardDisableKeys",
        0x4531 => "Multiplatform",
        0x4540 => "KeyboardLayout2",
        0x8060 => "ReportRate",
        0x8061 => "ExtendedReportRate",
        0x8070 => "ColorLedEffects",
        0x8090 => "ModeStatus",
        0x8100 => "OnboardProfiles",
        0x8110 => "MouseButtonSpy",
        _ => "Unknown",
    }
}
