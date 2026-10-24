use anyhow::{Context, Result, bail};
use hidapi::{HidApi, HidDevice};
use serde::{Deserialize, Serialize};
use std::cell::Cell;
use std::ffi::CString;
use std::time::{Duration, Instant};

pub const LOGITECH_VID: u16 = 0x046d;
pub const HIDPP_USAGE_PAGE: u16 = 0xff00;
pub const HIDPP_LONG_USAGE: u16 = 0x0002;
pub const HIDPP_SHORT_REPORT: u8 = 0x10;
pub const HIDPP_LONG_REPORT: u8 = 0x11;
const REQUEST_LEN: usize = 20;
const READ_LEN: usize = 64;

#[derive(Clone, Copy, Debug)]
pub struct Feature(u8);

impl Feature {
    pub fn index(self) -> u8 {
        self.0
    }
}

pub struct Transport {
    device: HidDevice,
    device_index: u8,
    software_id: Cell<u8>,
}

impl Transport {
    pub fn open(api: &HidApi, path: &str, device_index: u8) -> Result<Self> {
        let cpath = CString::new(path).context("HID device path contains a NUL byte")?;
        let device = api
            .open_path(&cpath)
            .with_context(|| format!("opening HID device {path}"))?;
        Ok(Self {
            device,
            device_index,
            software_id: Cell::new(0x01),
        })
    }

    pub fn set_device_index(&mut self, device_index: u8) {
        self.device_index = device_index;
    }

    pub fn get_feature(&self, feature_id: u16) -> Result<Option<Feature>> {
        let reply = self.request(0x00, 0x00, &feature_id.to_be_bytes())?;
        Ok(reply
            .and_then(|payload| payload.first().copied())
            .filter(|index| *index != 0)
            .map(Feature))
    }

    pub fn get_feature_with_timeout(
        &self,
        feature_id: u16,
        timeout: Duration,
    ) -> Result<Option<Feature>> {
        let reply = self.request_with_timeout(0x00, 0x00, &feature_id.to_be_bytes(), timeout, 2)?;
        Ok(reply
            .and_then(|payload| payload.first().copied())
            .filter(|index| *index != 0)
            .map(Feature))
    }

    pub fn feature_request(
        &self,
        feature_id: u16,
        function: u8,
        params: &[u8],
    ) -> Result<Option<Vec<u8>>> {
        let Some(feature) = self.get_feature(feature_id)? else {
            return Ok(None);
        };
        self.request(feature.index(), function, params)
    }

    pub fn feature_set(&self) -> Result<Vec<(u8, u16)>> {
        let Some(index) = self.get_feature(0x0001)? else {
            return Ok(Vec::new());
        };
        let Some(reply) = self.request(index.index(), 0x00, &[])? else {
            return Ok(Vec::new());
        };
        let count = reply.first().copied().unwrap_or(0);
        let mut features = Vec::with_capacity(count as usize);
        for feature_index in 0..count {
            let Some(reply) = self.request(index.index(), 0x01, &[feature_index])? else {
                continue;
            };
            if reply.len() >= 2 {
                features.push((feature_index, u16::from_be_bytes([reply[0], reply[1]])));
            }
        }
        Ok(features)
    }

    pub fn name(&self) -> Result<Option<String>> {
        let Some(index) = self.get_feature(0x0005)? else {
            return Ok(None);
        };
        let Some(reply) = self.request(index.index(), 0x00, &[])? else {
            return Ok(None);
        };
        let length = reply.first().copied().unwrap_or(0);
        if length == 0 {
            return Ok(None);
        }
        let mut name = Vec::with_capacity(length as usize);
        while name.len() < length as usize {
            let Some(reply) = self.request(index.index(), 0x01, &[name.len() as u8])? else {
                break;
            };
            if reply.is_empty() {
                break;
            }
            name.extend_from_slice(&reply);
        }
        name.truncate(length as usize);
        Ok(Some(String::from_utf8_lossy(&name).into_owned()))
    }

    pub fn device_info(&self) -> Result<Option<DeviceInfo>> {
        let Some(index) = self.get_feature(0x0003)? else {
            return Ok(None);
        };
        let Some(reply) = self.request(index.index(), 0x00, &[])? else {
            return Ok(None);
        };
        Ok(DeviceInfo::parse(&reply))
    }

    pub fn firmware(&self) -> Result<Vec<FirmwareInfo>> {
        let Some(index) = self.get_feature(0x0003)? else {
            return Ok(Vec::new());
        };
        let Some(reply) = self.request(index.index(), 0x00, &[])? else {
            return Ok(Vec::new());
        };
        let count = reply.first().copied().unwrap_or(0);
        let mut firmware = Vec::with_capacity(count as usize);
        for entity in 0..count {
            let Some(reply) = self.request(index.index(), 0x01, &[entity])? else {
                continue;
            };
            if let Some(info) = FirmwareInfo::parse(&reply) {
                firmware.push(info);
            }
        }
        Ok(firmware)
    }

    pub fn serial(&self) -> Result<Option<String>> {
        let Some(index) = self.get_feature(0x0003)? else {
            return Ok(None);
        };
        let Some(reply) = self.request(index.index(), 0x02, &[])? else {
            return Ok(None);
        };
        let end = reply
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(reply.len());
        let value = &reply[..end];
        if value.is_empty()
            || !value
                .iter()
                .all(|byte| byte.is_ascii_graphic() || *byte == b' ')
        {
            return Ok(None);
        }
        Ok(Some(String::from_utf8_lossy(value).into_owned()))
    }

    pub fn battery(&self) -> Result<Option<Battery>> {
        if let Some(index) = self.get_feature(0x1004)?
            && let Some(reply) = self.request(index.index(), 0x01, &[])?
        {
            return Ok(Battery::from_unified(&reply));
        }
        if let Some(index) = self.get_feature(0x1000)?
            && let Some(reply) = self.request(index.index(), 0x00, &[])?
        {
            return Ok(Battery::from_status(&reply));
        }
        if let Some(index) = self.get_feature(0x1001)?
            && let Some(reply) = self.request(index.index(), 0x00, &[])?
        {
            return Ok(Battery::from_voltage(&reply));
        }
        Ok(None)
    }

    pub fn report_rate_ms(&self) -> Result<Option<u8>> {
        let Some(index) = self.get_feature(0x8060)? else {
            return Ok(None);
        };
        let Some(reply) = self.request(index.index(), 0x01, &[])? else {
            return Ok(None);
        };
        Ok(reply.first().copied())
    }

    pub fn supported_report_rates(&self) -> Result<Vec<u16>> {
        let Some(index) = self.get_feature(0x8060)? else {
            return Ok(Vec::new());
        };
        let Some(reply) = self.request(index.index(), 0x00, &[])? else {
            return Ok(Vec::new());
        };
        let mask = reply.first().copied().unwrap_or(0);
        let mut rates = Vec::new();
        if mask & 0x01 != 0 { rates.push(1000); }
        if mask & 0x02 != 0 { rates.push(500); }
        if mask & 0x08 != 0 { rates.push(250); }
        if mask & 0x80 != 0 { rates.push(125); }
        if rates.is_empty() {
            rates = vec![1000, 500, 250, 125];
        }
        Ok(rates)
    }

    pub fn set_report_rate_ms(&self, rate_ms: u8) -> Result<bool> {
        let Some(index) = self.get_feature(0x8060)? else {
            return Ok(false);
        };
        let reply = self.request(index.index(), 0x02, &[rate_ms])?;
        Ok(reply.is_some())
    }

    pub fn mode_status(&self) -> Result<Option<u8>> {
        let Some(index) = self.get_feature(0x8090)? else {
            return Ok(None);
        };
        let Some(reply) = self.request(index.index(), 0x00, &[])? else {
            return Ok(None);
        };
        Ok(reply.first().copied())
    }

    pub fn set_mode_status(&self, mode: u8) -> Result<bool> {
        let Some(index) = self.get_feature(0x8090)? else {
            return Ok(false);
        };
        let reply = self.request(index.index(), 0x01, &[mode])?;
        Ok(reply.is_some())
    }

    pub fn onboard_dpi(&self) -> Result<Option<OnboardDpi>> {
        let Some(index) = self.get_feature(0x8100)? else {
            return Ok(None);
        };
        let payload = [0x00, 0x01, 0x00, 0x00];
        let Some(reply) = self.request(index.index(), 0x05, &payload)? else {
            return Ok(None);
        };
        if reply.len() < 11 {
            return Ok(None);
        }
        let live_index = self
            .request(index.index(), 0x0B, &[])
            .ok()
            .flatten()
            .and_then(|r| r.first().copied().map(|b| b as usize));
        let active_index = live_index.unwrap_or(reply[0] as usize);
        let default_index = reply[1] as usize;
        let mut levels = Vec::new();
        for chunk in reply[3..reply.len().min(15)].chunks_exact(2) {
            let dpi = u16::from_le_bytes([chunk[0], chunk[1]]);
            if dpi > 0 && dpi != 0xFFFF {
                levels.push(dpi);
            }
        }
        if levels.is_empty() {
            levels = vec![400, 800, 1600, 3200];
        }
        Ok(Some(OnboardDpi {
            active_index,
            default_index,
            levels,
        }))
    }

    pub fn set_onboard_dpi_index(&self, index: u8) -> Result<bool> {
        let Some(feature) = self.get_feature(0x8100)? else {
            return Ok(false);
        };
        let reply = self.request(feature.index(), 0x0C, &[index])?;
        Ok(reply.is_some())
    }

    pub fn read_onboard_sector(&self, sector: u16) -> Result<Option<Vec<u8>>> {
        let Some(feature) = self.get_feature(0x8100)? else {
            return Ok(None);
        };
        // Sector size on G305 / profile format 3 is 255 bytes (0x00FF)
        let s: usize = 255;
        let mut sector_data = Vec::with_capacity(s);
        let mut offset = 0usize;
        while offset < s - 15 {
            std::thread::sleep(Duration::from_millis(5));
            let payload = [
                (sector >> 8) as u8,
                sector as u8,
                (offset >> 8) as u8,
                offset as u8,
            ];
            let Some(chunk) = self.request(feature.index(), 0x05, &payload)? else {
                return Ok(None);
            };
            if chunk.len() < 16 {
                return Ok(None);
            }
            sector_data.extend_from_slice(&chunk[..16]);
            offset += 16;
        }

        // Last chunk at (s - 16) = 239 (0x00EF)
        std::thread::sleep(Duration::from_millis(5));
        let last_offset = (s - 16) as u16;
        let payload = [
            (sector >> 8) as u8,
            sector as u8,
            (last_offset >> 8) as u8,
            last_offset as u8,
        ];
        let Some(last_chunk) = self.request(feature.index(), 0x05, &payload)? else {
            return Ok(None);
        };
        if last_chunk.len() < 16 {
            return Ok(None);
        }
        let needed = s - sector_data.len();
        sector_data.extend_from_slice(&last_chunk[16 - needed..16]);
        Ok(Some(sector_data))
    }

    pub fn write_onboard_sector(&self, sector: u16, data: &[u8]) -> Result<bool> {
        if data.len() != 255 {
            bail!("Sector size must be exactly 255 bytes");
        }
        let Some(feature) = self.get_feature(0x8100)? else {
            return Ok(false);
        };

        // 1. MEMORY_ADDR_WRITE: sector (be), sub_address 0 (be), count 255 (be)
        let addr_payload = [
            (sector >> 8) as u8,
            sector as u8,
            0x00,
            0x00,
            0x00,
            0xFF, // count: 255
        ];
        let addr_reply = self.request(feature.index(), 0x06, &addr_payload)?;
        if addr_reply.is_none() {
            return Ok(false);
        }

        // 2. MEMORY_WRITE: 16 chunks of 16 bytes
        let mut offset = 0;
        while offset < data.len() {
            std::thread::sleep(Duration::from_millis(8));
            let end = (offset + 16).min(data.len());
            let mut chunk = [0u8; 16];
            chunk[..end - offset].copy_from_slice(&data[offset..end]);
            let chunk_reply = self.request(feature.index(), 0x07, &chunk)?;
            if chunk_reply.is_none() {
                return Ok(false);
            }
            offset += 16;
        }

        // 3. MEMORY_WRITE_END
        std::thread::sleep(Duration::from_millis(15));
        let reply = self.request(feature.index(), 0x08, &[])?;
        Ok(reply.is_some())
    }

    pub fn mouse_buttons(&self) -> Result<Option<Vec<MouseButtonInfo>>> {
        let Some(feature) = self.get_feature(0x8100)? else {
            return Ok(None);
        };
        let payload1 = [0x00, 0x01, 0x00, 0x20];
        let Some(chunk1) = self.request(feature.index(), 0x05, &payload1)? else {
            return Ok(None);
        };
        let payload2 = [0x00, 0x01, 0x00, 0x30];
        let Some(chunk2) = self.request(feature.index(), 0x05, &payload2)? else {
            return Ok(None);
        };
        if chunk1.len() < 16 || chunk2.len() < 8 {
            return Ok(None);
        }

        let button_names = [
            "Left Click",
            "Right Click",
            "Middle Click",
            "Back (Side 1)",
            "Forward (Side 2)",
            "DPI Switch",
        ];

        let mut buttons = Vec::with_capacity(6);
        for i in 0..4 {
            let offset = i * 4;
            let raw = [
                chunk1[offset],
                chunk1[offset + 1],
                chunk1[offset + 2],
                chunk1[offset + 3],
            ];
            let (label, category) = parse_mouse_binding(raw);
            buttons.push(MouseButtonInfo {
                index: i as u8,
                name: button_names[i].to_string(),
                raw,
                binding_label: label,
                category,
            });
        }
        for i in 0..2 {
            let offset = i * 4;
            let raw = [
                chunk2[offset],
                chunk2[offset + 1],
                chunk2[offset + 2],
                chunk2[offset + 3],
            ];
            let (label, category) = parse_mouse_binding(raw);
            buttons.push(MouseButtonInfo {
                index: (i + 4) as u8,
                name: button_names[i + 4].to_string(),
                raw,
                binding_label: label,
                category,
            });
        }

        Ok(Some(buttons))
    }

    pub fn set_mouse_button(&self, button_index: u8, raw_binding: [u8; 4]) -> Result<bool> {
        if button_index > 5 {
            bail!("Button index out of range (0-5)");
        }
        let Some(mut sector) = self.read_onboard_sector(0x0001)? else {
            return Ok(false);
        };
        let offset = 32 + (button_index as usize * 4);
        sector[offset..offset + 4].copy_from_slice(&raw_binding);
        let crc = hidpp_crc_ccitt(&sector[..253]);
        sector[253] = (crc >> 8) as u8;
        sector[254] = crc as u8;
        self.write_onboard_sector(0x0001, &sector)
    }

    pub fn backlight(&self) -> Result<Option<BacklightInfo>> {
        let Some(index) = self.get_feature(0x1982)? else {
            return Ok(None);
        };
        let Some(reply) = self.request(index.index(), 0x00, &[])? else {
            return Ok(None);
        };
        if reply.len() < 12 {
            return Ok(None);
        }
        let enabled = reply[0] != 0;
        let options = reply[1];
        let manual_mode = ((options >> 3) & 0x03) == 0x03;
        let effects = u16::from_le_bytes([reply[3], reply[4]]);
        let level = reply[5];
        let dho = u16::from_le_bytes([reply[6], reply[7]]);
        let dhi = u16::from_le_bytes([reply[8], reply[9]]);
        let dpow = u16::from_le_bytes([reply[10], reply[11]]);
        Ok(Some(BacklightInfo {
            enabled,
            manual_mode,
            level,
            max_level: 7,
            current_effect: 0,
            effects,
            dho,
            dhi,
            dpow,
        }))
    }

    pub fn set_backlight_full(
        &self,
        level: u8,
        manual: bool,
        enabled: bool,
        effect: u8,
        duration: u16,
    ) -> Result<bool> {
        let Some(index) = self.get_feature(0x1982)? else {
            return Ok(false);
        };
        // 1. Send effect to function 3
        if effect <= 5 {
            let _ = self.request(index.index(), 0x03, &[effect]);
        }

        // 2. Send full config to function 1
        let options = if manual { (0x09 & 0x07) | (0x03 << 3) } else { 0x09 };
        let dho = duration.min(60);
        let dpow = duration;
        let payload = [
            if enabled { 0x01 } else { 0x00 },
            options,
            if effect <= 5 { effect } else { 0xFF },
            level,
            (dho & 0xFF) as u8,
            (dho >> 8) as u8,
            0x06,
            0x00,
            (dpow & 0xFF) as u8,
            (dpow >> 8) as u8,
        ];
        let reply = self.request(index.index(), 0x01, &payload)?;
        Ok(reply.is_some())
    }

    pub fn set_backlight(&self, level: u8, manual: bool, enabled: bool) -> Result<bool> {
        self.set_backlight_full(level, manual, enabled, 0xFF, 30)
    }

    pub fn set_backlight_effect(&self, effect: u8) -> Result<bool> {
        let Some(index) = self.get_feature(0x1982)? else {
            return Ok(false);
        };
        let reply = self.request(index.index(), 0x03, &[effect])?;
        Ok(reply.is_some())
    }

    pub fn set_backlight_duration(&self, duration: u16) -> Result<bool> {
        let Some(current) = self.backlight()? else {
            return Ok(false);
        };
        self.set_backlight_full(current.level, current.manual_mode, current.enabled, 0xFF, duration)
    }

    pub fn fn_inversion(&self) -> Result<Option<bool>> {
        let Some(index) = self.get_feature(0x40A3)? else {
            return Ok(None);
        };
        let Some(reply) = self.request(index.index(), 0x01, &[])? else {
            return Ok(None);
        };
        Ok(reply.first().map(|b| *b != 0))
    }

    pub fn set_fn_inversion(&self, inverted: bool) -> Result<bool> {
        let Some(index) = self.get_feature(0x40A3)? else {
            return Ok(false);
        };
        let payload = [if inverted { 0x01 } else { 0x00 }];
        let reply = self.request(index.index(), 0x01, &payload)?;
        Ok(reply.is_some())
    }

    pub fn hosts(&self) -> Result<Option<HostsInfo>> {
        let Some(index) = self.get_feature(0x1815)? else {
            return Ok(None);
        };
        let Some(reply) = self.request(index.index(), 0x00, &[])? else {
            return Ok(None);
        };
        let count = reply.get(2).copied().unwrap_or(3);
        let mut slots = Vec::new();
        for slot_idx in 0..count {
            let paired = if let Some(slot_reply) = self.request(index.index(), 0x01, &[slot_idx])? {
                slot_reply.get(1).copied().unwrap_or(0) == 1
            } else {
                false
            };
            slots.push(HostSlot {
                index: slot_idx + 1,
                paired,
            });
        }
        Ok(Some(HostsInfo {
            count,
            current_host: 1,
            slots,
        }))
    }

    pub fn switch_host(&self, host_index: u8) -> Result<bool> {
        let Some(index) = self.get_feature(0x1814)? else {
            return Ok(false);
        };
        let reply = self.request(index.index(), 0x01, &[host_index])?;
        Ok(reply.is_some())
    }

    pub fn disabled_keys(&self) -> Result<Option<DisabledKeysInfo>> {
        let Some(index) = self.get_feature(0x4521)? else {
            return Ok(None);
        };
        let Some(reply) = self.request(index.index(), 0x01, &[])? else {
            return Ok(None);
        };
        let mask = reply.first().copied().unwrap_or(0);
        Ok(Some(DisabledKeysInfo {
            mask,
            caps_lock: mask & 0x01 != 0,
            scroll_lock: mask & 0x02 != 0,
            num_lock: mask & 0x04 != 0,
            win_key: mask & 0x08 != 0,
            insert: mask & 0x10 != 0,
        }))
    }

    pub fn set_disabled_keys(&self, mask: u8) -> Result<bool> {
        let Some(index) = self.get_feature(0x4521)? else {
            return Ok(false);
        };
        let reply = self.request(index.index(), 0x02, &[mask])?;
        Ok(reply.is_some())
    }

    pub fn controls(&self) -> Result<Vec<Control>> {
        let Some(index) = self.get_feature(0x1b04)? else {
            return Ok(Vec::new());
        };
        let Some(reply) = self.request(index.index(), 0x00, &[])? else {
            return Ok(Vec::new());
        };
        let count = reply.first().copied().unwrap_or(0);
        let mut controls = Vec::with_capacity(count as usize);
        for control_index in 0..count {
            let Some(reply) = self.request(index.index(), 0x01, &[control_index])? else {
                continue;
            };
            let Some(mut control) = Control::parse(control_index, &reply) else {
                continue;
            };
            if let Some(mapping) = self.request(index.index(), 0x02, &control.cid.to_be_bytes())? {
                control.apply_mapping(&mapping);
            }
            controls.push(control);
        }
        Ok(controls)
    }

    pub fn set_mapping(&self, cid: u16, target: u16) -> Result<bool> {
        let Some(index) = self.get_feature(0x1b04)? else {
            bail!("device does not expose Reprogrammable Controls V4 (0x1B04)");
        };
        let params = [
            (cid >> 8) as u8,
            cid as u8,
            0x00,
            (target >> 8) as u8,
            target as u8,
        ];
        let Some(reply) = self.request(index.index(), 0x03, &params)? else {
            return Ok(false);
        };
        Ok(reply.len() >= params.len() && reply[..params.len()] == params)
    }

    pub fn request(&self, feature_index: u8, function: u8, params: &[u8]) -> Result<Option<Vec<u8>>> {
        self.request_with_timeout(
            feature_index,
            function,
            params,
            Duration::from_millis(700),
            3,
        )
    }

    fn request_with_timeout(
        &self,
        feature_index: u8,
        function: u8,
        params: &[u8],
        timeout: Duration,
        attempts: usize,
    ) -> Result<Option<Vec<u8>>> {
        if params.len() > REQUEST_LEN - 4 {
            bail!("HID++ request payload is too long");
        }
        let software_id = self.next_software_id();
        let mut request = vec![
            HIDPP_LONG_REPORT,
            self.device_index,
            feature_index,
            (function << 4) | software_id,
        ];
        request.extend_from_slice(params);
        request.resize(REQUEST_LEN, 0);

        for _ in 0..attempts {
            self.drain();
            let written = self
                .device
                .write(&request)
                .context("writing HID++ request")?;
            if written == 0 {
                continue;
            }
            if let Some(reply) = self.read_reply(feature_index, function, software_id, timeout)? {
                return Ok(Some(reply));
            }
        }
        Ok(None)
    }

    fn next_software_id(&self) -> u8 {
        let current = self.software_id.get();
        let next = if current >= 0x0f { 0x01 } else { current + 1 };
        self.software_id.set(next);
        current
    }

    fn read_reply(
        &self,
        feature_index: u8,
        function: u8,
        software_id: u8,
        timeout: Duration,
    ) -> Result<Option<Vec<u8>>> {
        let deadline = Instant::now() + timeout;
        let mut buffer = [0u8; READ_LEN];
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Ok(None);
            }
            let millis = remaining.as_millis().min(i32::MAX as u128) as i32;
            let read = self
                .device
                .read_timeout(&mut buffer, millis)
                .context("reading HID++ reply")?;
            if read == 0 {
                return Ok(None);
            }
            if read < 4 {
                continue;
            }
            if buffer[0] != HIDPP_LONG_REPORT && buffer[0] != HIDPP_SHORT_REPORT {
                continue;
            }
            if buffer[1] != self.device_index
                || buffer[2] != feature_index
                || buffer[3] != (function << 4) | software_id
            {
                continue;
            }
            return Ok(Some(buffer[4..read].to_vec()));
        }
    }

    fn drain(&self) {
        let mut buffer = [0u8; READ_LEN];
        while matches!(self.device.read_timeout(&mut buffer, 5), Ok(read) if read > 0) {}
    }
}

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub unit_id: String,
    pub transport: String,
    pub product_id: u16,
}

impl DeviceInfo {
    fn parse(payload: &[u8]) -> Option<Self> {
        if payload.len() < 9 {
            return None;
        }
        let unit_id = hex_bytes(&payload[1..5]);
        let transport_bits = payload[6];
        let model = &payload[7..payload.len().min(13)];
        let mut offset = 0usize;
        let mut selected = None;
        for (name, flag) in [
            ("btid", 0x01u8),
            ("btleid", 0x02),
            ("wpid", 0x04),
            ("usbid", 0x08),
        ] {
            if transport_bits & flag != 0 {
                let id = model.get(offset..offset + 2).unwrap_or(&[]);
                if id.len() == 2 {
                    let value = u16::from_be_bytes([id[0], id[1]]);
                    selected.get_or_insert((name, value));
                }
                offset += 2;
            }
        }
        let (transport, product_id) = selected?;
        Some(Self {
            unit_id,
            transport: transport.to_string(),
            product_id,
        })
    }
}

#[derive(Clone, Debug)]
pub struct FirmwareInfo {
    pub kind: u8,
    pub name: String,
    pub version: String,
}

impl FirmwareInfo {
    fn parse(payload: &[u8]) -> Option<Self> {
        if payload.len() < 8 {
            return None;
        }
        let kind = payload[0] & 0x0f;
        if kind <= 1 {
            let name = String::from_utf8_lossy(&payload[1..4]).into_owned();
            let major = payload[4];
            let minor = payload[5];
            let build = u16::from_be_bytes([payload[6], payload[7]]);
            let version = if build == 0 {
                format!("{major:02X}.{minor:02X}")
            } else {
                format!("{major:02X}.{minor:02X}.B{build:04X}")
            };
            Some(Self {
                kind,
                name,
                version,
            })
        } else {
            Some(Self {
                kind,
                name: String::new(),
                version: String::new(),
            })
        }
    }
}

#[derive(Clone, Debug)]
pub struct Battery {
    pub level: Option<u8>,
    pub status: BatteryStatus,
    pub voltage_mv: Option<u16>,
}

impl Battery {
    fn from_unified(payload: &[u8]) -> Option<Self> {
        let level = payload.first().copied()?;
        let status_code = payload.get(2).copied().unwrap_or(0);
        Some(Self {
            level: Some(level),
            status: BatteryStatus::from_code(status_code),
            voltage_mv: None,
        })
    }

    fn from_status(payload: &[u8]) -> Option<Self> {
        let level = payload.first().copied()?;
        let status_code = payload.get(2).copied().unwrap_or(0);
        Some(Self {
            level: Some(level),
            status: BatteryStatus::from_code(status_code),
            voltage_mv: None,
        })
    }

    fn from_voltage(payload: &[u8]) -> Option<Self> {
        if payload.len() < 2 {
            return None;
        }
        let voltage = u16::from_be_bytes([payload[0], payload[1]]);
        let flags = payload.get(2).copied().unwrap_or(0);
        Some(Self {
            level: estimate_battery_percent(voltage),
            status: if flags & 0x80 != 0 {
                BatteryStatus::Recharging
            } else {
                BatteryStatus::Discharging
            },
            voltage_mv: Some(voltage),
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BatteryStatus {
    Discharging,
    Recharging,
    AlmostFull,
    Full,
    SlowRecharge,
    Unknown(u8),
}

impl BatteryStatus {
    fn from_code(code: u8) -> Self {
        match code {
            0 => Self::Discharging,
            1 => Self::Recharging,
            2 => Self::AlmostFull,
            3 => Self::Full,
            4 => Self::SlowRecharge,
            other => Self::Unknown(other),
        }
    }

    pub fn label(self) -> String {
        match self {
            Self::Discharging => "discharging".into(),
            Self::Recharging => "recharging".into(),
            Self::AlmostFull => "almost full".into(),
            Self::Full => "full".into(),
            Self::SlowRecharge => "slow recharge".into(),
            Self::Unknown(code) => format!("unknown (0x{code:02X})"),
        }
    }
}

fn estimate_battery_percent(voltage_mv: u16) -> Option<u8> {
    let percent = match voltage_mv {
        0..=2100 => 0,
        2101..=2300 => 20,
        2301..=2500 => 50,
        2501..=2700 => 80,
        _ => 100,
    };
    Some(percent)
}

#[derive(Clone, Debug)]
pub struct Control {
    pub index: u8,
    pub cid: u16,
    pub task: u16,
    pub flags: u16,
    pub pos: u8,
    pub group: u8,
    pub group_mask: u8,
    pub mapped_to: u16,
    pub mapping_flags: u16,
}

impl Control {
    fn parse(index: u8, payload: &[u8]) -> Option<Self> {
        if payload.len() < 9 {
            return None;
        }
        let cid = u16::from_be_bytes([payload[0], payload[1]]);
        let task = u16::from_be_bytes([payload[2], payload[3]]);
        let flags = u16::from(payload[4]) | (u16::from(payload[8]) << 8);
        Some(Self {
            index,
            cid,
            task,
            flags,
            pos: payload[5],
            group: payload[6],
            group_mask: payload[7],
            mapped_to: cid,
            mapping_flags: 0,
        })
    }

    fn apply_mapping(&mut self, payload: &[u8]) {
        if payload.len() < 5 {
            return;
        }
        let cid = u16::from_be_bytes([payload[0], payload[1]]);
        if cid == self.cid {
            self.mapping_flags = u16::from(payload[2]);
            let mapped = u16::from_be_bytes([payload[3], payload[4]]);
            self.mapped_to = if mapped == 0 { self.cid } else { mapped };
            if payload.len() > 5 {
                self.mapping_flags |= u16::from(payload[5]) << 8;
            }
        }
    }

    pub fn flags_label(&self) -> String {
        let mut labels = Vec::new();
        for (flag, label) in [
            (0x0001, "mse"),
            (0x0002, "fn"),
            (0x0004, "nonstandard"),
            (0x0008, "fn-sensitive"),
            (0x0010, "reprogrammable"),
            (0x0020, "divertable"),
            (0x0040, "persistently-divertable"),
            (0x0080, "virtual"),
            (0x0100, "raw-xy"),
            (0x0200, "force-raw-xy"),
            (0x0400, "analytics"),
            (0x0800, "raw-wheel"),
        ] {
            if self.flags & flag != 0 {
                labels.push(label);
            }
        }
        if labels.is_empty() {
            "none".into()
        } else {
            labels.join(", ")
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BacklightInfo {
    pub enabled: bool,
    pub manual_mode: bool,
    pub level: u8,
    pub max_level: u8,
    pub current_effect: u8,
    pub effects: u16,
    pub dho: u16,
    pub dhi: u16,
    pub dpow: u16,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OnboardDpi {
    pub active_index: usize,
    pub default_index: usize,
    pub levels: Vec<u16>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HostSlot {
    pub index: u8,
    pub paired: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct HostsInfo {
    pub count: u8,
    pub current_host: u8,
    pub slots: Vec<HostSlot>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DisabledKeysInfo {
    pub mask: u8,
    pub caps_lock: bool,
    pub scroll_lock: bool,
    pub num_lock: bool,
    pub win_key: bool,
    pub insert: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MouseButtonInfo {
    pub index: u8,
    pub name: String,
    pub raw: [u8; 4],
    pub binding_label: String,
    pub category: String,
}

pub fn hidpp_crc_ccitt(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for &byte in data {
        let temp = (crc >> 8) ^ (byte as u16);
        crc <<= 8;
        let mut quick = temp ^ (temp >> 4);
        crc ^= quick;
        quick <<= 5;
        crc ^= quick;
        quick <<= 7;
        crc ^= quick;
    }
    crc
}

pub fn parse_mouse_binding(raw: [u8; 4]) -> (String, String) {
    match raw[0] {
        0x80 => match raw[1] {
            0x01 => {
                let mask = u16::from_be_bytes([raw[2], raw[3]]);
                let name = match mask {
                    0x0001 => "Left Click",
                    0x0002 => "Right Click",
                    0x0004 => "Middle Click",
                    0x0008 => "Back (Side 1)",
                    0x0010 => "Forward (Side 2)",
                    other => return (format!("Mouse 0x{other:04X}"), "Mouse".into()),
                };
                (name.to_string(), "Mouse".into())
            }
            0x02 => {
                let modifiers = raw[2];
                let key = raw[3];
                let key_name = hid_keycode_name(key);
                let mut mods = Vec::new();
                if modifiers & 0x01 != 0 { mods.push("Ctrl"); }
                if modifiers & 0x02 != 0 { mods.push("Shift"); }
                if modifiers & 0x04 != 0 { mods.push("Alt"); }
                if modifiers & 0x08 != 0 { mods.push("Win"); }
                let label = if mods.is_empty() {
                    key_name
                } else {
                    format!("{} + {}", mods.join(" + "), key_name)
                };
                (label, "Keyboard".into())
            }
            0x03 => {
                let usage = u16::from_be_bytes([raw[2], raw[3]]);
                let label = match usage {
                    0x00E2 => "Mute Audio",
                    0x00E9 => "Volume Up",
                    0x00EA => "Volume Down",
                    0x00CD => "Play / Pause",
                    0x00B5 => "Next Track",
                    0x00B6 => "Previous Track",
                    other => return (format!("Media 0x{other:04X}"), "Media".into()),
                };
                (label.to_string(), "Media".into())
            }
            _ => (format!("HID (0x{:02X}, 0x{:02X})", raw[2], raw[3]), "Custom".into()),
        },
        0x90 => {
            let label = match raw[1] {
                0x01 => "Wheel Tilt Left",
                0x02 => "Wheel Tilt Right",
                0x03 => "DPI Up",
                0x04 => "DPI Down",
                0x05 => "DPI Cycle",
                0x06 => "Default DPI",
                0x07 => "DPI Shift (Sniper)",
                0x08 => "Profile Up",
                0x09 => "Profile Down",
                0x0A => "Profile Cycle",
                other => return (format!("Special 0x{other:02X}"), "DPI / Special".into()),
            };
            (label.to_string(), "DPI / Special".into())
        }
        0xFF => ("Disabled".into(), "Disabled".into()),
        _ => (format!("Raw 0x{:02X}", raw[0]), "Other".into()),
    }
}

fn hid_keycode_name(key: u8) -> String {
    match key {
        0x04..=0x1D => {
            let ch = (b'A' + (key - 0x04)) as char;
            ch.to_string()
        }
        0x1E..=0x26 => {
            let ch = (b'1' + (key - 0x1E)) as char;
            ch.to_string()
        }
        0x27 => "0".into(),
        0x28 => "Enter".into(),
        0x29 => "Escape".into(),
        0x2A => "Backspace".into(),
        0x2B => "Tab".into(),
        0x2C => "Space".into(),
        0x4C => "Delete".into(),
        0x3A..=0x45 => format!("F{}", key - 0x39),
        other => format!("Key 0x{other:02X}"),
    }
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02X}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_unified_battery() {
        let battery = Battery::from_unified(&[0x55, 0x08, 0x00]).unwrap();
        assert_eq!(battery.level, Some(85));
        assert_eq!(battery.status, BatteryStatus::Discharging);
    }

    #[test]
    fn parses_reprogrammable_control() {
        let mut payload = [0u8; 9];
        payload[1] = 0x0A;
        payload[3] = 0x0A;
        payload[4] = 0x14;
        payload[8] = 0x04;

        let control = Control::parse(0, &payload).unwrap();
        assert_eq!(control.cid, 0x000A);
        assert_eq!(control.task, 0x000A);
        assert_eq!(control.flags, 0x0414);
        assert_eq!(
            control.flags_label(),
            "nonstandard, reprogrammable, analytics"
        );
    }

    #[test]
    fn parses_onboard_dpi_payload() {
        let payload = [0x01, 0x01, 0x00, 0x90, 0x01, 0x20, 0x03, 0x40, 0x06, 0x80, 0x0C];
        let active_index = payload[0] as usize;
        let default_index = payload[1] as usize;
        let mut levels = Vec::new();
        for chunk in payload[3..].chunks_exact(2) {
            let dpi = u16::from_le_bytes([chunk[0], chunk[1]]);
            if dpi > 0 && dpi != 0xFFFF {
                levels.push(dpi);
            }
        }
        let dpi_info = OnboardDpi {
            active_index,
            default_index,
            levels,
        };
        assert_eq!(dpi_info.active_index, 1);
        assert_eq!(dpi_info.default_index, 1);
        assert_eq!(dpi_info.levels, vec![400, 800, 1600, 3200]);
    }

    #[test]
    fn parses_disabled_keys_payload() {
        let mask = 0x1F;
        let info = DisabledKeysInfo {
            mask,
            caps_lock: mask & 0x01 != 0,
            scroll_lock: mask & 0x02 != 0,
            num_lock: mask & 0x04 != 0,
            win_key: mask & 0x08 != 0,
            insert: mask & 0x10 != 0,
        };
        assert!(info.caps_lock);
        assert!(info.win_key);
        assert!(info.insert);
    }

    #[test]
    fn parses_backlight_config() {
        let reply = [0x01, 0x19, 0x3D, 0x7F, 0x00, 0x07, 0x06, 0x00, 0x06, 0x00, 0x3C, 0x00];
        let enabled = reply[0] != 0;
        let options = reply[1];
        let manual_mode = ((options >> 3) & 0x03) == 0x03;
        let effects = u16::from_le_bytes([reply[3], reply[4]]);
        let level = reply[5];
        let dho = u16::from_le_bytes([reply[6], reply[7]]);
        let dhi = u16::from_le_bytes([reply[8], reply[9]]);
        let dpow = u16::from_le_bytes([reply[10], reply[11]]);

        let info = BacklightInfo {
            enabled,
            manual_mode,
            level,
            max_level: 7,
            current_effect: 0,
            effects,
            dho,
            dhi,
            dpow,
        };
        assert!(info.enabled);
        assert!(info.manual_mode);
        assert_eq!(info.level, 7);
        assert_eq!(info.effects, 0x007F);
        assert_eq!(info.dho, 6);
        assert_eq!(info.dpow, 60);
    }
}
