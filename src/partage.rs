use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn nom_pc() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "PC".into())
}

pub fn chemin_unc(nom_partage: &str) -> String {
    format!("\\\\{}\\{}", nom_pc(), nom_partage)
}

pub fn partage_existe(imprimante: &str, nom_partage: &str) -> bool {
    if imprimante.is_empty() {
        return false;
    }
    let imp = imprimante.replace('\'', "''");
    let part = nom_partage.replace('\'', "''");
    let script = format!(
        "$p = Get-Printer -Name '{}' -ErrorAction SilentlyContinue; \
         if ($p -and $p.Shared -and $p.ShareName -eq '{}') {{ 'OUI' }} else {{ 'NON' }}",
        imp, part
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
