use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn assurer_regle() -> Result<(), String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("Chemin de l'executable introuvable : {}", e))?;
    let exe_str = exe.to_string_lossy().to_string();

    if regle_existe() {
        return Ok(());
    }

    if ajouter_regle_directe(&exe_str).is_ok() {
        return Ok(());
    }

    ajouter_regle_uac(&exe_str)
}

fn regle_existe() -> bool {
    let sortie = Command::new("netsh")
        .args([
            "advfirewall",
            "firewall",
            "show",
            "rule",
            "name=OmniPrint",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    matches!(sortie, Ok(o) if o.status.success())
}

fn ajouter_regle_directe(exe: &str) -> Result<(), String> {
    let prog_arg = format!("program=\"{}\"", exe);
    let sortie = Command::new("netsh")
        .args([
            "advfirewall",
            "firewall",
            "add",
            "rule",
            "name=OmniPrint",
            "dir=in",
            "action=allow",
            &prog_arg,
            "enable=yes",
            "profile=private,domain",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    match sortie {
        Ok(o) if o.status.success() => Ok(()),
        Ok(_) => Err("Droits administrateur requis".into()),
        Err(e) => Err(format!("netsh indisponible : {}", e)),
    }
}

fn ajouter_regle_uac(exe: &str) -> Result<(), String> {
    let prog_arg = format!("program=\"{}\"", exe.replace('"', ""));
    let script = format!(
        "try {{ Start-Process -FilePath 'netsh' -ArgumentList 'advfirewall','firewall','add','rule','name=OmniPrint','dir=in','action=allow','{}','enable=yes','profile=private,domain' -Verb RunAs -Wait -WindowStyle Hidden; exit 0 }} catch {{ Write-Output $_.Exception.Message; exit 1 }}",
        prog_arg.replace('\'', "''"),
    );
    let sortie = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    match sortie {
        Ok(o) if o.status.success() => {
            if regle_existe() {
                Ok(())
            } else {
                Err("Regle non creee apres elevation".into())
            }
        }
        Ok(o) => {
            let msg = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if msg.is_empty() {
                Err("Demande d'elevation refusee".into())
            } else {
                Err(msg)
            }
        }
        Err(e) => Err(format!("PowerShell indisponible : {}", e)),
    }
}
