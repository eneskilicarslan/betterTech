# betterTech ⚡

> **A fast-paced, lightweight, modern Logitech hardware controller in Rust.**  
> Unifies all the capabilities of **Logitech G HUB** (gaming mice like G305 Lightspeed) and **Logi Options+** (productivity keyboards like MX Mechanical) into a single, native desktop application.

---

## 🚀 Why betterTech?

Logitech's official software ecosystem is divided between two heavy Electron-based suites:
- **Logitech G HUB** (~350–500 MB RAM, multiple background telemetry and agent services) for gaming hardware.
- **Logi Options+** (~300–450 MB RAM, persistent background node processes and analytics) for productivity hardware.

Running both apps simultaneously consumes almost **1 GB of RAM** just to manage mouse DPI and keyboard backlight.

**betterTech** completely replaces both with:
- **~4.8 MB standalone native executable** written in pure Rust.
- **~8.5 MB RAM footprint** during active GUI use (drops to ~1.1 MB when idle).
- **Zero background bloat**, zero analytics, and zero cloud logins.
- **Direct raw communication** over Logitech's HID++ 2.0 protocol (`hidapi`) via USB, Lightspeed wireless receivers, and Bluetooth Low Energy / Logi Bolt.

---

## 📥 Download

Pre-compiled standalone Windows executables are available on the **[Releases](https://github.com/OWNER/betterTech/releases)** page:
1. Download `bettertech-windows-x86_64.zip` or `bettertech.exe` directly from the latest release.
2. Run `bettertech.exe` (no installer required).
3. Optional: Open `⚙ Preferences` in the app and enable **Run on Windows startup** to start minimized to the notification tray.

---

## ✨ Feature Parity Overview

| Feature | Official App | `betterTech` GUI & CLI | Protocol Feature |
| :--- | :--- | :--- | :--- |
| **Mouse Button Remapping (6 Hardware Buttons)** | G HUB | ✅ Visual schematic + 27 presets | `0x8100` (OnboardProfiles) |
| **Mouse Polling Rate (125 - 1000 Hz)** | G HUB | ✅ Instant toggle & auto-restore | `0x8060` (ReportRate) |
| **Power Operating Modes (Performance vs Endurance)** | G HUB | ✅ One-click toggle | `0x8090` (ModeStatus) |
| **HERO Sensor DPI Stages (400 - 3200 DPI)** | G HUB | ✅ Live stage detection & switching | `0x8100` (OnboardProfiles) |
| **Keyboard Backlight Effects (6 Presets)** | Logi Options+ | ✅ Static, Contrast, Breathing, Waves, Reaction, Random | `0x1982` (Backlight2) |
| **Keyboard Illumination Schematic & Badges** | Logi Options+ | ✅ Vector keyboard chassis with live callout pins | Dynamic Painter |
| **Backlight Inactivity Duration & Sleep Timers** | Logi Options+ | ✅ Adjustable duration slider (5–300s) | `0x1982` (Backlight2) |
| **Smart Backlight Brightness (Levels 0–7)** | Logi Options+ | ✅ Real-time level slider & warnings | `0x1982` (Backlight2) |
| **Auto Ambient Light Sensor vs Manual Mode** | Logi Options+ | ✅ Toggle auto sensor vs manual | `0x1982` (Backlight2) |
| **Battery Saving Mode (<10% Auto-Off)** | Logi Options+ | ✅ Automatic power preservation | Client Daemon & Config |
| **Fn-Lock Inversion (F1–F12 vs Media Keys)** | Logi Options+ | ✅ One-click toggle & restore | `0x40A3` (K375sFnInversion) |
| **Disable Nuisance Keys (Win, Caps, Num, Scroll, Ins)** | Logi Options+ | ✅ Interactive key mask toggles | `0x4521` (KeyboardDisableKeys) |
| **Easy-Switch Multi-Host Management** | Logi Options+ | ✅ Inspect slots 1–3 & switch channel | `0x1815` / `0x1814` |
| **Reprogrammable Controls & Key Remap** | Logi Options+ | ✅ 41 searchable controls & reset | `0x1B04` (ReprogControlsV4) |
| **Accurate Battery Voltage & Diagnostics** | Logi Options+ | ✅ Percentage, status & millivolts | `0x1000` / `0x1004` |
| **Windows System Tray & Autostart** | Both | ✅ Minimize to tray, start on boot | Native Win32 Registry |
| **Independent Windows Process Lifetime** | Both | ✅ Detached from console session | `#![windows_subsystem = "windows"]` |

---

## 🖥️ Modern Dark GUI

Built with modern `egui` (`eframe` v0.37):
- **Responsive Layout**: Sidebar device selector with live battery indicators, device index badges, and serial numbers.
- **Dynamic Device Routing**: Contextual tabs tailored specifically to your connected device:
  - *MX Mechanical*: `⌨ Controls & Remap`, `💡 Backlight`, `⚙ Keyboard Settings`, `🔀 Easy-Switch`, `🔋 Hardware Info`, `📋 Features`.
  - *G305 Lightspeed*: `🖱 Mouse Buttons`, `⚡ Polling & Power`, `🎯 Sensor & DPI`, `🔋 Hardware Info`, `📋 Features`.
- **Dedicated Mouse Button Remapping Studio**:
  - Top-down visual blueprint graphic representing mouse hardware layout.
  - Interactive callout badges identifying Left Click (1), Right Click (2), Middle Click (3), Back (4), Forward (5), and DPI Switch (6).
  - 27 pre-configured presets across 5 categories: Mouse Actions, DPI & Special Controls, Keyboard Shortcuts (Copy, Paste, Cut, Undo, Redo, Enter, Escape, etc.), Media Playback, and Disabled.
  - Real-time onboard memory flashing with checksum verification and instant config persistence.
- **Dedicated Keyboard Backlighting & Illumination Studio**:
  - Top-down vector schematic of the MX Mechanical keyboard with dual-tone keycaps, glowing backlighting simulation, highlighted `Fn` and `Lightbulb` keys, and live callout pins.
  - **Select Backlight Effect**: 6 selectable interactive cards with custom radio indicators for `Static`, `Contrast`, `Breathing`, `Waves`, `Reaction`, and `Random`.
  - **Modify Settings**:
    - Master Backlighting toggle switch (ON / OFF).
    - Backlighting Duration slider with live readout (5–300 seconds) and quick presets (`5s`, `10s`, `30s`, `60s`, `120s`, `300s`).
    - Smart Ambient Automatic Brightness sensor mode toggle.
    - Manual Brightness slider (Levels 0–7) with quick presets and battery optimization warnings.
    - Battery saving mode toggle (automatically shuts off backlighting when battery level falls below 10%).
    - One-click `RESET BACKLIGHTING` button restoring all factory defaults.
- **System Tray Integration & Console Detachment**:
  - Built as a native Win32 window application (`windows_subsystem = "windows"`): double-clicking or launching from CMD/PowerShell runs completely independently—closing the launching terminal does NOT close the application.
  - Automatically attaches to terminal consoles when running CLI commands (`bettertech list`, `bettertech button`, etc.).
  - Closing the window smoothly minimizes betterTech to the notification tray.
  - Left-clicking or double-clicking the cyan lightning bolt icon restores the window instantly.
  - Right-click tray menu provides "Show betterTech" and clean "Quit".
- **Preferences Modal**: Toggle Windows Autostart (`HKCU\Software\Microsoft\Windows\CurrentVersion\Run`) with `--minimized` background startup.

---

## ⌨️ Command Line Interface (CLI)

Every setting is also fully scriptable via the command line:

```bash
# List all connected Logitech peripherals
bettertech list

# Show comprehensive device diagnostics (battery, firmware, features, controls)
bettertech info --device 4074
bettertech info --device B366

# Get or set mouse polling rate in Hz (125, 250, 500, 1000)
bettertech rate --device 4074 --set 1000

# Get or set power operating mode (performance or endurance)
bettertech mode --device 4074 --set performance
bettertech mode --device 4074 --set endurance

# Show or switch HERO sensor DPI stages
bettertech dpi --device 4074
bettertech dpi --device 4074 --set 2

# Inspect, remap, and reset mouse buttons (G305 & gaming mice)
bettertech button --device 4074
bettertech button --device 4074 --index 4 --set copy
bettertech button --device 4074 --index 6 --set dpi-up
bettertech button --device 4074 --index 4 --reset
bettertech button --device 4074 --reset

# Query keyboard backlight status (power, mode, effect, timers, battery saving)
bettertech backlight --device B366

# Set backlight effect (static, contrast, breathing, waves, reaction, random)
bettertech backlight --device B366 --effect breathing
bettertech backlight --device B366 --effect reaction

# Set backlight timeout duration in seconds (e.g. 10, 30, 60, 120, 300)
bettertech backlight --device B366 --duration 30

# Set manual brightness level (0-7) or switch to auto ambient sensor
bettertech backlight --device B366 --set 5 --manual
bettertech backlight --device B366 --auto

# Configure battery saving auto-off (<10% threshold)
bettertech backlight --device B366 --battery-saving true

# Reset backlight settings to factory defaults
bettertech backlight --device B366 --reset

# Turn off backlight completely
bettertech backlight --device B366 --off

# Get or toggle Fn-lock (standard F1-F12 vs media shortcuts)
bettertech fn-lock --device B366 --toggle

# Inspect and switch Easy-Switch host channels (1, 2, or 3)
bettertech hosts --device B366
bettertech hosts --device B366 --switch 1

# List programmable controls and remap
bettertech controls --device B366
bettertech remap --device B366 --from 0x00C7 --to 0x00C8 --apply
bettertech reset --device B366 --control 0x00C7

# Reapply all saved configurations from config.json
bettertech apply

# Launch GUI directly (or start minimized to tray on boot)
bettertech gui
bettertech gui --minimized
```

---

## 💾 Local Configuration

All settings and remappings are stored locally in human-readable JSON:
`%APPDATA%\betterTech\config.json`

Example:
```json
{
  "mappings": [
    {
      "product_id": "B366",
      "unit_id": "97279B4A",
      "from": 199,
      "to": 200,
      "from_name": "Brightness Down",
      "to_name": "Brightness Up"
    }
  ],
  "device_settings": [
    {
      "product_id": "4074",
      "unit_id": "54AA1533",
      "polling_rate_hz": 1000,
      "power_mode": 1,
      "dpi_stage": 1,
      "fn_inverted": null,
      "backlight_level": null,
      "backlight_manual": null,
      "backlight_enabled": null,
      "disabled_keys_mask": null
    }
  ]
}
```

---

## 🛠️ Building & Developing

### Requirements
- Rust 1.85+ (2024 edition)
- Windows 10/11 (with USB / Bluetooth Logitech peripherals)

### Compilation
```bash
# Debug build and run
cargo run -- gui

# Run automated tests
cargo test

# Build optimized release binary
cargo build --release
```
The optimized executable will be located at:
`target\release\bettertech.exe` (~4.8 MB single binary, ~8.5 MB RAM footprint)

---

## ⚖️ Legal Disclaimer & Trademarks

- **betterTech** is an independent, open-source project and is **not** affiliated with, endorsed by, sponsored by, or supported by Logitech Inc. or Logitech Europe S.A.
- Logitech, Logi, G HUB, Logi Options+, Lightspeed, HERO, and MX Mechanical are registered trademarks of Logitech Europe S.A. and/or its affiliates in the U.S. and other countries.
- This software communicates with hardware using the publicly known and documented HID++ protocol for hardware interoperability purposes under fair use and legal reverse engineering provisions (e.g., DMCA 1201(f) and EU Directive 2009/24/EC).

---

## 📜 License

This project is licensed under the [MIT License](LICENSE).
