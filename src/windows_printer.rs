use std::process::Command;
use std::os::windows::process::CommandExt;
use std::path::PathBuf;

const CREATE_NO_WINDOW: u32 = 0x08000000;

fn run_ps(script: &str) -> Result<String, String> {
    let output = Command::new("powershell")
        .args(&["-NoProfile", "-ExecutionPolicy", "Bypass", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|e| format!("PowerShell launch failed: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

pub fn ensure_spooler_running() -> Result<(), String> {
    let script = r#"
$svc = Get-Service Spooler -ErrorAction SilentlyContinue
if ($svc.Status -ne 'Running') {
    Set-Service Spooler -StartupType Automatic
    Start-Service Spooler
}
if ((Get-Service Spooler).Status -ne 'Running') { throw 'Spooler not running' }
"#;
    run_ps(script).map(|_| ())
}

pub fn ensure_share_enabled(printer_name: &str, share_name: &str) -> Result<(), String> {
    let script = format!(r#"
$p = Get-Printer -Name '{}' -ErrorAction SilentlyContinue
if (-not $p) {{ throw 'Printer not found: {}' }}
if ($p.ShareName -ne '{}') {{
    Set-Printer -Name '{}' -Shared $true -ShareName '{}'
}}
if ((Get-Printer -Name '{}').ShareName -ne '{}') {{ throw 'Share setup failed' }}
"#, printer_name, printer_name, share_name, printer_name, share_name, printer_name, share_name);
    run_ps(&script).map(|_| ())
}

pub fn create_virtual_port(port_name: &str) -> Result<(), String> {
    let script = format!(r#"
if (-not (Get-PrinterPort -Name '{}' -ErrorAction SilentlyContinue)) {{
    Add-PrinterPort -Name '{}'
}}
"#, port_name, port_name);
    run_ps(&script).map(|_| ())
}

pub fn install_virtual_printer(printer_name: &str, driver_name: &str, port_name: &str) -> Result<(), String> {
    let script = format!(r#"
$existing = Get-Printer -Name '{}' -ErrorAction SilentlyContinue
if ($existing) {{
    if ($existing.PortName -ne '{}' -or $existing.DriverName -ne '{}') {{
        Remove-Printer -Name '{}' -ErrorAction SilentlyContinue
    }} else {{
        exit 0
    }}
}}
Add-Printer -Name '{}' -DriverName '{}' -PortName '{}'
"#, printer_name, port_name, driver_name, printer_name, printer_name, driver_name, port_name);
    run_ps(&script).map(|_| ())
}

pub fn verify_printer_ready(printer_name: &str) -> Result<bool, String> {
    let script = format!(r#"
$p = Get-Printer -Name '{}' -ErrorAction SilentlyContinue
if (-not $p) {{ 'MISSING' }}
elseif ($p.PrinterStatus -ne 'Normal') {{ 'NOT_READY:' + $p.PrinterStatus }}
else {{ 'READY' }}
"#, printer_name);
    let out = run_ps(&script)?;
    Ok(out == "READY")
}

pub fn setup_omniprint(printer_name: &str, share_name: &str, driver_name: &str, port_name: &str) -> Result<(), String> {
    ensure_spooler_running()?;
    create_virtual_port(port_name)?;
    install_virtual_printer(printer_name, driver_name, port_name)?;
    ensure_share_enabled(printer_name, share_name)?;
    if !verify_printer_ready(printer_name)? {
        return Err(format!("Printer '{}' not ready after setup", printer_name));
    }
    Ok(())
}
