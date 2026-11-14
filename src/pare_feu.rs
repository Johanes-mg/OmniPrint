use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

use crate::elevation;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

pub fn assurer_regle(port: u16) -> Result<(), String> {
    let exe = std::env::current_exe()
        .map_err(|e| format!("Chemin de l'exécutable introuvable : {}", e))?;
    let exe_str = exe.to_string_lossy().to_string();

    if regle_existe(port, &exe_str) {
        return Ok(());
    }

    let bat = ecrire_bat(&exe_str, port)?;
    let resultat = elevation::executer_en_admin(&bat);
    let _ = std::fs::remove_file(&bat);
    resultat?;

    if regle_existe(port, &exe_str) {
        Ok(())
    } else {
        Err("Règle non créée après élévation.".into())
    }
}

fn ecrire_bat(exe: &str, port: u16) -> Result<PathBuf, String> {
    let chemin = std::env::temp_dir().join(format!(
        "omniprint_fw_{}.bat",
        uuid::Uuid::new_v4()
    ));
    let exe_propre = exe.replace('"', "");
    let contenu = format!(
        "@echo off\r\nnetsh advfirewall firewall delete rule name=OmniPrint >nul 2>&1\r\nnetsh advfirewall firewall add rule name=OmniPrint dir=in action=allow program=\"{}\" protocol=TCP localport={} enable=yes profile=private,domain\r\n",
        exe_propre, port
    );
    std::fs::write(&chemin, contenu.as_bytes())
        .map_err(|e| format!("Ecriture du script pare-feu impossible : {}", e))?;
    Ok(chemin)
}

fn regle_existe(port: u16, exe: &str) -> bool {
    let sortie = Command::new("netsh")
        .args(["advfirewall", "firewall", "show", "rule", "name=OmniPrint"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    let Ok(o) = sortie else { return false; };
    if !o.status.success() {
        return false;
    }
    let texte = String::from_utf8_lossy(&o.stdout);
    let port_ok = texte.lines().any(|l| {
        let l = l.trim_start();
        if let Some(rest) = l.strip_prefix("LocalPort:") {
            rest.trim() == port.to_string()
        } else {
            false
        }
    });
    let exe_ok = texte.lines().any(|l| {
        let l = l.trim_start();
        if let Some(rest) = l.strip_prefix("Program:") {
            rest.trim().eq_ignore_ascii_case(exe)
        } else {
            false
        }
    });
    port_ok && exe_ok
}

pub fn supprimer_toutes_regles() {
    let _ = Command::new("netsh")
        .args(["advfirewall", "firewall", "delete", "rule", "name=OmniPrint"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
}
