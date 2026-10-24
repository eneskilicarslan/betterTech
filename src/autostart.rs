use anyhow::{Context, Result};
use std::process::Command;

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const APP_NAME: &str = "betterTech";

pub fn is_enabled() -> bool {
    let output = Command::new("reg.exe")
        .args(["query", RUN_KEY, "/v", APP_NAME])
        .output();
    matches!(output, Ok(out) if out.status.success())
}

pub fn set_enabled(enabled: bool) -> Result<()> {
    if enabled {
        let exe = std::env::current_exe().context("retrieving executable path")?;
        let exe_str = exe.to_string_lossy();
        let cmd_val = format!("\"{exe_str}\" gui --minimized");
        let status = Command::new("reg.exe")
            .args([
                "add",
                RUN_KEY,
                "/v",
                APP_NAME,
                "/t",
                "REG_SZ",
                "/d",
                &cmd_val,
                "/f",
            ])
            .status()
            .context("executing reg.exe add")?;
        if !status.success() {
            anyhow::bail!("reg.exe add failed with status: {status}");
        }
    } else {
        let _ = Command::new("reg.exe")
            .args(["delete", RUN_KEY, "/v", APP_NAME, "/f"])
            .output();
    }
    Ok(())
}
