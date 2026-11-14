use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use crate::elevation;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn nom_pc() -> String {
    std::env::var("COMPUTERNAME").unwrap_or_else(|_| "PC".into())
}

pub fn chemin_unc(nom_partage: &str) -> String {
    format!("\\\\{}\\{}", nom_pc(), nom_partage)
}

fn powershell(script: &str) -> std::io::Result<std::process::Output> {
    Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-WindowStyle",
            "Hidden",
            "-Command",
            script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
}

fn echapper(v: &str) -> String {
    v.replace('\'', "''")
}

fn est_eleve() -> bool {
    let script = "([Security.Principal.WindowsPrincipal][Security.Principal.WindowsIdentity]::GetCurrent()).IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)";
    match powershell(script) {
        Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout)
            .trim()
            .eq_ignore_ascii_case("True"),
        _ => false,
    }
}

pub fn partage_actif(nom_imprimante: &str) -> bool {
    if nom_imprimante.is_empty() {
        return false;
    }
    let nom = echapper(nom_imprimante);
    let script = format!(
        "$p = Get-Printer -Name '{}' -ErrorAction SilentlyContinue; if ($p -ne $null -and $p.Shared -eq $true) {{ 'OUI' }} else {{ 'NON' }}",
        nom
    );
    if let Ok(o) = powershell(&script) {
        if o.status.success() {
            return String::from_utf8_lossy(&o.stdout).contains("OUI");
        }
    }
    false
}

fn script_activer(imprimante: &str, nom_partage: &str) -> String {
    let imp = echapper(imprimante);
    let part = echapper(nom_partage);
    format!(
        r#"$ErrorActionPreference = 'Stop'
try {{
    $svc = Get-Service Spooler -ErrorAction SilentlyContinue
    if ($svc -ne $null -and $svc.Status -ne 'Running') {{
        Set-Service Spooler -StartupType Automatic -ErrorAction SilentlyContinue
        Start-Service Spooler -ErrorAction SilentlyContinue
    }}

    $profils = Get-NetConnectionProfile -ErrorAction SilentlyContinue
    if ($profils -ne $null) {{
        foreach ($pf in $profils) {{
            if ($pf.NetworkCategory -ne 'Private') {{
                Set-NetConnectionProfile -InterfaceIndex $pf.InterfaceIndex -NetworkCategory Private -ErrorAction SilentlyContinue
            }}
        }}
    }}

    Get-NetFirewallRule -DisplayGroup 'Network Discovery' -ErrorAction SilentlyContinue | Where-Object {{ $_.Profile -match 'Private|Domain' }} | Enable-NetFirewallRule -ErrorAction SilentlyContinue
    Get-NetFirewallRule -DisplayGroup 'File and Printer Sharing' -ErrorAction SilentlyContinue | Where-Object {{ $_.Profile -match 'Private|Domain' }} | Enable-NetFirewallRule -ErrorAction SilentlyContinue

    Set-SmbServerConfiguration -EnableSMB2Protocol $true -Force -ErrorAction SilentlyContinue

    $p = Get-Printer -Name '{imp}' -ErrorAction Stop
    if ($p.Shared) {{ Set-Printer -Name '{imp}' -Shared $false -ErrorAction SilentlyContinue }}
    Set-Printer -Name '{imp}' -Shared $true -ShareName '{part}' -Published $true -ErrorAction Stop

    try {{ Grant-SmbShareAccess -Name '{part}' -AccountName 'Everyone' -AccessRight Full -Force -ErrorAction SilentlyContinue | Out-Null }} catch {{}}
    try {{ Grant-SmbShareAccess -Name '{part}' -AccountName 'ANONYMOUS LOGON' -AccessRight Read -Force -ErrorAction SilentlyContinue | Out-Null }} catch {{}}

    try {{
        if ($p.DriverName) {{
            Add-PrinterDriver -Name $p.DriverName -ErrorAction SilentlyContinue | Out-Null
        }}
    }} catch {{}}

    'OK'
}} catch {{
    Write-Output ('ERREUR: ' + $_.Exception.Message)
    exit 1
}}"#
    )
}

fn ecrire_ps1(script: &str) -> Result<PathBuf, String> {
    let chemin = std::env::temp_dir().join(format!(
        "omniprint_share_{}.ps1",
        uuid::Uuid::new_v4()
    ));
    std::fs::write(&chemin, script.as_bytes())
        .map_err(|e| format!("Ecriture du script de partage impossible : {}", e))?;
    Ok(chemin)
}

pub fn activer_partage(imprimante: &str, nom_partage: &str) -> Result<(), String> {
    if imprimante.is_empty() {
        return Err("Aucune imprimante sélectionnée.".into());
    }
    if nom_partage.is_empty() {
        return Err("Nom de partage vide.".into());
    }

    let script = script_activer(imprimante, nom_partage);

    if est_eleve() {
        match powershell(&script) {
            Ok(o) if o.status.success() => {
                if partage_actif(imprimante) {
                    Ok(())
                } else {
                    Err("Le partage n'a pas pu être confirmé.".into())
                }
            }
            Ok(o) => {
                let msg = String::from_utf8_lossy(&o.stdout).trim().to_string();
                Err(if msg.is_empty() {
                    "Le script de partage a échoué.".into()
                } else {
                    msg
                })
            }
            Err(e) => Err(format!("PowerShell indisponible : {}", e)),
        }
    } else {
        let ps1 = ecrire_ps1(&script)?;
        let resultat = elevation::executer_en_admin(&ps1);
        let _ = std::fs::remove_file(&ps1);
        resultat?;
        if partage_actif(imprimante) {
            Ok(())
        } else {
            Err("Élévation acceptée mais partage non confirmé.".into())
        }
    }
}
