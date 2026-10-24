use anyhow::{Context, Result};
use directories::BaseDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Config {
    #[serde(default)]
    pub mappings: Vec<SavedMapping>,
    #[serde(default)]
    pub device_settings: Vec<DeviceSettings>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedMouseButtonRemap {
    pub button_index: u8,
    pub raw: [u8; 4],
    pub label: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct DeviceSettings {
    pub product_id: String,
    pub unit_id: String,
    pub polling_rate_hz: Option<u16>,
    pub power_mode: Option<u8>,
    pub dpi_stage: Option<u8>,
    pub fn_inverted: Option<bool>,
    pub backlight_level: Option<u8>,
    pub backlight_manual: Option<bool>,
    pub backlight_enabled: Option<bool>,
    pub backlight_effect: Option<u8>,
    pub backlight_duration: Option<u16>,
    pub backlight_battery_saving: Option<bool>,
    pub disabled_keys_mask: Option<u8>,
    #[serde(default)]
    pub mouse_button_remaps: Vec<SavedMouseButtonRemap>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedMapping {
    pub product_id: String,
    pub unit_id: String,
    pub from: u16,
    pub to: u16,
    pub from_name: String,
    pub to_name: String,
}

pub fn path() -> Result<PathBuf> {
    let base = BaseDirs::new().context("locating the user configuration directory")?;
    Ok(base.config_dir().join("betterTech").join("config.json"))
}

pub fn load() -> Result<Config> {
    let path = path()?;
    if !path.exists() {
        return Ok(Config::default());
    }
    let contents =
        fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    serde_json::from_str(&contents).with_context(|| format!("parsing {}", path.display()))
}

pub fn save(config: &Config) -> Result<()> {
    let path = path()?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    let contents = serde_json::to_string_pretty(config)?;
    fs::write(&path, contents).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

pub fn record(
    product_id: &str,
    unit_id: &str,
    from: u16,
    to: u16,
    from_name: &str,
    to_name: &str,
) -> Result<()> {
    let mut config = load()?;
    config.mappings.retain(|mapping| {
        !(mapping.product_id == product_id && mapping.unit_id == unit_id && mapping.from == from)
    });
    if from != to {
        config.mappings.push(SavedMapping {
            product_id: product_id.to_string(),
            unit_id: unit_id.to_string(),
            from,
            to,
            from_name: from_name.to_string(),
            to_name: to_name.to_string(),
        });
    }
    save(&config)
}

pub fn remove(product_id: &str, unit_id: &str, from: u16) -> Result<()> {
    let mut config = load()?;
    config.mappings.retain(|mapping| {
        !(mapping.product_id == product_id && mapping.unit_id == unit_id && mapping.from == from)
    });
    save(&config)
}

#[allow(dead_code)]
pub fn get_device_settings(product_id: &str, unit_id: &str) -> Result<Option<DeviceSettings>> {
    let config = load()?;
    Ok(config
        .device_settings
        .into_iter()
        .find(|s| s.product_id.eq_ignore_ascii_case(product_id) && (unit_id.is_empty() || s.unit_id.eq_ignore_ascii_case(unit_id))))
}

pub fn update_device_settings<F>(product_id: &str, unit_id: &str, mut f: F) -> Result<()>
where
    F: FnMut(&mut DeviceSettings),
{
    let mut config = load()?;
    let pos = config.device_settings.iter().position(|s| {
        s.product_id.eq_ignore_ascii_case(product_id)
            && (unit_id.is_empty() || s.unit_id.eq_ignore_ascii_case(unit_id))
    });
    let idx = match pos {
        Some(i) => i,
        None => {
            config.device_settings.push(DeviceSettings {
                product_id: product_id.to_string(),
                unit_id: unit_id.to_string(),
                ..Default::default()
            });
            config.device_settings.len() - 1
        }
    };
    f(&mut config.device_settings[idx]);
    save(&config)
}
