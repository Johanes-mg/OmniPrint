use std::io::Read;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::journal::Statut;
use crate::session::ModeCouleur;

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

fn est_document_word(extension: &str) -> bool {
    matches!(
        extension,
        "doc" | "docx" | "docm" | "dot" | "dotx" | "dotm" | "rtf" | "odt"
    )
}

fn echapper_chaine_powershell(valeur: &str) -> String {
    valeur.replace('\'', "''")
}

fn convertir_word_en_pdf(chemin: &Path, imprimante: &str) -> Result<PathBuf, String> {
    if !chemin.is_file() {
        return Err("Le document Word est introuvable.".into());
    }

    let destination = std::env::temp_dir().join(format!(
        "omniprint_docx_{}.pdf",
        uuid::Uuid::new_v4()
    ));
    let source_ps = echapper_chaine_powershell(&chemin.to_string_lossy());
    let destination_ps = echapper_chaine_powershell(&destination.to_string_lossy());
    let imprimante_ps = echapper_chaine_powershell(imprimante);

    let script = format!(
        r#"$ErrorActionPreference = 'Stop'
$word = $null
$document = $null
try {{
    $word = New-Object -ComObject Word.Application
    $word.Visible = $false
    $word.DisplayAlerts = 0
    try {{ $word.AutomationSecurity = 3 }} catch {{}}
    try {{ $word.Options.UpdateLinksAtOpen = $false }} catch {{}}

    if ('{printer}' -ne '') {{
        try {{
            $printerInfo = Get-Printer -Name '{printer}' -ErrorAction Stop
            $word.ActivePrinter = "$($printerInfo.Name) on $($printerInfo.PortName)"
        }} catch {{
            try {{ $word.ActivePrinter = '{printer}' }} catch {{}}
        }}
    }}

    $document = $word.Documents.Open('{source}', $false, $true, $false)
    try {{ $word.ActiveWindow.View.Type = 3 }} catch {{}}
    try {{ $document.Repaginate() }} catch {{}}

    $document.ExportAsFixedFormat(
        '{destination}', 17, $false, 0, 0, 1, 1, 0,
        $true, $true, 0, $true, $true, $false
    )

    $document.Close($false)
    $word.Quit()
    $document = $null
    $word = $null

    if (-not (Test-Path -LiteralPath '{destination}')) {{
        throw 'Word n''a pas créé le fichier PDF.'
    }}
    $fichier = Get-Item -LiteralPath '{destination}'
    if ($fichier.Length -lt 100) {{
        throw 'Le PDF généré est vide ou incomplet.'
    }}
    exit 0
}} catch {{
    $detail = $_.Exception.Message
    if ($document -ne $null) {{ try {{ $document.Close($false) }} catch {{}} }}
    if ($word -ne $null) {{ try {{ $word.Quit() }} catch {{}} }}
    [Console]::Error.WriteLine($detail)
    exit 1
}}"#,
        printer = imprimante_ps,
        source = source_ps,
        destination = destination_ps,
    );

    let mut enfant = Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| format!("Impossible de démarrer Microsoft Word : {}", e))?;

    let debut = Instant::now();
    loop {
        match enfant.try_wait() {
            Ok(Some(statut)) => {
                let sortie = enfant
                    .wait_with_output()
                    .map_err(|e| format!("Lecture du résultat Word impossible : {}", e))?;
                if statut.success() && destination.is_file() {
                    return Ok(destination);
                }

                let detail = String::from_utf8_lossy(&sortie.stderr).trim().to_string();
                let detail = if detail.is_empty() {
                    String::from_utf8_lossy(&sortie.stdout).trim().to_string()
                } else {
                    detail
                };
                let _ = std::fs::remove_file(&destination);
                return Err(if detail.is_empty() {
                    "Microsoft Word n'a pas réussi à convertir ce document en PDF. Vérifiez que Word est installé et que le fichier peut être ouvert.".into()
                } else {
                    format!("Conversion Word en PDF impossible : {}", detail)
                });
            }
            Ok(None) => {
                if debut.elapsed() > Duration::from_secs(TIMEOUT_COM) {
                    let _ = enfant.kill();
                    let _ = std::fs::remove_file(&destination);
                    return Err("Le traitement Word a dépassé le délai autorisé (90 secondes).".into());
                }
                std::thread::sleep(Duration::from_millis(150));
            }
            Err(e) => {
                let _ = std::fs::remove_file(&destination);
                return Err(format!("Erreur pendant le traitement Word : {}", e));
            }
        }
    }
}

pub fn imprimer_fichier(
    nom_imprimante: &str,
    chemin_fichier: &PathBuf,
    journal_id: String,
    journal: Arc<Mutex<crate::journal::Journal>>,
    mode_couleur: ModeCouleur,
    pages_spec: String,
    copies: u32,
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

        let (chemin_pdf, pdf_temporaire, erreur_conversion) = if extension == "pdf" {
            (Some(chemin.clone()), false, None)
        } else if est_document_word(&extension) {
            match convertir_word_en_pdf(&chemin, &nom_imprimante) {
                Ok(pdf) => (Some(pdf), true, None),
                Err(e) => (None, false, Some(e)),
            }
        } else {
            (None, false, None)
        };

        let resultat = if let Some(pdf) = chemin_pdf.as_ref() {
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
                if !reglages.is_empty() {
                    args.push("-print-settings".into());
                    args.push(reglages.join(","));
                }

                args.push("-silent".into());
                args.push(pdf.to_string_lossy().to_string());

                match Command::new(&sumatra)
                    .args(&args)
                    .creation_flags(CREATE_NO_WINDOW)
                    .output()
                {
                    Ok(o) if o.status.success() => Ok("Document transmis à l'imprimante".to_string()),
                    Ok(o) => Err(format!(
                        "SumatraPDF a échoué (code {}). {}",
                        o.status.code().unwrap_or(-1),
                        String::from_utf8_lossy(&o.stderr).trim()
                    )),
                    Err(e) => Err(format!("Lancement de SumatraPDF impossible : {}", e)),
                }
            } else if pages_spec.trim().is_empty() && copies == 1 && !monochrome {
                if let Some(edge) = trouver_edge() {
                    match Command::new(&edge)
                        .args([
                            "--headless",
                            &format!("--print-to-printer={}", nom_imprimante),
                            &pdf.to_string_lossy(),
                        ])
                        .creation_flags(CREATE_NO_WINDOW)
                        .output()
                    {
                        Ok(o) if o.status.success() => Ok("Document transmis à l'imprimante".to_string()),
                        Ok(o) => Err(format!("Edge a échoué (code {})", o.status.code().unwrap_or(-1))),
                        Err(e) => Err(format!("Lancement Edge impossible : {}", e)),
                    }
                } else {
                    Err("Aucun moteur PDF trouvé. Placez SumatraPDF.exe dans le dossier bin/.".into())
                }
            } else {
                Err("SumatraPDF est nécessaire pour respecter les pages sélectionnées, le nombre de copies et le mode noir et blanc. Placez SumatraPDF.exe dans le dossier bin/.".into())
            }
        } else if let Some(err) = erreur_conversion {
            Err(err)
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
                Ok(o) if o.status.success() => Ok("Document transmis à l'imprimante".to_string()),
                Ok(o) => {
                    let msg = String::from_utf8_lossy(&o.stdout).trim().to_string();
                    Err(if msg.is_empty() {
                        "Aucune application associée à ce format sur le poste hôte.".to_string()
                    } else {
                        msg
                    })
                }
                Err(e) => Err(format!("Erreur système : {}", e)),
            }
        };

        if pdf_temporaire {
            if let Some(pdf) = chemin_pdf {
                let _ = std::fs::remove_file(pdf);
            }
        }

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
        let longueur_chiffres = valeur
            .chars()
            .take_while(|caractere| caractere.is_ascii_digit())
            .map(char::len_utf8)
            .sum::<usize>();
        if longueur_chiffres > 0 {
            if let Ok(nombre) = valeur[..longueur_chiffres].parse::<u32>() {
                maximum = maximum.max(nombre);
            }
        }
        let espaces = reste.len() - valeur.len();
        let avance = espaces + longueur_chiffres;
        if avance == 0 {
            let longueur = reste.chars().next().map(char::len_utf8).unwrap_or(0);
            if longueur == 0 {
                break;
            }
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
    if compte > 0 {
        Some(compte)
    } else {
        None
    }
}


fn compter_pages_excel(chemin: &str, imprimante: &str) -> Option<u32> {
    let chemin_esc = chemin.replace('\'', "''");
    let imprimante_esc = imprimante.replace('\'', "''");
    let script = format!(
        r#"$ErrorActionPreference='Stop'
$x = $null
$w = $null
try {{
  $x = New-Object -ComObject Excel.Application
  $x.Visible = $false
  $x.DisplayAlerts = $false
  if ('{imp}' -ne '') {{ try {{ $x.ActivePrinter = '{imp}' }} catch {{}} }}
  $w = $x.Workbooks.Open('{chemin}', 0, $true)
  $total = 0
  foreach ($sheet in $w.Worksheets) {{
    try {{ $total += $sheet.PageSetup.Pages.Count }} catch {{}}
  }}
  try {{ $w.Close($false) }} catch {{}}
  try {{ $x.Quit() }} catch {{}}
  if ($total -gt 0) {{ Write-Output $total; exit 0 }}
  exit 1
}} catch {{
  if ($w -ne $null) {{ try {{ $w.Close($false) }} catch {{}} }}
  if ($x -ne $null) {{ try {{ $x.Quit() }} catch {{}} }}
  exit 1
}}"#,
        imp = imprimante_esc,
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
                    tuer_processus(&["EXCEL.EXE"]);
                    return None;
                }
                std::thread::sleep(Duration::from_millis(150));
            }
            Err(_) => return None,
        }
    }
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

pub fn compter_pages_document(chemin: &PathBuf, imprimante: &str) -> Result<u32, String> {
    let extension = chemin
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    let chemin_str = chemin.to_string_lossy().to_string();

    let nombre = if extension == "pdf" {
        compter_pages_pdf(chemin)
    } else if est_document_word(&extension) {
        let pdf = convertir_word_en_pdf(chemin, imprimante)?;
        let nombre = compter_pages_pdf(&pdf);
        let _ = std::fs::remove_file(&pdf);
        nombre
    } else {
        match extension.as_str() {
            "pptx" | "pptm" | "potx" | "potm" => lire_slides_pptx(chemin)
                .or_else(|| compter_pages_powerpoint(&chemin_str))
                .unwrap_or(1),
            "xlsx" | "xlsm" | "xltx" | "xltm" | "xls" | "xlsb" | "ods" | "csv" => {
                compter_pages_excel(&chemin_str, imprimante).unwrap_or(1)
            }
            "ppt" | "pot" | "odp" => compter_pages_powerpoint(&chemin_str).unwrap_or(1),
            _ => 1,
        }
    };

    if nombre > 0 {
        Ok(nombre)
    } else {
        Err(format!(
            "Impossible de déterminer le nombre de pages de « {} ».",
            chemin.file_name().unwrap_or_default().to_string_lossy()
        ))
    }
}

pub fn nettoyer_processus_office() {
    tuer_processus(&["WINWORD.EXE", "EXCEL.EXE", "POWERPNT.EXE"]);
}

