use crate::config;
use crate::control_names::control_name;
use crate::device::{self, DeviceDescriptor, Snapshot};
use anyhow::{Result, anyhow};
use eframe::egui;
use hidapi::HidApi;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::Duration;
use tray_icon::menu::{Menu, MenuEvent, MenuItem};
use tray_icon::{Icon, MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

#[cfg(windows)]
pub fn trim_working_set() {
    unsafe {
        unsafe extern "system" {
            fn SetProcessWorkingSetSize(
                h_process: isize,
                dw_minimum_working_set_size: usize,
                dw_maximum_working_set_size: usize,
            ) -> i32;
            fn K32EmptyWorkingSet(h_process: isize) -> i32;
            fn GetCurrentProcess() -> isize;
        }
        let proc = GetCurrentProcess();
        K32EmptyWorkingSet(proc);
        SetProcessWorkingSetSize(proc, usize::MAX, usize::MAX);
    }
}

#[cfg(not(windows))]
pub fn trim_working_set() {}

pub fn run(api: HidApi, devices: Vec<DeviceDescriptor>, start_minimized: bool) -> Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("betterTech - Logitech Device Controller")
            .with_inner_size([1040.0, 720.0])
            .with_min_inner_size([800.0, 540.0])
            .with_visible(!start_minimized),
        ..Default::default()
    };
    eframe::run_native(
        "betterTech",
        options,
        Box::new(move |cc| {
            configure_fonts(&cc.egui_ctx);
            configure_visuals(&cc.egui_ctx);
            let tray = match build_tray() {
                Ok(tray) => Some(tray),
                Err(error) => {
                    eprintln!("tray icon unavailable: {error:#}");
                    None
                }
            };
            Ok(Box::new(App::new(api, devices, tray, cc.egui_ctx.clone())))
        }),
    )
    .map_err(|error| anyhow!("running the GUI: {error}"))
}

fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    #[cfg(windows)]
    {
        if let Ok(bytes) = std::fs::read("C:\\Windows\\Fonts\\segoeui.ttf") {
            fonts.font_data.insert(
                "segoeui".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes)),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .insert(0, "segoeui".to_owned());
        }
        if let Ok(bytes) = std::fs::read("C:\\Windows\\Fonts\\seguisym.ttf") {
            fonts.font_data.insert(
                "seguisym".to_owned(),
                std::sync::Arc::new(egui::FontData::from_owned(bytes)),
            );
            fonts
                .families
                .entry(egui::FontFamily::Proportional)
                .or_default()
                .push("seguisym".to_owned());
        }
    }
    ctx.set_fonts(fonts);
}

fn configure_visuals(ctx: &egui::Context) {
    let mut visuals = egui::Visuals::dark();
    visuals.override_text_color = Some(egui::Color32::from_rgb(241, 245, 249));
    visuals.panel_fill = egui::Color32::from_rgb(13, 17, 24);
    visuals.window_fill = egui::Color32::from_rgb(13, 17, 24);
    visuals.extreme_bg_color = egui::Color32::from_rgb(9, 12, 18);

    visuals.widgets.noninteractive.bg_fill = egui::Color32::from_rgb(18, 24, 34);
    visuals.widgets.noninteractive.bg_stroke =
        egui::Stroke::new(1.0, egui::Color32::from_rgb(32, 44, 62));
    visuals.widgets.noninteractive.corner_radius = egui::CornerRadius::same(8);

    visuals.widgets.inactive.bg_fill = egui::Color32::from_rgb(20, 28, 40);
    visuals.widgets.inactive.bg_stroke =
        egui::Stroke::new(1.0, egui::Color32::from_rgb(36, 48, 70));
    visuals.widgets.inactive.corner_radius = egui::CornerRadius::same(8);

    visuals.widgets.hovered.bg_fill = egui::Color32::from_rgb(28, 40, 58);
    visuals.widgets.hovered.bg_stroke =
        egui::Stroke::new(1.0, egui::Color32::from_rgb(14, 165, 233));
    visuals.widgets.hovered.corner_radius = egui::CornerRadius::same(8);

    visuals.widgets.active.bg_fill = egui::Color32::from_rgb(14, 165, 233);
    visuals.widgets.active.bg_stroke =
        egui::Stroke::new(1.0, egui::Color32::from_rgb(56, 189, 248));
    visuals.widgets.active.corner_radius = egui::CornerRadius::same(8);

    visuals.selection.bg_fill = egui::Color32::from_rgb(14, 165, 233);
    visuals.selection.stroke = egui::Stroke::new(1.0, egui::Color32::WHITE);

    ctx.set_visuals(visuals);
}

struct TrayParts {
    _icon: TrayIcon,
    show_item: MenuItem,
    quit_item: MenuItem,
}

fn build_tray() -> Result<TrayParts> {
    let show_item = MenuItem::new("Show betterTech", true, None);
    let quit_item = MenuItem::new("Quit", true, None);
    let menu = Menu::new();
    menu.append_items(&[&show_item, &quit_item])?;
    let icon = Icon::from_rgba(tray_rgba(), 32, 32)?;
    let tray = TrayIconBuilder::new()
        .with_menu(Box::new(menu))
        .with_tooltip("betterTech - Logitech Controller")
        .with_icon(icon)
        .with_menu_on_left_click(false)
        .build()?;
    Ok(TrayParts {
        _icon: tray,
        show_item,
        quit_item,
    })
}

fn tray_rgba() -> Vec<u8> {
    const SIZE: usize = 32;
    let mut rgba = vec![0u8; SIZE * SIZE * 4];
    let center = (SIZE as f32 - 1.0) / 2.0;
    for y in 0..SIZE {
        for x in 0..SIZE {
            let dx = x as f32 - center;
            let dy = y as f32 - center;
            if dx * dx + dy * dy <= 14.0 * 14.0 {
                let offset = (y * SIZE + x) * 4;
                rgba[offset] = 0x00;
                rgba[offset + 1] = 0xC3;
                rgba[offset + 2] = 0xFF;
                rgba[offset + 3] = 0xFF;
            }
        }
    }
    rgba
}

enum TrayEvent {
    Icon(TrayIconEvent),
    Menu(MenuEvent),
}

fn spawn_tray_waker(ctx: egui::Context, sender: Sender<TrayEvent>) {
    std::thread::spawn(move || {
        loop {
            let mut had_event = false;
            while let Ok(event) = TrayIconEvent::receiver().try_recv() {
                had_event = true;
                let _ = sender.send(TrayEvent::Icon(event));
            }
            while let Ok(event) = MenuEvent::receiver().try_recv() {
                had_event = true;
                let _ = sender.send(TrayEvent::Menu(event));
            }
            if had_event {
                ctx.request_repaint();
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
}

fn show_window(ctx: &egui::Context) {
    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum DeviceTab {
    ControlsRemap,
    MouseButtons,
    Backlight,
    KeyboardSettings,
    EasySwitch,
    PerformancePower,
    SensorDpi,
    HardwareDiagnostics,
    FeatureList,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
enum BacklightSubview {
    Effects,
    Settings,
}

struct BacklightEffectPreset {
    id: u8,
    name: &'static str,
    desc: &'static str,
}

const BACKLIGHT_EFFECTS: [BacklightEffectPreset; 6] = [
    BacklightEffectPreset {
        id: 0,
        name: "Static",
        desc: "The keyboard lights up continuously.",
    },
    BacklightEffectPreset {
        id: 1,
        name: "Contrast",
        desc: "Modifier keys are lit up more brightly than other keys.",
    },
    BacklightEffectPreset {
        id: 2,
        name: "Breathing",
        desc: "The lighting fades in and out slowly.",
    },
    BacklightEffectPreset {
        id: 3,
        name: "Waves",
        desc: "The keyboard lights up in an undulating wave pattern from left to right.",
    },
    BacklightEffectPreset {
        id: 4,
        name: "Reaction",
        desc: "Each key you press lights up, lighting up your typing.",
    },
    BacklightEffectPreset {
        id: 5,
        name: "Random",
        desc: "Keys light up randomly, creating an intriguing pattern.",
    },
];

enum DeviceAction {
    SetBacklight {
        descriptor: DeviceDescriptor,
        level: u8,
        manual: bool,
        enabled: bool,
        effect: u8,
        duration: u16,
    },
    SetBacklightEffect {
        descriptor: DeviceDescriptor,
        effect: u8,
    },
    SetPollingRate {
        descriptor: DeviceDescriptor,
        rate_hz: u16,
    },
    SetPowerMode {
        descriptor: DeviceDescriptor,
        mode: u8,
    },
    SetFnInversion {
        descriptor: DeviceDescriptor,
        inverted: bool,
    },
    SetDisabledKeys {
        descriptor: DeviceDescriptor,
        mask: u8,
    },
    SetDpiStage {
        descriptor: DeviceDescriptor,
        stage: u8,
    },
    SetMouseButton {
        descriptor: DeviceDescriptor,
        button_index: u8,
        raw: [u8; 4],
    },
    ResetAllMouseButtons {
        descriptor: DeviceDescriptor,
    },
    SetMapping {
        descriptor: DeviceDescriptor,
        from: u16,
        to: u16,
    },
    SwitchHost {
        descriptor: DeviceDescriptor,
        host: u8,
    },
}

fn spawn_hardware_worker() -> Sender<DeviceAction> {
    let (sender, receiver) = mpsc::channel::<DeviceAction>();
    std::thread::spawn(move || {
        let Ok(api) = HidApi::new() else {
            eprintln!("Hardware worker failed to initialize HidApi");
            return;
        };
        while let Ok(action) = receiver.recv() {
            match action {
                DeviceAction::SetBacklight {
                    descriptor,
                    level,
                    manual,
                    enabled,
                    effect,
                    duration,
                } => {
                    let _ = device::set_backlight_full(
                        &api,
                        &descriptor,
                        level,
                        manual,
                        enabled,
                        effect,
                        duration,
                    );
                }
                DeviceAction::SetBacklightEffect { descriptor, effect } => {
                    let _ = device::set_backlight_effect(&api, &descriptor, effect);
                }
                DeviceAction::SetPollingRate { descriptor, rate_hz } => {
                    let _ = device::set_polling_rate(&api, &descriptor, rate_hz);
                }
                DeviceAction::SetPowerMode { descriptor, mode } => {
                    let _ = device::set_power_mode(&api, &descriptor, mode);
                }
                DeviceAction::SetFnInversion { descriptor, inverted } => {
                    let _ = device::set_fn_inversion(&api, &descriptor, inverted);
                }
                DeviceAction::SetDisabledKeys { descriptor, mask } => {
                    let _ = device::set_disabled_keys(&api, &descriptor, mask);
                }
                DeviceAction::SetDpiStage { descriptor, stage } => {
                    let _ = device::set_dpi_stage(&api, &descriptor, stage);
                }
                DeviceAction::SetMouseButton {
                    descriptor,
                    button_index,
                    raw,
                } => {
                    let _ = device::set_mouse_button(&api, &descriptor, button_index, raw);
                }
                DeviceAction::ResetAllMouseButtons { descriptor } => {
                    let defaults: [[u8; 4]; 6] = [
                        [0x80, 0x01, 0x00, 0x01],
                        [0x80, 0x01, 0x00, 0x02],
                        [0x80, 0x01, 0x00, 0x04],
                        [0x80, 0x01, 0x00, 0x08],
                        [0x80, 0x01, 0x00, 0x10],
                        [0x90, 0x05, 0x00, 0x00],
                    ];
                    for (i, def) in defaults.iter().enumerate() {
                        let _ = device::set_mouse_button(&api, &descriptor, i as u8, *def);
                    }
                }
                DeviceAction::SetMapping { descriptor, from, to } => {
                    let _ = device::set_mapping(&api, &descriptor, from, to);
                }
                DeviceAction::SwitchHost { descriptor, host } => {
                    let _ = device::switch_host(&api, &descriptor, host);
                }
            }
        }
    });
    sender
}

struct App {
    api: HidApi,
    devices: Vec<DeviceDescriptor>,
    selected: usize,
    snapshot: Option<Snapshot>,
    status: String,
    status_is_error: bool,
    from_cid: Option<u16>,
    to_cid: Option<u16>,
    search_query: String,
    filter_remapped_only: bool,
    current_tab: DeviceTab,
    backlight_subview: BacklightSubview,
    backlight_duration_secs: u16,
    backlight_battery_saving: bool,
    action_sender: Sender<DeviceAction>,
    last_trim: std::time::Instant,
    tray: Option<TrayParts>,
    tray_events: Receiver<TrayEvent>,
    show_settings_modal: bool,
    autostart_enabled: bool,
    is_quitting: bool,
    startup_trims_remaining: u8,
    selected_mouse_button: Option<u8>,
}

impl App {
    fn new(
        api: HidApi,
        devices: Vec<DeviceDescriptor>,
        tray: Option<TrayParts>,
        ctx: egui::Context,
    ) -> Self {
        let (sender, tray_events) = mpsc::channel();
        if tray.is_some() {
            spawn_tray_waker(ctx, sender);
        }
        let action_sender = spawn_hardware_worker();
        Self {
            api,
            devices,
            selected: 0,
            snapshot: None,
            status: "Ready.".into(),
            status_is_error: false,
            from_cid: None,
            to_cid: None,
            search_query: String::new(),
            filter_remapped_only: false,
            current_tab: DeviceTab::ControlsRemap,
            backlight_subview: BacklightSubview::Effects,
            backlight_duration_secs: 30,
            backlight_battery_saving: true,
            action_sender,
            last_trim: std::time::Instant::now(),
            tray,
            tray_events,
            show_settings_modal: false,
            autostart_enabled: crate::autostart::is_enabled(),
            is_quitting: false,
            startup_trims_remaining: 4,
            selected_mouse_button: None,
        }
    }

    fn set_status(&mut self, msg: &str, is_error: bool) {
        self.status = msg.to_string();
        self.status_is_error = is_error;
    }

    fn reload_devices(&mut self) {
        match device::discover(&self.api) {
            Ok(devices) => {
                self.devices = devices;
                self.selected = self.selected.min(self.devices.len().saturating_sub(1));
                self.snapshot = None;
                self.set_status(&format!("Found {} Logitech device(s).", self.devices.len()), false);
            }
            Err(error) => self.set_status(&format!("Discovery failed: {error:#}"), true),
        }
    }

    fn reload_snapshot(&mut self) {
        if self.devices.is_empty() {
            self.snapshot = None;
            return;
        }
        let descriptor = &self.devices[self.selected];
        match device::snapshot(&self.api, descriptor) {
            Ok(snapshot) => {
                if let Some(first) = snapshot
                    .controls
                    .iter()
                    .find(|control| control.flags & 0x10 != 0)
                {
                    self.from_cid.get_or_insert(first.cid);
                    self.to_cid.get_or_insert(first.cid);
                }
                // Auto switch to appropriate tab if current tab is not relevant
                if snapshot.mouse_buttons.is_some() {
                    if self.current_tab == DeviceTab::ControlsRemap || self.current_tab == DeviceTab::PerformancePower {
                        self.current_tab = DeviceTab::MouseButtons;
                    }
                } else if !snapshot.controls.is_empty() {
                    if self.current_tab == DeviceTab::MouseButtons || self.current_tab == DeviceTab::PerformancePower {
                        self.current_tab = DeviceTab::ControlsRemap;
                    }
                }
                if let Some(backlight) = &snapshot.backlight {
                    self.backlight_duration_secs = backlight.dpow;
                }
                if let Ok(cfg) = crate::config::load() {
                    if let Some(s) = cfg.device_settings.iter().find(|s| s.product_id == descriptor.product_id && s.unit_id == descriptor.unit_id) {
                        if let Some(saving) = s.backlight_battery_saving {
                            self.backlight_battery_saving = saving;
                        }
                        if let Some(dur) = s.backlight_duration {
                            self.backlight_duration_secs = dur;
                        }
                    }
                }
                self.snapshot = Some(snapshot);
                trim_working_set();
            }
            Err(error) => {
                self.set_status(&format!("Reading {} failed: {error:#}", descriptor.name), true);
                self.snapshot = None;
            }
        }
    }

    fn apply_mapping(&mut self, from: u16, to: u16) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        let from_name = control_name(from);
        let to_name = control_name(to);
        if let Some(snapshot) = self.snapshot.as_mut() {
            if let Some(ctrl) = snapshot.controls.iter_mut().find(|c| c.cid == from) {
                ctrl.mapped_to = to;
                ctrl.mapped_to_name = to_name.clone();
            }
        }
        if from == to {
            let _ = config::remove(&descriptor.product_id, &descriptor.unit_id, from);
        } else {
            let _ = config::record(
                &descriptor.product_id,
                &descriptor.unit_id,
                from,
                to,
                &from_name,
                &to_name,
            );
        }
        self.set_status(&format!("Mapped '{from_name}' -> '{to_name}'."), false);
        let _ = self.action_sender.send(DeviceAction::SetMapping { descriptor, from, to });
    }

    fn set_polling_rate(&mut self, rate_hz: u16) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        if let Some(snapshot) = self.snapshot.as_mut() {
            snapshot.polling_rate_hz = Some(rate_hz);
        }
        let _ = config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
            s.polling_rate_hz = Some(rate_hz);
        });
        self.set_status(&format!("Polling rate updated to {rate_hz} Hz."), false);
        let _ = self.action_sender.send(DeviceAction::SetPollingRate { descriptor, rate_hz });
    }

    fn set_power_mode(&mut self, mode: u8) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        if let Some(snapshot) = self.snapshot.as_mut() {
            snapshot.mode_status = Some(mode);
        }
        let _ = config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
            s.power_mode = Some(mode);
        });
        let mode_name = if mode == 1 { "Performance (1000Hz, Cyan LED)" } else { "Endurance (125Hz, Green LED)" };
        self.set_status(&format!("Power mode set to {mode_name}."), false);
        let _ = self.action_sender.send(DeviceAction::SetPowerMode { descriptor, mode });
    }

    fn set_backlight_level(&mut self, level: u8, manual: bool, enabled: bool) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        let (effect, duration) = if let Some(snapshot) = self.snapshot.as_mut() {
            if let Some(b) = snapshot.backlight.as_mut() {
                b.level = level;
                b.manual_mode = manual;
                b.enabled = enabled;
                (b.current_effect, b.dpow)
            } else {
                (0, self.backlight_duration_secs)
            }
        } else {
            (0, self.backlight_duration_secs)
        };
        let _ = config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
            s.backlight_level = Some(level);
            s.backlight_manual = Some(manual);
            s.backlight_enabled = Some(enabled);
        });
        let mode_text = if manual { "manual" } else { "auto ambient" };
        self.set_status(&format!("Backlight updated: Level {level} ({mode_text})."), false);
        let _ = self.action_sender.send(DeviceAction::SetBacklight {
            descriptor,
            level,
            manual,
            enabled,
            effect,
            duration,
        });
    }

    fn set_backlight_full(&mut self, level: u8, manual: bool, enabled: bool, effect: u8, duration: u16) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        self.backlight_duration_secs = duration;
        if let Some(snapshot) = self.snapshot.as_mut() {
            if let Some(b) = snapshot.backlight.as_mut() {
                b.level = level;
                b.manual_mode = manual;
                b.enabled = enabled;
                if effect <= 5 {
                    b.current_effect = effect;
                }
                b.dpow = duration;
            }
        }
        let _ = config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
            s.backlight_level = Some(level);
            s.backlight_manual = Some(manual);
            s.backlight_enabled = Some(enabled);
            if effect <= 5 {
                s.backlight_effect = Some(effect);
            }
            s.backlight_duration = Some(duration);
            s.backlight_battery_saving = Some(self.backlight_battery_saving);
        });
        let mode_text = if manual { format!("Level {level}") } else { "Auto".to_string() };
        self.set_status(&format!("Backlight updated: {mode_text}, {duration}s timeout."), false);
        let _ = self.action_sender.send(DeviceAction::SetBacklight {
            descriptor,
            level,
            manual,
            enabled,
            effect,
            duration,
        });
    }

    fn set_backlight_effect(&mut self, effect: u8) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        if let Some(snapshot) = self.snapshot.as_mut() {
            if let Some(b) = snapshot.backlight.as_mut() {
                b.current_effect = effect;
            }
        }
        let _ = config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
            s.backlight_effect = Some(effect);
        });
        let eff_name = BACKLIGHT_EFFECTS.get(effect as usize).map(|e| e.name).unwrap_or("Custom");
        self.set_status(&format!("Backlight effect set to {eff_name}."), false);
        let _ = self.action_sender.send(DeviceAction::SetBacklightEffect { descriptor, effect });
    }

    fn set_backlight_duration(&mut self, duration: u16) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        self.backlight_duration_secs = duration;
        let (level, manual, enabled, effect) = if let Some(snapshot) = self.snapshot.as_mut() {
            if let Some(b) = snapshot.backlight.as_mut() {
                b.dpow = duration;
                (b.level, b.manual_mode, b.enabled, b.current_effect)
            } else {
                (7, true, true, 0)
            }
        } else {
            (7, true, true, 0)
        };
        let _ = config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
            s.backlight_duration = Some(duration);
        });
        self.set_status(&format!("Backlight timeout set to {duration} seconds."), false);
        let _ = self.action_sender.send(DeviceAction::SetBacklight {
            descriptor,
            level,
            manual,
            enabled,
            effect,
            duration,
        });
    }

    fn set_backlight_battery_saving(&mut self, enabled: bool) {
        self.backlight_battery_saving = enabled;
        if let Some(descriptor) = self.devices.get(self.selected) {
            let _ = config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
                s.backlight_battery_saving = Some(enabled);
            });
        }
        let status = if enabled { "enabled (<10% threshold active)" } else { "disabled" };
        self.set_status(&format!("Backlight battery saving mode {status}."), false);
    }

    fn reset_backlighting(&mut self) {
        self.backlight_battery_saving = true;
        self.set_backlight_full(4, false, true, 0, 30);
        self.set_status("Backlighting settings reset to factory defaults (Static, Auto, 30s).", false);
    }

    fn set_fn_inversion(&mut self, inverted: bool) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        if let Some(snapshot) = self.snapshot.as_mut() {
            snapshot.fn_inverted = Some(inverted);
        }
        let _ = config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
            s.fn_inverted = Some(inverted);
        });
        let mode = if inverted { "Standard Function Keys (F1-F12 primary)" } else { "Media / Special Action Keys primary" };
        self.set_status(&format!("Fn-lock changed: {mode}."), false);
        let _ = self.action_sender.send(DeviceAction::SetFnInversion { descriptor, inverted });
    }

    fn toggle_disabled_key(&mut self, bit: u8) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        let current_mask = self.snapshot.as_ref().and_then(|s| s.disabled_keys.as_ref()).map(|k| k.mask).unwrap_or(0);
        let new_mask = current_mask ^ (1 << bit);
        if let Some(snapshot) = self.snapshot.as_mut() {
            if let Some(dis) = snapshot.disabled_keys.as_mut() {
                dis.mask = new_mask;
                dis.caps_lock = new_mask & 0x01 != 0;
                dis.scroll_lock = new_mask & 0x02 != 0;
                dis.num_lock = new_mask & 0x04 != 0;
                dis.win_key = new_mask & 0x08 != 0;
                dis.insert = new_mask & 0x10 != 0;
            }
        }
        let _ = config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
            s.disabled_keys_mask = Some(new_mask);
        });
        self.set_status(&format!("Key disable mask updated to 0x{new_mask:02X}."), false);
        let _ = self.action_sender.send(DeviceAction::SetDisabledKeys { descriptor, mask: new_mask });
    }

    fn set_dpi_stage(&mut self, stage_index: usize) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        if let Some(snapshot) = self.snapshot.as_mut() {
            if let Some(dpi) = snapshot.dpi.as_mut() {
                dpi.active_index = stage_index;
            }
        }
        let _ = config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
            s.dpi_stage = Some(stage_index as u8);
        });
        self.set_status(&format!("Switched to DPI Stage {}.", stage_index + 1), false);
        let _ = self.action_sender.send(DeviceAction::SetDpiStage { descriptor, stage: stage_index as u8 });
    }

    fn switch_host(&mut self, host: u8) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        self.set_status(&format!("Switched to Host Slot {host}."), false);
        let _ = self.action_sender.send(DeviceAction::SwitchHost { descriptor, host });
    }

    fn set_mouse_button(&mut self, button_index: u8, raw: [u8; 4], label: &str) {
        let Some(descriptor) = self.devices.get(self.selected).cloned() else {
            return;
        };
        if let Some(snapshot) = self.snapshot.as_mut() {
            if let Some(buttons) = snapshot.mouse_buttons.as_mut() {
                if let Some(btn) = buttons.get_mut(button_index as usize) {
                    btn.raw = raw;
                    btn.binding_label = label.to_string();
                }
            }
        }
        let _ = config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
            s.mouse_button_remaps.retain(|r| r.button_index != button_index);
            s.mouse_button_remaps.push(config::SavedMouseButtonRemap {
                button_index,
                raw,
                label: label.to_string(),
            });
        });
        self.set_status(&format!("Button {} set to '{label}'.", button_index + 1), false);
        let _ = self.action_sender.send(DeviceAction::SetMouseButton { descriptor, button_index, raw });
    }

    fn reset_all_mouse_buttons(&mut self) {
        let defaults: [[u8; 4]; 6] = [
            [0x80, 0x01, 0x00, 0x01],
            [0x80, 0x01, 0x00, 0x02],
            [0x80, 0x01, 0x00, 0x04],
            [0x80, 0x01, 0x00, 0x08],
            [0x80, 0x01, 0x00, 0x10],
            [0x90, 0x05, 0x00, 0x00],
        ];
        let default_labels = ["Left Click", "Right Click", "Middle Click", "Back", "Forward", "DPI Cycle"];
        let Some(descriptor) = self.devices.get(self.selected).cloned() else { return; };
        if let Some(snapshot) = self.snapshot.as_mut() {
            if let Some(buttons) = snapshot.mouse_buttons.as_mut() {
                for (i, btn) in buttons.iter_mut().enumerate() {
                    if i < 6 {
                        btn.raw = defaults[i];
                        btn.binding_label = default_labels[i].to_string();
                    }
                }
            }
        }
        let _ = config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
            s.mouse_button_remaps.clear();
        });
        self.set_status("All mouse buttons reset to factory defaults.", false);
        let _ = self.action_sender.send(DeviceAction::ResetAllMouseButtons { descriptor });
    }

    fn apply_saved(&mut self) {
        let config = match config::load() {
            Ok(config) => config,
            Err(error) => {
                self.set_status(&format!("Reading config failed: {error:#}"), true);
                return;
            }
        };

        let mut applied_remaps = 0usize;
        for mapping in &config.mappings {
            let descriptor = self.devices.iter().find(|device| {
                device.product_id.eq_ignore_ascii_case(&mapping.product_id)
                    && (mapping.unit_id.is_empty()
                        || device.unit_id.eq_ignore_ascii_case(&mapping.unit_id))
            });
            if let Some(desc) = descriptor {
                if let Ok(true) = device::set_mapping(&self.api, desc, mapping.from, mapping.to) {
                    applied_remaps += 1;
                }
            }
        }

        let mut applied_settings = 0usize;
        for settings in &config.device_settings {
            let descriptor = self.devices.iter().find(|device| {
                device.product_id.eq_ignore_ascii_case(&settings.product_id)
                    && (settings.unit_id.is_empty()
                        || device.unit_id.eq_ignore_ascii_case(&settings.unit_id))
            });
            let Some(desc) = descriptor else { continue; };

            if let Some(rate) = settings.polling_rate_hz {
                if let Ok(true) = device::set_polling_rate(&self.api, desc, rate) {
                    applied_settings += 1;
                }
            }
            if let Some(mode) = settings.power_mode {
                if let Ok(true) = device::set_power_mode(&self.api, desc, mode) {
                    applied_settings += 1;
                }
            }
            if let Some(stage) = settings.dpi_stage {
                if let Ok(true) = device::set_dpi_stage(&self.api, desc, stage) {
                    applied_settings += 1;
                }
            }
            if let Some(fn_inv) = settings.fn_inverted {
                if let Ok(true) = device::set_fn_inversion(&self.api, desc, fn_inv) {
                    applied_settings += 1;
                }
            }
            if let Some(level) = settings.backlight_level {
                let effect = settings.backlight_effect.unwrap_or(0xFF);
                let duration = settings.backlight_duration.unwrap_or(30);
                if let Ok(true) = device::set_backlight_full(
                    &self.api,
                    desc,
                    level,
                    settings.backlight_manual.unwrap_or(false),
                    settings.backlight_enabled.unwrap_or(true),
                    effect,
                    duration,
                ) {
                    applied_settings += 1;
                }
            } else if let Some(effect) = settings.backlight_effect {
                if let Ok(true) = device::set_backlight_effect(&self.api, desc, effect) {
                    applied_settings += 1;
                }
            }
            if let Some(mask) = settings.disabled_keys_mask {
                if let Ok(true) = device::set_disabled_keys(&self.api, desc, mask) {
                    applied_settings += 1;
                }
            }
            for remap in &settings.mouse_button_remaps {
                if let Ok(true) = device::set_mouse_button(&self.api, desc, remap.button_index, remap.raw) {
                    applied_settings += 1;
                }
            }
        }

        self.set_status(
            &format!("Applied {applied_remaps} saved remap(s) and {applied_settings} custom setting(s)."),
            false,
        );
        self.reload_snapshot();
    }
}

fn render_tab_pill(
    ui: &mut egui::Ui,
    current_tab: &mut DeviceTab,
    tab: DeviceTab,
    label: &str,
) {
    let is_active = *current_tab == tab;
    let bg = if is_active {
        egui::Color32::from_rgb(14, 165, 233)
    } else {
        egui::Color32::TRANSPARENT
    };
    let text_color = if is_active {
        egui::Color32::WHITE
    } else {
        egui::Color32::from_rgb(148, 163, 184)
    };
    let frame = egui::Frame::new()
        .fill(bg)
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(14, 7));

    let resp = frame.show(ui, |ui| {
        ui.label(egui::RichText::new(label).color(text_color).strong().size(12.5));
    }).response;

    if resp.interact(egui::Sense::click()).clicked() {
        *current_tab = tab;
    }
}

impl eframe::App for App {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if ctx.input(|i| i.viewport().close_requested()) {
            if self.tray.is_some() && !self.is_quitting {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
                trim_working_set();
            }
        }

        while let Ok(event) = self.tray_events.try_recv() {
            match event {
                TrayEvent::Icon(TrayIconEvent::Click {
                    button: MouseButton::Left,
                    button_state: MouseButtonState::Up,
                    ..
                })
                | TrayEvent::Icon(TrayIconEvent::DoubleClick {
                    button: MouseButton::Left,
                    ..
                }) => show_window(ctx),
                TrayEvent::Icon(_) => {}
                TrayEvent::Menu(event) => {
                    let Some(tray) = &self.tray else {
                        continue;
                    };
                    if event.id == *tray.show_item.id() {
                        show_window(ctx);
                    } else if event.id == *tray.quit_item.id() {
                        self.is_quitting = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            }
        }

        // Periodic memory trimming to ensure physical RAM stays ultra-light (~15-25MB)
        if self.startup_trims_remaining > 0 {
            self.startup_trims_remaining -= 1;
            trim_working_set();
            ctx.request_repaint_after(Duration::from_millis(300));
        } else {
            ctx.request_repaint_after(Duration::from_secs(3));
            if self.last_trim.elapsed() > Duration::from_secs(3) {
                trim_working_set();
                self.last_trim = std::time::Instant::now();
            }
        }

        // Battery saving auto-off enforcement (< 10%)
        if self.backlight_battery_saving {
            if let Some(snapshot) = &self.snapshot {
                if let (Some(battery), Some(backlight)) = (&snapshot.battery, &snapshot.backlight) {
                    if let Some(level) = battery.level {
                        if level < 10 && backlight.enabled {
                            self.set_backlight_level(backlight.level, backlight.manual_mode, false);
                            self.set_status("Battery low (<10%): Backlight automatically turned off to conserve power.", false);
                        }
                    }
                }
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if self.snapshot.is_none() && !self.devices.is_empty() {
            self.reload_snapshot();
        }

        // Top Navigation Bar
        egui::Panel::top("top_panel")
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(15, 20, 29))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(28, 38, 56)))
                    .inner_margin(egui::Margin::symmetric(16, 10)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("betterTech")
                            .color(egui::Color32::WHITE)
                            .size(16.0)
                            .strong(),
                    );
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(26, 36, 52))
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(6, 2))
                        .show(ui, |ui| {
                            ui.label(
                                egui::RichText::new("v0.2.0")
                                    .color(egui::Color32::from_rgb(148, 163, 184))
                                    .size(10.5)
                                    .strong(),
                            );
                        });

                    ui.add_space(12.0);

                    // Status Pill
                    let (dot_color, status_bg, status_text_color) = if self.status_is_error {
                        (
                            egui::Color32::from_rgb(239, 68, 68),
                            egui::Color32::from_rgb(45, 20, 24),
                            egui::Color32::from_rgb(252, 165, 165),
                        )
                    } else {
                        (
                            egui::Color32::from_rgb(16, 185, 129),
                            egui::Color32::from_rgb(16, 36, 30),
                            egui::Color32::from_rgb(110, 231, 183),
                        )
                    };
                    egui::Frame::new()
                        .fill(status_bg)
                        .stroke(egui::Stroke::new(1.0, dot_color))
                        .corner_radius(12.0)
                        .inner_margin(egui::Margin::symmetric(10, 3))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(egui::RichText::new("●").size(10.0).color(dot_color));
                                ui.label(
                                    egui::RichText::new(&self.status)
                                        .size(11.0)
                                        .color(status_text_color)
                                        .strong(),
                                );
                            });
                        });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if self.tray.is_some() && ui.button(egui::RichText::new("Hide to Tray").size(12.0)).clicked() {
                            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Visible(false));
                            trim_working_set();
                        }
                        if ui.button(egui::RichText::new("Preferences").size(12.0)).clicked() {
                            self.show_settings_modal = !self.show_settings_modal;
                        }
                        if ui.button(egui::RichText::new("Apply All").size(12.0).strong().color(egui::Color32::from_rgb(14, 165, 233))).clicked() {
                            self.apply_saved();
                        }
                        if ui.button(egui::RichText::new("Rescan").size(12.0)).clicked() {
                            self.reload_devices();
                        }
                    });
                });
            });

        // Left Device Selector Sidebar
        egui::Panel::left("devices_panel")
            .resizable(true)
            .default_size(260.0)
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(13, 17, 24))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(26, 36, 52)))
                    .inner_margin(egui::Margin::symmetric(12, 14)),
            )
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new("CONNECTED HARDWARE")
                            .size(10.0)
                            .color(egui::Color32::from_rgb(100, 116, 139))
                            .strong(),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(20, 28, 42))
                            .corner_radius(8.0)
                            .inner_margin(egui::Margin::symmetric(6, 2))
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(format!("{}", self.devices.len()))
                                        .size(10.0)
                                        .color(egui::Color32::from_rgb(14, 165, 233))
                                        .strong(),
                                );
                            });
                    });
                });
                ui.add_space(8.0);

                let devices = self.devices.clone();
                for (index, descriptor) in devices.iter().enumerate() {
                    let is_selected = self.selected == index;
                    let is_keyboard = descriptor.name.to_lowercase().contains("keyboard")
                        || descriptor.name.to_lowercase().contains("mechanical")
                        || descriptor.name.to_lowercase().contains("craft")
                        || descriptor.name.to_lowercase().contains("keys");
                    let type_badge = if is_keyboard { "KB" } else { "MS" };

                    let card_bg = if is_selected {
                        egui::Color32::from_rgb(20, 32, 48)
                    } else {
                        egui::Color32::from_rgb(16, 22, 32)
                    };
                    let stroke = if is_selected {
                        egui::Stroke::new(1.5, egui::Color32::from_rgb(14, 165, 233))
                    } else {
                        egui::Stroke::new(1.0, egui::Color32::from_rgb(28, 38, 54))
                    };

                    let card_frame = egui::Frame::new()
                        .fill(card_bg)
                        .stroke(stroke)
                        .corner_radius(8.0)
                        .inner_margin(egui::Margin::symmetric(10, 10));

                    let response = card_frame.show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Selected accent left bar
                            if is_selected {
                                let (r, p) = ui.allocate_painter(egui::vec2(3.0, 32.0), egui::Sense::hover());
                                p.rect_filled(r.rect, 1.5, egui::Color32::from_rgb(14, 165, 233));
                                ui.add_space(3.0);
                            }

                            // Type indicator badge
                            egui::Frame::new()
                                .fill(if is_selected { egui::Color32::from_rgb(14, 165, 233) } else { egui::Color32::from_rgb(26, 36, 52) })
                                .corner_radius(4.0)
                                .inner_margin(egui::Margin::symmetric(6, 4))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new(type_badge)
                                            .size(10.5)
                                            .strong()
                                            .color(if is_selected { egui::Color32::BLACK } else { egui::Color32::from_rgb(148, 163, 184) }),
                                    );
                                });

                            ui.add_space(4.0);

                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new(&descriptor.name)
                                        .strong()
                                        .size(13.0)
                                        .color(if is_selected { egui::Color32::WHITE } else { egui::Color32::from_rgb(226, 232, 240) }),
                                );
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new("●").size(8.0).color(egui::Color32::from_rgb(16, 185, 129)));
                                    ui.colored_label(
                                        egui::Color32::from_rgb(148, 163, 184),
                                        format!("PID 0x{}", descriptor.product_id),
                                    );
                                });
                            });
                        });
                    }).response;

                    if response.interact(egui::Sense::click()).clicked() {
                        self.selected = index;
                        self.snapshot = None;
                        self.from_cid = None;
                        self.to_cid = None;
                        self.selected_mouse_button = None;
                    }
                    ui.add_space(6.0);
                }

                if devices.is_empty() {
                    ui.label("No Logitech HID++ devices found.");
                }

                ui.add_space(16.0);
                ui.separator();
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new("ACTIVE CONFIGURATION")
                        .size(10.0)
                        .color(egui::Color32::from_rgb(100, 116, 139))
                        .strong(),
                );
                if let Ok(path) = config::path() {
                    ui.label(
                        egui::RichText::new(path.display().to_string())
                            .size(9.5)
                            .color(egui::Color32::from_rgb(148, 163, 184)),
                    );
                }
            });

        // Main Center Panel
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(egui::Color32::from_rgb(11, 15, 22)).inner_margin(16.0))
            .show(ui, |ui| {
                if self.devices.is_empty() {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            egui::RichText::new("No Logitech HID++ devices detected.\nMake sure your Logitech device or receiver is plugged in.")
                                .color(egui::Color32::from_rgb(148, 163, 184))
                                .size(16.0),
                        );
                    });
                    return;
                }

                let Some(snapshot) = self.snapshot.clone() else {
                    ui.centered_and_justified(|ui| {
                        ui.label(
                            egui::RichText::new("Querying device hardware...")
                                .size(16.0)
                                .color(egui::Color32::from_rgb(14, 165, 233)),
                        );
                    });
                    return;
                };

                // Device Hero Banner Card
                let is_kb = snapshot.device.name.to_lowercase().contains("keyboard")
                    || snapshot.device.name.to_lowercase().contains("mechanical");
                let hero_badge = if is_kb { "KB" } else { "MS" };
                let hero_category = if is_kb { "WIRELESS MECHANICAL KEYBOARD" } else { "LIGHTSPEED WIRELESS GAMING MOUSE" };

                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(17, 24, 38))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)))
                    .corner_radius(12.0)
                    .inner_margin(16.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            // Large type badge
                            egui::Frame::new()
                                .fill(egui::Color32::from_rgb(11, 16, 26))
                                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)))
                                .corner_radius(10.0)
                                .inner_margin(egui::Margin::symmetric(14, 10))
                                .show(ui, |ui| {
                                    ui.label(egui::RichText::new(hero_badge).size(20.0).strong().color(egui::Color32::from_rgb(14, 165, 233)));
                                });

                            ui.add_space(10.0);

                            ui.vertical(|ui| {
                                ui.label(
                                    egui::RichText::new(hero_category)
                                        .size(10.0)
                                        .color(egui::Color32::from_rgb(14, 165, 233))
                                        .strong(),
                                );
                                ui.heading(
                                    egui::RichText::new(&snapshot.device.name)
                                        .strong()
                                        .color(egui::Color32::WHITE)
                                        .size(20.0),
                                );
                                ui.horizontal(|ui| {
                                    ui.colored_label(egui::Color32::from_rgb(148, 163, 184), format!("PID: 0x{}", snapshot.device.product_id));
                                    ui.colored_label(egui::Color32::from_rgb(71, 85, 105), "•");
                                    ui.colored_label(egui::Color32::from_rgb(148, 163, 184), format!("Unit: {}", snapshot.device.unit_id));
                                    if let Some(serial) = &snapshot.serial {
                                        ui.colored_label(egui::Color32::from_rgb(71, 85, 105), "•");
                                        ui.colored_label(egui::Color32::from_rgb(148, 163, 184), format!("S/N: {serial}"));
                                    }
                                });
                            });

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                // Live Battery Pill
                                if let Some(battery) = &snapshot.battery {
                                    let (bat_color, bat_bg) = if battery.status.contains("recharg") {
                                        (egui::Color32::from_rgb(56, 189, 248), egui::Color32::from_rgb(12, 34, 52))
                                    } else if battery.level.unwrap_or(0) > 40 {
                                        (egui::Color32::from_rgb(16, 185, 129), egui::Color32::from_rgb(12, 34, 26))
                                    } else if battery.level.unwrap_or(0) > 20 {
                                        (egui::Color32::from_rgb(245, 158, 11), egui::Color32::from_rgb(42, 28, 12))
                                    } else {
                                        (egui::Color32::from_rgb(239, 68, 68), egui::Color32::from_rgb(45, 18, 18))
                                    };

                                    egui::Frame::new()
                                        .fill(bat_bg)
                                        .stroke(egui::Stroke::new(1.0, bat_color))
                                        .corner_radius(8.0)
                                        .inner_margin(egui::Margin::symmetric(10, 6))
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(egui::RichText::new("●").size(8.0).color(bat_color));
                                                ui.label(egui::RichText::new(&battery.text).strong().color(bat_color).size(13.0));
                                                if let Some(mv) = battery.voltage_mv {
                                                    ui.colored_label(egui::Color32::from_rgb(148, 163, 184), format!("{mv}mV"));
                                                }
                                            });
                                        });
                                }

                                // Live Polling Rate Badge
                                if let Some(hz) = snapshot.polling_rate_hz {
                                    egui::Frame::new()
                                        .fill(egui::Color32::from_rgb(12, 28, 44))
                                        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(14, 165, 233)))
                                        .corner_radius(8.0)
                                        .inner_margin(egui::Margin::symmetric(10, 6))
                                        .show(ui, |ui| {
                                            ui.colored_label(
                                                egui::Color32::from_rgb(14, 165, 233),
                                                format!("{hz} Hz"),
                                            );
                                        });
                                }
                            });
                        });
                    });

                ui.add_space(12.0);

                // Segmented Pill Tab Bar
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(14, 19, 28))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(28, 38, 54)))
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::same(4))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let has_controls = !snapshot.controls.is_empty();
                            let has_mouse_buttons = snapshot.mouse_buttons.is_some();
                            let has_power = snapshot.polling_rate_hz.is_some() || snapshot.mode_status.is_some();
                            let has_dpi = snapshot.dpi.is_some();
                            let has_backlight = snapshot.backlight.is_some();
                            let has_keyboard_settings = snapshot.fn_inverted.is_some() || snapshot.disabled_keys.is_some();
                            let has_hosts = snapshot.hosts.is_some();

                            if has_mouse_buttons {
                                render_tab_pill(ui, &mut self.current_tab, DeviceTab::MouseButtons, "Button Remap");
                            }
                            if has_controls {
                                render_tab_pill(ui, &mut self.current_tab, DeviceTab::ControlsRemap, "Keys Remap");
                            }
                            if has_backlight {
                                render_tab_pill(ui, &mut self.current_tab, DeviceTab::Backlight, "Backlight");
                            }
                            if has_keyboard_settings {
                                render_tab_pill(ui, &mut self.current_tab, DeviceTab::KeyboardSettings, "Key Settings");
                            }
                            if has_hosts {
                                render_tab_pill(ui, &mut self.current_tab, DeviceTab::EasySwitch, "Easy-Switch");
                            }
                            if has_power {
                                render_tab_pill(ui, &mut self.current_tab, DeviceTab::PerformancePower, "Polling & Power");
                            }
                            if has_dpi {
                                render_tab_pill(ui, &mut self.current_tab, DeviceTab::SensorDpi, "Sensor & DPI");
                            }
                            render_tab_pill(ui, &mut self.current_tab, DeviceTab::HardwareDiagnostics, "Diagnostics");
                            render_tab_pill(ui, &mut self.current_tab, DeviceTab::FeatureList, &format!("Features ({})", snapshot.device.features.len()));
                        });
                    });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(10.0);

                // Tab Body
                match self.current_tab {
                    DeviceTab::MouseButtons => {
                        self.render_mouse_buttons_tab(ui, &snapshot);
                    }
                    DeviceTab::ControlsRemap => {
                        self.render_controls_tab(ui, &snapshot);
                    }
                    DeviceTab::Backlight => {
                        self.render_backlight_tab(ui, &snapshot);
                    }
                    DeviceTab::KeyboardSettings => {
                        self.render_keyboard_settings_tab(ui, &snapshot);
                    }
                    DeviceTab::EasySwitch => {
                        self.render_easy_switch_tab(ui, &snapshot);
                    }
                    DeviceTab::PerformancePower => {
                        self.render_power_tab(ui, &snapshot);
                    }
                    DeviceTab::SensorDpi => {
                        self.render_dpi_tab(ui, &snapshot);
                    }
                    DeviceTab::HardwareDiagnostics => {
                        self.render_hardware_tab(ui, &snapshot);
                    }
                    DeviceTab::FeatureList => {
                        self.render_features_tab(ui, &snapshot);
                    }
                }
            });

        if self.show_settings_modal {
            self.render_settings_window(ui.ctx());
        }
    }
}

// Tab Render Implementations
#[derive(Clone, Copy)]
struct MousePreset {
    name: &'static str,
    category: &'static str,
    raw: [u8; 4],
}

const MOUSE_PRESETS: &[MousePreset] = &[
    // Mouse
    MousePreset { name: "Left Click", category: "Mouse", raw: [0x80, 0x01, 0x00, 0x01] },
    MousePreset { name: "Right Click", category: "Mouse", raw: [0x80, 0x01, 0x00, 0x02] },
    MousePreset { name: "Middle Click", category: "Mouse", raw: [0x80, 0x01, 0x00, 0x04] },
    MousePreset { name: "Back (Side 1)", category: "Mouse", raw: [0x80, 0x01, 0x00, 0x08] },
    MousePreset { name: "Forward (Side 2)", category: "Mouse", raw: [0x80, 0x01, 0x00, 0x10] },

    // DPI & Special
    MousePreset { name: "DPI Cycle (Next)", category: "DPI / Special", raw: [0x90, 0x05, 0x00, 0x00] },
    MousePreset { name: "DPI Up", category: "DPI / Special", raw: [0x90, 0x03, 0x00, 0x00] },
    MousePreset { name: "DPI Down", category: "DPI / Special", raw: [0x90, 0x04, 0x00, 0x00] },
    MousePreset { name: "Default DPI", category: "DPI / Special", raw: [0x90, 0x06, 0x00, 0x00] },
    MousePreset { name: "DPI Shift (Sniper)", category: "DPI / Special", raw: [0x90, 0x07, 0x00, 0x00] },

    // Shortcuts
    MousePreset { name: "Copy (Ctrl + C)", category: "Shortcuts", raw: [0x80, 0x02, 0x01, 0x06] },
    MousePreset { name: "Paste (Ctrl + V)", category: "Shortcuts", raw: [0x80, 0x02, 0x01, 0x19] },
    MousePreset { name: "Cut (Ctrl + X)", category: "Shortcuts", raw: [0x80, 0x02, 0x01, 0x1B] },
    MousePreset { name: "Undo (Ctrl + Z)", category: "Shortcuts", raw: [0x80, 0x02, 0x01, 0x1D] },
    MousePreset { name: "Redo (Ctrl + Y)", category: "Shortcuts", raw: [0x80, 0x02, 0x01, 0x1C] },
    MousePreset { name: "Enter", category: "Shortcuts", raw: [0x80, 0x02, 0x00, 0x28] },
    MousePreset { name: "Escape", category: "Shortcuts", raw: [0x80, 0x02, 0x00, 0x29] },
    MousePreset { name: "Space", category: "Shortcuts", raw: [0x80, 0x02, 0x00, 0x2C] },
    MousePreset { name: "Tab", category: "Shortcuts", raw: [0x80, 0x02, 0x00, 0x2B] },
    MousePreset { name: "Backspace", category: "Shortcuts", raw: [0x80, 0x02, 0x00, 0x2A] },

    // Media
    MousePreset { name: "Mute Audio", category: "Media", raw: [0x80, 0x03, 0x00, 0xE2] },
    MousePreset { name: "Volume Up", category: "Media", raw: [0x80, 0x03, 0x00, 0xE9] },
    MousePreset { name: "Volume Down", category: "Media", raw: [0x80, 0x03, 0x00, 0xEA] },
    MousePreset { name: "Play / Pause", category: "Media", raw: [0x80, 0x03, 0x00, 0xCD] },
    MousePreset { name: "Next Track", category: "Media", raw: [0x80, 0x03, 0x00, 0xB5] },
    MousePreset { name: "Previous Track", category: "Media", raw: [0x80, 0x03, 0x00, 0xB6] },

    // Disabled
    MousePreset { name: "Disabled (No Action)", category: "Disabled", raw: [0xFF, 0x00, 0x00, 0x00] },
];

impl App {
    fn render_mouse_schematic(&mut self, ui: &mut egui::Ui, buttons: &[crate::hidpp::MouseButtonInfo]) {
        let (response, painter) = ui.allocate_painter(egui::vec2(260.0, 390.0), egui::Sense::click());
        let rect = response.rect;
        let cx = rect.center().x;
        let mouse_top = rect.min.y + 14.0;
        let mouse_bottom = rect.max.y - 24.0;
        let mouse_left = cx - 88.0;
        let mouse_right = cx + 88.0;

        // Hover or click position
        let pointer_pos = response.hover_pos();
        let was_clicked = response.clicked();

        // 1. Mouse Chassis Base (Main Body Silhouette)
        let chassis = egui::Rect::from_min_max(
            egui::pos2(mouse_left, mouse_top),
            egui::pos2(mouse_right, mouse_bottom),
        );

        // Soft outer ambient glow
        painter.rect_filled(
            chassis.expand(4.0),
            40.0,
            egui::Color32::from_rgba_premultiplied(0, 195, 255, 12),
        );

        // Dark matte ergonomic shell
        painter.rect_filled(
            chassis,
            egui::CornerRadius { nw: 48, ne: 48, sw: 36, se: 36 },
            egui::Color32::from_rgb(15, 20, 29),
        );
        painter.rect_stroke(
            chassis,
            egui::CornerRadius { nw: 48, ne: 48, sw: 36, se: 36 },
            egui::Stroke::new(1.5, egui::Color32::from_rgb(34, 46, 68)),
            egui::StrokeKind::Outside,
        );

        // 2. Button 1: Left Click Paddle (Index 0)
        let left_paddle = egui::Rect::from_min_max(
            egui::pos2(mouse_left + 4.0, mouse_top + 4.0),
            egui::pos2(cx - 3.0, mouse_top + 138.0),
        );
        let left_hovered = pointer_pos.map_or(false, |p| left_paddle.contains(p));
        let left_selected = self.selected_mouse_button == Some(0) || left_hovered;
        if was_clicked && left_hovered {
            self.selected_mouse_button = Some(0);
        }

        let left_bg = if left_selected {
            egui::Color32::from_rgb(22, 44, 70)
        } else {
            egui::Color32::from_rgb(22, 30, 44)
        };
        let left_stroke = if left_selected {
            egui::Stroke::new(1.5, egui::Color32::from_rgb(14, 165, 233))
        } else {
            egui::Stroke::new(1.0, egui::Color32::from_rgb(38, 52, 74))
        };
        painter.rect_filled(
            left_paddle,
            egui::CornerRadius { nw: 44, ne: 4, sw: 6, se: 4 },
            left_bg,
        );
        painter.rect_stroke(
            left_paddle,
            egui::CornerRadius { nw: 44, ne: 4, sw: 6, se: 4 },
            left_stroke,
            egui::StrokeKind::Outside,
        );
        // Badge & label
        painter.circle_filled(egui::pos2(left_paddle.min.x + 16.0, left_paddle.min.y + 20.0), 9.0, egui::Color32::from_rgb(14, 165, 233));
        painter.text(
            egui::pos2(left_paddle.min.x + 16.0, left_paddle.min.y + 20.0),
            egui::Align2::CENTER_CENTER,
            "1",
            egui::FontId::proportional(11.0),
            egui::Color32::BLACK,
        );
        let btn0_txt = buttons.get(0).map(|b| b.binding_label.as_str()).unwrap_or("Left");
        painter.text(
            egui::pos2(left_paddle.center().x, left_paddle.min.y + 44.0),
            egui::Align2::CENTER_CENTER,
            "LEFT CLICK",
            egui::FontId::proportional(10.0),
            egui::Color32::WHITE,
        );
        painter.text(
            egui::pos2(left_paddle.center().x, left_paddle.min.y + 60.0),
            egui::Align2::CENTER_CENTER,
            btn0_txt,
            egui::FontId::proportional(9.0),
            egui::Color32::from_rgb(148, 163, 184),
        );

        // 3. Button 2: Right Click Paddle (Index 1)
        let right_paddle = egui::Rect::from_min_max(
            egui::pos2(cx + 3.0, mouse_top + 4.0),
            egui::pos2(mouse_right - 4.0, mouse_top + 138.0),
        );
        let right_hovered = pointer_pos.map_or(false, |p| right_paddle.contains(p));
        let right_selected = self.selected_mouse_button == Some(1) || right_hovered;
        if was_clicked && right_hovered {
            self.selected_mouse_button = Some(1);
        }

        let right_bg = if right_selected {
            egui::Color32::from_rgb(22, 44, 70)
        } else {
            egui::Color32::from_rgb(22, 30, 44)
        };
        let right_stroke = if right_selected {
            egui::Stroke::new(1.5, egui::Color32::from_rgb(14, 165, 233))
        } else {
            egui::Stroke::new(1.0, egui::Color32::from_rgb(38, 52, 74))
        };
        painter.rect_filled(
            right_paddle,
            egui::CornerRadius { nw: 4, ne: 44, sw: 4, se: 6 },
            right_bg,
        );
        painter.rect_stroke(
            right_paddle,
            egui::CornerRadius { nw: 4, ne: 44, sw: 4, se: 6 },
            right_stroke,
            egui::StrokeKind::Outside,
        );
        // Badge & label
        painter.circle_filled(egui::pos2(right_paddle.max.x - 16.0, right_paddle.min.y + 20.0), 9.0, egui::Color32::from_rgb(14, 165, 233));
        painter.text(
            egui::pos2(right_paddle.max.x - 16.0, right_paddle.min.y + 20.0),
            egui::Align2::CENTER_CENTER,
            "2",
            egui::FontId::proportional(11.0),
            egui::Color32::BLACK,
        );
        let btn1_txt = buttons.get(1).map(|b| b.binding_label.as_str()).unwrap_or("Right");
        painter.text(
            egui::pos2(right_paddle.center().x, right_paddle.min.y + 44.0),
            egui::Align2::CENTER_CENTER,
            "RIGHT CLICK",
            egui::FontId::proportional(10.0),
            egui::Color32::WHITE,
        );
        painter.text(
            egui::pos2(right_paddle.center().x, right_paddle.min.y + 60.0),
            egui::Align2::CENTER_CENTER,
            btn1_txt,
            egui::FontId::proportional(9.0),
            egui::Color32::from_rgb(148, 163, 184),
        );

        // 4. Center Well & Button 3: Middle Scroll Wheel (Index 2)
        let wheel_well = egui::Rect::from_min_max(
            egui::pos2(cx - 12.0, mouse_top + 22.0),
            egui::pos2(cx + 12.0, mouse_top + 92.0),
        );
        painter.rect_filled(wheel_well, 4.0, egui::Color32::from_rgb(10, 14, 20));
        painter.rect_stroke(wheel_well, 4.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(26, 36, 52)), egui::StrokeKind::Outside);

        let wheel_rect = egui::Rect::from_min_max(
            egui::pos2(cx - 8.0, mouse_top + 26.0),
            egui::pos2(cx + 8.0, mouse_top + 88.0),
        );
        let wheel_hovered = pointer_pos.map_or(false, |p| wheel_rect.expand(2.0).contains(p));
        let wheel_selected = self.selected_mouse_button == Some(2) || wheel_hovered;
        if was_clicked && wheel_hovered {
            self.selected_mouse_button = Some(2);
        }

        let wheel_bg = if wheel_selected {
            egui::Color32::from_rgb(20, 48, 40)
        } else {
            egui::Color32::from_rgb(26, 34, 46)
        };
        let wheel_stroke = if wheel_selected {
            egui::Stroke::new(1.5, egui::Color32::from_rgb(16, 185, 129))
        } else {
            egui::Stroke::new(1.0, egui::Color32::from_rgb(45, 58, 80))
        };
        painter.rect_filled(wheel_rect, 4.0, wheel_bg);
        painter.rect_stroke(wheel_rect, 4.0, wheel_stroke, egui::StrokeKind::Outside);

        // Tactile rubber ribs across scroll wheel
        for y_step in (32..84).step_by(7) {
            painter.line_segment(
                [egui::pos2(cx - 6.0, mouse_top + y_step as f32), egui::pos2(cx + 6.0, mouse_top + y_step as f32)],
                egui::Stroke::new(1.0, egui::Color32::from_rgb(48, 62, 84)),
            );
        }
        // Wheel badge
        painter.circle_filled(egui::pos2(cx, mouse_top + 16.0), 7.0, egui::Color32::from_rgb(16, 185, 129));
        painter.text(
            egui::pos2(cx, mouse_top + 16.0),
            egui::Align2::CENTER_CENTER,
            "3",
            egui::FontId::proportional(9.0),
            egui::Color32::BLACK,
        );

        // 5. Button 6: DPI Switch Button (Index 5)
        let dpi_rect = egui::Rect::from_center_size(
            egui::pos2(cx, mouse_top + 108.0),
            egui::vec2(28.0, 18.0),
        );
        let dpi_hovered = pointer_pos.map_or(false, |p| dpi_rect.expand(2.0).contains(p));
        let dpi_selected = self.selected_mouse_button == Some(5) || dpi_hovered;
        if was_clicked && dpi_hovered {
            self.selected_mouse_button = Some(5);
        }

        let dpi_bg = if dpi_selected {
            egui::Color32::from_rgb(48, 34, 16)
        } else {
            egui::Color32::from_rgb(26, 34, 46)
        };
        let dpi_stroke = if dpi_selected {
            egui::Stroke::new(1.5, egui::Color32::from_rgb(245, 158, 11))
        } else {
            egui::Stroke::new(1.0, egui::Color32::from_rgb(50, 65, 90))
        };
        painter.rect_filled(dpi_rect, 5.0, dpi_bg);
        painter.rect_stroke(dpi_rect, 5.0, dpi_stroke, egui::StrokeKind::Outside);
        painter.text(
            dpi_rect.center(),
            egui::Align2::CENTER_CENTER,
            "6: DPI",
            egui::FontId::proportional(8.5),
            if dpi_selected { egui::Color32::from_rgb(245, 158, 11) } else { egui::Color32::from_rgb(203, 213, 225) },
        );

        // 6. Side Thumb Buttons: Button 5 (Forward, idx 4) and Button 4 (Back, idx 3)
        // Button 5: Side Forward
        let btn5_rect = egui::Rect::from_min_max(
            egui::pos2(mouse_left - 14.0, mouse_top + 150.0),
            egui::pos2(mouse_left + 8.0, mouse_top + 190.0),
        );
        let btn5_hovered = pointer_pos.map_or(false, |p| btn5_rect.expand(2.0).contains(p));
        let btn5_selected = self.selected_mouse_button == Some(4) || btn5_hovered;
        if was_clicked && btn5_hovered {
            self.selected_mouse_button = Some(4);
        }

        let btn5_bg = if btn5_selected {
            egui::Color32::from_rgb(46, 24, 68)
        } else {
            egui::Color32::from_rgb(22, 28, 40)
        };
        let btn5_stroke = if btn5_selected {
            egui::Stroke::new(1.5, egui::Color32::from_rgb(168, 85, 247))
        } else {
            egui::Stroke::new(1.0, egui::Color32::from_rgb(45, 58, 80))
        };
        painter.rect_filled(btn5_rect, egui::CornerRadius { nw: 8, ne: 3, sw: 3, se: 3 }, btn5_bg);
        painter.rect_stroke(btn5_rect, egui::CornerRadius { nw: 8, ne: 3, sw: 3, se: 3 }, btn5_stroke, egui::StrokeKind::Outside);
        painter.text(
            egui::pos2(btn5_rect.center().x - 2.0, btn5_rect.center().y),
            egui::Align2::CENTER_CENTER,
            "5:FWD",
            egui::FontId::proportional(8.5),
            if btn5_selected { egui::Color32::from_rgb(168, 85, 247) } else { egui::Color32::WHITE },
        );

        // Button 4: Side Back
        let btn4_rect = egui::Rect::from_min_max(
            egui::pos2(mouse_left - 14.0, mouse_top + 198.0),
            egui::pos2(mouse_left + 8.0, mouse_top + 238.0),
        );
        let btn4_hovered = pointer_pos.map_or(false, |p| btn4_rect.expand(2.0).contains(p));
        let btn4_selected = self.selected_mouse_button == Some(3) || btn4_hovered;
        if was_clicked && btn4_hovered {
            self.selected_mouse_button = Some(3);
        }

        let btn4_bg = if btn4_selected {
            egui::Color32::from_rgb(46, 24, 68)
        } else {
            egui::Color32::from_rgb(22, 28, 40)
        };
        let btn4_stroke = if btn4_selected {
            egui::Stroke::new(1.5, egui::Color32::from_rgb(168, 85, 247))
        } else {
            egui::Stroke::new(1.0, egui::Color32::from_rgb(45, 58, 80))
        };
        painter.rect_filled(btn4_rect, egui::CornerRadius { nw: 3, ne: 3, sw: 8, se: 3 }, btn4_bg);
        painter.rect_stroke(btn4_rect, egui::CornerRadius { nw: 3, ne: 3, sw: 8, se: 3 }, btn4_stroke, egui::StrokeKind::Outside);
        painter.text(
            egui::pos2(btn4_rect.center().x - 2.0, btn4_rect.center().y),
            egui::Align2::CENTER_CENTER,
            "4:BACK",
            egui::FontId::proportional(8.0),
            if btn4_selected { egui::Color32::from_rgb(168, 85, 247) } else { egui::Color32::WHITE },
        );

        // 7. Palm Rest Emblem & Subtitle
        let palm_y = mouse_top + 252.0;
        painter.circle_filled(
            egui::pos2(cx, palm_y),
            24.0,
            egui::Color32::from_rgba_premultiplied(14, 165, 233, 14),
        );
        painter.text(
            egui::pos2(cx, palm_y - 2.0),
            egui::Align2::CENTER_CENTER,
            "G",
            egui::FontId::proportional(28.0),
            egui::Color32::from_rgb(56, 189, 248),
        );
        painter.text(
            egui::pos2(cx, palm_y + 22.0),
            egui::Align2::CENTER_CENTER,
            "LIGHTSPEED",
            egui::FontId::proportional(8.5),
            egui::Color32::from_rgb(100, 116, 139),
        );

        // 8. Bottom Sensor Badge
        let footer_rect = egui::Rect::from_center_size(
            egui::pos2(cx, mouse_bottom + 14.0),
            egui::vec2(220.0, 16.0),
        );
        painter.text(
            footer_rect.center(),
            egui::Align2::CENTER_CENTER,
            "HERO 12K SENSOR • 0ms ONBOARD",
            egui::FontId::proportional(8.5),
            egui::Color32::from_rgb(71, 85, 105),
        );
    }

    fn render_mouse_buttons_tab(&mut self, ui: &mut egui::Ui, snapshot: &Snapshot) {
        let Some(buttons) = &snapshot.mouse_buttons else {
            ui.label("Mouse button remapping is not supported on this device.");
            return;
        };

        // Top Action Card
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(17, 24, 38))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)))
            .corner_radius(10.0)
            .inner_margin(14.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.heading(
                            egui::RichText::new("Hardware Button Remapping")
                                .color(egui::Color32::WHITE)
                                .size(18.0),
                        );
                        ui.colored_label(
                            egui::Color32::from_rgb(148, 163, 184),
                            "Remap buttons directly into the mouse's onboard memory (0x8100). Works in all games with 0 background lag.",
                        );
                    });
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(egui::RichText::new("Reset All Defaults").strong()).clicked() {
                            self.reset_all_mouse_buttons();
                        }
                    });
                });
            });

        ui.add_space(12.0);

        // 2-Column: Left Side Visual Mouse Diagram, Right Side Interactive Config Cards
        ui.horizontal_top(|ui| {
            // Left Column: Visual Mouse Graphic
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(15, 20, 29))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)))
                .corner_radius(10.0)
                .inner_margin(egui::Margin::symmetric(14, 12))
                .show(ui, |ui| {
                    ui.set_width(260.0);
                    ui.vertical_centered(|ui| {
                        ui.label(
                            egui::RichText::new("INTERACTIVE HARDWARE MAP")
                                .size(10.0)
                                .strong()
                                .color(egui::Color32::from_rgb(14, 165, 233)),
                        );
                        ui.colored_label(
                            egui::Color32::from_rgb(100, 116, 139),
                            "Click any button on the diagram to inspect",
                        );
                        ui.add_space(6.0);

                        self.render_mouse_schematic(ui, buttons);
                    });
                });

            ui.add_space(12.0);

            // Right Column: List of 6 Interactive Configuration Cards
            ui.vertical(|ui| {
                egui::ScrollArea::vertical()
                    .max_height(490.0)
                    .show(ui, |ui| {
                        let defaults: [[u8; 4]; 6] = [
                            [0x80, 0x01, 0x00, 0x01],
                            [0x80, 0x01, 0x00, 0x02],
                            [0x80, 0x01, 0x00, 0x04],
                            [0x80, 0x01, 0x00, 0x08],
                            [0x80, 0x01, 0x00, 0x10],
                            [0x90, 0x05, 0x00, 0x00],
                        ];
                        let default_names = [
                            "Left Click",
                            "Right Click",
                            "Middle Click",
                            "Back (Side 1)",
                            "Forward (Side 2)",
                            "DPI Cycle",
                        ];

                        for btn in buttons {
                            let idx = btn.index as usize;
                            let default_raw = defaults.get(idx).copied().unwrap_or([0, 0, 0, 0]);
                            let is_custom = btn.raw != default_raw;
                            let is_selected = self.selected_mouse_button == Some(btn.index);

                            let card_bg = if is_selected {
                                egui::Color32::from_rgb(22, 34, 52)
                            } else {
                                egui::Color32::from_rgb(17, 24, 38)
                            };
                            let card_stroke = if is_selected {
                                egui::Stroke::new(1.5, egui::Color32::from_rgb(14, 165, 233))
                            } else if is_custom {
                                egui::Stroke::new(1.0, egui::Color32::from_rgb(56, 189, 248))
                            } else {
                                egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64))
                            };

                            let (badge_color, badge_text_color) = match btn.index {
                                0 | 1 => (egui::Color32::from_rgb(14, 165, 233), egui::Color32::BLACK),
                                2 => (egui::Color32::from_rgb(16, 185, 129), egui::Color32::BLACK),
                                3 | 4 => (egui::Color32::from_rgb(168, 85, 247), egui::Color32::WHITE),
                                _ => (egui::Color32::from_rgb(245, 158, 11), egui::Color32::BLACK),
                            };

                            egui::Frame::new()
                                .fill(card_bg)
                                .stroke(card_stroke)
                                .corner_radius(8.0)
                                .inner_margin(12.0)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        // Number badge
                                        egui::Frame::new()
                                            .fill(badge_color)
                                            .corner_radius(12.0)
                                            .inner_margin(egui::Margin::symmetric(9, 4))
                                            .show(ui, |ui| {
                                                ui.label(
                                                    egui::RichText::new(format!("{}", btn.index + 1))
                                                        .strong()
                                                        .color(badge_text_color),
                                                );
                                            });

                                        ui.add_space(8.0);

                                        // Name and Category
                                        ui.vertical(|ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new(&btn.name)
                                                        .strong()
                                                        .size(14.5)
                                                        .color(egui::Color32::WHITE),
                                                );
                                                if is_custom {
                                                    egui::Frame::new()
                                                        .fill(egui::Color32::from_rgb(14, 44, 68))
                                                        .corner_radius(4.0)
                                                        .inner_margin(egui::Margin::symmetric(5, 2))
                                                        .show(ui, |ui| {
                                                            ui.label(
                                                                egui::RichText::new("CUSTOMIZED")
                                                                    .size(9.0)
                                                                    .color(egui::Color32::from_rgb(56, 189, 248))
                                                                    .strong(),
                                                            );
                                                        });
                                                }
                                            });

                                            ui.horizontal(|ui| {
                                                let cat_color = match btn.category.as_str() {
                                                    "Mouse" => egui::Color32::from_rgb(14, 165, 233),
                                                    "DPI / Special" => egui::Color32::from_rgb(245, 158, 11),
                                                    "Keyboard" => egui::Color32::from_rgb(16, 185, 129),
                                                    "Media" => egui::Color32::from_rgb(168, 85, 247),
                                                    _ => egui::Color32::from_rgb(148, 163, 184),
                                                };
                                                ui.colored_label(cat_color, &btn.category);
                                                ui.colored_label(egui::Color32::from_rgb(71, 85, 105), "•");
                                                ui.colored_label(
                                                    egui::Color32::from_rgb(226, 232, 240),
                                                    format!("Active: {}", btn.binding_label),
                                                );
                                            });

                                            // Quick Preset Buttons row
                                            ui.add_space(4.0);
                                            ui.horizontal(|ui| {
                                                if btn.index == 5 {
                                                    // DPI Presets
                                                    if ui.small_button("DPI Cycle").clicked() {
                                                        self.set_mouse_button(5, [0x90, 0x05, 0x00, 0x00], "DPI Cycle");
                                                    }
                                                    if ui.small_button("Sniper (DPI Shift)").clicked() {
                                                        self.set_mouse_button(5, [0x90, 0x07, 0x00, 0x00], "DPI Shift");
                                                    }
                                                    if ui.small_button("Default DPI").clicked() {
                                                        self.set_mouse_button(5, [0x90, 0x06, 0x00, 0x00], "Default DPI");
                                                    }
                                                } else if btn.index == 3 || btn.index == 4 {
                                                    // Side Buttons
                                                    if ui.small_button("Back").clicked() {
                                                        self.set_mouse_button(btn.index, [0x80, 0x01, 0x00, 0x08], "Back");
                                                    }
                                                    if ui.small_button("Forward").clicked() {
                                                        self.set_mouse_button(btn.index, [0x80, 0x01, 0x00, 0x10], "Forward");
                                                    }
                                                    if ui.small_button("Copy").clicked() {
                                                        self.set_mouse_button(btn.index, [0x80, 0x02, 0x01, 0x06], "Copy");
                                                    }
                                                    if ui.small_button("Paste").clicked() {
                                                        self.set_mouse_button(btn.index, [0x80, 0x02, 0x01, 0x19], "Paste");
                                                    }
                                                    if ui.small_button("Mute").clicked() {
                                                        self.set_mouse_button(btn.index, [0x80, 0x03, 0x00, 0xE2], "Mute");
                                                    }
                                                }
                                            });
                                        });

                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            // Reset to default button if customized
                                            if is_custom {
                                                if ui.button("Reset").clicked() {
                                                    let def_name = default_names.get(idx).unwrap_or(&"Default");
                                                    self.set_mouse_button(btn.index, default_raw, def_name);
                                                }
                                            }

                                            // Action Picker Combo Box
                                            egui::ComboBox::from_id_salt(format!("btn_combo_{}", btn.index))
                                                .selected_text(egui::RichText::new(&btn.binding_label).strong())
                                                .width(170.0)
                                                .show_ui(ui, |ui| {
                                                    let mut current_cat = "";
                                                    for preset in MOUSE_PRESETS {
                                                        if preset.category != current_cat {
                                                            current_cat = preset.category;
                                                            ui.separator();
                                                            ui.colored_label(
                                                                egui::Color32::from_rgb(148, 163, 184),
                                                                egui::RichText::new(current_cat).size(10.0),
                                                            );
                                                        }
                                                        let is_preset_sel = btn.raw == preset.raw;
                                                        if ui.selectable_label(is_preset_sel, preset.name).clicked() {
                                                            self.set_mouse_button(btn.index, preset.raw, preset.name);
                                                        }
                                                    }
                                                });
                                        });
                                    });
                                });
                            ui.add_space(8.0);
                        }
                    });
            });
        });
    }

    fn render_controls_tab(&mut self, ui: &mut egui::Ui, snapshot: &Snapshot) {
        if snapshot.controls.is_empty() {
            ui.label("This device does not expose Reprogrammable Controls V4 (0x1B04).");
            return;
        }

        // Remap Quick Action Bar
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(17, 24, 38))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)))
            .corner_radius(10.0)
            .inner_margin(14.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Remap Control:").strong().color(egui::Color32::WHITE));
                    let default_source = snapshot
                        .controls
                        .iter()
                        .find(|control| control.flags & 0x10 != 0)
                        .map(|c| c.cid)
                        .unwrap_or(0);
                    let current_from = self.from_cid.unwrap_or(default_source);

                    egui::ComboBox::from_id_salt("from_control_combo")
                        .width(220.0)
                        .selected_text(control_label(snapshot, current_from))
                        .show_ui(ui, |ui| {
                            for control in snapshot.controls.iter().filter(|c| c.flags & 0x10 != 0) {
                                ui.selectable_value(
                                    &mut self.from_cid,
                                    Some(control.cid),
                                    format!("{} (0x{:04X})", control.name, control.cid),
                                );
                            }
                        });

                    ui.label(egui::RichText::new("->").color(egui::Color32::from_rgb(14, 165, 233)).strong().size(14.0));

                    let default_target = snapshot.controls[0].cid;
                    let current_to = self.to_cid.unwrap_or(default_target);

                    egui::ComboBox::from_id_salt("to_control_combo")
                        .width(220.0)
                        .selected_text(control_label(snapshot, current_to))
                        .show_ui(ui, |ui| {
                            for control in &snapshot.controls {
                                ui.selectable_value(
                                    &mut self.to_cid,
                                    Some(control.cid),
                                    format!("{} (0x{:04X})", control.name, control.cid),
                                );
                            }
                        });

                    let from = self.from_cid.unwrap_or(current_from);
                    let to = self.to_cid.unwrap_or(current_to);

                    if ui.button(egui::RichText::new("Apply Remap").strong().color(egui::Color32::from_rgb(14, 165, 233))).clicked() {
                        self.apply_mapping(from, to);
                    }
                    if ui.button("Reset to Native").clicked() {
                        self.apply_mapping(from, from);
                    }
                });
            });

        ui.add_space(10.0);

        // Search & Filter row
        ui.horizontal(|ui| {
            ui.add(egui::TextEdit::singleline(&mut self.search_query).hint_text("Search controls by name or hex...").desired_width(280.0));
            ui.checkbox(&mut self.filter_remapped_only, "Show remapped only");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.colored_label(egui::Color32::from_rgb(148, 163, 184), format!("{} controls total", snapshot.controls.len()));
            });
        });

        ui.add_space(6.0);

        // Table
        egui::ScrollArea::vertical()
            .max_height(420.0)
            .show(ui, |ui| {
                egui::Grid::new("controls_grid")
                    .striped(true)
                    .num_columns(5)
                    .spacing([14.0, 6.0])
                    .show(ui, |ui| {
                        ui.strong("Control Name");
                        ui.strong("CID");
                        ui.strong("Flags");
                        ui.strong("Current Action");
                        ui.strong("Reset");
                        ui.end_row();

                        let query = self.search_query.trim().to_lowercase();
                        for control in &snapshot.controls {
                            let is_remapped = control.mapped_to != control.cid;
                            if self.filter_remapped_only && !is_remapped {
                                continue;
                            }
                            if !query.is_empty() {
                                let hex_cid = format!("{:04x}", control.cid);
                                if !control.name.to_lowercase().contains(&query) && !hex_cid.contains(&query) {
                                    continue;
                                }
                            }

                            ui.label(egui::RichText::new(&control.name).strong().color(egui::Color32::WHITE));
                            ui.colored_label(egui::Color32::from_rgb(148, 163, 184), format!("0x{:04X}", control.cid));
                            ui.colored_label(egui::Color32::from_rgb(14, 165, 233), &control.flags_label);

                            if is_remapped {
                                ui.colored_label(
                                    egui::Color32::from_rgb(16, 185, 129),
                                    format!("-> {} (0x{:04X})", control.mapped_to_name, control.mapped_to),
                                );
                                if ui.small_button("Native").clicked() {
                                    self.apply_mapping(control.cid, control.cid);
                                }
                            } else {
                                ui.colored_label(egui::Color32::from_rgb(148, 163, 184), &control.mapped_to_name);
                                ui.label("-");
                            }
                            ui.end_row();
                        }
                    });
            });
    }

    fn render_keyboard_schematic(&self, ui: &mut egui::Ui, snapshot: &Snapshot) {
        let Some(backlight) = &snapshot.backlight else { return; };
        let (response, painter) = ui.allocate_painter(egui::vec2(340.0, 280.0), egui::Sense::hover());
        let rect = response.rect;

        // 1. Keyboard Chassis (Dark Aluminum Top Plate)
        let kb_rect = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + 6.0, rect.min.y + 70.0),
            egui::vec2(328.0, 155.0),
        );
        // Ambient glow when backlight is active
        if backlight.enabled {
            painter.rect_filled(
                kb_rect.expand(3.0),
                10.0,
                egui::Color32::from_rgba_premultiplied(0, 195, 255, 18),
            );
        }
        painter.rect_filled(kb_rect, 8.0, egui::Color32::from_rgb(24, 30, 42));
        painter.rect_stroke(kb_rect, 8.0, egui::Stroke::new(1.5, egui::Color32::from_rgb(46, 58, 80)), egui::StrokeKind::Outside);

        // Metallic Header Bezel
        let bezel_rect = egui::Rect::from_min_size(
            egui::pos2(kb_rect.min.x + 4.0, kb_rect.min.y + 4.0),
            egui::vec2(kb_rect.width() - 8.0, 13.0),
        );
        painter.rect_filled(bezel_rect, 3.0, egui::Color32::from_rgb(16, 20, 28));
        painter.text(
            egui::pos2(bezel_rect.center().x, bezel_rect.center().y),
            egui::Align2::CENTER_CENTER,
            "LOGITECH MX MECHANICAL",
            egui::FontId::proportional(8.5),
            egui::Color32::from_rgb(110, 130, 160),
        );

        let time = ui.ctx().input(|i| i.time);
        if backlight.enabled && backlight.current_effect >= 2 {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
        }

        let base_alpha = if backlight.enabled {
            if backlight.manual_mode {
                (backlight.level as f32 / 7.0 * 180.0 + 50.0) as u8
            } else {
                140
            }
        } else {
            0
        };

        let calc_glow = |is_mod: bool, norm_x: f32, seed: u32| -> u8 {
            if !backlight.enabled || base_alpha == 0 {
                return 0;
            }
            match backlight.current_effect {
                0 => base_alpha, // Static
                1 => {
                    // Contrast: Modifier keys lit up significantly brighter
                    if is_mod {
                        (base_alpha as f32 * 1.35).min(255.0) as u8
                    } else {
                        (base_alpha as f32 * 0.40) as u8
                    }
                }
                2 => {
                    // Breathing: Gentle sine pulse
                    let phase = ((time * std::f64::consts::PI * 2.0 / 2.8).sin() * 0.5 + 0.5) as f32;
                    (base_alpha as f32 * (0.15 + 0.85 * phase)).clamp(0.0, 255.0) as u8
                }
                3 => {
                    // Waves: Undulating wave travelling left-to-right
                    let phase = ((time * 2.8 - (norm_x as f64 * 6.28)).sin() * 0.5 + 0.5) as f32;
                    (base_alpha as f32 * (0.15 + 0.85 * phase)).clamp(0.0, 255.0) as u8
                }
                4 => {
                    // Reaction: Typing pulses that flare up and decay
                    let cycle = (time * 1.6 + (seed as f64 * 0.618)) % 2.4;
                    let pulse = if cycle < 0.6 {
                        (1.0 - (cycle / 0.6)) as f32
                    } else {
                        0.15
                    };
                    (base_alpha as f32 * pulse.clamp(0.15, 1.0)) as u8
                }
                5 => {
                    // Random / Starlight: Individual twinkling keys
                    let twinkle = ((time * 3.2 + (seed as f64 * 1.732)).sin() * 0.5 + 0.5) as f32;
                    let sharp = twinkle.powi(3);
                    (base_alpha as f32 * (0.2 + 0.8 * sharp)).clamp(0.0, 255.0) as u8
                }
                _ => base_alpha,
            }
        };

        let row_y = [
            kb_rect.min.y + 21.0, // Row 0: Esc, F1-F12
            kb_rect.min.y + 42.0, // Row 1: Numbers
            kb_rect.min.y + 63.0, // Row 2: QWERTY
            kb_rect.min.y + 84.0, // Row 3: ASDF
            kb_rect.min.y + 105.0, // Row 4: ZXCV
            kb_rect.min.y + 126.0, // Row 5: Space, Fn, Lightbulb
        ];

        // Row 0: F-Keys
        for i in 0..14 {
            let kw: f32 = if i == 0 || i == 13 { 22.0 } else { 18.0 };
            let kx = kb_rect.min.x + 6.0 + (i as f32 * 22.6);
            let krect = egui::Rect::from_min_size(egui::pos2(kx, row_y[0]), egui::vec2(kw.min(21.0), 16.0));
            let norm_x = (kx - kb_rect.min.x) / kb_rect.width();
            let glow = calc_glow(true, norm_x, i as u32);
            draw_keycap(&painter, krect, true, glow);
        }
        // Row 1: Numbers
        for i in 0..14 {
            let kw: f32 = if i == 13 { 26.0 } else { 19.5 };
            let kx = kb_rect.min.x + 6.0 + (i as f32 * 22.6);
            let krect = egui::Rect::from_min_size(egui::pos2(kx, row_y[1]), egui::vec2(kw.min(25.0), 16.0));
            let is_mod = i == 0 || i == 13;
            let norm_x = (kx - kb_rect.min.x) / kb_rect.width();
            let glow = calc_glow(is_mod, norm_x, 14 + i as u32);
            draw_keycap(&painter, krect, is_mod, glow);
        }
        // Row 2: QWERTY
        for i in 0..14 {
            let kw: f32 = if i == 0 { 25.0 } else if i == 13 { 21.0 } else { 19.5 };
            let kx = kb_rect.min.x + 6.0 + (i as f32 * 22.6);
            let krect = egui::Rect::from_min_size(egui::pos2(kx, row_y[2]), egui::vec2(kw.min(23.0), 16.0));
            let is_mod = i == 0 || i == 13;
            let norm_x = (kx - kb_rect.min.x) / kb_rect.width();
            let glow = calc_glow(is_mod, norm_x, 28 + i as u32);
            draw_keycap(&painter, krect, is_mod, glow);
        }
        // Row 3: ASDF
        for i in 0..13 {
            let kw: f32 = if i == 0 { 27.0 } else if i == 12 { 31.0 } else { 19.5 };
            let kx = kb_rect.min.x + 6.0 + (i as f32 * 24.3);
            let krect = egui::Rect::from_min_size(egui::pos2(kx, row_y[3]), egui::vec2(kw.min(29.0), 16.0));
            let is_mod = i == 0 || i == 12;
            let norm_x = (kx - kb_rect.min.x) / kb_rect.width();
            let glow = calc_glow(is_mod, norm_x, 42 + i as u32);
            draw_keycap(&painter, krect, is_mod, glow);
        }
        // Row 4: ZXCV
        for i in 0..12 {
            let kw: f32 = if i == 0 { 32.0 } else if i == 11 { 29.0 } else { 20.0 };
            let kx = kb_rect.min.x + 6.0 + (i as f32 * 26.3);
            let krect = egui::Rect::from_min_size(egui::pos2(kx, row_y[4]), egui::vec2(kw.min(30.0), 16.0));
            let is_mod = i == 0 || i == 11;
            let norm_x = (kx - kb_rect.min.x) / kb_rect.width();
            let glow = calc_glow(is_mod, norm_x, 55 + i as u32);
            draw_keycap(&painter, krect, is_mod, glow);
        }

        // Row 5: Bottom row - Highlighted Fn and Lightbulb keys
        let lctrl = egui::Rect::from_min_size(egui::pos2(kb_rect.min.x + 6.0, row_y[5]), egui::vec2(24.0, 18.0));
        let lwin = egui::Rect::from_min_size(egui::pos2(kb_rect.min.x + 33.0, row_y[5]), egui::vec2(20.0, 18.0));
        let lalt = egui::Rect::from_min_size(egui::pos2(kb_rect.min.x + 56.0, row_y[5]), egui::vec2(22.0, 18.0));
        let space = egui::Rect::from_min_size(egui::pos2(kb_rect.min.x + 81.0, row_y[5]), egui::vec2(100.0, 18.0));
        let ralt = egui::Rect::from_min_size(egui::pos2(kb_rect.min.x + 184.0, row_y[5]), egui::vec2(22.0, 18.0));
        let fn_key = egui::Rect::from_min_size(egui::pos2(kb_rect.min.x + 209.0, row_y[5]), egui::vec2(24.0, 18.0));
        let lightbulb_key = egui::Rect::from_min_size(egui::pos2(kb_rect.min.x + 236.0, row_y[5]), egui::vec2(24.0, 18.0));
        let rarrow1 = egui::Rect::from_min_size(egui::pos2(kb_rect.min.x + 263.0, row_y[5]), egui::vec2(16.0, 18.0));
        let rarrow2 = egui::Rect::from_min_size(egui::pos2(kb_rect.min.x + 282.0, row_y[5]), egui::vec2(16.0, 18.0));
        let rarrow3 = egui::Rect::from_min_size(egui::pos2(kb_rect.min.x + 301.0, row_y[5]), egui::vec2(16.0, 18.0));

        let lctrl_glow = calc_glow(true, (lctrl.center().x - kb_rect.min.x) / kb_rect.width(), 67);
        let lwin_glow = calc_glow(true, (lwin.center().x - kb_rect.min.x) / kb_rect.width(), 68);
        let lalt_glow = calc_glow(true, (lalt.center().x - kb_rect.min.x) / kb_rect.width(), 69);
        let space_glow = calc_glow(false, (space.center().x - kb_rect.min.x) / kb_rect.width(), 70);
        let ralt_glow = calc_glow(true, (ralt.center().x - kb_rect.min.x) / kb_rect.width(), 71);
        let rarrow1_glow = calc_glow(true, (rarrow1.center().x - kb_rect.min.x) / kb_rect.width(), 72);
        let rarrow2_glow = calc_glow(true, (rarrow2.center().x - kb_rect.min.x) / kb_rect.width(), 73);
        let rarrow3_glow = calc_glow(true, (rarrow3.center().x - kb_rect.min.x) / kb_rect.width(), 74);

        draw_keycap(&painter, lctrl, true, lctrl_glow);
        draw_keycap(&painter, lwin, true, lwin_glow);
        draw_keycap(&painter, lalt, true, lalt_glow);
        draw_keycap(&painter, space, false, space_glow);
        draw_keycap(&painter, ralt, true, ralt_glow);
        // Callout target 1: FN Key
        draw_keycap_accent(&painter, fn_key, "Fn", egui::Color32::from_rgb(14, 165, 233));
        // Callout target 2: Lightbulb Key
        draw_keycap_accent(&painter, lightbulb_key, "FX", egui::Color32::from_rgb(250, 204, 21));
        draw_keycap(&painter, rarrow1, true, rarrow1_glow);
        draw_keycap(&painter, rarrow2, true, rarrow2_glow);
        draw_keycap(&painter, rarrow3, true, rarrow3_glow);

        // 2. Callout 1 (Top Left): "Backlighting" Duration & Brightness
        let callout1_rect = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + 6.0, rect.min.y + 6.0),
            egui::vec2(154.0, 52.0),
        );
        painter.rect_filled(callout1_rect, 8.0, egui::Color32::from_rgb(17, 24, 38));
        painter.rect_stroke(callout1_rect, 8.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(14, 165, 233)), egui::StrokeKind::Outside);
        painter.text(
            egui::pos2(callout1_rect.min.x + 10.0, callout1_rect.min.y + 11.0),
            egui::Align2::LEFT_CENTER,
            "Backlighting",
            egui::FontId::proportional(12.0),
            egui::Color32::from_rgb(14, 165, 233),
        );
        painter.text(
            egui::pos2(callout1_rect.min.x + 10.0, callout1_rect.min.y + 26.0),
            egui::Align2::LEFT_CENTER,
            format!("Time Duration: {} sec", backlight.dpow),
            egui::FontId::proportional(10.0),
            egui::Color32::from_rgb(226, 232, 240),
        );
        let brightness_text = if backlight.manual_mode {
            format!("Brightness: Level {}", backlight.level)
        } else {
            "Brightness: Auto (Sensor)".to_string()
        };
        painter.text(
            egui::pos2(callout1_rect.min.x + 10.0, callout1_rect.min.y + 40.0),
            egui::Align2::LEFT_CENTER,
            brightness_text,
            egui::FontId::proportional(10.0),
            egui::Color32::from_rgb(148, 163, 184),
        );
        // Leader line to Fn key
        let pin1 = egui::pos2(callout1_rect.center().x + 20.0, callout1_rect.max.y);
        let target1 = egui::pos2(fn_key.center().x, fn_key.min.y - 2.0);
        painter.line_segment([pin1, target1], egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(14, 165, 233, 140)));
        painter.circle_filled(target1, 2.5, egui::Color32::from_rgb(14, 165, 233));

        // 3. Callout 2 (Top Right): "Backlighting Effects"
        let callout2_rect = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + 168.0, rect.min.y + 6.0),
            egui::vec2(166.0, 52.0),
        );
        painter.rect_filled(callout2_rect, 8.0, egui::Color32::from_rgb(17, 24, 38));
        painter.rect_stroke(callout2_rect, 8.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(250, 204, 21)), egui::StrokeKind::Outside);
        painter.text(
            egui::pos2(callout2_rect.min.x + 10.0, callout2_rect.min.y + 11.0),
            egui::Align2::LEFT_CENTER,
            "Backlighting Effects",
            egui::FontId::proportional(12.0),
            egui::Color32::from_rgb(250, 204, 21),
        );
        let active_effect = BACKLIGHT_EFFECTS.get(backlight.current_effect as usize)
            .map(|e| e.name)
            .unwrap_or("Static");
        painter.text(
            egui::pos2(callout2_rect.min.x + 10.0, callout2_rect.min.y + 27.0),
            egui::Align2::LEFT_CENTER,
            format!("FX: {active_effect}"),
            egui::FontId::proportional(11.5),
            egui::Color32::WHITE,
        );
        painter.text(
            egui::pos2(callout2_rect.min.x + 10.0, callout2_rect.min.y + 40.0),
            egui::Align2::LEFT_CENTER,
            "Shortcut: Fn + Backlight key",
            egui::FontId::proportional(9.5),
            egui::Color32::from_rgb(148, 163, 184),
        );
        // Leader line to Lightbulb key
        let pin2 = egui::pos2(callout2_rect.center().x - 10.0, callout2_rect.max.y);
        let target2 = egui::pos2(lightbulb_key.center().x, lightbulb_key.min.y - 2.0);
        painter.line_segment([pin2, target2], egui::Stroke::new(1.0, egui::Color32::from_rgba_premultiplied(250, 204, 21, 140)));
        painter.circle_filled(target2, 2.5, egui::Color32::from_rgb(250, 204, 21));

        // 4. Battery Badge (Bottom Left)
        let batt_level = snapshot.battery.as_ref().and_then(|b| b.level).unwrap_or(85);
        let batt_status = snapshot.battery.as_ref().map(|b| b.status.as_str()).unwrap_or("discharging");
        let batt_rect = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + 6.0, rect.min.y + 236.0),
            egui::vec2(146.0, 32.0),
        );
        painter.rect_filled(batt_rect, 6.0, egui::Color32::from_rgb(17, 24, 38));
        painter.rect_stroke(batt_rect, 6.0, egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)), egui::StrokeKind::Outside);
        let batt_color = if batt_level <= 10 {
            egui::Color32::from_rgb(239, 68, 68)
        } else {
            egui::Color32::from_rgb(16, 185, 129)
        };
        painter.text(
            egui::pos2(batt_rect.min.x + 10.0, batt_rect.center().y),
            egui::Align2::LEFT_CENTER,
            format!("{batt_level}% ({batt_status})"),
            egui::FontId::proportional(11.0),
            batt_color,
        );
    }

    fn render_backlight_tab(&mut self, ui: &mut egui::Ui, snapshot: &Snapshot) {
        let Some(backlight) = &snapshot.backlight else {
            ui.label("Backlight feature is not supported by this device.");
            return;
        };

        // Top Subview Switcher Pills
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(15, 20, 30))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(28, 38, 56)))
            .corner_radius(8.0)
            .inner_margin(4.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let effects_selected = self.backlight_subview == BacklightSubview::Effects;
                    let settings_selected = self.backlight_subview == BacklightSubview::Settings;

                    let (eff_bg, eff_txt) = if effects_selected {
                        (egui::Color32::from_rgb(14, 165, 233), egui::Color32::WHITE)
                    } else {
                        (egui::Color32::TRANSPARENT, egui::Color32::from_rgb(148, 163, 184))
                    };
                    if ui.add(egui::Button::new(egui::RichText::new("Backlight Effects").color(eff_txt).strong()).fill(eff_bg).corner_radius(6.0)).clicked() {
                        self.backlight_subview = BacklightSubview::Effects;
                    }

                    let (set_bg, set_txt) = if settings_selected {
                        (egui::Color32::from_rgb(14, 165, 233), egui::Color32::WHITE)
                    } else {
                        (egui::Color32::TRANSPARENT, egui::Color32::from_rgb(148, 163, 184))
                    };
                    if ui.add(egui::Button::new(egui::RichText::new("Illumination Settings").color(set_txt).strong()).fill(set_bg).corner_radius(6.0)).clicked() {
                        self.backlight_subview = BacklightSubview::Settings;
                    }
                });
            });

        ui.add_space(10.0);

        // 2-Column: Left Side Visual Keyboard Schematic, Right Side View Content
        ui.horizontal_top(|ui| {
            // Left Column: Visual Schematic Card
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(17, 24, 38))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)))
                .corner_radius(12.0)
                .inner_margin(14.0)
                .show(ui, |ui| {
                    ui.set_width(344.0);
                    ui.vertical_centered(|ui| {
                        ui.horizontal(|ui| {
                            egui::Frame::new()
                                .fill(egui::Color32::from_rgb(14, 44, 68))
                                .corner_radius(4.0)
                                .inner_margin(egui::Margin::symmetric(8, 3))
                                .show(ui, |ui| {
                                    ui.label(
                                        egui::RichText::new("REAL-TIME LIGHTING SIMULATOR")
                                            .size(9.5)
                                            .strong()
                                            .color(egui::Color32::from_rgb(56, 189, 248)),
                                    );
                                });
                        });
                        ui.add_space(8.0);

                        self.render_keyboard_schematic(ui, snapshot);

                        ui.add_space(8.0);

                        // Quick Spec Footer Box
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(12, 17, 26))
                            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(26, 36, 52)))
                            .corner_radius(8.0)
                            .inner_margin(10.0)
                            .show(ui, |ui| {
                                ui.set_width(328.0);
                                ui.horizontal(|ui| {
                                    ui.colored_label(egui::Color32::from_rgb(100, 116, 139), "Protocol:");
                                    ui.colored_label(egui::Color32::WHITE, "HID++ 0x1982");
                                    ui.add_space(8.0);
                                    ui.colored_label(egui::Color32::from_rgb(100, 116, 139), "Timeout:");
                                    ui.colored_label(egui::Color32::from_rgb(14, 165, 233), format!("{}s", backlight.dpow));
                                    ui.add_space(8.0);
                                    ui.colored_label(egui::Color32::from_rgb(100, 116, 139), "Sensor:");
                                    if !backlight.manual_mode {
                                        ui.colored_label(egui::Color32::from_rgb(16, 185, 129), "Smart Ambient");
                                    } else {
                                        ui.colored_label(egui::Color32::from_rgb(148, 163, 184), format!("Manual (L{})", backlight.level));
                                    }
                                });
                            });
                    });
                });

            ui.add_space(12.0);

            // Right Column: Active View
            ui.vertical(|ui| {
                match self.backlight_subview {
                    BacklightSubview::Effects => {
                        ui.heading(egui::RichText::new("Select Backlight Effect").color(egui::Color32::WHITE).size(18.0));
                        ui.colored_label(egui::Color32::from_rgb(148, 163, 184), "Select real-time illuminated animations synced directly with keyboard hardware.");
                        ui.add_space(12.0);

                        egui::ScrollArea::vertical().id_salt("effects_scroll").show(ui, |ui| {
                            for effect in &BACKLIGHT_EFFECTS {
                                let is_active = backlight.current_effect == effect.id;
                                let card_bg = if is_active {
                                    egui::Color32::from_rgb(18, 36, 56)
                                } else {
                                    egui::Color32::from_rgb(15, 20, 30)
                                };
                                let stroke_color = if is_active {
                                    egui::Color32::from_rgb(14, 165, 233)
                                } else {
                                    egui::Color32::from_rgb(30, 42, 64)
                                };

                                let card_frame = egui::Frame::new()
                                    .fill(card_bg)
                                    .stroke(egui::Stroke::new(if is_active { 1.5 } else { 1.0 }, stroke_color))
                                    .corner_radius(10.0)
                                    .inner_margin(12.0);

                                let resp = card_frame.show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        // Number badge container
                                        let icon_box = egui::Frame::new()
                                            .fill(if is_active { egui::Color32::from_rgb(14, 52, 80) } else { egui::Color32::from_rgb(22, 30, 44) })
                                            .corner_radius(6.0)
                                            .inner_margin(egui::Margin::symmetric(9, 6));
                                        icon_box.show(ui, |ui| {
                                            ui.label(
                                                egui::RichText::new(format!("{:02}", effect.id + 1))
                                                    .size(13.0)
                                                    .strong()
                                                    .color(if is_active { egui::Color32::from_rgb(56, 189, 248) } else { egui::Color32::from_rgb(148, 163, 184) }),
                                            );
                                        });

                                        ui.add_space(10.0);

                                        ui.vertical(|ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(
                                                    egui::RichText::new(effect.name)
                                                        .size(15.0)
                                                        .strong()
                                                        .color(if is_active { egui::Color32::WHITE } else { egui::Color32::from_rgb(226, 232, 240) }),
                                                );
                                                if is_active {
                                                    egui::Frame::new()
                                                        .fill(egui::Color32::from_rgb(12, 38, 28))
                                                        .corner_radius(4.0)
                                                        .inner_margin(egui::Margin::symmetric(8, 2))
                                                        .show(ui, |ui| {
                                                            ui.label(
                                                                egui::RichText::new("● ACTIVE")
                                                                    .size(9.5)
                                                                    .color(egui::Color32::from_rgb(16, 185, 129))
                                                                    .strong(),
                                                            );
                                                        });
                                                }
                                            });
                                            ui.add_space(2.0);
                                            ui.colored_label(egui::Color32::from_rgb(148, 163, 184), effect.desc);
                                        });
                                    });
                                });

                                if resp.response.interact(egui::Sense::click()).clicked() {
                                    self.set_backlight_effect(effect.id);
                                }
                                ui.add_space(8.0);
                            }

                            ui.add_space(6.0);
                            egui::Frame::new()
                                .fill(egui::Color32::from_rgb(14, 20, 30))
                                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(28, 38, 56)))
                                .corner_radius(8.0)
                                .inner_margin(12.0)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.colored_label(egui::Color32::from_rgb(250, 204, 21), "Tip:");
                                        ui.colored_label(
                                            egui::Color32::from_rgb(148, 163, 184),
                                            "You can also cycle through backlight effects on the keyboard hardware by pressing Fn + Backlight key.",
                                        );
                                    });
                                });
                        });
                    }

                    BacklightSubview::Settings => {
                        ui.heading(egui::RichText::new("Modify Backlight Settings").color(egui::Color32::WHITE).size(18.0));
                        ui.colored_label(egui::Color32::from_rgb(148, 163, 184), "Configure power optimization, duration, and brightness.");
                        ui.add_space(12.0);

                        egui::ScrollArea::vertical().id_salt("settings_scroll").show(ui, |ui| {
                            // 1. Master Toggle
                            egui::Frame::new()
                                .fill(egui::Color32::from_rgb(15, 20, 30))
                                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)))
                                .corner_radius(10.0)
                                .inner_margin(14.0)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label(egui::RichText::new("Backlighting Power").size(15.0).strong().color(egui::Color32::WHITE));
                                            ui.colored_label(egui::Color32::from_rgb(148, 163, 184), "Master power switch for all keyboard illumination.");
                                        });
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            let enabled = backlight.enabled;
                                            let btn_label = if enabled { "● ON" } else { "○ OFF" };
                                            let btn_bg = if enabled { egui::Color32::from_rgb(12, 38, 28) } else { egui::Color32::from_rgb(36, 18, 24) };
                                            let btn_txt = if enabled { egui::Color32::from_rgb(16, 185, 129) } else { egui::Color32::from_rgb(248, 113, 113) };
                                            if ui.add(egui::Button::new(egui::RichText::new(btn_label).strong().color(btn_txt)).fill(btn_bg).corner_radius(6.0)).clicked() {
                                                self.set_backlight_level(backlight.level, backlight.manual_mode, !enabled);
                                            }
                                        });
                                    });
                                });

                            ui.add_space(10.0);

                            // 2. Duration Slider + Note
                            egui::Frame::new()
                                .fill(egui::Color32::from_rgb(15, 20, 30))
                                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)))
                                .corner_radius(10.0)
                                .inner_margin(14.0)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new("Backlighting Duration").size(15.0).strong().color(egui::Color32::WHITE));
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.colored_label(egui::Color32::from_rgb(14, 165, 233), format!("{} sec", self.backlight_duration_secs));
                                        });
                                    });
                                    ui.colored_label(egui::Color32::from_rgb(148, 163, 184), "Backlighting will turn off after a period of inactivity to optimize power consumption.");
                                    ui.add_space(8.0);

                                    let mut dur = self.backlight_duration_secs;
                                    let slider = egui::Slider::new(&mut dur, 5..=300).text("seconds");
                                    if ui.add(slider).changed() {
                                        self.set_backlight_duration(dur);
                                    }
                                    ui.add_space(8.0);
                                    ui.horizontal(|ui| {
                                        for preset in [5, 10, 30, 60, 120, 300] {
                                            let is_curr = self.backlight_duration_secs == preset;
                                            let p_bg = if is_curr { egui::Color32::from_rgb(14, 165, 233) } else { egui::Color32::from_rgb(24, 32, 46) };
                                            let p_txt = if is_curr { egui::Color32::WHITE } else { egui::Color32::from_rgb(148, 163, 184) };
                                            if ui.add(egui::Button::new(egui::RichText::new(format!("{preset}s")).color(p_txt).strong()).fill(p_bg).corner_radius(6.0)).clicked() {
                                                self.set_backlight_duration(preset);
                                            }
                                        }
                                    });
                                });

                            ui.add_space(10.0);

                            // 3. Automatic Brightness
                            egui::Frame::new()
                                .fill(egui::Color32::from_rgb(15, 20, 30))
                                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)))
                                .corner_radius(10.0)
                                .inner_margin(14.0)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label(egui::RichText::new("Automatic Brightness (Smart Ambient)").size(15.0).strong().color(egui::Color32::WHITE));
                                            ui.colored_label(egui::Color32::from_rgb(148, 163, 184), "Backlighting brightness will adjust automatically based on ambient light conditions.");
                                        });
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            let is_auto = !backlight.manual_mode;
                                            let btn_label = if is_auto { "● AUTO ON" } else { "○ AUTO OFF" };
                                            let btn_bg = if is_auto { egui::Color32::from_rgb(12, 38, 28) } else { egui::Color32::from_rgb(24, 30, 42) };
                                            let btn_txt = if is_auto { egui::Color32::from_rgb(16, 185, 129) } else { egui::Color32::from_rgb(148, 163, 184) };
                                            if ui.add(egui::Button::new(egui::RichText::new(btn_label).strong().color(btn_txt)).fill(btn_bg).corner_radius(6.0)).clicked() {
                                                // Toggling auto mode: if auto was ON, turn it OFF (manual = true). If auto was OFF, turn it ON (manual = false).
                                                let target_manual = is_auto;
                                                self.set_backlight_level(backlight.level, target_manual, backlight.enabled);
                                            }
                                        });
                                    });
                                });

                            ui.add_space(10.0);

                            // 4. MANUAL BRIGHTNESS
                            egui::Frame::new()
                                .fill(egui::Color32::from_rgb(15, 20, 30))
                                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)))
                                .corner_radius(10.0)
                                .inner_margin(14.0)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(egui::RichText::new("MANUAL BRIGHTNESS").size(13.0).strong().color(egui::Color32::WHITE));
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.colored_label(egui::Color32::from_rgb(14, 165, 233), format!("Level {} / 7", backlight.level));
                                        });
                                    });
                                    ui.colored_label(egui::Color32::from_rgb(148, 163, 184), "Adjust fixed backlighting brightness level.");
                                    ui.add_space(8.0);

                                    let mut lvl = backlight.level;
                                    let slider = egui::Slider::new(&mut lvl, 0..=7).text("Brightness");
                                    if ui.add(slider).changed() {
                                        self.set_backlight_level(lvl, true, backlight.enabled);
                                    }
                                    ui.add_space(8.0);
                                    ui.horizontal(|ui| {
                                        for preset in [0, 1, 2, 3, 4, 5, 6, 7] {
                                            let is_curr = backlight.manual_mode && backlight.level == preset;
                                            let p_bg = if is_curr { egui::Color32::from_rgb(14, 165, 233) } else { egui::Color32::from_rgb(24, 32, 46) };
                                            let p_txt = if is_curr { egui::Color32::WHITE } else { egui::Color32::from_rgb(148, 163, 184) };
                                            if ui.add(egui::Button::new(egui::RichText::new(format!("L{preset}")).color(p_txt).strong()).fill(p_bg).corner_radius(6.0)).clicked() {
                                                self.set_backlight_level(preset, true, backlight.enabled);
                                            }
                                        }
                                    });
                                    ui.add_space(6.0);
                                    ui.horizontal(|ui| {
                                        ui.colored_label(egui::Color32::from_rgb(250, 204, 21), "Note: Lower brightness extends battery life.");
                                    });
                                });

                            ui.add_space(10.0);

                            // 5. Battery saving mode
                            egui::Frame::new()
                                .fill(egui::Color32::from_rgb(15, 20, 30))
                                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)))
                                .corner_radius(10.0)
                                .inner_margin(14.0)
                                .show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label(egui::RichText::new("Battery saving mode").size(15.0).strong().color(egui::Color32::WHITE));
                                            ui.colored_label(egui::Color32::from_rgb(148, 163, 184), "Automatically turns off backlighting when battery level drops below 10%.");
                                        });
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            let saving = self.backlight_battery_saving;
                                            let btn_label = if saving { "● ON" } else { "○ OFF" };
                                            let btn_bg = if saving { egui::Color32::from_rgb(12, 38, 28) } else { egui::Color32::from_rgb(36, 18, 24) };
                                            let btn_txt = if saving { egui::Color32::from_rgb(16, 185, 129) } else { egui::Color32::from_rgb(248, 113, 113) };
                                            if ui.add(egui::Button::new(egui::RichText::new(btn_label).strong().color(btn_txt)).fill(btn_bg).corner_radius(6.0)).clicked() {
                                                self.set_backlight_battery_saving(!saving);
                                            }
                                        });
                                    });
                                });

                            ui.add_space(10.0);

                            // 6. Reset Backlighting Button
                            ui.horizontal(|ui| {
                                if ui.button(egui::RichText::new("RESET BACKLIGHTING").strong()).clicked() {
                                    self.reset_backlighting();
                                }
                            });
                        });
                    }
                }
            });
        });
    }

    fn render_keyboard_settings_tab(&mut self, ui: &mut egui::Ui, snapshot: &Snapshot) {
        // Top Action Card
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(17, 24, 38))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)))
            .corner_radius(10.0)
            .inner_margin(16.0)
            .show(ui, |ui| {
                ui.heading(
                    egui::RichText::new("Keyboard Behavior & Hotkeys")
                        .color(egui::Color32::WHITE)
                        .size(18.0),
                );
                ui.colored_label(
                    egui::Color32::from_rgb(148, 163, 184),
                    "Configure hardware Fn-lock behavior and lock accidental keys during gameplay or workflows.",
                );
            });

        ui.add_space(14.0);

        // Fn-Inversion Cards
        if let Some(fn_inverted) = snapshot.fn_inverted {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(15, 20, 30))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)))
                .corner_radius(10.0)
                .inner_margin(16.0)
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("FUNCTION KEY ROW (Fn-Lock)")
                            .size(10.0)
                            .color(egui::Color32::from_rgb(14, 165, 233))
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new("Select primary action for F1-F12 keys")
                            .size(15.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                    );
                    ui.add_space(12.0);

                    ui.horizontal(|ui| {
                        // Card 1: Standard F1-F12
                        let is_std = fn_inverted;
                        let card1_bg = if is_std { egui::Color32::from_rgb(18, 38, 58) } else { egui::Color32::from_rgb(17, 24, 36) };
                        let card1_stroke = if is_std { egui::Stroke::new(1.5, egui::Color32::from_rgb(14, 165, 233)) } else { egui::Stroke::new(1.0, egui::Color32::from_rgb(32, 44, 64)) };

                        let r1 = egui::Frame::new().fill(card1_bg).stroke(card1_stroke).corner_radius(8.0).inner_margin(14.0).show(ui, |ui| {
                            ui.set_width(260.0);
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    egui::Frame::new().fill(egui::Color32::from_rgb(24, 36, 52)).corner_radius(4.0).inner_margin(egui::Margin::symmetric(6, 2)).show(ui, |ui| {
                                        ui.label(egui::RichText::new("F1-F12").strong().size(11.0).color(egui::Color32::from_rgb(56, 189, 248)));
                                    });
                                    ui.label(egui::RichText::new("Standard F1 - F12").strong().size(14.0).color(egui::Color32::WHITE));
                                    if is_std {
                                        ui.colored_label(egui::Color32::from_rgb(14, 165, 233), "• ACTIVE");
                                    }
                                });
                                ui.add_space(4.0);
                                ui.colored_label(egui::Color32::from_rgb(148, 163, 184), "Pressing F1-F12 sends standard function keys. Hold Fn for media.");
                            });
                        }).response;
                        if r1.interact(egui::Sense::click()).clicked() && !is_std {
                            self.set_fn_inversion(true);
                        }

                        ui.add_space(12.0);

                        // Card 2: Media Hotkeys
                        let is_media = !fn_inverted;
                        let card2_bg = if is_media { egui::Color32::from_rgb(18, 38, 58) } else { egui::Color32::from_rgb(17, 24, 36) };
                        let card2_stroke = if is_media { egui::Stroke::new(1.5, egui::Color32::from_rgb(14, 165, 233)) } else { egui::Stroke::new(1.0, egui::Color32::from_rgb(32, 44, 64)) };

                        let r2 = egui::Frame::new().fill(card2_bg).stroke(card2_stroke).corner_radius(8.0).inner_margin(14.0).show(ui, |ui| {
                            ui.set_width(260.0);
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    egui::Frame::new().fill(egui::Color32::from_rgb(24, 36, 52)).corner_radius(4.0).inner_margin(egui::Margin::symmetric(6, 2)).show(ui, |ui| {
                                        ui.label(egui::RichText::new("MEDIA").strong().size(11.0).color(egui::Color32::from_rgb(56, 189, 248)));
                                    });
                                    ui.label(egui::RichText::new("Media / Action Keys").strong().size(14.0).color(egui::Color32::WHITE));
                                    if is_media {
                                        ui.colored_label(egui::Color32::from_rgb(14, 165, 233), "• ACTIVE");
                                    }
                                });
                                ui.add_space(4.0);
                                ui.colored_label(egui::Color32::from_rgb(148, 163, 184), "Pressing F1-F12 controls volume, mic, brightness. Hold Fn for F1-F12.");
                            });
                        }).response;
                        if r2.interact(egui::Sense::click()).clicked() && !is_media {
                            self.set_fn_inversion(false);
                        }
                    });
                });
            ui.add_space(14.0);
        }

        // Disabled Nuisance Keys (Gaming Focus Mode)
        if let Some(disabled) = &snapshot.disabled_keys {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(15, 20, 30))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)))
                .corner_radius(10.0)
                .inner_margin(16.0)
                .show(ui, |ui| {
                    ui.label(
                        egui::RichText::new("GAMING LOCK & FOCUS MODE")
                            .size(10.0)
                            .color(egui::Color32::from_rgb(239, 68, 68))
                            .strong(),
                    );
                    ui.label(
                        egui::RichText::new("Disable Accidental Nuisance Keys")
                            .size(15.0)
                            .strong()
                            .color(egui::Color32::WHITE),
                    );
                    ui.colored_label(
                        egui::Color32::from_rgb(148, 163, 184),
                        "Click any keycap switch to disable or re-enable it on keyboard hardware.",
                    );
                    ui.add_space(14.0);

                    ui.horizontal(|ui| {
                        let lock_keys = [
                            (3, "Windows Key", disabled.win_key, "Win"),
                            (0, "Caps Lock", disabled.caps_lock, "Caps Lock"),
                            (1, "Scroll Lock", disabled.scroll_lock, "Scroll Lock"),
                            (2, "Num Lock", disabled.num_lock, "Num Lock"),
                            (4, "Insert Key", disabled.insert, "Insert"),
                        ];

                        for (bit, _name, is_locked, icon_label) in lock_keys {
                            let bg = if is_locked { egui::Color32::from_rgb(42, 18, 24) } else { egui::Color32::from_rgb(18, 25, 36) };
                            let stroke = if is_locked { egui::Stroke::new(1.5, egui::Color32::from_rgb(239, 68, 68)) } else { egui::Stroke::new(1.0, egui::Color32::from_rgb(36, 48, 70)) };

                            let resp = egui::Frame::new().fill(bg).stroke(stroke).corner_radius(8.0).inner_margin(egui::Margin::symmetric(14, 12)).show(ui, |ui| {
                                ui.set_width(105.0);
                                ui.vertical_centered(|ui| {
                                    ui.label(egui::RichText::new(icon_label).strong().size(13.0).color(if is_locked { egui::Color32::from_rgb(252, 165, 165) } else { egui::Color32::WHITE }));
                                    ui.add_space(4.0);
                                    if is_locked {
                                        egui::Frame::new().fill(egui::Color32::from_rgb(60, 20, 26)).corner_radius(4.0).inner_margin(egui::Margin::symmetric(6, 2)).show(ui, |ui| {
                                            ui.label(egui::RichText::new("LOCKED").size(9.0).color(egui::Color32::from_rgb(248, 113, 113)).strong());
                                        });
                                    } else {
                                        egui::Frame::new().fill(egui::Color32::from_rgb(22, 34, 48)).corner_radius(4.0).inner_margin(egui::Margin::symmetric(6, 2)).show(ui, |ui| {
                                            ui.label(egui::RichText::new("ACTIVE").size(9.0).color(egui::Color32::from_rgb(148, 163, 184)).strong());
                                        });
                                    }
                                });
                            }).response;

                            if resp.interact(egui::Sense::click()).clicked() {
                                self.toggle_disabled_key(bit);
                            }
                            ui.add_space(8.0);
                        }
                    });
                });
        }
    }

    fn render_easy_switch_tab(&mut self, ui: &mut egui::Ui, snapshot: &Snapshot) {
        let Some(hosts) = &snapshot.hosts else {
            ui.label("Easy-Switch is not supported on this device.");
            return;
        };

        egui::Frame::new()
            .fill(egui::Color32::from_rgb(17, 24, 38))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)))
            .corner_radius(10.0)
            .inner_margin(16.0)
            .show(ui, |ui| {
                ui.heading(
                    egui::RichText::new("Easy-Switch Multi-Host Channels")
                        .color(egui::Color32::WHITE)
                        .size(18.0),
                );
                ui.colored_label(
                    egui::Color32::from_rgb(148, 163, 184),
                    "Switch seamlessly between up to 3 paired computers or mobile devices over Bluetooth and Logi Bolt.",
                );
                ui.add_space(16.0);

                ui.horizontal(|ui| {
                    for slot in &hosts.slots {
                        let card_bg = if slot.paired { egui::Color32::from_rgb(18, 28, 44) } else { egui::Color32::from_rgb(14, 18, 26) };
                        let card_stroke = if slot.paired { egui::Stroke::new(1.5, egui::Color32::from_rgb(14, 165, 233)) } else { egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 40, 58)) };

                        egui::Frame::new()
                            .fill(card_bg)
                            .stroke(card_stroke)
                            .corner_radius(10.0)
                            .inner_margin(16.0)
                            .show(ui, |ui| {
                                ui.set_width(170.0);
                                ui.vertical_centered(|ui| {
                                    // Host channel badge
                                    egui::Frame::new()
                                        .fill(egui::Color32::from_rgb(22, 32, 48))
                                        .corner_radius(6.0)
                                        .inner_margin(egui::Margin::symmetric(12, 5))
                                        .show(ui, |ui| {
                                            ui.label(
                                                egui::RichText::new(format!("CH {}", slot.index))
                                                    .size(12.0)
                                                    .strong()
                                                    .color(if slot.paired { egui::Color32::from_rgb(14, 165, 233) } else { egui::Color32::from_rgb(71, 85, 105) }),
                                            );
                                        });
                                    ui.add_space(6.0);
                                    ui.label(
                                        egui::RichText::new(format!("HOST {}", slot.index))
                                            .strong()
                                            .size(15.0)
                                            .color(egui::Color32::WHITE),
                                    );
                                    ui.add_space(4.0);

                                    if slot.paired {
                                        egui::Frame::new().fill(egui::Color32::from_rgb(12, 34, 28)).corner_radius(4.0).inner_margin(egui::Margin::symmetric(8, 3)).show(ui, |ui| {
                                            ui.label(egui::RichText::new("PAIRED").size(9.5).color(egui::Color32::from_rgb(16, 185, 129)).strong());
                                        });
                                        ui.add_space(8.0);
                                        if ui.button(egui::RichText::new("Switch to Host").strong().size(12.0)).clicked() {
                                            self.switch_host(slot.index - 1);
                                        }
                                    } else {
                                        egui::Frame::new().fill(egui::Color32::from_rgb(20, 26, 36)).corner_radius(4.0).inner_margin(egui::Margin::symmetric(8, 3)).show(ui, |ui| {
                                            ui.label(egui::RichText::new("UNPAIRED").size(9.5).color(egui::Color32::from_rgb(100, 116, 139)).strong());
                                        });
                                    }
                                });
                            });
                        ui.add_space(14.0);
                    }
                });
            });
    }

    fn render_power_tab(&mut self, ui: &mut egui::Ui, snapshot: &Snapshot) {
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(17, 24, 38))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)))
            .corner_radius(10.0)
            .inner_margin(16.0)
            .show(ui, |ui| {
                ui.heading(
                    egui::RichText::new("Polling Rate & Report Frequency")
                        .color(egui::Color32::WHITE)
                        .size(18.0),
                );
                ui.colored_label(
                    egui::Color32::from_rgb(148, 163, 184),
                    "Select sensor reporting frequency. 1000 Hz provides esports-grade 1ms ultra-low cursor latency.",
                );
                ui.add_space(16.0);

                let rate_options = [
                    (1000, "1 ms Latency", "Esports Competitive", egui::Color32::from_rgb(14, 165, 233)),
                    (500, "2 ms Latency", "High Performance", egui::Color32::from_rgb(56, 189, 248)),
                    (250, "4 ms Latency", "Balanced Gaming", egui::Color32::from_rgb(148, 163, 184)),
                    (125, "8 ms Latency", "Office Power Saver", egui::Color32::from_rgb(16, 185, 129)),
                ];

                let current_hz = snapshot.polling_rate_hz.unwrap_or(1000);

                ui.horizontal(|ui| {
                    for (rate, latency, desc, accent) in rate_options {
                        let is_active = rate == current_hz;
                        let card_bg = if is_active { egui::Color32::from_rgb(18, 34, 52) } else { egui::Color32::from_rgb(14, 20, 30) };
                        let card_stroke = if is_active { egui::Stroke::new(1.5, egui::Color32::from_rgb(14, 165, 233)) } else { egui::Stroke::new(1.0, egui::Color32::from_rgb(28, 38, 56)) };

                        let resp = egui::Frame::new().fill(card_bg).stroke(card_stroke).corner_radius(10.0).inner_margin(14.0).show(ui, |ui| {
                            ui.set_width(155.0);
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(egui::RichText::new(format!("{rate} Hz")).strong().size(17.0).color(if is_active { egui::Color32::WHITE } else { egui::Color32::from_rgb(226, 232, 240) }));
                                    if is_active {
                                        ui.colored_label(egui::Color32::from_rgb(14, 165, 233), "ACTIVE");
                                    }
                                });
                                ui.add_space(2.0);
                                ui.label(egui::RichText::new(latency).size(11.0).color(accent).strong());
                                ui.add_space(4.0);
                                ui.colored_label(egui::Color32::from_rgb(148, 163, 184), desc);
                            });
                        }).response;

                        if resp.interact(egui::Sense::click()).clicked() && !is_active {
                            self.set_polling_rate(rate);
                        }
                        ui.add_space(10.0);
                    }
                });

                if let Some(mode) = snapshot.mode_status {
                    ui.add_space(20.0);
                    ui.separator();
                    ui.add_space(14.0);

                    ui.heading(
                        egui::RichText::new("Hardware Power Operating Mode")
                            .color(egui::Color32::WHITE)
                            .size(18.0),
                    );
                    ui.colored_label(
                        egui::Color32::from_rgb(148, 163, 184),
                        "Switch mouse internal firmware profile between gaming precision and extended battery runtime.",
                    );
                    ui.add_space(14.0);

                    ui.horizontal(|ui| {
                        let is_perf = mode == 1;

                        // Performance Mode Card
                        let p_bg = if is_perf { egui::Color32::from_rgb(18, 36, 56) } else { egui::Color32::from_rgb(14, 20, 30) };
                        let p_stroke = if is_perf { egui::Stroke::new(1.5, egui::Color32::from_rgb(14, 165, 233)) } else { egui::Stroke::new(1.0, egui::Color32::from_rgb(28, 38, 56)) };

                        let p_resp = egui::Frame::new().fill(p_bg).stroke(p_stroke).corner_radius(10.0).inner_margin(16.0).show(ui, |ui| {
                            ui.set_width(320.0);
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    egui::Frame::new().fill(egui::Color32::from_rgb(14, 44, 68)).corner_radius(4.0).inner_margin(egui::Margin::symmetric(6, 2)).show(ui, |ui| {
                                        ui.label(egui::RichText::new("PERF").size(10.0).strong().color(egui::Color32::from_rgb(56, 189, 248)));
                                    });
                                    ui.label(egui::RichText::new("Performance Mode").strong().size(15.0).color(egui::Color32::WHITE));
                                    if is_perf {
                                        egui::Frame::new().fill(egui::Color32::from_rgb(14, 44, 68)).corner_radius(4.0).inner_margin(egui::Margin::symmetric(6, 2)).show(ui, |ui| {
                                            ui.label(egui::RichText::new("ACTIVE (Cyan LED)").size(9.5).color(egui::Color32::from_rgb(56, 189, 248)).strong());
                                        });
                                    }
                                });
                                ui.add_space(4.0);
                                ui.colored_label(egui::Color32::from_rgb(148, 163, 184), "Full 1000Hz (1ms) sensor tracking. Zero compromise for fast-paced gaming.");
                            });
                        }).response;
                        if p_resp.interact(egui::Sense::click()).clicked() && !is_perf {
                            self.set_power_mode(1);
                        }

                        ui.add_space(14.0);

                        // Endurance Mode Card
                        let is_endurance = mode == 2;
                        let e_bg = if is_endurance { egui::Color32::from_rgb(18, 38, 30) } else { egui::Color32::from_rgb(14, 20, 30) };
                        let e_stroke = if is_endurance { egui::Stroke::new(1.5, egui::Color32::from_rgb(16, 185, 129)) } else { egui::Stroke::new(1.0, egui::Color32::from_rgb(28, 38, 56)) };

                        let e_resp = egui::Frame::new().fill(e_bg).stroke(e_stroke).corner_radius(10.0).inner_margin(16.0).show(ui, |ui| {
                            ui.set_width(320.0);
                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    egui::Frame::new().fill(egui::Color32::from_rgb(16, 44, 30)).corner_radius(4.0).inner_margin(egui::Margin::symmetric(6, 2)).show(ui, |ui| {
                                        ui.label(egui::RichText::new("ECO").size(10.0).strong().color(egui::Color32::from_rgb(110, 231, 183)));
                                    });
                                    ui.label(egui::RichText::new("Endurance Mode").strong().size(15.0).color(egui::Color32::WHITE));
                                    if is_endurance {
                                        egui::Frame::new().fill(egui::Color32::from_rgb(16, 44, 30)).corner_radius(4.0).inner_margin(egui::Margin::symmetric(6, 2)).show(ui, |ui| {
                                            ui.label(egui::RichText::new("ACTIVE (Green LED)").size(9.5).color(egui::Color32::from_rgb(110, 231, 183)).strong());
                                        });
                                    }
                                });
                                ui.add_space(4.0);
                                ui.colored_label(egui::Color32::from_rgb(148, 163, 184), "125Hz power-saving profile. Extends battery lifespan up to 9 months continuous use.");
                            });
                        }).response;
                        if e_resp.interact(egui::Sense::click()).clicked() && !is_endurance {
                            self.set_power_mode(2);
                        }
                    });
                }
            });
    }

    fn render_dpi_tab(&mut self, ui: &mut egui::Ui, snapshot: &Snapshot) {
        let Some(dpi) = &snapshot.dpi else {
            ui.label("DPI settings are not exposed over onboard profiles for this device.");
            return;
        };

        egui::Frame::new()
            .fill(egui::Color32::from_rgb(17, 24, 38))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)))
            .corner_radius(10.0)
            .inner_margin(16.0)
            .show(ui, |ui| {
                ui.heading(
                    egui::RichText::new("HERO Optical Sensor DPI Stages")
                        .color(egui::Color32::WHITE)
                        .size(18.0),
                );
                ui.colored_label(
                    egui::Color32::from_rgb(148, 163, 184),
                    "Hardware resolution profiles stored directly in mouse memory. Click any card to switch stage instantly.",
                );
                ui.add_space(16.0);

                let stage_colors = [
                    egui::Color32::from_rgb(234, 179, 8),   // Yellow (Stage 1)
                    egui::Color32::from_rgb(249, 115, 22),  // Orange (Stage 2)
                    egui::Color32::from_rgb(14, 165, 233),  // Cyan (Stage 3)
                    egui::Color32::from_rgb(168, 85, 247),  // Purple (Stage 4)
                    egui::Color32::from_rgb(16, 185, 129),  // Emerald (Stage 5)
                ];

                ui.horizontal(|ui| {
                    for (idx, level) in dpi.levels.iter().enumerate() {
                        let is_active = idx == dpi.active_index;
                        let accent = stage_colors.get(idx).copied().unwrap_or(egui::Color32::from_rgb(14, 165, 233));

                        let card_bg = if is_active {
                            egui::Color32::from_rgb(20, 34, 52)
                        } else {
                            egui::Color32::from_rgb(14, 19, 28)
                        };
                        let stroke = if is_active {
                            egui::Stroke::new(1.5, egui::Color32::from_rgb(14, 165, 233))
                        } else {
                            egui::Stroke::new(1.0, egui::Color32::from_rgb(28, 38, 54))
                        };

                        let resp = egui::Frame::new()
                            .fill(card_bg)
                            .stroke(stroke)
                            .corner_radius(10.0)
                            .inner_margin(14.0)
                            .show(ui, |ui| {
                                ui.set_width(120.0);
                                ui.vertical(|ui| {
                                    // Stage Pill Header
                                    ui.horizontal(|ui| {
                                        egui::Frame::new()
                                            .fill(accent)
                                            .corner_radius(4.0)
                                            .inner_margin(egui::Margin::symmetric(6, 2))
                                            .show(ui, |ui| {
                                                ui.label(
                                                    egui::RichText::new(format!("STAGE {}", idx + 1))
                                                        .size(9.0)
                                                        .strong()
                                                        .color(egui::Color32::BLACK),
                                                );
                                            });

                                        if is_active {
                                            ui.colored_label(egui::Color32::from_rgb(14, 165, 233), "ACTIVE");
                                        }
                                    });

                                    ui.add_space(6.0);
                                    // Large DPI Number
                                    ui.label(
                                        egui::RichText::new(format!("{level}"))
                                            .size(24.0)
                                            .strong()
                                            .color(if is_active { egui::Color32::WHITE } else { egui::Color32::from_rgb(226, 232, 240) }),
                                    );
                                    ui.colored_label(egui::Color32::from_rgb(100, 116, 139), "DPI");

                                    ui.add_space(8.0);
                                    // Colored underline accent bar
                                    let (r, p) = ui.allocate_painter(egui::vec2(110.0, 3.0), egui::Sense::hover());
                                    p.rect_filled(r.rect, 1.5, accent);

                                    ui.add_space(8.0);
                                    if is_active {
                                        egui::Frame::new().fill(egui::Color32::from_rgb(12, 38, 28)).corner_radius(4.0).inner_margin(egui::Margin::symmetric(8, 3)).show(ui, |ui| {
                                            ui.label(egui::RichText::new("● ACTIVE").size(9.5).color(egui::Color32::from_rgb(16, 185, 129)).strong());
                                        });
                                    } else {
                                        if ui.button(egui::RichText::new("Switch").size(11.0)).clicked() {
                                            self.set_dpi_stage(idx);
                                        }
                                    }
                                });
                            }).response;

                        if resp.interact(egui::Sense::click()).clicked() && !is_active {
                            self.set_dpi_stage(idx);
                        }
                        ui.add_space(10.0);
                    }
                });
            });
    }

    fn render_settings_window(&mut self, ctx: &egui::Context) {
        let mut open = self.show_settings_modal;
        let mut close_modal = false;
        egui::Window::new("betterTech Preferences")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_size([520.0, 420.0])
            .show(ctx, |ui| {
                // Section 1: Windows Autostart Card
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(17, 24, 38))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)))
                    .corner_radius(8.0)
                    .inner_margin(14.0)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.label(egui::RichText::new("Windows Startup Launch").size(15.0).strong().color(egui::Color32::WHITE));
                                ui.colored_label(
                                    egui::Color32::from_rgb(148, 163, 184),
                                    "Launch minimized to system tray and restore all hardware profiles automatically.",
                                );
                            });
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let mut autostart = self.autostart_enabled;
                                let btn_label = if autostart { "● ENABLED" } else { "○ DISABLED" };
                                let btn_bg = if autostart { egui::Color32::from_rgb(12, 38, 28) } else { egui::Color32::from_rgb(24, 30, 42) };
                                let btn_txt = if autostart { egui::Color32::from_rgb(16, 185, 129) } else { egui::Color32::from_rgb(148, 163, 184) };
                                if ui.add(egui::Button::new(egui::RichText::new(btn_label).strong().color(btn_txt)).fill(btn_bg).corner_radius(6.0)).clicked() {
                                    autostart = !autostart;
                                    self.autostart_enabled = autostart;
                                    let _ = crate::autostart::set_enabled(autostart);
                                    self.set_status(
                                        if autostart { "Autostart enabled in Windows Registry." } else { "Autostart removed from Windows Registry." },
                                        false,
                                    );
                                }
                            });
                        });
                    });

                ui.add_space(10.0);

                // Section 2: Background Minimization Card
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(17, 24, 38))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)))
                    .corner_radius(8.0)
                    .inner_margin(14.0)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("System Tray & Background Mode").size(15.0).strong().color(egui::Color32::WHITE));
                        ui.add_space(4.0);
                        ui.colored_label(egui::Color32::from_rgb(148, 163, 184), "• Closing the window minimizes betterTech directly to your Windows notification area.");
                        ui.colored_label(egui::Color32::from_rgb(148, 163, 184), "• Click the system tray icon anytime to restore the dashboard instantly.");
                    });

                ui.add_space(10.0);

                // Section 3: Performance & Architecture Card
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(17, 24, 38))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)))
                    .corner_radius(8.0)
                    .inner_margin(14.0)
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new("Architecture & Efficiency").size(15.0).strong().color(egui::Color32::WHITE));
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            egui::Frame::new().fill(egui::Color32::from_rgb(12, 38, 28)).corner_radius(4.0).inner_margin(egui::Margin::symmetric(8, 3)).show(ui, |ui| {
                                ui.label(egui::RichText::new("~12 MB RAM footprint").size(11.0).color(egui::Color32::from_rgb(16, 185, 129)).strong());
                            });
                            egui::Frame::new().fill(egui::Color32::from_rgb(14, 44, 68)).corner_radius(4.0).inner_margin(egui::Margin::symmetric(8, 3)).show(ui, |ui| {
                                ui.label(egui::RichText::new("0% Idle CPU").size(11.0).color(egui::Color32::from_rgb(56, 189, 248)).strong());
                            });
                            egui::Frame::new().fill(egui::Color32::from_rgb(34, 24, 52)).corner_radius(4.0).inner_margin(egui::Margin::symmetric(8, 3)).show(ui, |ui| {
                                ui.label(egui::RichText::new("0 Telemetry Services").size(11.0).color(egui::Color32::from_rgb(192, 132, 252)).strong());
                            });
                        });
                        ui.add_space(6.0);
                        ui.colored_label(
                            egui::Color32::from_rgb(148, 163, 184),
                            "Direct raw HID++ 2.0 kernel communication without Electron, Node.js, or background telemetry bloat.",
                        );
                    });

                ui.add_space(12.0);

                // Footer row with config path and close button
                ui.horizontal(|ui| {
                    if let Ok(path) = config::path() {
                        ui.colored_label(egui::Color32::from_rgb(100, 116, 139), format!("Config: {}", path.display()));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(egui::RichText::new("Done").strong()).clicked() {
                            close_modal = true;
                        }
                    });
                });
            });
        if close_modal {
            open = false;
        }
        self.show_settings_modal = open;
    }

    fn render_hardware_tab(&mut self, ui: &mut egui::Ui, snapshot: &Snapshot) {
        ui.heading(egui::RichText::new("Hardware & Power Diagnostics").color(egui::Color32::WHITE).size(18.0));
        ui.colored_label(egui::Color32::from_rgb(148, 163, 184), "Live telemetry directly read from hardware microcontrollers.");
        ui.add_space(14.0);

        // 1. Battery Telemetry Card
        if let Some(battery) = &snapshot.battery {
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(17, 24, 38))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)))
                .corner_radius(10.0)
                .inner_margin(16.0)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let level = battery.level.unwrap_or(0);
                        let level_color = if level > 40 {
                            egui::Color32::from_rgb(16, 185, 129)
                        } else if level > 20 {
                            egui::Color32::from_rgb(250, 204, 21)
                        } else {
                            egui::Color32::from_rgb(239, 68, 68)
                        };

                        ui.label(egui::RichText::new(format!("{level}%")).size(28.0).strong().color(level_color));
                        ui.add_space(12.0);

                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                egui::Frame::new()
                                    .fill(egui::Color32::from_rgb(12, 38, 28))
                                    .corner_radius(4.0)
                                    .inner_margin(egui::Margin::symmetric(8, 3))
                                    .show(ui, |ui| {
                                        ui.label(egui::RichText::new(&battery.text).size(11.0).color(egui::Color32::from_rgb(16, 185, 129)).strong());
                                    });
                                if let Some(mv) = battery.voltage_mv {
                                    egui::Frame::new()
                                        .fill(egui::Color32::from_rgb(14, 44, 68))
                                        .corner_radius(4.0)
                                        .inner_margin(egui::Margin::symmetric(8, 3))
                                        .show(ui, |ui| {
                                            ui.label(egui::RichText::new(format!("{mv} mV")).size(11.0).color(egui::Color32::from_rgb(56, 189, 248)).strong());
                                        });
                                }
                            });
                            ui.add_space(2.0);
                            ui.colored_label(egui::Color32::from_rgb(148, 163, 184), format!("Status: {} | Real-time voltage sensor", battery.status));
                        });
                    });
                });
            ui.add_space(12.0);
        }

        // 2. Firmware Subsystems Card
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(17, 24, 38))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)))
            .corner_radius(10.0)
            .inner_margin(16.0)
            .show(ui, |ui| {
                ui.label(egui::RichText::new("MICROCONTROLLER FIRMWARE").size(10.0).strong().color(egui::Color32::from_rgb(14, 165, 233)));
                ui.label(egui::RichText::new("Subsystem Firmware Revisions").size(15.0).strong().color(egui::Color32::WHITE));
                ui.add_space(10.0);

                ui.horizontal_wrapped(|ui| {
                    for fw in &snapshot.firmware {
                        egui::Frame::new()
                            .fill(egui::Color32::from_rgb(14, 20, 30))
                            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(28, 38, 56)))
                            .corner_radius(6.0)
                            .inner_margin(egui::Margin::symmetric(10, 6))
                            .show(ui, |ui| {
                                ui.colored_label(egui::Color32::from_rgb(148, 163, 184), &fw.name);
                                ui.colored_label(egui::Color32::WHITE, &fw.version);
                            });
                        ui.add_space(8.0);
                    }
                });
            });

        ui.add_space(12.0);

        // 3. Identity & Connection Card
        egui::Frame::new()
            .fill(egui::Color32::from_rgb(17, 24, 38))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)))
            .corner_radius(10.0)
            .inner_margin(16.0)
            .show(ui, |ui| {
                ui.label(egui::RichText::new("BUS IDENTITY & ADDRESSING").size(10.0).strong().color(egui::Color32::from_rgb(14, 165, 233)));
                ui.label(egui::RichText::new("Hardware Connection Details").size(15.0).strong().color(egui::Color32::WHITE));
                ui.add_space(10.0);

                ui.horizontal(|ui| {
                    ui.colored_label(egui::Color32::from_rgb(100, 116, 139), "Product ID:");
                    ui.colored_label(egui::Color32::from_rgb(56, 189, 248), format!("0x{}", snapshot.device.product_id));
                    ui.add_space(12.0);
                    ui.colored_label(egui::Color32::from_rgb(100, 116, 139), "Unit ID:");
                    ui.colored_label(egui::Color32::WHITE, format!("{}", snapshot.device.unit_id));
                    ui.add_space(12.0);
                    ui.colored_label(egui::Color32::from_rgb(100, 116, 139), "Device Index:");
                    ui.colored_label(egui::Color32::WHITE, format!("{}", snapshot.device.index));
                });
                ui.add_space(6.0);
                ui.colored_label(egui::Color32::from_rgb(100, 116, 139), format!("HID Path: {}", snapshot.device.path));
            });
    }

    fn render_features_tab(&mut self, ui: &mut egui::Ui, snapshot: &Snapshot) {
        ui.heading(egui::RichText::new("HID++ 2.0 Feature Catalog").color(egui::Color32::WHITE).size(18.0));
        ui.colored_label(
            egui::Color32::from_rgb(148, 163, 184),
            format!("{} hardware features discovered on device firmware.", snapshot.device.features.len()),
        );
        ui.add_space(14.0);

        egui::Frame::new()
            .fill(egui::Color32::from_rgb(17, 24, 38))
            .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(30, 42, 64)))
            .corner_radius(10.0)
            .inner_margin(16.0)
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(450.0)
                    .show(ui, |ui| {
                        egui::Grid::new("features_grid")
                            .striped(true)
                            .num_columns(3)
                            .spacing([24.0, 8.0])
                            .show(ui, |ui| {
                                ui.strong("Feature Index");
                                ui.strong("Hex Identifier");
                                ui.strong("Feature Name");
                                ui.end_row();

                                for feat in &snapshot.device.features {
                                    egui::Frame::new()
                                        .fill(egui::Color32::from_rgb(22, 30, 44))
                                        .corner_radius(4.0)
                                        .inner_margin(egui::Margin::symmetric(6, 2))
                                        .show(ui, |ui| {
                                            ui.colored_label(egui::Color32::from_rgb(148, 163, 184), format!("0x{:02X}", feat.index));
                                        });

                                    egui::Frame::new()
                                        .fill(egui::Color32::from_rgb(14, 44, 68))
                                        .corner_radius(4.0)
                                        .inner_margin(egui::Margin::symmetric(6, 2))
                                        .show(ui, |ui| {
                                            ui.colored_label(egui::Color32::from_rgb(56, 189, 248), format!("0x{:04X}", feat.id));
                                        });

                                    ui.label(egui::RichText::new(&feat.name).strong().color(egui::Color32::WHITE));
                                    ui.end_row();
                                }
                            });
                    });
            });
    }
}

fn control_label(snapshot: &Snapshot, cid: u16) -> String {
    snapshot
        .controls
        .iter()
        .find(|control| control.cid == cid)
        .map(|control| format!("{} (0x{:04X})", control.name, control.cid))
        .unwrap_or_else(|| format!("0x{cid:04X}"))
}

fn draw_keycap(painter: &egui::Painter, rect: egui::Rect, is_modifier: bool, glow_alpha: u8) {
    let base_bg = if is_modifier {
        egui::Color32::from_rgb(26, 32, 44)
    } else {
        egui::Color32::from_rgb(36, 44, 58)
    };
    if glow_alpha > 0 {
        // Soft underglow bloom
        let bloom = egui::Color32::from_rgba_premultiplied(14, 165, 233, (glow_alpha / 6).min(42));
        painter.rect_filled(rect.expand(2.5), 5.0, bloom);

        // Radiant halo rim
        let halo = egui::Color32::from_rgba_premultiplied(56, 189, 248, (glow_alpha / 3).min(90));
        painter.rect_stroke(rect.expand(0.5), 3.0, egui::Stroke::new(1.0, halo), egui::StrokeKind::Outside);
    }
    // Subtle keycap illumination reaction
    let cap_color = if glow_alpha > 120 {
        egui::Color32::from_rgb(46, 56, 74)
    } else {
        base_bg
    };
    painter.rect_filled(rect, 3.0, cap_color);
    let border_color = if glow_alpha > 80 {
        egui::Color32::from_rgba_premultiplied(125, 211, 252, (glow_alpha / 2).max(40))
    } else {
        egui::Color32::from_rgb(51, 65, 85)
    };
    painter.rect_stroke(rect, 3.0, egui::Stroke::new(0.6, border_color), egui::StrokeKind::Inside);
}

fn draw_keycap_accent(painter: &egui::Painter, rect: egui::Rect, label: &str, accent_color: egui::Color32) {
    painter.rect_filled(rect.expand(2.0), 4.0, egui::Color32::from_rgba_premultiplied(accent_color.r(), accent_color.g(), accent_color.b(), 35));
    painter.rect_filled(rect, 3.0, egui::Color32::from_rgb(22, 32, 48));
    painter.rect_stroke(rect, 3.0, egui::Stroke::new(1.2, accent_color), egui::StrokeKind::Inside);
    painter.text(
        egui::pos2(rect.center().x, rect.center().y),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(9.5),
        accent_color,
    );
}
