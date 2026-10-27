use anyhow::{Context, Result};
#[cfg(windows)]
use std::os::windows::process::CommandExt;
use std::process::Command;

const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const APP_NAME: &str = "betterTech";
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn reg_command() -> Command {
    let mut cmd = Command::new("reg.exe");
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

pub fn is_enabled() -> bool {
    let output = reg_command()
        .args(["query", RUN_KEY, "/v", APP_NAME])
        .output();
    matches!(output, Ok(out) if out.status.success())
}

pub fn set_enabled(enabled: bool) -> Result<()> {
    if enabled {
        let exe = std::env::current_exe().context("retrieving executable path")?;
        let exe_str = exe.to_string_lossy();
        let cmd_val = format!("\"{exe_str}\" gui --minimized");
        let status = reg_command()
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
        let _ = reg_command()
            .args(["delete", RUN_KEY, "/v", APP_NAME, "/f"])
            .output();
    }
    Ok(())
}
