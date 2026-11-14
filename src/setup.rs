use std::path::PathBuf;

use crate::elevation;
use crate::pare_feu;
use crate::partage;

pub struct ResultatConfig {
    pub pare_feu: Result<(), String>,
    pub partage: Result<(), String>,
    pub elevation_demandee: bool,
}

pub fn preparer(port: u16, imprimante: &str, nom_partage: &str, forcer: bool) -> ResultatConfig {
    let exe = match std::env::current_exe() {
        Ok(e) => e.to_string_lossy().to_string(),
        Err(e) => {
            let msg = format!("Exécutable introuvable : {}", e);
            return ResultatConfig {
                pare_feu: Err(msg.clone()),
                partage: Err(msg),
                elevation_demandee: false,
            };
        }
    };

    let fw_ok = !forcer && pare_feu::regle_existe(port, &exe);
    let partage_ok = imprimante.is_empty()
        || (!forcer && partage::partage_existe(imprimante, nom_partage));

    if fw_ok && partage_ok {
        return ResultatConfig {
            pare_feu: Ok(()),
            partage: Ok(()),
            elevation_demandee: false,
        };
    }

    let script = construire_script(&exe, port, imprimante, nom_partage, fw_ok, partage_ok);
    let ps1 = ecrire_ps1(&script);
    let elev = match &ps1 {
        Ok(p) => elevation::executer_en_admin(p),
        Err(e) => Err(e.clone()),
    };
    if let Ok(p) = &ps1 {
        let _ = std::fs::remove_file(p);
    }

    let fw_final = if pare_feu::regle_existe(port, &exe) {
        Ok(())
    } else {
        Err(elev
            .clone()
            .err()
            .unwrap_or_else(|| "Règle pare-feu non créée.".into()))
    };
    let partage_final = if imprimante.is_empty() || partage::partage_existe(imprimante, nom_partage) {
        Ok(())
    } else {
        Err(elev
            .clone()
            .err()
            .unwrap_or_else(|| "Partage non confirmé.".into()))
    };

    ResultatConfig {
        pare_feu: fw_final,
        partage: partage_final,
        elevation_demandee: true,
    }
}

fn construire_script(
    exe: &str,
    port: u16,
    imprimante: &str,
    nom_partage: &str,
    fw_ok: bool,
    partage_ok: bool,
) -> String {
    let exe_p = exe.replace('\'', "''");
    let imp = imprimante.replace('\'', "''");
    let part = nom_partage.replace('\'', "''");

    let mut l = vec![
        "$ErrorActionPreference = 'Stop'".to_string(),
        "try {".to_string(),
    ];

    if !fw_ok {
        l.push("  Remove-NetFirewallRule -DisplayName 'OmniPrint' -ErrorAction SilentlyContinue".to_string());
        l.push(format!(
            "  New-NetFirewallRule -DisplayName 'OmniPrint' -Direction Inbound -Action Allow -Program '{}' -Protocol TCP -LocalPort {} -Profile Any -Enabled True | Out-Null",
            exe_p, port
        ));
        l.push("  Get-NetFirewallRule -Group '*-28502*' | Where-Object { $_.Profile -match 'Any|Private|Domain|Public' } | Set-NetFirewallRule -Enabled True -Profile Any -ErrorAction SilentlyContinue".to_string());
        l.push("  Get-NetFirewallRule -Group '*-32752*' | Where-Object { $_.Profile -match 'Any|Private|Domain|Public' } | Set-NetFirewallRule -Enabled True -Profile Any -ErrorAction SilentlyContinue".to_string());
    }

    l.push("  Set-ItemProperty -Path 'HKLM:\\SYSTEM\\CurrentControlSet\\Control\\Lsa' -Name 'LimitBlankPasswordUse' -Value 0 -Type DWord -Force".to_string());
    l.push("  $path = 'HKLM:\\SOFTWARE\\Policies\\Microsoft\\Windows\\LanmanWorkstation'".to_string());
    l.push("  if (-not (Test-Path $path)) { New-Item -Path $path -Force | Out-Null }".to_string());
    l.push("  Set-ItemProperty -Path $path -Name 'AllowInsecureGuestAuth' -Value 1 -Type DWord -Force".to_string());
    l.push("  $path2 = 'HKLM:\\SYSTEM\\CurrentControlSet\\Services\\LanmanWorkstation\\Parameters'".to_string());
    l.push("  Set-ItemProperty -Path $path2 -Name 'RequireSecuritySignature' -Value 0 -Type DWord -Force".to_string());
    l.push("  Set-SmbClientConfiguration -RequireSecuritySignature $false -Force -ErrorAction SilentlyContinue".to_string());
    l.push("  Set-SmbServerConfiguration -RequireSecuritySignature $false -EncryptData $false -Force -ErrorAction SilentlyContinue".to_string());

    if !imprimante.is_empty() && !partage_ok {
        l.push("  $svc = Get-Service Spooler -ErrorAction SilentlyContinue".to_string());
        l.push("  if ($svc -and $svc.Status -ne 'Running') { Set-Service Spooler -StartupType Automatic -ErrorAction SilentlyContinue; Start-Service Spooler -ErrorAction SilentlyContinue }".to_string());
        l.push("  Get-NetConnectionProfile -ErrorAction SilentlyContinue | ForEach-Object { if ($_.NetworkCategory -ne 'Private') { Set-NetConnectionProfile -InterfaceIndex $_.InterfaceIndex -NetworkCategory Private -ErrorAction SilentlyContinue } }".to_string());
        l.push("  Set-SmbServerConfiguration -EnableSMB2Protocol $true -Force -ErrorAction SilentlyContinue".to_string());
        l.push(format!(
            "  Get-Printer -ErrorAction SilentlyContinue | Where-Object {{ $_.Shared -and $_.ShareName -eq '{}' -and $_.Name -ne '{}' }} | ForEach-Object {{ Set-Printer -Name $_.Name -Shared $false -ErrorAction SilentlyContinue }}",
            part, imp
        ));
        l.push(format!("  $p = Get-Printer -Name '{}' -ErrorAction Stop", imp));
        l.push(format!(
            "  if (-not $p.Shared -or $p.ShareName -ne '{}') {{ Set-Printer -Name '{}' -Shared $true -ShareName '{}' -Published $true -ErrorAction Stop }}",
            part, imp, part
        ));
        l.push(format!(
            "  Grant-SmbShareAccess -Name '{}' -AccountName 'Everyone' -AccessRight Full -Force -ErrorAction SilentlyContinue | Out-Null",
            part
        ));
        l.push("  icacls 'C:\\Windows\\System32\\spool\\PRINTERS' /grant '*S-1-1-0:(OI)(CI)F' /T /C /Q 2>$null | Out-Null".to_string());
    }

    l.push("  Write-Output 'OK'".to_string());
    l.push("  exit 0".to_string());
    l.push("} catch {".to_string());
    l.push("  Write-Output ('ERREUR: ' + $_.Exception.Message)".to_string());
    l.push("  exit 1".to_string());
    l.push("}".to_string());

    l.join("\r\n")
}

fn ecrire_ps1(script: &str) -> Result<PathBuf, String> {
    let chemin = std::env::temp_dir().join(format!(
        "omniprint_setup_{}.ps1",
        uuid::Uuid::new_v4()
    ));
    std::fs::write(&chemin, script.as_bytes())
        .map_err(|e| format!("Ecriture script impossible : {}", e))?;
    Ok(chemin)
}
