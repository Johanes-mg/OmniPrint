use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Deserialize;
use tiny_http::{Header, Method, Response, Server, StatusCode};

use crate::journal::{Entree, Journal, Statut};
use crate::plage_pages::{ModePages, SelectionPages};
use crate::session::{ModeCouleur, Orientation};

#[derive(Clone)]
pub struct EtatServeur {
    pub journal: Arc<Mutex<Journal>>,
    pub dossier: PathBuf,
    pub tx: Sender<()>,
    pub imprimante: Arc<Mutex<String>>,
    pub logo: Arc<Vec<u8>>,
    pub icone_clair: Arc<Vec<u8>>,
    pub icone_sombre: Arc<Vec<u8>>,
}

pub fn demarrer(port: u16, etat: EtatServeur) -> Result<u16, String> {
    let serveur = Server::http(format!("0.0.0.0:{}", port))
        .map_err(|e| format!("Impossible de demarrer sur le port {} : {}", port, e))?;
    let port_reel = serveur
        .server_addr()
        .to_ip()
        .map(|a| a.port())
        .unwrap_or(port);
    std::thread::spawn(move || {
        for requete in serveur.incoming_requests() {
            traiter(requete, &etat);
        }
    });
    Ok(port_reel)
}

fn traiter(requete: tiny_http::Request, etat: &EtatServeur) {
    let methode = requete.method().clone();
    let url = requete.url().to_string();

    if methode == Method::Get && (url == "/" || url.starts_with("/?")) {
        let _ = repondre_html(requete, PAGE_CLIENT);
        return;
    }
    if methode == Method::Get && url == "/logo.png" {
        let _ = repondre_png(requete, &etat.logo);
        return;
    }
    if methode == Method::Get && url == "/light.png" {
        let _ = repondre_png(requete, &etat.icone_clair);
        return;
    }
    if methode == Method::Get && url == "/dark.png" {
        let _ = repondre_png(requete, &etat.icone_sombre);
        return;
    }
    if methode == Method::Post && url == "/imprimer" {
        let _ = recevoir_document(requete, etat);
        return;
    }
    if methode == Method::Post && url == "/imprimer-maintenant" {
        let _ = imprimer_maintenant(requete, etat);
        return;
    }
    if methode == Method::Post && url == "/supprimer" {
        let _ = supprimer_document(requete, etat);
        return;
    }
    if methode == Method::Get {
        if let Some(id) = url.strip_prefix("/statut-doc?id=") {
            let _ = repondre_statut_doc(requete, etat, id);
            return;
        }
    }
    if methode == Method::Get {
        if let Some(query) = url.strip_prefix("/apercu?") {
            let _ = repondre_apercu(requete, etat, query);
            return;
        }
    }
    if methode == Method::Get {
        if let Some(id) = url.strip_prefix("/apercu-total?id=") {
            let _ = repondre_apercu_total(requete, etat, id);
            return;
        }
    }
    if methode == Method::Get && url == "/favicon.ico" {
        let _ = requete.respond(Response::empty(StatusCode(204)));
        return;
    }
    let _ = requete.respond(Response::from_string("Not found").with_status_code(StatusCode(404)));
}

fn repondre_html(requete: tiny_http::Request, contenu: &str) -> std::io::Result<()> {
    let entete = Header::from_bytes(&b"Content-Type"[..], &b"text/html; charset=utf-8"[..])
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "header"))?;
    let reponse = Response::from_string(contenu)
        .with_status_code(StatusCode(200))
        .with_header(entete);
    requete.respond(reponse)
}

fn repondre_png(requete: tiny_http::Request, octets: &[u8]) -> std::io::Result<()> {
    let entete = Header::from_bytes(&b"Content-Type"[..], &b"image/png"[..])
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "header"))?;
    let reponse = Response::from_data(octets.to_vec())
        .with_status_code(StatusCode(200))
        .with_header(entete);
    requete.respond(reponse)
}

fn repondre_json_statut(requete: tiny_http::Request, code: u16, corps: &str) -> std::io::Result<()> {
    let entete = Header::from_bytes(&b"Content-Type"[..], &b"application/json; charset=utf-8"[..])
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "header"))?;
    let reponse = Response::from_string(corps)
        .with_status_code(StatusCode(code))
        .with_header(entete);
    requete.respond(reponse)
}

fn lire_corps(reader: &mut dyn std::io::Read, tampon: &mut Vec<u8>) -> std::io::Result<usize> {
    std::io::Read::read_to_end(reader, tampon)
}

fn lire_corps_texte(reader: &mut dyn std::io::Read, tampon: &mut String) -> std::io::Result<usize> {
    std::io::Read::read_to_string(reader, tampon)
}

fn recevoir_document(mut requete: tiny_http::Request, etat: &EtatServeur) -> std::io::Result<()> {
    let content_type = requete
        .headers()
        .iter()
        .find(|h| h.field.equiv("Content-Type"))
        .map(|h| h.value.as_str().to_string())
        .unwrap_or_default();

    let boundary = match extraire_boundary(&content_type) {
        Some(b) => b,
        None => {
            return repondre_json_statut(requete, 400, "{\"ok\":false,\"message\":\"Requete invalide\"}");
        }
    };

    let mut corps = Vec::new();
    if let Err(e) = lire_corps(&mut requete.as_reader(), &mut corps) {
        return repondre_json_statut(
            requete,
            500,
            &format!("{{\"ok\":false,\"message\":\"Lecture impossible : {}\"}}", echapper_json(&e.to_string())),
        );
    }

    if corps.is_empty() {
        return repondre_json_statut(requete, 400, "{\"ok\":false,\"message\":\"Corps vide\"}");
    }

    let (nom_fichier, expediteur, contenu) = parser_multipart(&corps, &boundary);

    if contenu.is_empty() || nom_fichier.is_empty() {
        let detail = format!("Fichier vide (nom='{}', taille={})", nom_fichier, contenu.len());
        return repondre_json_statut(requete, 400, &format!("{{\"ok\":false,\"message\":\"{}\"}}", echapper_json(&detail)));
    }

    let id = uuid::Uuid::new_v4().to_string();
    let nom_sur = sanitize(&nom_fichier);
    let chemin = etat.dossier.join(format!("{}_{}", &id[..8], nom_sur));
    
    let ext = chemin.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let est_feuille = matches!(ext.as_str(), "xlsx" | "xlsm" | "xltx" | "xltm" | "xls" | "xlsb" | "ods" | "csv");

    if let Err(e) = std::fs::write(&chemin, &contenu) {
        return repondre_json_statut(requete, 500, &format!("{{\"ok\":false,\"message\":\"Ecriture impossible : {}\"}}", echapper_json(&e.to_string())));
    }

    let selection = SelectionPages { est_feuille, ..SelectionPages::default() };
    let entree = Entree {
        id: id.clone(),
        expediteur: if expediteur.trim().is_empty() { "navigateur".into() } else { expediteur },
        nom_fichier: nom_fichier.clone(),
        pages: 0,
        pages_reelles: 0,
        erreur_pages: None,
        version_analyse: 0,
        horodatage: chrono::Local::now(),
        statut: Statut::EnAttente,
        message: chemin.to_string_lossy().to_string(),
        selection,
    };
    {
        let mut j = etat.journal.lock().unwrap();
        j.ajouter(entree);
    }
    let _ = etat.tx.send(());

    let journal_bg = etat.journal.clone();
    let id_bg = id.clone();
    let chemin_bg = chemin.clone();
    let imp_bg = etat.imprimante.lock().unwrap().clone();
    std::thread::spawn(move || {
        let resultat = crate::printer::compter_pages_document(&chemin_bg, &imp_bg);
        if let Ok(mut j) = journal_bg.lock() {
            if let Some(e) = j.entrees.iter_mut().find(|e| e.id == id_bg) {
                match resultat {
                    Ok(pages) => {
                        e.pages = pages;
                        e.pages_reelles = pages;
                        e.erreur_pages = None;
                    }
                    Err(erreur) => {
                        e.pages = 0;
                        e.pages_reelles = 0;
                        e.erreur_pages = Some(erreur);
                    }
                }
                e.version_analyse = crate::journal::VERSION_ANALYSE_PAGES;
            }
            j.sauvegarder();
        }
    });

    let corps_reponse = format!(
        "{{\"ok\":true,\"id\":\"{}\",\"pages\":0,\"nom\":\"{}\",\"taille\":{},\"est_feuille\":{}}}",
        id, echapper_json(&nom_fichier), contenu.len(), est_feuille
    );
    repondre_json_statut(requete, 200, &corps_reponse)
}

#[derive(Deserialize)]
struct RequeteImpression {
    id: String,
    mode: String,
    orientation: String,
    expediteur: String,
    pages_mode: String,
    pages_cochees: Vec<u32>,
    copies: u32,
}

fn attendre_pages(
    journal: &Arc<Mutex<Journal>>,
    id: &str,
    timeout: Duration,
) -> Result<u32, String> {
    let debut = Instant::now();
    loop {
        {
            let j = journal.lock().unwrap();
            if let Some(e) = j.entrees.iter().find(|e| e.id == id) {
                if e.pages_reelles > 0 {
                    return Ok(e.pages_reelles);
                }
                if let Some(erreur) = e.erreur_pages.as_ref() {
                    return Err(erreur.clone());
                }
            } else {
                return Err("Document introuvable dans la file d'impression.".into());
            }
        }
        if debut.elapsed() > timeout {
            return Err("Le calcul du nombre de pages n'est pas termine. Reessayez dans quelques secondes.".into());
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}

fn imprimer_maintenant(mut requete: tiny_http::Request, etat: &EtatServeur) -> std::io::Result<()> {
    let mut corps = String::new();
    let _ = lire_corps_texte(&mut requete.as_reader(), &mut corps);
    let requete_json: RequeteImpression = match serde_json::from_str(&corps) {
        Ok(r) => r,
        Err(e) => {
            return repondre_json_statut(
                requete,
                400,
                &format!("{{\"ok\":false,\"message\":\"JSON invalide : {}\"}}", echapper_json(&e.to_string())),
            );
        }
    };
    if requete_json.id.is_empty() {
        return repondre_json_statut(requete, 400, "{\"ok\":false,\"message\":\"ID manquant\"}");
    }
    let imprimante = etat.imprimante.lock().unwrap().clone();
    if imprimante.is_empty() {
        return repondre_json_statut(requete, 400, "{\"ok\":false,\"message\":\"Aucune imprimante selectionnee sur le poste hote\"}");
    }

    let total = match attendre_pages(&etat.journal, &requete_json.id, Duration::from_secs(60)) {
        Ok(n) => n,
        Err(erreur) => {
            return repondre_json_statut(
                requete,
                400,
                &format!("{{\"ok\":false,\"message\":\"{}\"}}", echapper_json(&erreur)),
            );
        }
    };

    let entree = {
        let j = etat.journal.lock().unwrap();
        j.entrees.iter().find(|e| e.id == requete_json.id).cloned()
    };
    match entree {
        Some(e) if e.statut == Statut::EnAttente => {
            let chemin = PathBuf::from(&e.message);
            if !chemin.exists() {
                let mut j = etat.journal.lock().unwrap();
                j.mettre_a_jour_statut(&e.id, Statut::Erreur, "Fichier absent".into());
                return repondre_json_statut(requete, 400, "{\"ok\":false,\"message\":\"Fichier absent sur le poste hote\"}");
            }
            let mode_pages = match requete_json.pages_mode.as_str() {
                "personnalise" => ModePages::Personnalise,
                _ => ModePages::Toutes,
            };
            let selection = SelectionPages {
                mode: mode_pages,
                pages_cochees: requete_json.pages_cochees.clone(),
                copies: if requete_json.copies == 0 { 1 } else { requete_json.copies },
                est_feuille: e.selection.est_feuille,
            };
            let spec = match selection.spec_imprimante(total) {
                Ok(s) => s,
                Err(err) => {
                    return repondre_json_statut(requete, 400, &format!("{{\"ok\":false,\"message\":\"{}\"}}", echapper_json(&err)));
                }
            };
            {
                let mut j = etat.journal.lock().unwrap();
                if let Some(e2) = j.entrees.iter_mut().find(|e2| e2.id == e.id) {
                    if !requete_json.expediteur.is_empty() {
                        e2.expediteur = requete_json.expediteur.clone();
                    }
                    e2.selection = selection.clone();
                }
                j.sauvegarder();
            }
            let mode = if requete_json.mode == "noirblanc" { ModeCouleur::NoirBlanc } else { ModeCouleur::Couleur };
            let orientation = if requete_json.orientation == "paysage" { Orientation::Paysage } else { Orientation::Portrait };
            
            crate::printer::imprimer_fichier(
                &imprimante,
                &chemin,
                e.id.clone(),
                etat.journal.clone(),
                mode,
                orientation,
                spec,
                selection.copies_effectives(),
                selection.est_feuille,
            );
            repondre_json_statut(requete, 200, "{\"ok\":true,\"message\":\"Impression lancee\"}")
        }
        Some(_) => repondre_json_statut(requete, 400, "{\"ok\":false,\"message\":\"Document deja traite\"}"),
        None => repondre_json_statut(requete, 404, "{\"ok\":false,\"message\":\"Document inconnu\"}"),
    }
}

fn supprimer_document(mut requete: tiny_http::Request, etat: &EtatServeur) -> std::io::Result<()> {
    let mut corps = String::new();
    let _ = lire_corps_texte(&mut requete.as_reader(), &mut corps);
    let id = corps.trim().to_string();
    if id.is_empty() {
        return repondre_json_statut(requete, 400, "{\"ok\":false,\"message\":\"ID manquant\"}");
    }
    let supprime = {
        let mut j = etat.journal.lock().unwrap();
        j.supprimer(&id)
    };
    if supprime {
        repondre_json_statut(requete, 200, "{\"ok\":true}")
    } else {
        repondre_json_statut(requete, 404, "{\"ok\":false,\"message\":\"Document introuvable\"}")
    }
}

fn repondre_statut_doc(requete: tiny_http::Request, etat: &EtatServeur, id: &str) -> std::io::Result<()> {
    let entree = {
        let j = etat.journal.lock().unwrap();
        j.entrees.iter().find(|e| e.id == id).cloned()
    };
    match entree {
        Some(e) => {
            let statut = match e.statut {
                Statut::EnAttente => "EnAttente",
                Statut::EnCours => "EnCours",
                Statut::Imprime => "Imprime",
                Statut::Erreur => "Erreur",
            };
            let total = e.pages_reelles;
            let pret = if e.pages_reelles > 0 { "true" } else { "false" };
            let erreur_pages = e.erreur_pages.as_deref().unwrap_or("");
            let est_feuille = if e.selection.est_feuille { "true" } else { "false" };
            let corps = format!(
                "{{\"ok\":true,\"statut\":\"{}\",\"message\":\"{}\",\"pages\":{},\"pret\":{},\"erreur_pages\":\"{}\",\"est_feuille\":{}}}",
                statut,
                echapper_json(&e.message),
                total,
                pret,
                echapper_json(erreur_pages),
                est_feuille
            );
            repondre_json_statut(requete, 200, &corps)
        }
        None => repondre_json_statut(requete, 404, "{\"ok\":false,\"message\":\"Document introuvable\"}"),
    }
}

fn repondre_apercu_total(requete: tiny_http::Request, etat: &EtatServeur, id: &str) -> std::io::Result<()> {
    let entree = {
        let j = etat.journal.lock().unwrap();
        j.entrees.iter().find(|e| e.id == id).cloned()
    };
    match entree {
        Some(e) => {
            if e.pages_reelles > 0 {
                repondre_json_statut(requete, 200, &format!("{{\"ok\":true,\"pages\":{}}}", e.pages_reelles))
            } else if let Some(err) = e.erreur_pages {
                repondre_json_statut(requete, 400, &format!("{{\"ok\":false,\"message\":\"{}\"}}", echapper_json(&err)))
            } else {
                repondre_json_statut(requete, 202, "{\"ok\":false,\"message\":\"Analyse en cours\"}")
            }
        }
        None => repondre_json_statut(requete, 404, "{\"ok\":false,\"message\":\"Document introuvable\"}"),
    }
}

fn repondre_apercu(requete: tiny_http::Request, etat: &EtatServeur, query: &str) -> std::io::Result<()> {
    let mut id = String::new();
    let mut page = 1u32;
    for partie in query.split('&') {
        let mut it = partie.splitn(2, '=');
        if let (Some(cle), Some(valeur)) = (it.next(), it.next()) {
            match cle {
                "id" => id = valeur.to_string(),
                "page" => page = valeur.parse().unwrap_or(1),
                _ => {}
            }
        }
    }
    if id.is_empty() {
        return repondre_json_statut(requete, 400, "{\"ok\":false,\"message\":\"ID manquant\"}");
    }
    let entree = {
        let j = etat.journal.lock().unwrap();
        j.entrees.iter().find(|e| e.id == id).cloned()
    };
    let e = match entree {
        Some(e) => e,
        None => return repondre_json_statut(requete, 404, "{\"ok\":false,\"message\":\"Document introuvable\"}"),
    };
    let chemin = PathBuf::from(&e.message);
    match crate::printer::generer_apercu(&chemin, page) {
        Ok(fichier) => {
            let donnees = std::fs::read(&fichier).unwrap_or_default();
            let _ = std::fs::remove_file(&fichier);
            repondre_png(requete, &donnees)
        }
        Err(erreur) => repondre_json_statut(
            requete,
            500,
            &format!("{{\"ok\":false,\"message\":\"{}\"}}", echapper_json(&erreur)),
        ),
    }
}

fn extraire_boundary(content_type: &str) -> Option<String> {
    for morceau in content_type.split(';') {
        let m = morceau.trim();
        if let Some(rest) = m.strip_prefix("boundary=") {
            return Some(rest.trim().trim_matches('"').to_string());
        }
    }
    None
}

fn parser_multipart(corps: &[u8], boundary: &str) -> (String, String, Vec<u8>) {
    let mut nom_fichier = String::new();
    let mut expediteur = String::new();
    let mut contenu = Vec::new();

    let delimiteur = {
        let mut v = Vec::with_capacity(boundary.len() + 4);
        v.extend_from_slice(b"--");
        v.extend_from_slice(boundary.as_bytes());
        v
    };

    let mut positions: Vec<usize> = Vec::new();
    if !delimiteur.is_empty() && corps.len() >= delimiteur.len() {
        let mut i = 0usize;
        while i + delimiteur.len() <= corps.len() {
            if &corps[i..i + delimiteur.len()] == delimiteur.as_slice() {
                positions.push(i);
                i += delimiteur.len();
            } else {
                i += 1;
            }
        }
    }

    if positions.len() < 2 {
        return (nom_fichier, expediteur, contenu);
    }

    for k in 0..positions.len().saturating_sub(1) {
        let debut = positions[k] + delimiteur.len();
        let fin = positions[k + 1];
        if debut >= fin || debut >= corps.len() { continue; }
        let mut curseur = debut;
        if curseur + 1 < fin && corps[curseur] == b'\r' && corps[curseur + 1] == b'\n' { curseur += 2; }
        let entetes_debut = curseur;
        let mut entetes_fin = curseur;
        while entetes_fin + 3 < fin {
            if corps[entetes_fin] == b'\r' && corps[entetes_fin + 1] == b'\n' && corps[entetes_fin + 2] == b'\r' && corps[entetes_fin + 3] == b'\n' { break; }
            entetes_fin += 1;
        }
        if entetes_fin + 3 >= fin { continue; }
        let entetes = String::from_utf8_lossy(&corps[entetes_debut..entetes_fin]).to_string();
        let donnees_debut = entetes_fin + 4;
        let donnees_fin = fin.saturating_sub(2);
        if donnees_debut > donnees_fin { continue; }
        let donnees = corps[donnees_debut..donnees_fin].to_vec();
        let nom_champ = extraire_nom_champ(&entetes);
        if nom_champ == "fichier" {
            nom_fichier = extraire_nom_fichier(&entetes);
            contenu = donnees;
        } else if nom_champ == "expediteur" {
            expediteur = String::from_utf8_lossy(&donnees).trim().to_string();
        }
    }
    (nom_fichier, expediteur, contenu)
}

fn extraire_nom_champ(entetes: &str) -> String {
    for ligne in entetes.lines() {
        if ligne.to_ascii_lowercase().contains("content-disposition") {
            for partie in ligne.split(';') {
                let p = partie.trim();
                if let Some(rest) = p.strip_prefix("name=") {
                    return rest.trim_matches('"').to_string();
                }
            }
        }
    }
    String::new()
}

fn extraire_nom_fichier(entetes: &str) -> String {
    for ligne in entetes.lines() {
        if ligne.to_ascii_lowercase().contains("content-disposition") {
            for partie in ligne.split(';') {
                let p = partie.trim();
                if let Some(rest) = p.strip_prefix("filename=") {
                    return rest.trim_matches('"').to_string();
                }
            }
        }
    }
    "document".into()
}

fn echapper_json(s: &str) -> String {
    let mut sortie = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => sortie.push_str("\\\""),
            '\\' => sortie.push_str("\\\\"),
            '\n' => sortie.push_str("\\n"),
            '\r' => sortie.push_str("\\r"),
            '\t' => sortie.push_str("\\t"),
            c if (c as u32) < 0x20 => sortie.push(' '),
            c => sortie.push(c),
        }
    }
    sortie
}

fn sanitize(nom: &str) -> String {
    nom.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-' { c } else { '_' }
        })
        .collect()
}

const PAGE_CLIENT: &str = r#"<!DOCTYPE html>
<html lang="fr">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<meta name="description" content="Envoyez et imprimez vos documents via OmniPrint.">
<title>OmniPrint</title>
<link rel="icon" type="image/png" href="/logo.png">
<style>
:root {
  --fond: #f4f6fa;
  --carte: #ffffff;
  --bordure: #d2d8e4;
  --texte: #1c202c;
  --texte-doux: #606a80;
  --accent: #2864d2;
  --accent-fonce: #2252b0;
  --succes: #229e6c;
  --erreur: #c84040;
  color-scheme: light;
}
[data-theme="dark"] {
  --fond: #101218;
  --carte: #181b24;
  --bordure: #2d3242;
  --texte: #e8ebf5;
  --texte-doux: #96a2b2;
  --accent: #5a9cff;
  --accent-fonce: #3a6ec8;
  --succes: #48c78e;
  --erreur: #e85e5e;
  color-scheme: dark;
}

* { box-sizing: border-box; }
html, body { margin: 0; padding: 0; min-height: 100%; }
body { font-family: system-ui, -apple-system, "Segoe UI", Roboto, sans-serif; background: var(--fond); color: var(--texte); padding: 32px 20px 48px; transition: background 0.2s, color 0.2s; }
header { width: 100%; max-width: 720px; margin: 0 auto 24px; text-align: center; position: relative; }
header img.logo { display: block; margin: 0 auto 12px; }
header h1 { font-size: 24px; font-weight: 600; margin: 0; letter-spacing: 0.2px; }
header .soustitre { color: var(--texte-doux); font-size: 14px; margin-top: 6px; }

.btn-theme { position: absolute; top: 0; right: 0; background: var(--carte); border: 1px solid var(--bordure); border-radius: 50%; width: 40px; height: 40px; cursor: pointer; display: flex; align-items: center; justify-content: center; transition: all 0.2s; padding: 0; }
.btn-theme:hover { background: var(--fond); }
.btn-theme img { display: block; width: 20px; height: 20px; }

main { width: 100%; max-width: 720px; margin: 0 auto; text-align: center; }
.zone { border: 2px dashed var(--bordure); border-radius: 14px; padding: 44px 24px; text-align: center; cursor: pointer; background: var(--carte); transition: all 0.2s; }
.zone.survol { border-color: var(--accent); background: var(--fond); }
.zone h2 { font-size: 17px; margin: 0 0 8px; }
.zone p { margin: 0; color: var(--texte-doux); font-size: 13px; }
.zone .bouton { margin-top: 16px; padding: 10px 20px; border-radius: 8px; background: var(--accent-fonce); color: #fff; border: none; font-size: 14px; cursor: pointer; }
input[type=file] { display: none; }
label.champ { display: block; margin-top: 22px; font-size: 13px; color: var(--texte-doux); text-align: left; }
input[type=text], input[type=number], select { width: 100%; padding: 10px 12px; background: var(--carte); color: var(--texte); border: 1px solid var(--bordure); border-radius: 8px; font-size: 14px; margin-top: 6px; text-align: left; }
.mode { margin-top: 18px; text-align: left; }
.mode .titre { font-size: 13px; color: var(--texte-doux); margin-bottom: 6px; }

.choix-groupe { display: flex; gap: 8px; margin-top: 6px; }
.choix-case { flex: 1; text-align: center; padding: 10px 12px; background: var(--carte); border: 1px solid var(--bordure); border-radius: 8px; font-size: 13px; color: var(--texte); cursor: pointer; user-select: none; transition: all 0.2s ease; }
.choix-case:hover { background: var(--fond); }
.choix-case input { display: none; }
.choix-case.actif { background: var(--accent-fonce); border-color: var(--accent); color: #fff; }

.liste { margin-top: 22px; display: flex; flex-direction: column; gap: 10px; text-align: left; }
.item { display: flex; align-items: center; gap: 12px; padding: 12px 14px; background: var(--carte); border: 1px solid var(--bordure); border-radius: 8px; font-size: 13px; flex-wrap: wrap; transition: all 0.2s; }
.item .nom { flex: 1; min-width: 150px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.item .info { color: var(--texte-doux); font-size: 12px; }
.item .etat { font-weight: 500; margin-left: auto; }
.etat-ok { color: var(--succes); }
.etat-err { color: var(--erreur); }
.item button { padding: 6px 14px; border-radius: 6px; border: 1px solid var(--accent); background: transparent; color: var(--accent); font-size: 12px; cursor: pointer; }
.item button:hover { background: var(--accent); color: #fff; }
.item button:disabled { opacity: 0.4; cursor: default; }
.item button.danger { border-color: var(--erreur); color: var(--erreur); }
.item button.danger:hover { background: var(--erreur); color: #fff; }
.pages-grid { display: grid; grid-template-columns: repeat(auto-fill, minmax(44px, 1fr)); gap: 6px; margin-top: 8px; max-height: 200px; overflow-y: auto; padding: 6px; background: var(--carte); border: 1px solid var(--bordure); border-radius: 8px; }
.pages-grid label { display: flex; align-items: center; justify-content: center; padding: 6px 0; background: var(--fond); border: 1px solid var(--bordure); border-radius: 6px; font-size: 12px; cursor: pointer; }
.pages-grid input { display: none; }
.pages-grid input:checked + span { color: var(--accent); font-weight: 700; }
.pages-grid label:has(input:checked) { border-color: var(--accent); background: var(--carte); }
.ligne-double { display: flex; gap: 12px; }
.ligne-double > div { flex: 1; }
footer { margin: 36px auto 0; color: var(--texte-doux); font-size: 12px; text-align: center; max-width: 720px; }

#modal-apercu { display: none; position: fixed; top: 0; left: 0; width: 100%; height: 100%; background: rgba(0,0,0,0.85); z-index: 1000; overflow-y: auto; padding: 20px; }
#modal-apercu .contenu { max-width: 800px; margin: 0 auto; background: var(--carte); border: 1px solid var(--bordure); border-radius: 12px; padding: 20px; position: relative; }
#modal-apercu .btn-fermer { position: absolute; top: 10px; right: 10px; background: var(--erreur); color: #fff; border: none; border-radius: 6px; padding: 6px 12px; cursor: pointer; font-weight: bold; }
#modal-apercu h3 { margin-top: 0; margin-bottom: 20px; }
#apercu-pages { display: flex; flex-direction: column; gap: 20px; align-items: center; }
#apercu-pages img { max-width: 100%; height: auto; box-shadow: 0 4px 12px rgba(0,0,0,0.5); border: 1px solid var(--bordure); background: #fff; }
#apercu-pages .chargement { color: var(--texte-doux); font-size: 14px; margin: 10px 0; }

@media (max-width: 520px) {
  body { padding: 20px 14px 32px; }
  .zone { padding: 32px 16px; }
  .ligne-double { flex-direction: column; }
  .choix-groupe { flex-direction: column; }
}
</style>
</head>
<body>
<header>
  <button id="btn-theme" class="btn-theme" title="Changer de thème">
    <img src="/dark.png" alt="Thème" id="img-theme">
  </button>
  <img class="logo" src="/logo.png" alt="Logo" width="50">
  <h1>OmniPrint</h1>
  <div class="soustitre">Envoyez et imprimez vos documents</div>
</header>
<main>
  <div class="zone" id="zone">
    <h2>Glissez vos fichiers ici</h2>
    <p>ou cliquez pour ouvrir et selectionner</p>
    <button class="bouton" type="button" id="bouton-ouvrir">Ouvrir et selectionner</button>
  </div>
  <label class="champ" for="nom">Votre nom (facultatif)</label>
  <input type="text" id="nom" placeholder="Johanes-mg" maxlength="60">
  
  <div class="ligne-double">
    <div>
      <label class="champ" for="pages-mode">Pages à imprimer</label>
      <select id="pages-mode">
        <option value="toutes">Toutes les pages</option>
        <option value="personnalise">Sélection par case</option>
      </select>
    </div>
    <div>
      <label class="champ" for="copies">Copies</label>
      <input type="number" id="copies" min="1" max="999" value="1">
    </div>
  </div>

  <div class="ligne-double" style="margin-top: 16px;">
    <div class="mode">
      <div class="titre">Mode d'impression</div>
      <div class="choix-groupe">
        <label class="choix-case actif">
          <input type="radio" name="mode" value="couleur" checked onchange="majCases('mode')"> Couleur
        </label>
        <label class="choix-case">
          <input type="radio" name="mode" value="noirblanc" onchange="majCases('mode')"> Noir et blanc
        </label>
      </div>
    </div>
    <div class="mode">
      <div class="titre">Orientation</div>
      <div class="choix-groupe">
        <label class="choix-case actif">
          <input type="radio" name="orientation" value="portrait" checked onchange="majCases('orientation')"> Portrait
        </label>
        <label class="choix-case">
          <input type="radio" name="orientation" value="paysage" onchange="majCases('orientation')"> Paysage
        </label>
      </div>
    </div>
  </div>

  <div id="bloc-selection" style="display:none; margin-top: 16px;">
    <div class="champ" id="titre-selection">Sélection par case (analyse du document en cours...)</div>
    <div class="pages-grid" id="pages-grid"></div>
  </div>
  <div class="liste" id="liste"></div>
</main>
<footer>Les documents restent sur le réseau local. Aucune donnée ne quitte l'ordinateur hôte.</footer>
<input type="file" id="fichier" multiple>

<!-- Fenêtre modale pour l'aperçu global -->
<div id="modal-apercu">
  <div class="contenu">
    <button class="btn-fermer" onclick="fermerApercu()">Fermer</button>
    <h3>Aperçu du document</h3>
    <div id="apercu-pages"></div>
  </div>
</div>

<script>
(function () {
  var btnTheme = document.getElementById('btn-theme');
  var imgTheme = document.getElementById('img-theme');
  var currentTheme = localStorage.getItem('theme') || 'light';

  function appliquerTheme(theme) {
    document.documentElement.setAttribute('data-theme', theme);
    localStorage.setItem('theme', theme);
    if (theme === 'dark') {
      imgTheme.src = '/light.png';
    } else {
      imgTheme.src = '/dark.png';
    }
  }

  appliquerTheme(currentTheme);

  btnTheme.addEventListener('click', function() {
    currentTheme = currentTheme === 'dark' ? 'light' : 'dark';
    appliquerTheme(currentTheme);
  });

  var zone = document.getElementById('zone');
  var bouton = document.getElementById('bouton-ouvrir');
  var input = document.getElementById('fichier');
  var liste = document.getElementById('liste');
  var nom = document.getElementById('nom');
  var pagesMode = document.getElementById('pages-mode');
  var blocSelection = document.getElementById('bloc-selection');
  var pagesGrid = document.getElementById('pages-grid');
  var titreSelection = document.getElementById('titre-selection');
  var copies = document.getElementById('copies');
  var visuelCourant = null;
  var totalPagesConnues = 0;
  var estFeuilleCourante = false;

  window.majCases = function(nomGroupe) {
    var inputs = document.querySelectorAll('input[name="' + nomGroupe + '"]');
    for (var i = 0; i < inputs.length; i++) {
      var label = inputs[i].parentElement;
      if (inputs[i].checked) {
        label.classList.add('actif');
      } else {
        label.classList.remove('actif');
      }
    }
  };

  function modeChoisi() {
    var r = document.querySelector('input[name="mode"]:checked');
    return r ? r.value : 'couleur';
  }
  
  function orientationChoisie() {
    var r = document.querySelector('input[name="orientation"]:checked');
    return r ? r.value : 'portrait';
  }

  function formatTaille(o) {
    if (o < 1024) return o + ' o';
    if (o < 1048576) return (o / 1024).toFixed(1) + ' Ko';
    return (o / 1048576).toFixed(1) + ' Mo';
  }

  function construireGrille(total, estFeuille) {
    if (total <= 0) return;
    pagesGrid.innerHTML = '';
    var mot = estFeuille ? 'feuille(s)' : 'page(s)';
    titreSelection.textContent = 'Sélection par case (' + total + ' ' + mot + ' disponible(s))';
    for (var i = 1; i <= total; i++) {
      var lab = document.createElement('label');
      var inp = document.createElement('input');
      inp.type = 'checkbox';
      inp.value = String(i);
      var sp = document.createElement('span');
      sp.textContent = String(i);
      lab.appendChild(inp);
      lab.appendChild(sp);
      pagesGrid.appendChild(lab);
    }
  }

  function majModeAffichage() {
    var m = pagesMode.value;
    blocSelection.style.display = m === 'personnalise' ? 'block' : 'none';
    if (m === 'personnalise') {
      if (totalPagesConnues > 0) {
        construireGrille(totalPagesConnues, estFeuilleCourante);
      } else {
        var mot = estFeuilleCourante ? 'feuille(s)' : 'page(s)';
        titreSelection.textContent = 'Sélection par case (analyse du document en cours...)';
      }
    }
  }

  function lireCoches() {
    var arr = [];
    var inputs = pagesGrid.querySelectorAll('input[type=checkbox]');
    for (var i = 0; i < inputs.length; i++) {
      if (inputs[i].checked) arr.push(parseInt(inputs[i].value, 10));
    }
    return arr;
  }

  function ouvrirApercuGlobal(id) {
    var modal = document.getElementById('modal-apercu');
    var conteneur = document.getElementById('apercu-pages');
    conteneur.innerHTML = '<div class="chargement">Chargement de l\'aperçu complet...</div>';
    modal.style.display = 'block';

    var xhr = new XMLHttpRequest();
    xhr.open('GET', '/apercu-total?id=' + encodeURIComponent(id), true);
    xhr.onload = function() {
      var rep = null;
      try { rep = JSON.parse(xhr.responseText); } catch(e) {}
      if (xhr.status === 200 && rep && rep.ok && rep.pages > 0) {
        conteneur.innerHTML = '';
        for (var p = 1; p <= rep.pages; p++) {
          var img = document.createElement('img');
          img.src = '/apercu?id=' + encodeURIComponent(id) + '&page=' + p;
          img.alt = 'Page ' + p;
          img.loading = 'lazy';
          conteneur.appendChild(img);
        }
      } else {
        conteneur.innerHTML = '<div class="chargement" style="color:var(--erreur)">Impossible de charger l\'aperçu complet.</div>';
      }
    };
    xhr.onerror = function() {
      conteneur.innerHTML = '<div class="chargement" style="color:var(--erreur)">Erreur réseau.</div>';
    };
    xhr.send();
  }

  window.fermerApercu = function() {
    document.getElementById('modal-apercu').style.display = 'none';
    document.getElementById('apercu-pages').innerHTML = '';
  }

  function creerItem(fichier) {
    var item = document.createElement('div');
    item.className = 'item';
    var wrap = document.createElement('div');
    wrap.style.flex = '1';
    var nomEl = document.createElement('div');
    nomEl.className = 'nom';
    nomEl.textContent = fichier.name;
    var info = document.createElement('div');
    info.className = 'info';
    info.textContent = formatTaille(fichier.size);
    wrap.appendChild(nomEl);
    wrap.appendChild(info);
    var etat = document.createElement('div');
    etat.className = 'etat';
    etat.textContent = 'Envoi...';
    var btnApercu = document.createElement('button');
    btnApercu.textContent = 'Aperçu';
    btnApercu.disabled = true;
    var btnImp = document.createElement('button');
    btnImp.textContent = 'Imprimer';
    btnImp.disabled = true;
    var btnSup = document.createElement('button');
    btnSup.className = 'danger';
    btnSup.textContent = 'Annuler';
    item.appendChild(wrap);
    item.appendChild(etat);
    item.appendChild(btnApercu);
    item.appendChild(btnImp);
    item.appendChild(btnSup);
    liste.appendChild(item);
    return { racine: item, etat: etat, btnApercu: btnApercu, btnImp: btnImp, btnSup: btnSup, id: null, pages: 0, pret: false };
  }

  function messageErreur(visuel, txt) {
    visuel.etat.className = 'etat etat-err';
    visuel.etat.textContent = txt;
  }

  function supprimer(visuel) {
    if (!visuel.id) { visuel.racine.remove(); return; }
    var xhr = new XMLHttpRequest();
    xhr.open('POST', '/supprimer', true);
    xhr.setRequestHeader('Content-Type', 'text/plain');
    xhr.onload = function () { visuel.racine.remove(); };
    xhr.onerror = function () { visuel.racine.remove(); };
    xhr.send(visuel.id);
  }

  function suivreStatut(visuel, id) {
    var tentatives = 0;
    function poll() {
      if (tentatives >= 120) return;
      tentatives++;
      var xhr = new XMLHttpRequest();
      xhr.open('GET', '/statut-doc?id=' + encodeURIComponent(id), true);
      xhr.onload = function () {
        var r = null;
        try { r = JSON.parse(xhr.responseText); } catch (e) {}
        if (r && r.ok) {
          if (r.erreur_pages) {
            messageErreur(visuel, r.erreur_pages);
            visuel.btnImp.disabled = true;
            visuel.btnImp.textContent = 'Indisponible';
            return;
          }
          if (r.pret === true && typeof r.pages === 'number' && r.pages > 0 && (!visuel.pret || visuel.pages !== r.pages)) {
            visuel.pages = r.pages;
            visuel.pret = true;
            if (visuel === visuelCourant) {
              totalPagesConnues = r.pages;
              estFeuilleCourante = r.est_feuille;
              majModeAffichage();
            }
            visuel.etat.className = 'etat etat-ok';
            var mot = r.est_feuille ? 'feuille(s)' : 'page(s)';
            visuel.etat.textContent = 'Prêt (' + r.pages + ' ' + mot + ')';
            visuel.btnImp.disabled = false;
            visuel.btnApercu.disabled = false;
          }
          if (r.statut === 'Imprime') {
            visuel.etat.className = 'etat etat-ok';
            visuel.etat.textContent = 'Imprimé';
            visuel.btnImp.textContent = 'Imprimé';
            visuel.btnImp.disabled = true;
            return;
          }
          if (r.statut === 'Erreur') {
            messageErreur(visuel, r.message || 'Erreur');
            visuel.btnImp.textContent = 'Réessayer';
            visuel.btnImp.disabled = false;
            return;
          }
        }
        setTimeout(poll, 1000);
      };
      xhr.onerror = function () { setTimeout(poll, 1500); };
      xhr.send();
    }
    setTimeout(poll, 400);
  }

  function envoyer(fichier) {
    return new Promise(function (resolve) {
      var visuel = creerItem(fichier);
      visuelCourant = visuel;
      visuel.btnSup.onclick = function () { supprimer(visuel); };
      var donnees = new FormData();
      donnees.append('expediteur', nom.value || 'navigateur');
      donnees.append('fichier', fichier, fichier.name);
      var xhr = new XMLHttpRequest();
      xhr.open('POST', '/imprimer', true);
      xhr.onload = function () {
        var rep = null;
        try { rep = JSON.parse(xhr.responseText); } catch (e) {}
        if (xhr.status === 200 && rep && rep.ok && rep.id) {
          visuel.id = rep.id;
          visuel.etat.className = 'etat';
          visuel.etat.textContent = 'Analyse du document...';
          suivreStatut(visuel, rep.id);
          
          visuel.btnApercu.onclick = function () {
            ouvrirApercuGlobal(rep.id);
          };
          
          visuel.btnImp.onclick = function () {
            visuel.btnImp.disabled = true;
            visuel.btnImp.textContent = 'Impression...';
            visuel.etat.className = 'etat';
            visuel.etat.textContent = 'Impression en cours...';
            var payload = {
              id: rep.id,
              mode: modeChoisi(),
              orientation: orientationChoisie(),
              expediteur: nom.value || 'navigateur',
              pages_mode: pagesMode.value,
              pages_cochees: lireCoches(),
              copies: parseInt(copies.value, 10) || 1
            };
            var xhr2 = new XMLHttpRequest();
            xhr2.open('POST', '/imprimer-maintenant', true);
            xhr2.setRequestHeader('Content-Type', 'application/json');
            xhr2.onload = function () {
              var r2 = null;
              try { r2 = JSON.parse(xhr2.responseText); } catch (e) {}
              if (xhr2.status === 200 && r2 && r2.ok) {
                suivreStatut(visuel, rep.id);
              } else {
                visuel.btnImp.textContent = 'Réessayer';
                visuel.btnImp.disabled = false;
                messageErreur(visuel, (r2 && r2.message) ? r2.message : 'Erreur impression');
              }
            };
            xhr2.onerror = function () {
              visuel.btnImp.textContent = 'Réessayer';
              visuel.btnImp.disabled = false;
              messageErreur(visuel, 'Erreur réseau');
            };
            xhr2.send(JSON.stringify(payload));
          };
        } else {
          messageErreur(visuel, (rep && rep.message) ? rep.message : ('Échec HTTP ' + xhr.status));
        }
        resolve();
      };
      xhr.onerror = function () { messageErreur(visuel, 'Échec réseau'); resolve(); };
      xhr.send(donnees);
    });
  }

  async function traiterFichiers(fichiers) {
    for (var i = 0; i < fichiers.length; i++) {
      var f = fichiers[i];
      if (f.size > 100 * 1024 * 1024) continue;
      await envoyer(f);
    }
  }

  pagesMode.addEventListener('change', majModeAffichage);
  majModeAffichage();

  zone.addEventListener('click', function () { input.click(); });
  bouton.addEventListener('click', function (e) { e.stopPropagation(); input.click(); });
  input.addEventListener('change', function () {
    if (input.files && input.files.length) traiterFichiers(input.files);
    input.value = '';
  });
  zone.addEventListener('dragover', function (e) { e.preventDefault(); zone.classList.add('survol'); });
  zone.addEventListener('dragleave', function () { zone.classList.remove('survol'); });
  zone.addEventListener('drop', function (e) {
    e.preventDefault();
    zone.classList.remove('survol');
    if (e.dataTransfer && e.dataTransfer.files && e.dataTransfer.files.length) {
      traiterFichiers(e.dataTransfer.files);
    }
  });
})();
</script>
</body>
</html>
"#;
