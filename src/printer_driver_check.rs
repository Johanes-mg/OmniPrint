use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn driver_exists(driver_name: &str) -> bool {
    let script = format!(
        "if (Get-PrinterDriver -Name '{}' -ErrorAction SilentlyContinue) {{ 'YES' }} else {{ 'NO' }}",
        driver_name
    );
    Command::new("powershell")
        .args(["-NoProfile", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).contains("YES"))
        .unwrap_or(false)
}

pub fn ensure_driver_installed(driver_name: &str) -> Result<(), String> {
    if driver_exists(driver_name) {
        Ok(())
    } else {
        Err(format!(
            "Pilote '{}' absent, imprimante virtuelle non créée.",
            driver_name
        ))
    }
}
