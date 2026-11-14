use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::journal::Statut;
use crate::session::{ModeCouleur, Orientation};

const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const TIMEOUT_COM: u64 = 90;

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

fn tuer_processus(noms: &[&str]) {
    for nom in noms {
        let _ = Command::new("taskkill")
            .args(["/F", "/IM", nom])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
    }
}

pub fn annuler_impression() {
    tuer_processus(&["SumatraPDF.exe"]);
}

pub fn trouver_sumatra() -> Option<PathBuf> {
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

fn est_document_word(extension: &str) -> bool {
    matches!(
        extension,
        "doc" | "docx" | "docm" | "dot" | "dotx" | "dotm" | "rtf" | "odt"
    )
}

fn est_document_excel(extension: &str) -> bool {
    matches!(
        extension,
        "xlsx" | "xlsm" | "xltx" | "xltm" | "xls" | "xlsb" | "ods" | "csv"
    )
}

fn echapper_chaine_powershell(valeur: &str) -> String {
    valeur.replace('\'', "''")
}

fn convertir_office_en_pdf(chemin: &Path) -> Result<PathBuf, String> {
    let destination = std::env::temp_dir().join(format!(
        "omniprint_office_{}.pdf",
        uuid::Uuid::new_v4()
    ));
    let resultat = office2pdf::convert(chemin)
        .map_err(|e| format!("Conversion Office en PDF impossible : {:?}", e))?;
    std::fs::write(&destination, &resultat.pdf)
        .map_err(|e| format!("Ecriture PDF temporaire impossible : {}", e))?;
    Ok(destination)
}

fn rendre_page_pdf_pure(pdf: &Path, page: u32, sortie: &Path) -> Result<(), String> {
    use hayro::hayro_interpret::InterpreterSettings;
    use hayro::hayro_syntax::Pdf;
    use hayro::{render, RenderSettings};

    let donnees = std::fs::read(pdf).map_err(|e| format!("Lecture PDF impossible : {}", e))?;
    let data: std::sync::Arc<dyn AsRef<[u8]> + Send + Sync> = std::sync::Arc::new(donnees);
    let pdf_doc = Pdf::new(data).map_err(|e| format!("Parsing PDF impossible : {:?}", e))?;
    let pages = pdf_doc.pages();
    let index = (page as usize).saturating_sub(1);
    let page_ref = pages
        .get(index)
        .ok_or_else(|| format!("Page {} introuvable", page))?;

    let settings = RenderSettings {
        x_scale: 96.0 / 72.0,
        y_scale: 96.0 / 72.0,
        ..Default::default()
    };

    let pixmap = render(page_ref, &InterpreterSettings::default(), &settings);

    let largeur = pixmap.width() as u32;
    let hauteur = pixmap.height() as u32;
    let data_rgba = pixmap.data_as_u8_slice();
    let image = image::RgbaImage::from_raw(largeur, hauteur, data_rgba.to_vec())
        .ok_or_else(|| "Conversion Pixmap -> RgbaImage impossible".to_string())?;
    image
        .save(sortie)
        .map_err(|e| format!("Ecriture PNG impossible : {}", e))?;
    Ok(())
}

pub fn generer_apercu(chemin_fichier: &PathBuf, page: u32) -> Result<PathBuf, String> {
    let extension = chemin_fichier
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    let (chemin_pdf, est_temporaire) = if extension == "pdf" {
        (chemin_fichier.clone(), false)
    } else if est_document_word(&extension) || est_document_excel(&extension) {
        (convertir_office_en_pdf(chemin_fichier)?, true)
    } else {
        return Err("Format non supporté pour l'aperçu.".into());
    };

    let fichier_sortie = std::env::temp_dir().join(format!("omniprint_apercu_{}.png", uuid::Uuid::new_v4()));
    let resultat = rendre_page_pdf_pure(&chemin_pdf, page, &fichier_sortie);

    if est_temporaire {
        let _ = std::fs::remove_file(&chemin_pdf);
    }

    resultat.map(|_| fichier_sortie)
}

pub fn imprimer_fichier(
    nom_imprimante: &str,
    chemin_fichier: &PathBuf,
    journal_id: String,
    journal: Arc<Mutex<crate::journal::Journal>>,
    mode_couleur: ModeCouleur,
    orientation: Orientation,
    pages_spec: String,
    copies: u32,
    _est_feuille: bool,
) {
    let nom_imprimante = nom_imprimante.to_string();
    let chemin = chemin_fichier.clone();
    let monochrome = matches!(mode_couleur, ModeCouleur::NoirBlanc);
    let copies = if copies == 0 { 1 } else { copies };

    std::thread::spawn(move || {
        {
            let mut j = journal.lock().unwrap();
            j.mettre_a_jour_statut(&journal_id, Statut::EnCours, String::new());
        }

        let extension = chemin
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();

        let resultat = if est_document_excel(&extension) {
            let pdf = match convertir_office_en_pdf(&chemin) {
                Ok(p) => p,
                Err(e) => {
                    let mut j = journal.lock().unwrap();
                    j.mettre_a_jour_statut(&journal_id, Statut::Erreur, e);
                    return;
                }
            };
            let res = if let Some(sumatra) = trouver_sumatra() {
                let mut args = vec!["-print-to".to_string(), nom_imprimante.clone()];
                let mut reglages = Vec::new();
                if monochrome {
                    reglages.push("monochrome".to_string());
                }
                if copies > 1 {
                    reglages.push(format!("{}x", copies));
                }
                if !pages_spec.trim().is_empty() {
                    reglages.push(pages_spec.trim().to_string());
                }
                match orientation {
                    Orientation::Portrait => reglages.push("portrait".to_string()),
                    Orientation::Paysage => reglages.push("landscape".to_string()),
                }
                if !reglages.is_empty() {
                    args.push("-print-settings".into());
                    args.push(reglages.join(","));
                }
                args.push("-silent".into());
                args.push(pdf.to_string_lossy().to_string());
                match Command::new(&sumatra).args(&args).creation_flags(CREATE_NO_WINDOW).output() {
                    Ok(o) if o.status.success() => Ok("Document transmis a l'imprimante".to_string()),
                    Ok(o) => Err(format!("SumatraPDF a echoue (code {}).", o.status.code().unwrap_or(-1))),
                    Err(e) => Err(format!("Lancement de SumatraPDF impossible : {}", e)),
                }
            } else {
                Err("SumatraPDF est introuvable.".into())
            };
            let _ = std::fs::remove_file(&pdf);
            res
        } else {
            let (chemin_pdf, pdf_temporaire) = if extension == "pdf" {
                (Some(chemin.clone()), false)
            } else if est_document_word(&extension) {
                match convertir_office_en_pdf(&chemin) {
                    Ok(pdf) => (Some(pdf), true),
                    Err(e) => {
                        let mut j = journal.lock().unwrap();
                        j.mettre_a_jour_statut(&journal_id, Statut::Erreur, e);
                        return;
                    }
                }
            } else {
                (None, false)
            };

            let res = if let Some(pdf) = chemin_pdf.as_ref() {
                if let Some(sumatra) = trouver_sumatra() {
                    let mut args = vec!["-print-to".to_string(), nom_imprimante.clone()];
                    let mut reglages = Vec::new();
                    if monochrome {
                        reglages.push("monochrome".to_string());
                    }
                    if copies > 1 {
                        reglages.push(format!("{}x", copies));
                    }
                    if !pages_spec.trim().is_empty() {
                        reglages.push(pages_spec.trim().to_string());
                    }
                    match orientation {
                        Orientation::Portrait => reglages.push("portrait".to_string()),
                        Orientation::Paysage => reglages.push("landscape".to_string()),
                    }
                    if !reglages.is_empty() {
                        args.push("-print-settings".into());
                        args.push(reglages.join(","));
                    }
                    args.push("-silent".into());
                    args.push(pdf.to_string_lossy().to_string());
                    match Command::new(&sumatra).args(&args).creation_flags(CREATE_NO_WINDOW).output() {
                        Ok(o) if o.status.success() => Ok("Document transmis a l'imprimante".to_string()),
                        Ok(o) => Err(format!("SumatraPDF a echoue (code {}).", o.status.code().unwrap_or(-1))),
                        Err(e) => Err(format!("Lancement de SumatraPDF impossible : {}", e)),
                    }
                } else {
                    Err("SumatraPDF est introuvable.".into())
                }
            } else {
                let chemin_ps = echapper_chaine_powershell(&chemin.to_string_lossy());
                let script = if monochrome {
                    format!(
                        "$ErrorActionPreference='Stop'; try {{ Start-Process -FilePath '{0}' -Verb Print -ArgumentList 'monochrome'; exit 0 }} catch {{ Start-Process -FilePath '{0}' -Verb Print; exit 0 }}",
                        chemin_ps,
                    )
                } else {
                    format!(
                        "$ErrorActionPreference='Stop'; try {{ Start-Process -FilePath '{0}' -Verb Print; exit 0 }} catch {{ Write-Output $_.Exception.Message; exit 4 }}",
                        chemin_ps,
                    )
                };
                match powershell_invisible(&script) {
                    Ok(o) if o.status.success() => Ok("Document transmis a l'imprimante".to_string()),
                    Ok(o) => {
                        let msg = String::from_utf8_lossy(&o.stdout).trim().to_string();
                        Err(if msg.is_empty() { "Aucune application associee.".to_string() } else { msg })
                    }
                    Err(e) => Err(format!("Erreur systeme : {}", e)),
                }
            };

            if pdf_temporaire {
                if let Some(pdf) = chemin_pdf {
                    let _ = std::fs::remove_file(pdf);
                }
            }
            res
        };

        let (statut, message) = match resultat {
            Ok(message) => (Statut::Imprime, message),
            Err(message) => (Statut::Erreur, message),
        };
        let mut j = journal.lock().unwrap();
        j.mettre_a_jour_statut(&journal_id, statut, message);
    });
}

pub fn compter_pages_pdf(chemin: &PathBuf) -> u32 {
    if let Ok(document) = lopdf::Document::load(chemin) {
        let nombre = document.get_pages().len() as u32;
        if nombre > 0 {
            return nombre;
        }
    }
    let donnees = match std::fs::read(chemin) {
        Ok(donnees) => donnees,
        Err(_) => return 0,
    };
    let contenu = String::from_utf8_lossy(&donnees);
    let mut maximum = 0u32;
    let mut reste = contenu.as_ref();
    while let Some(position) = reste.find("/Count") {
        reste = &reste[position + "/Count".len()..];
        let valeur = reste.trim_start();
        let longueur_chiffres = valeur.chars().take_while(|c| c.is_ascii_digit()).map(char::len_utf8).sum::<usize>();
        if longueur_chiffres > 0 {
            if let Ok(nombre) = valeur[..longueur_chiffres].parse::<u32>() {
                maximum = maximum.max(nombre);
            }
        }
        let espaces = reste.len() - valeur.len();
        let avance = espaces + longueur_chiffres;
        if avance == 0 {
            let longueur = reste.chars().next().map(char::len_utf8).unwrap_or(0);
            if longueur == 0 { break; }
            reste = &reste[longueur..];
        } else {
            reste = &reste[avance..];
        }
    }
    maximum
}

fn lire_entree_zip(chemin: &Path, nom: &str) -> Option<String> {
    let fichier = std::fs::File::open(chemin).ok()?;
    let mut archive = zip::ZipArchive::new(fichier).ok()?;
    let mut entree = archive.by_name(nom).ok()?;
    let mut tampon = Vec::new();
    entree.read_to_end(&mut tampon).ok()?;
    Some(String::from_utf8_lossy(&tampon).to_string())
}

fn lire_slides_pptx(chemin: &Path) -> Option<u32> {
    let contenu = lire_entree_zip(chemin, "ppt/presentation.xml")?;
    let mut compte = 0u32;
    let mut reste = contenu.as_str();
    while let Some(pos) = reste.find("<p:sldId ") {
        compte += 1;
        reste = &reste[pos + 9..];
    }
    if compte > 0 { Some(compte) } else { None }
}

fn compter_pages_powerpoint(chemin: &str) -> Option<u32> {
    let chemin_esc = chemin.replace('\'', "''");
    let script = format!(
        r#"$ErrorActionPreference='Stop'
$p = $null
$pres = $null
try {{
  $p = New-Object -ComObject PowerPoint.Application
  $pres = $p.Presentations.Open('{chemin}', $true, $false, $false)
  $n = $pres.Slides.Count
  try {{ $pres.Close() }} catch {{}}
  try {{ $p.Quit() }} catch {{}}
  if ($n -gt 0) {{ Write-Output $n; exit 0 }}
  exit 1
}} catch {{
  if ($pres -ne $null) {{ try {{ $pres.Close() }} catch {{}} }}
  if ($p -ne $null) {{ try {{ $p.Quit() }} catch {{}} }}
  exit 1
}}"#,
        chemin = chemin_esc
    );
    let mut enfant = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-WindowStyle", "Hidden", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .ok()?;
    let debut = Instant::now();
    loop {
        match enfant.try_wait() {
            Ok(Some(statut)) => {
                let sortie = enfant.wait_with_output().ok()?;
                if !statut.success() { return None; }
                let texte = String::from_utf8_lossy(&sortie.stdout).trim().to_string();
                return texte.parse::<u32>().ok();
            }
            Ok(None) => {
                if debut.elapsed() > Duration::from_secs(TIMEOUT_COM) {
                    let _ = enfant.kill();
                    tuer_processus(&["POWERPNT.EXE"]);
                    return None;
                }
                std::thread::sleep(Duration::from_millis(150));
            }
            Err(_) => return None,
        }
    }
}

pub fn compter_pages_document(chemin: &PathBuf, _imprimante: &str) -> Result<u32, String> {
    let extension = chemin.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let chemin_str = chemin.to_string_lossy().to_string();

    let nombre = if extension == "pdf" {
        compter_pages_pdf(chemin)
    } else if est_document_word(&extension) || est_document_excel(&extension) {
        let pdf = convertir_office_en_pdf(chemin)?;
        let nombre = compter_pages_pdf(&pdf);
        let _ = std::fs::remove_file(&pdf);
        nombre
    } else {
        match extension.as_str() {
            "pptx" | "pptm" | "potx" | "potm" => lire_slides_pptx(chemin).or_else(|| compter_pages_powerpoint(&chemin_str)).unwrap_or(1),
            "ppt" | "pot" | "odp" => compter_pages_powerpoint(&chemin_str).unwrap_or(1),
            _ => 1,
        }
    };

    if nombre > 0 {
        Ok(nombre)
    } else {
        Err(format!("Impossible de determiner le nombre de pages de « {} ».", chemin.file_name().unwrap_or_default().to_string_lossy()))
    }
}

pub fn nettoyer_processus_office() {
    tuer_processus(&["WINWORD.EXE", "EXCEL.EXE", "POWERPNT.EXE"]);
}
