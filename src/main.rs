#![windows_subsystem = "windows"]

mod autostart;
mod config;
mod control_names;
mod device;
mod hidpp;
mod ui;

use anyhow::{Context, Result, anyhow, bail};
use clap::{Parser, Subcommand};
use hidapi::HidApi;

#[derive(Parser)]
#[command(
    name = "bettertech",
    version,
    about = "Lightweight Logitech HID++ control for MX Mechanical and G305"
)]
struct Cli {
    /// Start minimized to system tray
    #[arg(long, short)]
    minimized: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// List connected Logitech devices
    List {
        #[arg(long)]
        json: bool,
    },
    /// Show device details, firmware, battery, features and controls
    Info {
        #[arg(long)]
        device: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Show battery state
    Battery {
        #[arg(long)]
        device: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// List programmable controls (MX Mechanical)
    Controls {
        #[arg(long)]
        device: Option<String>,
        #[arg(long)]
        json: bool,
    },
    /// Remap a control; dry-run unless --apply is passed
    Remap {
        #[arg(long)]
        device: Option<String>,
        /// Control name or CID, e.g. "Calculator" or 0x000A
        #[arg(long)]
        from: String,
        /// Target control name or CID
        #[arg(long)]
        to: String,
        #[arg(long)]
        apply: bool,
    },
    /// Restore a control to its native action
    Reset {
        #[arg(long)]
        device: Option<String>,
        /// Control name or CID, e.g. "Calculator" or 0x000A
        #[arg(long)]
        control: String,
        #[arg(long)]
        apply: bool,
    },
    /// Show saved remappings
    Mappings {
        #[arg(long)]
        json: bool,
    },
    /// Reapply saved remappings to connected devices
    Apply,
    /// Get or set mouse polling rate in Hz (125, 250, 500, 1000)
    Rate {
        #[arg(long)]
        device: Option<String>,
        /// Polling rate in Hz (e.g. 1000, 500, 250, 125)
        #[arg(long)]
        set: Option<u16>,
    },
    /// Get or set power mode (performance or endurance)
    Mode {
        #[arg(long)]
        device: Option<String>,
        /// "performance" (1000Hz) or "endurance" (125Hz)
        #[arg(long)]
        set: Option<String>,
    },
    /// Show or set onboard DPI stages
    Dpi {
        #[arg(long)]
        device: Option<String>,
        /// Switch to active DPI stage (1-indexed: 1, 2, 3, ...)
        #[arg(long)]
        set: Option<u8>,
    },
    /// Get or set keyboard backlight (effects, brightness 0-7, timeout, auto)
    Backlight {
        #[arg(long)]
        device: Option<String>,
        /// Brightness level (0-7)
        #[arg(long)]
        set: Option<u8>,
        /// Manual brightness mode
        #[arg(long)]
        manual: bool,
        /// Automatic ambient brightness mode
        #[arg(long)]
        auto: bool,
        /// Set backlighting effect (static, contrast, breathing, waves, reaction, random)
        #[arg(long)]
        effect: Option<String>,
        /// Backlighting timeout duration in seconds (e.g. 10, 30, 60, 120, 300)
        #[arg(long)]
        duration: Option<u16>,
        /// Battery saving mode: turn off backlight when battery < 10%
        #[arg(long)]
        battery_saving: Option<bool>,
        /// Reset backlighting settings to factory defaults
        #[arg(long)]
        reset: bool,
        /// Turn off backlight
        #[arg(long)]
        off: bool,
    },
    /// Get or toggle Fn-lock (standard F1-F12 vs media)
    FnLock {
        #[arg(long)]
        device: Option<String>,
        /// Set standard F1-F12 keys as primary (true) or media (false)
        #[arg(long)]
        set: Option<bool>,
        /// Toggle the current state
        #[arg(long)]
        toggle: bool,
    },
    /// View or switch Easy-Switch host channels
    Hosts {
        #[arg(long)]
        device: Option<String>,
        /// Switch to host slot (1, 2, or 3)
        #[arg(long)]
        switch: Option<u8>,
    },
    /// View or remap mouse buttons on G305 / gaming mice (1 to 6)
    Button {
        #[arg(long)]
        device: Option<String>,
        /// Button number (1: Left, 2: Right, 3: Middle, 4: Back, 5: Forward, 6: DPI)
        #[arg(long)]
        index: Option<u8>,
        /// Action preset: left, right, middle, back, forward, dpi-cycle, dpi-up, dpi-down, dpi-shift, copy, paste, undo, cut, mute, vol-up, vol-down, play-pause, disabled
        #[arg(long)]
        set: Option<String>,
        /// Reset button (or all buttons if no --index) to factory defaults
        #[arg(long)]
        reset: bool,
    },
    /// Probe raw HID++ feature and function
    Probe {
        #[arg(long)]
        device: Option<String>,
        #[arg(long)]
        feature: String,
        #[arg(long, default_value_t = 0)]
        function: u8,
        #[arg(long)]
        params: Option<String>,
    },
    /// Open the graphical device and remap window
    Gui {
        /// Start minimized to system tray
        #[arg(long, short)]
        minimized: bool,
    },
}

fn main() -> Result<()> {
    #[cfg(windows)]
    attach_parent_console();

    let cli = Cli::parse();
    let top_minimized = cli.minimized;
    let command = cli.command.unwrap_or(Command::Gui {
        minimized: top_minimized,
    });
    let api = HidApi::new().context("initializing hidapi")?;
    let devices = device::discover(&api)?;

    match command {
        Command::List { json } => {
            if json {
                println!("{}", serde_json::to_string_pretty(&devices)?);
            } else {
                println!(
                    "{:<24} {:<8} {:<5} {:<10} FEATURES",
                    "DEVICE", "PID", "IDX", "BATTERY"
                );
                for descriptor in &devices {
                    let battery = device::read_battery(&api, descriptor)
                        .ok()
                        .flatten()
                        .map(|battery| battery.text)
                        .unwrap_or_else(|| "-".into());
                    println!(
                        "{:<24} {:<8} {:<5} {:<10} {}",
                        descriptor.name,
                        descriptor.product_id,
                        descriptor.index,
                        battery,
                        descriptor.features.len()
                    );
                }
                if devices.is_empty() {
                    println!("No Logitech HID++ devices found.");
                }
            }
        }
        Command::Info { device, json } => {
            let descriptor = select_device(&devices, device.as_deref())?;
            let snapshot = device::snapshot(&api, descriptor)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&snapshot)?);
            } else {
                print_snapshot(&snapshot);
            }
        }
        Command::Battery { device, json } => {
            let descriptor = select_device(&devices, device.as_deref())?;
            let battery = device::read_battery(&api, descriptor)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&battery)?);
            } else {
                match battery {
                    Some(battery) => println!("{}: {}", descriptor.name, battery.text),
                    None => println!("{}: battery information is not available", descriptor.name),
                }
            }
        }
        Command::Controls { device, json } => {
            let descriptor = select_device(&devices, device.as_deref())?;
            let snapshot = device::snapshot(&api, descriptor)?;
            if json {
                println!("{}", serde_json::to_string_pretty(&snapshot.controls)?);
            } else if snapshot.controls.is_empty() {
                println!(
                    "{} does not expose Reprogrammable Controls V4 (0x1B04).",
                    descriptor.name
                );
            } else {
                println!(
                    "{:<3} {:<6} {:<28} {:<32} MAPPED TO",
                    "IDX", "CID", "CONTROL", "FLAGS"
                );
                for control in &snapshot.controls {
                    let mapped = if control.mapped_to == control.cid {
                        control.mapped_to_name.clone()
                    } else {
                        format!("{} (0x{:04X})", control.mapped_to_name, control.mapped_to)
                    };
                    println!(
                        "{:<3} 0x{:04X} {:<28} {:<32} {}",
                        control.index, control.cid, control.name, control.flags_label, mapped
                    );
                }
            }
        }
        Command::Remap {
            device,
            from,
            to,
            apply,
        } => {
            let descriptor = select_device(&devices, device.as_deref())?;
            let from = parse_control(&from)?;
            let to = parse_control(&to)?;
            apply_mapping(&api, descriptor, from, to, apply, "remap")?;
        }
        Command::Reset {
            device,
            control,
            apply,
        } => {
            let descriptor = select_device(&devices, device.as_deref())?;
            let control = parse_control(&control)?;
            apply_mapping(&api, descriptor, control, control, apply, "reset")?;
        }
        Command::Mappings { json } => {
            let config = config::load()?;
            if json {
                println!("{}", serde_json::to_string_pretty(&config)?);
            } else if config.mappings.is_empty() {
                println!("No saved remappings.");
            } else {
                println!("{:<10} {:<28} {:<28} IDS", "DEVICE", "FROM", "TO");
                for mapping in &config.mappings {
                    println!(
                        "{:<10} {:<28} {:<28} 0x{:04X} -> 0x{:04X}",
                        mapping.product_id,
                        mapping.from_name,
                        mapping.to_name,
                        mapping.from,
                        mapping.to
                    );
                }
            }
        }
        Command::Apply => {
            apply_saved(&api, &devices)?;
        }
        Command::Rate { device, set } => {
            let descriptor = select_device(&devices, device.as_deref())?;
            if let Some(hz) = set {
                let ok = device::set_polling_rate(&api, descriptor, hz)?;
                if ok {
                    config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
                        s.polling_rate_hz = Some(hz);
                    })?;
                    println!("{}: Polling rate set to {} Hz", descriptor.name, hz);
                } else {
                    println!("{}: Device rejected polling rate change", descriptor.name);
                }
            } else {
                let snapshot = device::snapshot(&api, descriptor)?;
                match snapshot.polling_rate_hz {
                    Some(hz) => println!("{}: Current polling rate is {} Hz ({} ms)", descriptor.name, hz, snapshot.report_rate_ms.unwrap_or(1)),
                    None => println!("{}: Polling rate feature not supported", descriptor.name),
                }
            }
        }
        Command::Mode { device, set } => {
            let descriptor = select_device(&devices, device.as_deref())?;
            if let Some(mode_str) = set {
                let mode = match mode_str.to_lowercase().as_str() {
                    "performance" | "perf" | "1" => 1,
                    "endurance" | "eco" | "2" => 2,
                    _ => bail!("Unknown mode '{mode_str}'. Use 'performance' (1ms) or 'endurance' (8ms)."),
                };
                let ok = device::set_power_mode(&api, descriptor, mode)?;
                if ok {
                    config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
                        s.power_mode = Some(mode);
                    })?;
                    let name = if mode == 1 { "Performance (1000Hz, Cyan LED)" } else { "Endurance (125Hz, Green LED)" };
                    println!("{}: Power mode set to {}", descriptor.name, name);
                } else {
                    println!("{}: Device rejected mode change", descriptor.name);
                }
            } else {
                let snapshot = device::snapshot(&api, descriptor)?;
                match snapshot.mode_status {
                    Some(1) => println!("{}: Current mode: Performance (1ms / 1000Hz)", descriptor.name),
                    Some(2) => println!("{}: Current mode: Endurance (8ms / 125Hz)", descriptor.name),
                    Some(m) => println!("{}: Current mode: Unknown ({})", descriptor.name, m),
                    None => println!("{}: Power mode feature not supported", descriptor.name),
                }
            }
        }
        Command::Backlight {
            device,
            set,
            manual,
            auto,
            effect,
            duration,
            battery_saving,
            reset,
            off,
        } => {
            let descriptor = select_device(&devices, device.as_deref())?;

            if reset {
                let ok = device::set_backlight_full(&api, descriptor, 4, false, true, 0, 30)?;
                if ok {
                    config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
                        s.backlight_level = Some(4);
                        s.backlight_manual = Some(false);
                        s.backlight_enabled = Some(true);
                        s.backlight_effect = Some(0);
                        s.backlight_duration = Some(30);
                        s.backlight_battery_saving = Some(true);
                    })?;
                    println!("{}: Backlight reset to factory defaults (Static effect, Auto brightness, 30s duration, battery saving ON).", descriptor.name);
                } else {
                    eprintln!("{}: Failed to reset backlight defaults.", descriptor.name);
                }
                return Ok(());
            }

            let mut modified = false;

            if let Some(saving) = battery_saving {
                config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
                    s.backlight_battery_saving = Some(saving);
                })?;
                println!("{}: Backlight battery saving (<10% auto-off): {}", descriptor.name, if saving { "Enabled" } else { "Disabled" });
                modified = true;
            }

            if let Some(eff_str) = effect {
                let eff_id = parse_backlight_effect(&eff_str).ok_or_else(|| anyhow!("Unknown effect '{eff_str}'. Supported: static, contrast, breathing, waves, reaction, random"))?;
                let ok = device::set_backlight_effect(&api, descriptor, eff_id)?;
                if ok {
                    config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
                        s.backlight_effect = Some(eff_id);
                    })?;
                    println!("{}: Backlight effect set to {} (id: {})", descriptor.name, backlight_effect_name(eff_id), eff_id);
                    modified = true;
                } else {
                    eprintln!("{}: Device rejected backlight effect change.", descriptor.name);
                }
            }

            if let Some(dur) = duration {
                let ok = device::set_backlight_duration(&api, descriptor, dur)?;
                if ok {
                    config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
                        s.backlight_duration = Some(dur);
                    })?;
                    println!("{}: Backlight timeout duration set to {} seconds", descriptor.name, dur);
                    modified = true;
                } else {
                    eprintln!("{}: Device rejected backlight duration change.", descriptor.name);
                }
            }

            if off {
                let ok = device::set_backlight(&api, descriptor, 0, true, false)?;
                if ok {
                    config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
                        s.backlight_enabled = Some(false);
                    })?;
                    println!("{}: Backlight turned OFF", descriptor.name);
                    modified = true;
                }
            } else if auto {
                let lvl = set.unwrap_or(4);
                let ok = device::set_backlight(&api, descriptor, lvl, false, true)?;
                if ok {
                    config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
                        s.backlight_level = Some(lvl);
                        s.backlight_manual = Some(false);
                        s.backlight_enabled = Some(true);
                    })?;
                    println!("{}: Backlight set to Smart Ambient Sensor (Auto)", descriptor.name);
                    modified = true;
                }
            } else if let Some(lvl) = set {
                let ok = device::set_backlight(&api, descriptor, lvl.min(7), manual, true)?;
                if ok {
                    config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
                        s.backlight_level = Some(lvl.min(7));
                        s.backlight_manual = Some(manual);
                        s.backlight_enabled = Some(true);
                    })?;
                    println!("{}: Backlight set to level {} (manual: {})", descriptor.name, lvl.min(7), manual);
                    modified = true;
                }
            }

            if !modified {
                let snapshot = device::snapshot(&api, descriptor)?;
                match snapshot.backlight {
                    Some(b) => {
                        let eff_name = backlight_effect_name(b.current_effect);
                        let mode = if b.manual_mode { format!("manual (Level {}/{})", b.level, b.max_level) } else { "auto ambient".to_string() };
                        let saving_str = if let Ok(cfg) = config::load() {
                            cfg.device_settings.iter().find(|s| s.product_id == descriptor.product_id && s.unit_id == descriptor.unit_id)
                                .and_then(|s| s.backlight_battery_saving)
                                .map(|b| if b { "enabled" } else { "disabled" })
                                .unwrap_or("enabled")
                        } else {
                            "enabled"
                        };
                        println!("{}: Backlight Status:", descriptor.name);
                        println!("  Power:          {}", if b.enabled { "Enabled (ON)" } else { "Disabled (OFF)" });
                        println!("  Mode:           {}", mode);
                        println!("  Active Effect:  {} (id: {})", eff_name, b.current_effect);
                        println!("  Sleep Timeout:  {} seconds", b.dpow);
                        println!("  Hands-Off:      {} seconds", b.dho);
                        println!("  Hands-In:       {} seconds", b.dhi);
                        println!("  Battery Saving: {} (<10% threshold)", saving_str);
                    }
                    None => println!("{}: Backlight feature not supported", descriptor.name),
                }
            }
        }
        Command::FnLock { device, set, toggle } => {
            let descriptor = select_device(&devices, device.as_deref())?;
            let snapshot = device::snapshot(&api, descriptor)?;
            let current = snapshot.fn_inverted.unwrap_or(false);
            let target = if toggle {
                !current
            } else if let Some(s) = set {
                s
            } else {
                let mode = if current { "Standard Function Keys (F1-F12)" } else { "Media / Action Keys" };
                println!("{}: Current Fn-lock state: {}", descriptor.name, mode);
                return Ok(());
            };
            let ok = device::set_fn_inversion(&api, descriptor, target)?;
            if ok {
                config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
                    s.fn_inverted = Some(target);
                })?;
                let mode = if target { "Standard Function Keys (F1-F12)" } else { "Media / Action Keys" };
                println!("{}: Fn-lock set to: {}", descriptor.name, mode);
            }
        }
        Command::Dpi { device, set } => {
            let descriptor = select_device(&devices, device.as_deref())?;
            if let Some(stage) = set {
                if stage == 0 {
                    bail!("DPI stage must be 1 or higher (e.g. --set 1, --set 2)");
                }
                let stage_idx = stage - 1;
                let ok = device::set_dpi_stage(&api, descriptor, stage_idx)?;
                if ok {
                    config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
                        s.dpi_stage = Some(stage_idx);
                    })?;
                    println!("{}: Switched to DPI Stage {}", descriptor.name, stage);
                } else {
                    println!("{}: Device rejected DPI stage switch", descriptor.name);
                }
            } else {
                let snapshot = device::snapshot(&api, descriptor)?;
                match snapshot.dpi {
                    Some(dpi) => {
                        println!("{}: Onboard DPI Stages:", descriptor.name);
                        for (i, lvl) in dpi.levels.iter().enumerate() {
                            let active = if i == dpi.active_index { " [ACTIVE]" } else { "" };
                            println!("  Stage {}: {} DPI{}", i + 1, lvl, active);
                        }
                    }
                    None => println!("{}: Onboard DPI profiles not exposed on this device", descriptor.name),
                }
            }
        }
        Command::Hosts { device, switch } => {
            let descriptor = select_device(&devices, device.as_deref())?;
            if let Some(slot) = switch {
                let ok = device::switch_host(&api, descriptor, slot.saturating_sub(1))?;
                if ok {
                    println!("{}: Switched to host slot {}", descriptor.name, slot);
                } else {
                    println!("{}: Failed to switch host", descriptor.name);
                }
            } else {
                let snapshot = device::snapshot(&api, descriptor)?;
                match snapshot.hosts {
                    Some(hosts) => {
                        println!("{}: Easy-Switch Host Channels ({}):", descriptor.name, hosts.count);
                        for slot in &hosts.slots {
                            let status = if slot.paired { "Paired" } else { "Empty / Available" };
                            println!("  Slot {}: {}", slot.index, status);
                        }
                    }
                    None => println!("{}: Easy-Switch not supported on this device", descriptor.name),
                }
            }
        }
        Command::Button {
            device,
            index,
            set,
            reset,
        } => {
            let descriptor = select_device(&devices, device.as_deref())?;
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

            if reset {
                match index {
                    Some(btn_num) => {
                        if btn_num < 1 || btn_num > 6 {
                            bail!("Button number must be between 1 and 6");
                        }
                        let idx = (btn_num - 1) as usize;
                        let ok = device::set_mouse_button(&api, descriptor, idx as u8, defaults[idx])?;
                        if ok {
                            config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
                                s.mouse_button_remaps.retain(|r| r.button_index != idx as u8);
                            })?;
                            println!("{}: Reset Button {} to default ({})", descriptor.name, btn_num, default_names[idx]);
                        } else {
                            println!("{}: Failed to reset Button {}", descriptor.name, btn_num);
                        }
                    }
                    None => {
                        for (i, def) in defaults.iter().enumerate() {
                            let _ = device::set_mouse_button(&api, descriptor, i as u8, *def);
                        }
                        config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
                            s.mouse_button_remaps.clear();
                        })?;
                        println!("{}: Reset all 6 mouse buttons to factory defaults", descriptor.name);
                    }
                }
            } else if let Some(action_str) = set {
                let Some(btn_num) = index else {
                    bail!("Please specify which button to remap with --index <1-6>");
                };
                if btn_num < 1 || btn_num > 6 {
                    bail!("Button number must be between 1 and 6");
                }
                let raw = parse_mouse_action_preset(&action_str)?;
                let (label, _) = hidpp::parse_mouse_binding(raw);
                let idx = btn_num - 1;
                let ok = device::set_mouse_button(&api, descriptor, idx, raw)?;
                if ok {
                    config::update_device_settings(&descriptor.product_id, &descriptor.unit_id, |s| {
                        s.mouse_button_remaps.retain(|r| r.button_index != idx);
                        s.mouse_button_remaps.push(config::SavedMouseButtonRemap {
                            button_index: idx,
                            raw,
                            label: label.clone(),
                        });
                    })?;
                    println!("{}: Button {} mapped -> {} and saved to mouse hardware memory", descriptor.name, btn_num, label);
                } else {
                    println!("{}: Device rejected button mapping", descriptor.name);
                }
            } else {
                let snapshot = device::snapshot(&api, descriptor)?;
                match snapshot.mouse_buttons {
                    Some(buttons) => {
                        println!("{}: Mouse Button Mappings ({}):", descriptor.name, buttons.len());
                        for btn in buttons {
                            println!("  Button {}: {:<18} -> {} [{}]", btn.index + 1, btn.name, btn.binding_label, btn.category);
                        }
                    }
                    None => println!("{}: Onboard mouse button remapping not supported on this device", descriptor.name),
                }
            }
        }
        Command::Probe {
            device,
            feature,
            function,
            params,
        } => {
            let descriptor = select_device(&devices, device.as_deref())?;
            let feature_id = if feature.starts_with("0x") || feature.starts_with("0X") {
                u16::from_str_radix(&feature[2..], 16)?
            } else {
                u16::from_str_radix(&feature, 16)?
            };
            let param_bytes: Vec<u8> = if let Some(p) = params {
                p.split(',')
                    .filter_map(|s| {
                        let s = s.trim();
                        if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
                            u8::from_str_radix(hex, 16).ok()
                        } else {
                            s.parse::<u8>().ok()
                        }
                    })
                    .collect()
            } else {
                Vec::new()
            };
            let transport = device::open(&api, descriptor)?;
            let reply = transport.feature_request(feature_id, function, &param_bytes)?;
            match reply {
                Some(bytes) => {
                    print!("Reply ({} bytes):", bytes.len());
                    for b in &bytes {
                        print!(" {:02X}", b);
                    }
                    println!();
                }
                None => println!("No reply / error from device."),
            }
        }
        Command::Gui { minimized } => {
            ui::run(api, devices, minimized || top_minimized)?;
        }
    }

    Ok(())
}

fn apply_saved(api: &HidApi, devices: &[device::DeviceDescriptor]) -> Result<()> {
    let config = config::load()?;
    let mut applied_remaps = 0;
    let mut applied_settings = 0;

    for mapping in &config.mappings {
        let descriptor = devices.iter().find(|device| {
            device.product_id.eq_ignore_ascii_case(&mapping.product_id)
                && (mapping.unit_id.is_empty()
                    || device.unit_id.eq_ignore_ascii_case(&mapping.unit_id))
        });
        let Some(descriptor) = descriptor else {
            println!(
                "skip: {} ({}) is not connected",
                mapping.from_name, mapping.product_id
            );
            continue;
        };
        match device::set_mapping(api, descriptor, mapping.from, mapping.to) {
            Ok(true) => {
                println!(
                    "Remap: {}: {} -> {}",
                    descriptor.name, mapping.from_name, mapping.to_name
                );
                applied_remaps += 1;
            }
            Ok(false) => println!(
                "warning: {} did not confirm {} -> {}",
                descriptor.name, mapping.from_name, mapping.to_name
            ),
            Err(error) => println!(
                "error: {}: {} -> {}: {error:#}",
                descriptor.name, mapping.from_name, mapping.to_name
            ),
        }
    }

    for settings in &config.device_settings {
        let descriptor = devices.iter().find(|device| {
            device.product_id.eq_ignore_ascii_case(&settings.product_id)
                && (settings.unit_id.is_empty()
                    || device.unit_id.eq_ignore_ascii_case(&settings.unit_id))
        });
        let Some(descriptor) = descriptor else { continue; };

        if let Some(rate) = settings.polling_rate_hz {
            if let Ok(true) = device::set_polling_rate(api, descriptor, rate) {
                println!("{}: Polling rate -> {} Hz", descriptor.name, rate);
                applied_settings += 1;
            }
        }
        if let Some(mode) = settings.power_mode {
            if let Ok(true) = device::set_power_mode(api, descriptor, mode) {
                let name = if mode == 1 { "Performance" } else { "Endurance" };
                println!("{}: Power mode -> {}", descriptor.name, name);
                applied_settings += 1;
            }
        }
        if let Some(stage) = settings.dpi_stage {
            if let Ok(true) = device::set_dpi_stage(api, descriptor, stage) {
                println!("{}: DPI stage -> Stage {}", descriptor.name, stage + 1);
                applied_settings += 1;
            }
        }
        if let Some(fn_inv) = settings.fn_inverted {
            if let Ok(true) = device::set_fn_inversion(api, descriptor, fn_inv) {
                let name = if fn_inv { "Standard F1-F12" } else { "Media keys" };
                println!("{}: Fn-lock -> {}", descriptor.name, name);
                applied_settings += 1;
            }
        }
        if settings.backlight_level.is_some()
            || settings.backlight_duration.is_some()
            || settings.backlight_manual.is_some()
            || settings.backlight_enabled.is_some()
        {
            let lvl = settings.backlight_level.unwrap_or(5);
            let effect = settings.backlight_effect.unwrap_or(0xFF);
            let duration = settings.backlight_duration.unwrap_or(30);
            if let Ok(true) = device::set_backlight_full(
                api,
                descriptor,
                lvl,
                settings.backlight_manual.unwrap_or(false),
                settings.backlight_enabled.unwrap_or(true),
                effect,
                duration,
            ) {
                println!("{}: Backlight -> level {}, duration {}s", descriptor.name, lvl, duration);
                applied_settings += 1;
            }
        } else if let Some(effect) = settings.backlight_effect {
            if let Ok(true) = device::set_backlight_effect(api, descriptor, effect) {
                println!("{}: Backlight effect -> {}", descriptor.name, effect);
                applied_settings += 1;
            }
        }
        if let Some(mask) = settings.disabled_keys_mask {
            if let Ok(true) = device::set_disabled_keys(api, descriptor, mask) {
                println!("{}: Disabled keys mask -> 0x{:02X}", descriptor.name, mask);
                applied_settings += 1;
            }
        }
        for remap in &settings.mouse_button_remaps {
            if let Ok(true) = device::set_mouse_button(api, descriptor, remap.button_index, remap.raw) {
                println!("{}: Mouse Button {} -> {}", descriptor.name, remap.button_index + 1, remap.label);
                applied_settings += 1;
            }
        }
    }

    if applied_remaps == 0 && applied_settings == 0 {
        println!("No saved remaps or custom settings to apply.");
    } else {
        println!("Applied {applied_remaps} remap(s) and {applied_settings} setting(s).");
    }
    Ok(())
}

fn select_device<'a>(
    devices: &'a [device::DeviceDescriptor],
    selector: Option<&str>,
) -> Result<&'a device::DeviceDescriptor> {
    if devices.is_empty() {
        bail!("no Logitech HID++ devices found");
    }
    match selector {
        None => {
            if devices.len() == 1 {
                Ok(&devices[0])
            } else {
                let names = devices
                    .iter()
                    .map(|device| format!("{} ({})", device.name, device.product_id))
                    .collect::<Vec<_>>()
                    .join(", ");
                bail!("multiple devices found; pass --device with one of: {names}")
            }
        }
        Some(selector) => {
            let needle = selector.to_lowercase();
            let matches = devices
                .iter()
                .filter(|device| {
                    device.name.to_lowercase().contains(&needle)
                        || device.product_id.to_lowercase() == needle
                        || device.unit_id.to_lowercase() == needle
                        || device.unit_id.to_lowercase().contains(&needle)
                        || device.index.to_string() == needle
                })
                .collect::<Vec<_>>();
            match matches.as_slice() {
                [device] => Ok(*device),
                [] => bail!("no device matched {selector:?}"),
                _ => bail!("device selector {selector:?} is ambiguous"),
            }
        }
    }
}

fn parse_control(value: &str) -> Result<u16> {
    control_names::lookup_control(value)
        .with_context(|| format!("unknown control {value:?}; use a CID such as 0x00E9"))
}

fn apply_mapping(
    api: &HidApi,
    device: &device::DeviceDescriptor,
    from: u16,
    to: u16,
    apply: bool,
    action: &str,
) -> Result<()> {
    let from_name = control_names::control_name(from);
    let to_name = control_names::control_name(to);
    println!(
        "{}: {from_name} (0x{from:04X}) -> {to_name} (0x{to:04X})",
        if apply { action } else { "dry-run" }
    );
    println!("device: {} ({})", device.name, device.product_id);
    if !apply {
        println!("no changes written; re-run with --apply to commit");
        return Ok(());
    }
    let echoed = device::set_mapping(api, device, from, to)?;
    if echoed {
        println!("device confirmed the new mapping");
    } else {
        println!("warning: device did not echo the mapping request");
    }
    if from == to {
        config::remove(&device.product_id, &device.unit_id, from)?;
    } else {
        config::record(
            &device.product_id,
            &device.unit_id,
            from,
            to,
            &from_name,
            &to_name,
        )?;
    }
    Ok(())
}

fn backlight_effect_name(id: u8) -> &'static str {
    match id {
        0 => "Static",
        1 => "Contrast",
        2 => "Breathing",
        3 => "Waves",
        4 => "Reaction",
        5 => "Random",
        _ => "Unknown",
    }
}

fn parse_backlight_effect(name: &str) -> Option<u8> {
    match name.to_lowercase().as_str() {
        "static" | "0" => Some(0),
        "contrast" | "1" => Some(1),
        "breathing" | "breath" | "2" => Some(2),
        "waves" | "wave" | "3" => Some(3),
        "reaction" | "react" | "4" => Some(4),
        "random" | "5" => Some(5),
        _ => None,
    }
}

fn parse_single_hid_key(token: &str) -> Option<u8> {
    if token.len() == 1 {
        let b = token.as_bytes()[0];
        if b.is_ascii_lowercase() {
            return Some(0x04 + (b - b'a'));
        }
        if (b'1'..=b'9').contains(&b) {
            return Some(0x1E + (b - b'1'));
        }
        if b == b'0' {
            return Some(0x27);
        }
    }
    if let Some(num_str) = token.strip_prefix('f') {
        if let Ok(n) = num_str.parse::<u8>() {
            if (1..=12).contains(&n) {
                return Some(0x3A + (n - 1));
            }
        }
    }
    match token {
        "enter" | "return" => Some(0x28),
        "escape" | "esc" => Some(0x29),
        "backspace" | "bksp" => Some(0x2A),
        "tab" => Some(0x2B),
        "space" => Some(0x2C),
        "minus" | "-" => Some(0x2D),
        "equals" | "=" => Some(0x2E),
        "capslock" | "caps" => Some(0x39),
        "insert" | "ins" => Some(0x49),
        "home" => Some(0x4A),
        "pageup" | "pgup" => Some(0x4B),
        "delete" | "del" => Some(0x4C),
        "end" => Some(0x4D),
        "pagedown" | "pgdn" => Some(0x4E),
        "right-arrow" | "arrow-right" | "rightkey" => Some(0x4F),
        "left-arrow" | "arrow-left" | "leftkey" => Some(0x50),
        "down-arrow" | "arrow-down" | "down" => Some(0x51),
        "up-arrow" | "arrow-up" | "up" => Some(0x52),
        _ => None,
    }
}

fn parse_mouse_action_preset(s: &str) -> Result<[u8; 4]> {
    let lower = s.trim().to_lowercase().replace('_', "-").replace(' ', "-");
    match lower.as_str() {
        "left" | "left-click" => return Ok([0x80, 0x01, 0x00, 0x01]),
        "right" | "right-click" => return Ok([0x80, 0x01, 0x00, 0x02]),
        "middle" | "middle-click" | "wheel" => return Ok([0x80, 0x01, 0x00, 0x04]),
        "back" | "side-1" => return Ok([0x80, 0x01, 0x00, 0x08]),
        "forward" | "side-2" => return Ok([0x80, 0x01, 0x00, 0x10]),
        "dpi-cycle" | "dpi" => return Ok([0x90, 0x05, 0x00, 0x00]),
        "dpi-up" => return Ok([0x90, 0x03, 0x00, 0x00]),
        "dpi-down" => return Ok([0x90, 0x04, 0x00, 0x00]),
        "dpi-default" => return Ok([0x90, 0x06, 0x00, 0x00]),
        "dpi-shift" | "sniper" => return Ok([0x90, 0x07, 0x00, 0x00]),
        "copy" | "ctrl-c" => return Ok([0x80, 0x02, 0x01, 0x06]),
        "paste" | "ctrl-v" => return Ok([0x80, 0x02, 0x01, 0x19]),
        "cut" | "ctrl-x" => return Ok([0x80, 0x02, 0x01, 0x1B]),
        "undo" | "ctrl-z" => return Ok([0x80, 0x02, 0x01, 0x1D]),
        "redo" | "ctrl-y" => return Ok([0x80, 0x02, 0x01, 0x1C]),
        "mute" => return Ok([0x80, 0x03, 0x00, 0xE2]),
        "vol-up" | "volume-up" => return Ok([0x80, 0x03, 0x00, 0xE9]),
        "vol-down" | "volume-down" => return Ok([0x80, 0x03, 0x00, 0xEA]),
        "play-pause" => return Ok([0x80, 0x03, 0x00, 0xCD]),
        "next-track" => return Ok([0x80, 0x03, 0x00, 0xB5]),
        "prev-track" => return Ok([0x80, 0x03, 0x00, 0xB6]),
        "disabled" | "none" => return Ok([0xFF, 0x00, 0x00, 0x00]),
        _ => {}
    }

    // Support direct single keys (e.g. "c", "v", "f5", "space") or modifier combos (e.g. "ctrl+shift+c", "alt-f4")
    let normalized = lower.replace('+', "-");
    let parts: Vec<&str> = normalized.split('-').filter(|p| !p.is_empty()).collect();
    if !parts.is_empty() {
        let mut mods = 0u8;
        let mut key_code = None;
        let mut valid = true;
        for (i, &part) in parts.iter().enumerate() {
            if i + 1 < parts.len() {
                match part {
                    "ctrl" | "control" => mods |= 0x01,
                    "shift" => mods |= 0x02,
                    "alt" => mods |= 0x04,
                    "win" | "meta" | "super" => mods |= 0x08,
                    _ => {
                        valid = false;
                        break;
                    }
                }
            } else {
                key_code = parse_single_hid_key(part);
            }
        }
        if valid {
            if let Some(code) = key_code {
                return Ok([0x80, 0x02, mods, code]);
            }
        }
    }

    if let Some(code) = parse_single_hid_key(&lower) {
        return Ok([0x80, 0x02, 0x00, code]);
    }

    bail!("Unknown mouse action '{s}'. Supported: any key (a-z, 0-9, f1-f12), combos (ctrl+c, shift+v, alt+f4), left, right, middle, back, forward, dpi-cycle, dpi-up, dpi-down, dpi-shift, copy, paste, undo, cut, redo, enter, escape, space, tab, backspace, mute, vol-up, vol-down, play-pause, next-track, prev-track, disabled")
}

fn print_snapshot(snapshot: &device::Snapshot) {
    println!("{}", snapshot.device.name);
    println!("  product:     {}", snapshot.device.product_id);
    println!("  unit id:     {}", snapshot.device.unit_id);
    println!("  transport:   {}", snapshot.device.transport);
    println!("  device index:{}", snapshot.device.index);
    if let Some(serial) = &snapshot.serial {
        println!("  serial:      {serial}");
    }
    if let Some(battery) = &snapshot.battery {
        println!("  battery:     {}", battery.text);
        if let Some(voltage) = battery.voltage_mv {
            println!("  voltage:     {voltage} mV");
        }
    }
    if let Some(rate) = snapshot.report_rate_ms {
        let hz = snapshot.polling_rate_hz.unwrap_or(1000);
        println!("  polling rate:{} Hz ({rate} ms)", hz);
    }
    if let Some(mode) = snapshot.mode_status {
        let name = if mode == 1 { "Performance (1000Hz)" } else { "Endurance (125Hz)" };
        println!("  power mode:  {}", name);
    }
    if let Some(dpi) = &snapshot.dpi {
        let stages: Vec<_> = dpi.levels.iter().enumerate().map(|(i, l)| {
            if i == dpi.active_index { format!("*{}*", l) } else { l.to_string() }
        }).collect();
        println!("  dpi stages:  {}", stages.join(", "));
    }
    if let Some(bl) = &snapshot.backlight {
        let mode = if bl.manual_mode { "manual" } else { "auto ambient" };
        println!("  backlight:   level {}/{} ({mode}, enabled: {})", bl.level, bl.max_level, bl.enabled);
    }
    if let Some(fn_inv) = snapshot.fn_inverted {
        let mode = if fn_inv { "Standard Function Keys (F1-F12)" } else { "Media Keys" };
        println!("  fn-lock:     {}", mode);
    }
    if let Some(hosts) = &snapshot.hosts {
        let paired = hosts.slots.iter().filter(|s| s.paired).count();
        println!("  easy-switch: {} paired / {} total slots", paired, hosts.count);
    }
    if let Some(dis) = &snapshot.disabled_keys {
        if dis.mask != 0 {
            println!("  disabled keys:0x{:02X}", dis.mask);
        }
    }
    if let Some(buttons) = &snapshot.mouse_buttons {
        println!("  mouse buttons ({}):", buttons.len());
        for btn in buttons {
            println!("    {}: {} [{}]", btn.name, btn.binding_label, btn.category);
        }
    }
    if !snapshot.firmware.is_empty() {
        println!("  firmware:");
        for firmware in &snapshot.firmware {
            println!("    - {}", firmware.text);
        }
    }
    println!("  features ({}):", snapshot.device.features.len());
    for feature in &snapshot.device.features {
        println!(
            "    0x{:02X}  0x{:04X}  {}",
            feature.index, feature.id, feature.name
        );
    }
    println!("  controls: {}", snapshot.controls.len());
}

#[cfg(windows)]
fn attach_parent_console() {
    unsafe {
        unsafe extern "system" {
            fn AttachConsole(dw_process_id: u32) -> i32;
            fn GetStdHandle(n_std_handle: u32) -> *mut std::ffi::c_void;
            fn GetFileType(h_file: *mut std::ffi::c_void) -> u32;
            fn CreateFileW(
                lp_file_name: *const u16,
                dw_desired_access: u32,
                dw_share_mode: u32,
                lp_security_attributes: *mut std::ffi::c_void,
                dw_creation_disposition: u32,
                dw_flags_and_attributes: u32,
                h_template_file: *mut std::ffi::c_void,
            ) -> *mut std::ffi::c_void;
            fn SetStdHandle(n_std_handle: u32, h_handle: *mut std::ffi::c_void) -> i32;
        }

        const STD_OUTPUT_HANDLE: u32 = 0xFFFFFFF5;
        const STD_ERROR_HANDLE: u32 = 0xFFFFFFF4;
        const FILE_TYPE_UNKNOWN: u32 = 0x0000;
        const INVALID_HANDLE_VALUE: *mut std::ffi::c_void = -1isize as *mut std::ffi::c_void;

        let stdout = GetStdHandle(STD_OUTPUT_HANDLE);
        let stdout_redirected = !stdout.is_null()
            && stdout != INVALID_HANDLE_VALUE
            && GetFileType(stdout) != FILE_TYPE_UNKNOWN;

        if !stdout_redirected {
            const ATTACH_PARENT_PROCESS: u32 = 0xFFFFFFFF;
            if AttachConsole(ATTACH_PARENT_PROCESS) != 0 {
                let conout_utf16: Vec<u16> = "CONOUT$\0".encode_utf16().collect();
                const GENERIC_READ: u32 = 0x80000000;
                const GENERIC_WRITE: u32 = 0x40000000;
                const FILE_SHARE_READ: u32 = 0x00000001;
                const FILE_SHARE_WRITE: u32 = 0x00000002;
                const OPEN_EXISTING: u32 = 3;

                let handle = CreateFileW(
                    conout_utf16.as_ptr(),
                    GENERIC_READ | GENERIC_WRITE,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    std::ptr::null_mut(),
                    OPEN_EXISTING,
                    0,
                    std::ptr::null_mut(),
                );

                if handle != INVALID_HANDLE_VALUE {
                    SetStdHandle(STD_OUTPUT_HANDLE, handle);
                    SetStdHandle(STD_ERROR_HANDLE, handle);
                }
            }
        }
    }
}
