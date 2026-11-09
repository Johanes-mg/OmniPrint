use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn assurer_regle(port: u16) -> Result<(), String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("Chemin de l'exécutable introuvable : {}", e))?;
    let exe_str = exe.to_string_lossy().to_string();

    if regle_existe(port) {
        return Ok(());
    }

    let _ = supprimer_regles();

    if ajouter_regle_directe(&exe_str, port).is_ok() {
        if regle_existe(port) {
            return Ok(());
        }
    }

    ajouter_regle_uac(&exe_str, port)
}

fn regle_existe(port: u16) -> bool {
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
    let Ok(o) = sortie else { return false; };
    if !o.status.success() {
        return false;
    }
    let texte = String::from_utf8_lossy(&o.stdout).to_lowercase();
    let port_str = format!("localport:{}", port);
    texte.contains(&port_str) || texte.contains(&format!("{}", port))
}

fn supprimer_regles() -> Result<(), String> {
    let sortie = Command::new("netsh")
        .args([
            "advfirewall",
            "firewall",
            "delete",
            "rule",
            "name=OmniPrint",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    match sortie {
        Ok(_) => Ok(()),
        Err(e) => Err(format!("netsh indisponible : {}", e)),
    }
}

fn ajouter_regle_directe(exe: &str, port: u16) -> Result<(), String> {
    let prog_arg = format!("program=\"{}\"", exe);
    let port_arg = format!("localport={}", port);
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
            "protocol=TCP",
            &port_arg,
            "enable=yes",
            "profile=private,domain,public",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    match sortie {
        Ok(o) if o.status.success() => Ok(()),
        Ok(_) => Err("Droits administrateur requis".into()),
        Err(e) => Err(format!("netsh indisponible : {}", e)),
    }
}

fn ajouter_regle_uac(exe: &str, port: u16) -> Result<(), String> {
    let prog_arg = format!("program=\"{}\"", exe.replace('"', ""));
    let port_arg = format!("localport={}", port);
    let script = format!(
        "try {{ Start-Process -FilePath 'netsh' -ArgumentList 'advfirewall','firewall','add','rule','name=OmniPrint','dir=in','action=allow','{}','protocol=TCP','{}','enable=yes','profile=private,domain,public' -Verb RunAs -Wait -WindowStyle Hidden; exit 0 }} catch {{ Write-Output $_.Exception.Message; exit 1 }}",
        prog_arg.replace('\'', "''"),
        port_arg
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
            if regle_existe(port) {
                Ok(())
            } else {
                Err("Règle non créée après élévation".into())
            }
        }
        Ok(o) => {
            let msg = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if msg.is_empty() {
                Err("Demande d'élévation refusée".into())
            } else {
                Err(msg)
            }
        }
        Err(e) => Err(format!("PowerShell indisponible : {}", e)),
    }
}

pub fn supprimer_toutes_regles() {
    let _ = Command::new("netsh")
        .args([
            "advfirewall",
            "firewall",
            "delete",
            "rule",
            "name=OmniPrint",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}
