use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};

use crate::journal::Statut;
use crate::session::ModeCouleur;

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn powershell_invisible(script: &str) -> std::io::Result<std::process::Output> {
    Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
}

pub fn lister_imprimantes() -> Vec<String> {
    let mut resultat = Vec::new();
    let script = "Get-Printer | Select-Object -ExpandProperty Name";
    if let Ok(out) = powershell_invisible(script) {
        if out.status.success() {
            let texte = String::from_utf8_lossy(&out.stdout);
            for ligne in texte.lines() {
                let l = ligne.trim().trim_start_matches('\u{feff}');
                if !l.is_empty() {
                    resultat.push(l.to_string());
                }
            }
        }
    }
    resultat
}

fn trouver_sumatra() -> Option<PathBuf> {
    let exe_dir = std::env::current_exe().ok()?.parent()?.to_path_buf();
    let candidats: Vec<PathBuf> = vec![
        exe_dir.join("bin").join("SumatraPDF.exe"),
        exe_dir.join("SumatraPDF.exe"),
        std::env::var("LOCALAPPDATA")
            .map(|p| PathBuf::from(p).join("SumatraPDF").join("SumatraPDF.exe"))
            .unwrap_or_default(),
        PathBuf::from(r"C:\Program Files\SumatraPDF\SumatraPDF.exe"),
        PathBuf::from(r"C:\Program Files (x86)\SumatraPDF\SumatraPDF.exe"),
    ];
    for c in candidats {
        if !c.as_os_str().is_empty() && c.exists() {
            return Some(c);
        }
    }
    if let Ok(p) = std::env::var("PATH") {
        for dir in p.split(';') {
            let cand = Path::new(dir).join("SumatraPDF.exe");
            if cand.exists() {
                return Some(cand);
            }
        }
    }
    None
}

fn trouver_edge() -> Option<PathBuf> {
    let candidats = vec![
        PathBuf::from(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"),
        PathBuf::from(r"C:\Program Files\Microsoft\Edge\Application\msedge.exe"),
    ];
    for c in candidats {
        if c.exists() {
            return Some(c);
        }
    }
    None
}

pub fn imprimer_fichier(
    nom_imprimante: &str,
    chemin_fichier: &PathBuf,
    journal_id: String,
    journal: Arc<Mutex<crate::journal::Journal>>,
    mode_couleur: ModeCouleur,
) {
    let nom_imprimante = nom_imprimante.to_string();
    let chemin = chemin_fichier.clone();
    let monochrome = matches!(mode_couleur, ModeCouleur::NoirBlanc);
    std::thread::spawn(move || {
        {
            let mut j = journal.lock().unwrap();
            j.mettre_a_jour_statut(&journal_id, Statut::EnCours, String::new());
        }

        let extension = chemin
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        let (statut, msg) = if extension == "pdf" {
            if let Some(sumatra) = trouver_sumatra() {
                let mut args: Vec<String> = vec![
                    "-print-to".into(),
                    nom_imprimante.clone(),
                ];
                if monochrome {
                    args.push("-print-settings".into());
                    args.push("monochrome".into());
                }
                args.push("-silent".into());
                args.push(chemin.to_string_lossy().to_string());
                let sortie = Command::new(&sumatra)
                    .args(&args)
                    .creation_flags(CREATE_NO_WINDOW)
                    .output();
                match sortie {
                    Ok(o) if o.status.success() => (
                        Statut::Imprime,
                        "Document transmis à l'imprimante".to_string(),
                    ),
                    Ok(o) => (
                        Statut::Erreur,
                        format!("SumatraPDF a échoué (code {})", o.status.code().unwrap_or(-1)),
                    ),
                    Err(e) => (Statut::Erreur, format!("Lancement SumatraPDF impossible : {}", e)),
                }
            } else if let Some(edge) = trouver_edge() {
                let sortie = Command::new(&edge)
                    .args([
                        "--headless",
                        &format!("--print-to-printer={}", nom_imprimante),
                        &chemin.to_string_lossy(),
                    ])
                    .creation_flags(CREATE_NO_WINDOW)
                    .output();
                match sortie {
                    Ok(o) if o.status.success() => (
                        Statut::Imprime,
                        "Document transmis à l'imprimante".to_string(),
                    ),
                    Ok(o) => (
                        Statut::Erreur,
                        format!("Edge a échoué (code {})", o.status.code().unwrap_or(-1)),
                    ),
                    Err(e) => (Statut::Erreur, format!("Lancement Edge impossible : {}", e)),
                }
            } else {
                (
                    Statut::Erreur,
                    "Aucune visionneuse PDF trouvée. Placez SumatraPDF.exe dans bin/.".to_string(),
                )
            }
        } else {
            let script = if monochrome {
                format!(
                    "$ErrorActionPreference='Stop'; try {{ Start-Process -FilePath '{}' -Verb Print -ArgumentList 'monochrome'; exit 0 }} catch {{ Start-Process -FilePath '{}' -Verb Print; exit 0 }}",
                    chemin.to_string_lossy().replace('\'', "''"),
                    chemin.to_string_lossy().replace('\'', "''"),
                )
            } else {
                format!(
                    "$ErrorActionPreference='Stop'; try {{ Start-Process -FilePath '{}' -Verb Print; exit 0 }} catch {{ Write-Output $_.Exception.Message; exit 4 }}",
                    chemin.to_string_lossy().replace('\'', "''"),
                )
            };
            match powershell_invisible(&script) {
                Ok(o) if o.status.success() => (
                    Statut::Imprime,
                    "Document transmis à l'imprimante".to_string(),
                ),
                Ok(o) => {
                    let msg = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    (
                        Statut::Erreur,
                        if msg.is_empty() {
                            "Aucune application associée à ce format sur le poste hôte.".to_string()
                        } else {
                            msg
                        },
                    )
                }
                Err(e) => (Statut::Erreur, format!("Erreur système : {}", e)),
            }
        };

        let mut j = journal.lock().unwrap();
        j.mettre_a_jour_statut(&journal_id, statut, msg);
    });
}

pub fn compter_pages_pdf(chemin: &PathBuf) -> u32 {
    let data = match std::fs::read(chemin) {
        Ok(d) => d,
        Err(_) => return 1,
    };
    let contenu = String::from_utf8_lossy(&data);
    let mut max = 1u32;
    for mot in contenu.split_whitespace() {
        if let Some(rest) = mot.strip_prefix("/Count") {
            let cleaned: String = rest.chars().filter(|c| c.is_ascii_digit()).collect();
            if let Ok(n) = cleaned.parse::<u32>() {
                if n > max {
                    max = n;
                }
            }
        }
    }
    max
}
