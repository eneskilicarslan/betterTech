const CONTROLS: &[(u16, &str)] = &[
    (0x0001, "Volume Up (old)"),
    (0x0002, "Volume Down (old)"),
    (0x0003, "Mute"),
    (0x0004, "Play/Pause (old)"),
    (0x0005, "Next"),
    (0x0006, "Previous"),
    (0x0007, "Stop"),
    (0x000a, "Calculator"),
    (0x0034, "Fn Key"),
    (0x0050, "Left Button"),
    (0x0051, "Right Button"),
    (0x0052, "Middle Button"),
    (0x0053, "Back Button"),
    (0x0056, "Forward Button"),
    (0x006e, "Show Desktop"),
    (0x006f, "Screen Lock"),
    (0x00c7, "Brightness Down"),
    (0x00c8, "Brightness Up"),
    (0x00d1, "Host Switch Channel 1"),
    (0x00d2, "Host Switch Channel 2"),
    (0x00d3, "Host Switch Channel 3"),
    (0x00d4, "Multiplatform Search"),
    (0x00de, "F Lock"),
    (0x00e2, "Backlight Down"),
    (0x00e3, "Backlight Up"),
    (0x00e4, "Previous Track"),
    (0x00e5, "Play/Pause"),
    (0x00e6, "Next Track"),
    (0x00e7, "Mute Sound"),
    (0x00e8, "Volume Down"),
    (0x00e9, "Volume Up"),
    (0x0103, "Dictation"),
    (0x0108, "Emoji"),
    (0x010a, "Screen Capture"),
    (0x010b, "Grave Accent"),
    (0x010c, "Tab"),
    (0x010d, "Caps Lock"),
    (0x010e, "Left Shift"),
    (0x010f, "Left Control"),
    (0x0110, "Left Option/Start"),
    (0x0111, "Left Command/Alt"),
    (0x0112, "Right Command/Alt"),
    (0x0113, "Right Option/Start"),
    (0x0114, "Right Control"),
    (0x0115, "Right Shift"),
    (0x0116, "Insert"),
    (0x0117, "Delete"),
    (0x0118, "Home"),
    (0x0119, "End"),
    (0x011a, "Page Up"),
    (0x011b, "Page Down"),
    (0x011c, "Mute Microphone"),
    (0x011e, "Backslash"),
];

pub fn control_name(cid: u16) -> String {
    CONTROLS
        .iter()
        .find(|(value, _)| *value == cid)
        .map(|(_, name)| (*name).to_string())
        .unwrap_or_else(|| format!("CID 0x{cid:04X}"))
}

pub fn lookup_control(input: &str) -> Option<u16> {
    let trimmed = input.trim();
    if let Some(hex) = trimmed
        .strip_prefix("0x")
        .or_else(|| trimmed.strip_prefix("0X"))
    {
        return u16::from_str_radix(hex, 16).ok();
    }
    if !trimmed.is_empty()
        && trimmed.chars().all(|c| c.is_ascii_hexdigit())
        && let Ok(value) = u16::from_str_radix(trimmed, 16)
    {
        return Some(value);
    }
    let normalized = normalize(trimmed);
    CONTROLS
        .iter()
        .find(|(_, name)| normalize(name) == normalized)
        .map(|(value, _)| *value)
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn looks_up_control_names_and_cids() {
        assert_eq!(lookup_control("Calculator"), Some(0x000A));
        assert_eq!(lookup_control("volume up"), Some(0x00E9));
        assert_eq!(lookup_control("0x00E9"), Some(0x00E9));
    }
}
