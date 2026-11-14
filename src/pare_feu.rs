use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn regle_existe(port: u16, exe: &str) -> bool {
    let exe_s = exe.replace('\'', "''");
    let script = format!(
        "$r = Get-NetFirewallRule -DisplayName 'OmniPrint' -ErrorAction SilentlyContinue; \
         if ($r) {{ \
           $pf = $r | Get-NetFirewallPortFilter | Where-Object {{ $_.LocalPort -eq '{}' }}; \
           $af = $r | Get-NetFirewallApplicationFilter | Where-Object {{ $_.Program -eq '{}' }}; \
           if ($pf -and $af) {{ 'OUI' }} else {{ 'NON' }} \
         }} else {{ 'NON' }}",
        port, exe_s
    );
    let out = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    match out {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).contains("OUI"),
        _ => false,
    }
}
