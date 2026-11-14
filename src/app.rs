use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use mdns_sd::ServiceDaemon;
use parking_lot::RwLock;

use crate::journal::{Entree, Journal, Statut};
use crate::mdns;
use crate::pare_feu;
use crate::partage;
use crate::plage_pages::{formater_pages, ModePages, SelectionPages};
use crate::printer;
use crate::reseau_utils;
use crate::session::{ModeCouleur, Orientation, Session};
use crate::theme::{self, badge, bouton_icone, separateur, Palette, PaletteClaire};
use crate::web::{self, EtatServeur};

pub struct App {
    session: Session,
    journal: Arc<Mutex<Journal>>,
    imprimantes: Vec<String>,
    fichiers_choisis: Vec<PathBuf>,
    selection_locale: SelectionPages,
    popup_selection_ouverte: bool,
    popup_cible: Option<PathBuf>,
    popup_total: u32,
    popup_cochees: Vec<u32>,
    message_statut: String,
    etat_pare_feu: String,
    etat_partage: String,
    chemin_partage: String,
    rx_web: Option<Receiver<()>>,
    serveur_demarre: bool,
    ip_locale: String,
    port_effectif: u16,
    imprimante_partagee: Arc<Mutex<String>>,
    imprimante_ipp: Arc<RwLock<String>>,
    icone_soleil: Option<egui::TextureHandle>,
    icone_lune: Option<egui::TextureHandle>,
    icone_qr_clair: Option<egui::TextureHandle>,
    icone_qr_sombre: Option<egui::TextureHandle>,
    icone_settings_clair: Option<egui::TextureHandle>,
    icone_settings_sombre: Option<egui::TextureHandle>,
    icone_historic_clair: Option<egui::TextureHandle>,
    icone_historic_sombre: Option<egui::TextureHandle>,
    daemon_mdns: Option<ServiceDaemon>,
    montrer_qr: bool,
    montrer_historique: bool,
    qr_texture: Option<egui::TextureHandle>,
    qr_adresse: String,
    panneau_parametres_ouvert: bool,
    copie_instant: Option<Instant>,
    copie_partage_instant: Option<Instant>,
    montrer_apercu: bool,
    apercu_texture: Option<egui::TextureHandle>,
    apercu_chemin: Option<PathBuf>,
    apercu_erreur: Option<String>,
    apercu_page: u32,
    apercu_cible: Option<PathBuf>,
    apercus_en_cours: HashMap<(String, u32), Receiver<Result<egui::ColorImage, String>>>,
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let session = Session::charger();
        if session.mode_sombre {
            theme::appliquer(&cc.egui_ctx);
        } else {
            theme::appliquer_clair(&cc.egui_ctx);
        }
        let journal = Arc::new(Mutex::new(Journal::charger()));
        let imprimantes = printer::lister_imprimantes();
        let ip_locale = reseau_utils::adresse_ip_locale();
        let imprimante_partagee = Arc::new(Mutex::new(session.nom_imprimante.clone()));
        let imprimante_ipp = Arc::new(RwLock::new(session.nom_imprimante.clone()));
        let icone_soleil = theme::charger_texture(&cc.egui_ctx, "images/light.png", "icone_soleil");
        let icone_lune = theme::charger_texture(&cc.egui_ctx, "images/dark.png", "icone_lune");
        let icone_qr_clair = theme::charger_texture(&cc.egui_ctx, "images/qr-light.png", "icone_qr_clair");
        let icone_qr_sombre = theme::charger_texture(&cc.egui_ctx, "images/qr-dark.png", "icone_qr_sombre");
        let icone_settings_clair = theme::charger_texture(&cc.egui_ctx, "images/settings-light.png", "icone_settings_clair");
        let icone_settings_sombre = theme::charger_texture(&cc.egui_ctx, "images/settings-dark.png", "icone_settings_sombre");
        let icone_historic_clair = theme::charger_texture(&cc.egui_ctx, "images/historic-light.png", "icone_historic_clair");
        let icone_historic_sombre = theme::charger_texture(&cc.egui_ctx, "images/historic-dark.png", "icone_historic_sombre");
        let chemin_partage = if session.nom_imprimante.is_empty() {
            String::new()
        } else {
            partage::chemin_unc(&session.nom_partage)
        };
        let mut app = Self {
            session,
            journal,
            imprimantes,
            fichiers_choisis: Vec::new(),
            selection_locale: SelectionPages::default(),
            popup_selection_ouverte: false,
            popup_cible: None,
            popup_total: 0,
            popup_cochees: Vec::new(),
            message_statut: String::new(),
            etat_pare_feu: String::new(),
            etat_partage: String::new(),
            chemin_partage,
            rx_web: None,
            serveur_demarre: false,
            ip_locale,
            port_effectif: 0,
            imprimante_partagee,
            imprimante_ipp,
            icone_soleil,
            icone_lune,
            icone_qr_clair,
            icone_qr_sombre,
            icone_settings_clair,
            icone_settings_sombre,
            icone_historic_clair,
            icone_historic_sombre,
            daemon_mdns: None,
            montrer_qr: false,
            montrer_historique: false,
            qr_texture: None,
            qr_adresse: String::new(),
            panneau_parametres_ouvert: false,
            copie_instant: None,
            copie_partage_instant: None,
            montrer_apercu: false,
            apercu_texture: None,
            apercu_chemin: None,
            apercu_erreur: None,
            apercu_page: 1,
            apercu_cible: None,
            apercus_en_cours: HashMap::new(),
        };
        app.reanalyser_entrees_anciennes();
        app.demarrer_serveur();
        app.lancer_partage_auto();
        app
    }

    fn couleur_texte(&self) -> egui::Color32 {
        if self.session.mode_sombre { Palette::TEXTE } else { PaletteClaire::TEXTE }
    }
    fn couleur_texte_doux(&self) -> egui::Color32 {
        if self.session.mode_sombre { Palette::TEXTE_DOUX } else { PaletteClaire::TEXTE_DOUX }
    }
    fn couleur_carte(&self) -> egui::Color32 {
        if self.session.mode_sombre { Palette::CARTE } else { PaletteClaire::CARTE }
    }
    fn couleur_bordure(&self) -> egui::Color32 {
        if self.session.mode_sombre { Palette::BORDURE } else { PaletteClaire::BORDURE }
    }
    fn couleur_fond(&self) -> egui::Color32 {
        if self.session.mode_sombre { Palette::FOND } else { PaletteClaire::FOND }
    }
    fn couleur_accent(&self) -> egui::Color32 {
        if self.session.mode_sombre { Palette::ACCENT } else { PaletteClaire::ACCENT }
    }
    fn couleur_accent_fonce(&self) -> egui::Color32 {
        if self.session.mode_sombre { Palette::ACCENT_FONCE } else { PaletteClaire::ACCENT_FONCE }
    }
    fn couleur_succes(&self) -> egui::Color32 {
        if self.session.mode_sombre { Palette::SUCCES } else { PaletteClaire::SUCCES }
    }
    fn couleur_avert(&self) -> egui::Color32 {
        if self.session.mode_sombre { Palette::AVERT } else { PaletteClaire::AVERT }
    }
    fn couleur_erreur(&self) -> egui::Color32 {
        if self.session.mode_sombre { Palette::ERREUR } else { PaletteClaire::ERREUR }
    }

    fn basculer_theme(&mut self, ctx: &egui::Context) {
        self.session.mode_sombre = !self.session.mode_sombre;
        self.session.sauvegarder();
        if self.session.mode_sombre { theme::appliquer(ctx); } else { theme::appliquer_clair(ctx); }
    }

    fn demarrer_serveur(&mut self) {
        if self.serveur_demarre { return; }
        let (tx, rx) = channel::<()>();
        let logo = std::fs::read("images/logo.png").unwrap_or_default();
        let etat = EtatServeur {
            journal: self.journal.clone(),
            dossier: crate::config::chemin_documents_recus(),
            tx,
            imprimante: self.imprimante_partagee.clone(),
            logo: Arc::new(logo),
        };
        let mut port_essaye = self.session.port;
        let port_max = self.session.port.saturating_add(crate::config::PORT_RECHERCHE_MAX);
        while port_essaye <= port_max {
            match web::demarrer(port_essaye, etat.clone()) {
                Ok(port_reel) => {
                    self.port_effectif = port_reel;
                    self.rx_web = Some(rx);
                    self.serveur_demarre = true;
                    self.ip_locale = reseau_utils::adresse_ip_locale();
                    self.session.port = port_reel;
                    self.session.sauvegarder();
                    match pare_feu::assurer_regle(port_reel) {
                        Ok(_) => { self.etat_pare_feu = "Pare-feu configuré".into(); }
                        Err(e) => { self.etat_pare_feu = format!("Pare-feu : {}", e); }
                    }
                    self.daemon_mdns = mdns::publier(port_reel);
                    self.message_statut = format!("Serveur prêt sur le port {}.", port_reel);
                    return;
                }
                Err(_) => { port_essaye = port_essaye.saturating_add(1); }
            }
        }
        self.message_statut = "Aucun port disponible.".into();
    }

    fn lancer_partage_auto(&mut self) {
        if self.session.nom_imprimante.is_empty() {
            self.etat_partage = "Aucune imprimante sélectionnée.".into();
            return;
        }
        let imprimante = self.session.nom_imprimante.clone();
        let nom_partage = self.session.nom_partage.clone();
        self.chemin_partage = partage::chemin_unc(&nom_partage);
        self.etat_partage = "Activation du partage Windows…".into();
        let resultat = partage::activer_partage(&imprimante, &nom_partage);
        match resultat {
            Ok(_) => {
                self.etat_partage = format!("Partagée sous {}", self.chemin_partage);
                self.message_statut = format!("Partage Windows prêt : {}", self.chemin_partage);
            }
            Err(e) => {
                self.etat_partage = format!("Partage : {}", e);
                self.message_statut = format!("Partage Windows échoué : {}", e);
            }
        }
    }

    fn reparer_partage(&mut self) {
        if self.session.nom_imprimante.is_empty() {
            self.message_statut = "Sélectionnez d'abord une imprimante.".into();
            return;
        }
        let imprimante = self.session.nom_imprimante.clone();
        let nom_partage = self.session.nom_partage.clone();
        self.chemin_partage = partage::chemin_unc(&nom_partage);
        self.etat_partage = "Réparation du partage…".into();
        match partage::activer_partage(&imprimante, &nom_partage) {
            Ok(_) => {
                self.etat_partage = format!("Partagée sous {}", self.chemin_partage);
                self.message_statut = format!("Partage réparé : {}", self.chemin_partage);
            }
            Err(e) => {
                self.etat_partage = format!("Partage : {}", e);
                self.message_statut = format!("Réparation échouée : {}", e);
            }
        }
    }

    fn reparer_reseau(&mut self) {
        pare_feu::supprimer_toutes_regles();
        match pare_feu::assurer_regle(self.port_effectif) {
            Ok(_) => {
                self.ip_locale = reseau_utils::adresse_ip_locale();
                self.qr_texture = None;
                self.qr_adresse.clear();
                self.etat_pare_feu = "Pare-feu reconfiguré".into();
                self.message_statut = format!("Accès réseau réparé : http://{}:{}", self.ip_locale, self.port_effectif);
            }
            Err(e) => {
                self.etat_pare_feu = format!("Pare-feu : {}", e);
                self.message_statut = format!("Réparation échouée : {}", e);
            }
        }
    }

    fn rafraichir_imprimantes(&mut self) {
        self.imprimantes = printer::lister_imprimantes();
    }

    fn reanalyser_entrees_anciennes(&mut self) {
        let mut journal = self.journal.lock().unwrap();
        let mut modifie = false;
        for entree in journal.entrees.iter_mut() {
            if entree.statut == Statut::EnAttente || entree.statut == Statut::EnCours {
                let chemin = PathBuf::from(&entree.message);
                if !chemin.exists() {
                    entree.statut = Statut::Erreur;
                    entree.message = "Fichier absent au redémarrage".into();
                    modifie = true;
                } else if entree.version_analyse < crate::journal::VERSION_ANALYSE_PAGES {
                    entree.pages = 0;
                    entree.pages_reelles = 0;
                    entree.erreur_pages = None;
                    entree.version_analyse = crate::journal::VERSION_ANALYSE_PAGES;
                    modifie = true;
                }
            }
        }
        if modifie { journal.sauvegarder(); }
    }

    fn ajouter_fichier(&mut self, chemin: PathBuf) {
        if !self.fichiers_choisis.contains(&chemin) {
            self.fichiers_choisis.push(chemin);
        }
    }

    fn ouvrir_popup_selection(&mut self, chemin: PathBuf) {
        let nom_fichier = chemin.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let ext = chemin.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        let est_feuille = matches!(ext.as_str(), "xlsx" | "xlsm" | "xltx" | "xltm" | "xls" | "xlsb" | "ods" | "csv");
        let existant = {
            let j = self.journal.lock().unwrap();
            j.entrees.iter().find(|e| e.nom_fichier == nom_fichier && (e.message == chemin.to_string_lossy() || PathBuf::from(&e.message).exists())).cloned()
        };
        let id;
        match existant {
            Some(e) => { id = e.id.clone(); }
            None => {
                id = uuid::Uuid::new_v4().to_string();
                let mut j = self.journal.lock().unwrap();
                j.ajouter(Entree {
                    id: id.clone(),
                    expediteur: self.session.nom_utilisateur.clone(),
                    nom_fichier: nom_fichier.clone(),
                    pages: 0,
                    pages_reelles: 0,
                    erreur_pages: None,
                    version_analyse: 0,
                    horodatage: chrono::Local::now(),
                    statut: Statut::EnAttente,
                    message: chemin.to_string_lossy().to_string(),
                    selection: SelectionPages { est_feuille, ..SelectionPages::default() },
                });
            }
        }
        let journal_bg = self.journal.clone();
        let id_bg = id.clone();
        let chemin_bg = chemin.clone();
        let imp_bg = self.session.nom_imprimante.clone();
        std::thread::spawn(move || {
            let resultat = printer::compter_pages_document(&chemin_bg, &imp_bg);
            if let Ok(mut j) = journal_bg.lock() {
                if let Some(e) = j.entrees.iter_mut().find(|e| e.id == id_bg) {
                    match resultat {
                        Ok(pages) => { e.pages = pages; e.pages_reelles = pages; e.erreur_pages = None; }
                        Err(erreur) => { e.pages = 0; e.pages_reelles = 0; e.erreur_pages = Some(erreur); }
                    }
                    e.version_analyse = crate::journal::VERSION_ANALYSE_PAGES;
                }
                j.sauvegarder();
            }
        });
        self.popup_cible = Some(chemin.clone());
        self.selection_locale.est_feuille = est_feuille;
        self.popup_cochees = if self.selection_locale.mode == ModePages::Personnalise { self.selection_locale.pages_cochees.clone() } else { Vec::new() };
        self.popup_total = 0;
        self.message_statut = format!("Analyse du document « {} » en cours…", nom_fichier);
        self.popup_selection_ouverte = true;
    }

    fn demander_apercu(&mut self, chemin: PathBuf, page: u32, ctx: &egui::Context) {
        let cle = (chemin.to_string_lossy().to_string(), page);
        if self.apercus_en_cours.contains_key(&cle) { return; }
        if self.apercu_cible.as_ref() == Some(&chemin) && self.apercu_page == page && self.apercu_texture.is_some() { return; }

        self.apercu_erreur = None;
        self.apercu_texture = None;
        self.apercu_chemin = None;
        self.apercu_page = page;
        self.apercu_cible = Some(chemin.clone());
        self.montrer_apercu = true;

        let (tx, rx) = std::sync::mpsc::channel();
        self.apercus_en_cours.insert(cle, rx);

        let ctx_clone = ctx.clone();
        std::thread::spawn(move || {
            let resultat = match printer::generer_apercu(&chemin, page) {
                Ok(chemin_img) => {
                    match image::open(&chemin_img) {
                        Ok(img) => {
                            let img = img.to_rgba8();
                            let (w, h) = img.dimensions();
                            let color_image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &img.into_raw());
                            let _ = std::fs::remove_file(chemin_img);
                            Ok(color_image)
                        }
                        Err(e) => Err(format!("Impossible de charger l'image : {}", e)),
                    }
                }
                Err(e) => Err(e),
            };
            let _ = tx.send(resultat);
            ctx_clone.request_repaint();
        });
    }

    fn traiter_apercus(&mut self, ctx: &egui::Context) {
        let mut cles_a_retirer = Vec::new();
        for (cle, rx) in &self.apercus_en_cours {
            if let Ok(resultat) = rx.try_recv() {
                cles_a_retirer.push(cle.clone());
                if let Some(cible) = &self.apercu_cible {
                    if cle.0 == cible.to_string_lossy() && cle.1 == self.apercu_page {
                        match resultat {
                            Ok(img) => {
                                let handle = ctx.load_texture("apercu", img, egui::TextureOptions::LINEAR);
                                self.apercu_texture = Some(handle);
                            }
                            Err(e) => { self.apercu_erreur = Some(e); }
                        }
                    }
                }
            }
        }
        for cle in cles_a_retirer {
            self.apercus_en_cours.remove(&cle);
        }
    }

    fn imprimer_choisis(&mut self) {
        let imprimante = self.session.nom_imprimante.clone();
        if imprimante.is_empty() { self.message_statut = "Choisissez d'abord une imprimante.".into(); return; }
        if self.fichiers_choisis.is_empty() { self.message_statut = "Aucun fichier sélectionné.".into(); return; }
        let nom = self.session.nom_utilisateur.clone();
        for chemin in self.fichiers_choisis.clone() {
            let chemin_str = chemin.to_string_lossy().to_string();
            let ext = chemin.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
            let est_feuille = matches!(ext.as_str(), "xlsx" | "xlsm" | "xltx" | "xltm" | "xls" | "xlsb" | "ods" | "csv");
            let selection = SelectionPages { mode: self.selection_locale.mode.clone(), pages_cochees: self.selection_locale.pages_cochees.clone(), copies: self.selection_locale.copies_effectives(), est_feuille };
            let nom_fichier = chemin.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            let existant_id = {
                let j = self.journal.lock().unwrap();
                j.entrees.iter().find(|e| e.message == chemin_str && matches!(e.statut, Statut::EnAttente | Statut::EnCours)).map(|e| e.id.clone())
            };
            if let Some(id_existant) = existant_id {
                let mut j = self.journal.lock().unwrap();
                if let Some(e) = j.entrees.iter_mut().find(|x| x.id == id_existant) {
                    e.selection = selection;
                    e.expediteur = nom.clone();
                }
                j.sauvegarder();
            } else {
                let id = uuid::Uuid::new_v4().to_string();
                {
                    let mut j = self.journal.lock().unwrap();
                    j.ajouter(Entree { id: id.clone(), expediteur: nom.clone(), nom_fichier, pages: 0, pages_reelles: 0, erreur_pages: None, version_analyse: 0, horodatage: chrono::Local::now(), statut: Statut::EnAttente, message: chemin_str, selection });
                }
                let journal_bg = self.journal.clone();
                let chemin_bg = chemin.clone();
                let imp_bg = imprimante.clone();
                let id_bg = id.clone();
                std::thread::spawn(move || {
                    let resultat = printer::compter_pages_document(&chemin_bg, &imp_bg);
                    if let Ok(mut j) = journal_bg.lock() {
                        if let Some(e) = j.entrees.iter_mut().find(|e| e.id == id_bg) {
                            match resultat {
                                Ok(pages) => { e.pages = pages; e.pages_reelles = pages; e.erreur_pages = None; }
                                Err(erreur) => { e.pages = 0; e.pages_reelles = 0; e.erreur_pages = Some(erreur); }
                            }
                            e.version_analyse = crate::journal::VERSION_ANALYSE_PAGES;
                        }
                        j.sauvegarder();
                    }
                });
            }
        }
        self.fichiers_choisis.clear();
    }

    fn imprimer_file(&mut self) {
        let imprimante = self.session.nom_imprimante.clone();
        let mode = self.session.mode_couleur;
        let orientation = self.session.orientation;
        if imprimante.is_empty() { self.message_statut = "Choisissez d'abord une imprimante.".into(); return; }
        let a_imprimer: Vec<Entree> = { let j = self.journal.lock().unwrap(); j.entrees.iter().filter(|e| e.statut == Statut::EnAttente).cloned().collect() };
        if a_imprimer.is_empty() { self.message_statut = "Aucun document en attente.".into(); return; }
        let mut impression_lancee = false;
        let mut message_analyse = None;
        for e in a_imprimer {
            let chemin = PathBuf::from(&e.message);
            if !chemin.exists() {
                let mut j = self.journal.lock().unwrap();
                j.mettre_a_jour_statut(&e.id, Statut::Erreur, "Fichier absent".into());
                continue;
            }
            if e.pages_reelles == 0 {
                message_analyse = Some(e.erreur_pages.clone().unwrap_or_else(|| format!("Le calcul des pages de « {} » est encore en cours.", e.nom_fichier)));
                continue;
            }
            let spec = match e.selection.spec_imprimante(e.pages_reelles) {
                Ok(s) => s,
                Err(err) => { let mut j = self.journal.lock().unwrap(); j.mettre_a_jour_statut(&e.id, Statut::Erreur, err); continue; }
            };
            printer::imprimer_fichier(&imprimante, &chemin, e.id.clone(), self.journal.clone(), mode, orientation, spec, e.selection.copies_effectives(), e.selection.est_feuille);
            impression_lancee = true;
        }
        self.message_statut = if impression_lancee { "Impression lancée.".into() } else { message_analyse.unwrap_or_else(|| "Aucun document n'a pu être lancé.".into()) };
    }

    fn supprimer_entree(&mut self, id: &str) {
        let doit_annuler = { let j = self.journal.lock().unwrap(); j.entrees.iter().find(|e| e.id == id).map(|e| matches!(e.statut, Statut::EnAttente | Statut::EnCours)).unwrap_or(false) };
        if doit_annuler { printer::annuler_impression(); }
        let mut j = self.journal.lock().unwrap();
        j.supprimer(id);
    }

    fn vider_file(&mut self) { printer::annuler_impression(); self.journal.lock().unwrap().vider_file(); }
    fn supprimer_historique(&mut self) { self.journal.lock().unwrap().supprimer_historique(); }
    fn traiter_evenements(&mut self) { if let Some(rx) = &self.rx_web { while rx.try_recv().is_ok() {} } }

    fn assurer_qr(&mut self, ctx: &egui::Context) {
        let ip_fraiche = reseau_utils::adresse_ip_locale();
        if ip_fraiche != self.ip_locale { self.ip_locale = ip_fraiche; self.qr_texture = None; self.qr_adresse.clear(); }
        let adresse = format!("http://{}:{}", self.ip_locale, self.port_effectif);
        if self.qr_adresse == adresse && self.qr_texture.is_some() { return; }
        if let Some(img) = theme::generer_qr(&adresse) {
            let handle = ctx.load_texture("qr_popup", img, egui::TextureOptions::NEAREST);
            self.qr_texture = Some(handle);
            self.qr_adresse = adresse;
        }
    }

    fn ui_entete(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let texte = self.couleur_texte();
        let mut basculer = false; let mut ouvrir_qr = false; let mut ouvrir_parametres = false; let mut ouvrir_historique = false;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("OmniPrint").size(22.0).strong().color(texte));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(4.0);
                let icone_theme = if self.session.mode_sombre { self.icone_soleil.as_ref() } else { self.icone_lune.as_ref() };
                if bouton_icone(ui, self.session.mode_sombre, icone_theme, "☀") { basculer = true; }
                ui.add_space(4.0);
                let qr_icone = if self.session.mode_sombre { self.icone_qr_clair.as_ref() } else { self.icone_qr_sombre.as_ref() };
                if bouton_icone(ui, self.session.mode_sombre, qr_icone, "QR") { ouvrir_qr = true; }
                ui.add_space(4.0);
                let historic_icone = if self.session.mode_sombre { self.icone_historic_clair.as_ref() } else { self.icone_historic_sombre.as_ref() };
                if bouton_icone(ui, self.session.mode_sombre, historic_icone, "H") { ouvrir_historique = true; }
                ui.add_space(4.0);
                let settings_icone = if self.session.mode_sombre { self.icone_settings_clair.as_ref() } else { self.icone_settings_sombre.as_ref() };
                if bouton_icone(ui, self.session.mode_sombre, settings_icone, "⚙") { ouvrir_parametres = true; }
            });
        });
        if basculer { self.basculer_theme(ctx); }
        if ouvrir_qr { self.montrer_qr = !self.montrer_qr; }
        if ouvrir_historique { self.montrer_historique = !self.montrer_historique; }
        if ouvrir_parametres { self.panneau_parametres_ouvert = !self.panneau_parametres_ouvert; }
    }

    fn ui_qr_popup(&mut self, ctx: &egui::Context) {
        if !self.montrer_qr { return; }
        if !self.serveur_demarre { self.montrer_qr = false; return; }
        self.assurer_qr(ctx);
        let texte = self.couleur_texte();
        let carte = self.couleur_carte();
        let bordure = self.couleur_bordure();
        let accent_fonce = self.couleur_accent_fonce();
        let succes = self.couleur_succes();
        let texte_doux = self.couleur_texte_doux();
        let ip = self.ip_locale.clone();
        let port = self.port_effectif;
        let chemin_unc = self.chemin_partage.clone();
        let copie_recente = self.copie_instant.map(|t| t.elapsed() < Duration::from_secs(1)).unwrap_or(false);
        let copie_partage_recente = self.copie_partage_instant.map(|t| t.elapsed() < Duration::from_secs(1)).unwrap_or(false);
        let mut ouvert = self.montrer_qr;
        egui::Window::new("Partage d'accès")
            .collapsible(false)
            .resizable(false)
            .open(&mut ouvert)
            .frame(egui::Frame::none().fill(carte).stroke(egui::Stroke::new(1.0_f32, bordure)).rounding(egui::Rounding::same(14.0)).inner_margin(egui::Margin::same(24.0)))
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_min_width(400.0);
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new("Impression depuis un navigateur").size(15.0).strong().color(texte));
                    ui.add_space(10.0);
                    if let Some(tex) = &self.qr_texture {
                        let taille = egui::vec2(220.0, 220.0);
                        let (rect, _) = ui.allocate_exact_size(taille, egui::Sense::hover());
                        ui.painter().rect_filled(rect, egui::Rounding::same(10.0), egui::Color32::WHITE);
                        let marge = 10.0;
                        let rect_img = egui::Rect::from_min_size(rect.min + egui::vec2(marge, marge), egui::vec2(taille.x - marge * 2.0, taille.y - marge * 2.0));
                        ui.painter().image(tex.id(), rect_img, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), egui::Color32::WHITE);
                    } else {
                        ui.label(egui::RichText::new("QR code indisponible").color(texte));
                    }
                    ui.add_space(14.0);
                    let url_ip = format!("{}:{}", ip, port);
                    egui::Frame::none().fill(accent_fonce).rounding(egui::Rounding::same(10.0)).inner_margin(egui::Margin::symmetric(20.0, 12.0)).show(ui, |ui| {
                        ui.vertical_centered(|ui| { ui.label(egui::RichText::new(&url_ip).size(20.0).strong().color(egui::Color32::WHITE)); });
                    });
                    if self.daemon_mdns.is_some() {
                        ui.add_space(8.0);
                        let largeur_totale = 380.0; let largeur_bouton = 90.0;
                        ui.horizontal(|ui| {
                            ui.allocate_ui_with_layout(egui::vec2(largeur_totale - largeur_bouton, 30.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                let texte_lien = format!("omniprint.local:{}", port);
                                ui.label(egui::RichText::new(&texte_lien).size(13.0).color(accent_fonce));
                            });
                            ui.allocate_ui_with_layout(egui::vec2(largeur_bouton, 30.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let libelle = if copie_recente { "Copié" } else { "Copier" };
                                let couleur_fond = if copie_recente { succes } else { accent_fonce };
                                let bouton = egui::Button::new(egui::RichText::new(libelle).size(12.0).color(egui::Color32::WHITE)).fill(couleur_fond).min_size(egui::vec2(largeur_bouton - 4.0, 26.0));
                                if ui.add(bouton).clicked() {
                                    let url = format!("http://omniprint.local:{}/", port);
                                    ui.output_mut(|o| o.copied_text = url);
                                    self.copie_instant = Some(Instant::now());
                                }
                            });
                        });
                    }
                    ui.add_space(18.0);
                    ui.separator();
                    ui.add_space(14.0);
                    ui.label(egui::RichText::new("Impression depuis Word / Excel / PDF").size(15.0).strong().color(texte));
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Sur le PC client, copiez ce chemin dans").size(12.0).color(texte_doux));
                    ui.label(egui::RichText::new("« Ajouter une imprimante » de Windows").size(12.0).color(texte_doux));
                    ui.add_space(10.0);
                    if chemin_unc.is_empty() {
                        ui.label(egui::RichText::new("Sélectionnez d'abord une imprimante sur le PC serveur.").size(12.0).color(texte_doux));
                    } else {
                        egui::Frame::none().fill(accent_fonce).rounding(egui::Rounding::same(10.0)).inner_margin(egui::Margin::symmetric(18.0, 12.0)).show(ui, |ui| {
                            ui.vertical_centered(|ui| { ui.label(egui::RichText::new(&chemin_unc).size(18.0).strong().color(egui::Color32::WHITE)); });
                        });
                        ui.add_space(8.0);
                        let largeur_totale = 380.0; let largeur_bouton = 90.0;
                        ui.horizontal(|ui| {
                            ui.allocate_ui_with_layout(egui::vec2(largeur_totale - largeur_bouton, 30.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.label(egui::RichText::new(&self.etat_partage).size(11.0).color(texte_doux));
                            });
                            ui.allocate_ui_with_layout(egui::vec2(largeur_bouton, 30.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let libelle = if copie_partage_recente { "Copié" } else { "Copier" };
                                let couleur_fond = if copie_partage_recente { succes } else { accent_fonce };
                                let bouton = egui::Button::new(egui::RichText::new(libelle).size(12.0).color(egui::Color32::WHITE)).fill(couleur_fond).min_size(egui::vec2(largeur_bouton - 4.0, 26.0));
                                if ui.add(bouton).clicked() {
                                    ui.output_mut(|o| o.copied_text = chemin_unc.clone());
                                    self.copie_partage_instant = Some(Instant::now());
                                }
                            });
                        });
                    }
                });
            });
        self.montrer_qr = ouvert;
        if let Some(t) = self.copie_instant {
            if t.elapsed() >= Duration::from_secs(1) { self.copie_instant = None; } else { ctx.request_repaint_after(Duration::from_millis(50)); }
        }
        if let Some(t) = self.copie_partage_instant {
            if t.elapsed() >= Duration::from_secs(1) { self.copie_partage_instant = None; } else { ctx.request_repaint_after(Duration::from_millis(50)); }
        }
    }

    fn ui_historique_popup(&mut self, ctx: &egui::Context) {
        if !self.montrer_historique { return; }
        let texte = self.couleur_texte(); let texte_doux = self.couleur_texte_doux(); let carte = self.couleur_carte(); let bordure = self.couleur_bordure(); let accent = self.couleur_accent(); let succes = self.couleur_succes(); let erreur = self.couleur_erreur();
        let mut ouvert = self.montrer_historique;
        egui::Window::new("Historique").collapsible(false).resizable(true).open(&mut ouvert)
            .frame(egui::Frame::none().fill(carte).stroke(egui::Stroke::new(1.0_f32, bordure)).rounding(egui::Rounding::same(14.0)).inner_margin(egui::Margin::same(20.0)))
            .default_width(800.0).default_height(600.0).anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                let total_pages = { let j = self.journal.lock().unwrap(); j.pages_imprimees() };
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new("Historique").size(15.0).strong().color(texte));
                    badge(ui, &format!("{} pages imprimées", total_pages), accent);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| { if ui.button("Effacer l'historique").clicked() { self.supprimer_historique(); } });
                });
                ui.add_space(4.0);
                let entrees: Vec<Entree> = { let j = self.journal.lock().unwrap(); j.entrees.iter().filter(|e| e.statut == Statut::Imprime || e.statut == Statut::Erreur).rev().take(500).cloned().collect() };
                let largeur = ui.available_width();
                let cols = [0.11, 0.13, 0.18, 0.06, 0.18, 0.06, 0.09, 0.11, 0.08];
                let largeurs: Vec<f32> = cols.iter().map(|c| c * largeur).collect();
                Self::ligne_entete(ui, &largeurs, &["Date", "Expéditeur", "Fichier", "Pages", "Sélection", "Copies", "Statut", "Message", "Action"], texte);
                ui.separator();
                let mut a_supprimer: Option<String> = None;
                if entrees.is_empty() {
                    ui.label(egui::RichText::new("Aucun document traité.").italics().color(texte_doux));
                } else {
                    egui::ScrollArea::vertical().id_source("historique_scroll_popup").auto_shrink([false, true]).show(ui, |ui| {
                        for (i, e) in entrees.iter().enumerate() {
                            let fond = if i % 2 == 1 { Some(accent.gamma_multiply(0.06)) } else { None };
                            let (txt, col) = match e.statut { Statut::Imprime => ("imprimé", succes), Statut::Erreur => ("erreur", erreur), _ => ("", texte) };
                            let total = if e.pages_reelles > 0 { e.pages_reelles } else { e.pages };
                            let cellules = vec![
                                (e.horodatage.format("%d/%m %H:%M:%S").to_string(), texte_doux), (e.expediteur.clone(), texte), (e.nom_fichier.clone(), texte),
                                (format!("{}", e.selection.pages_effectives(total) * e.selection.copies_effectives()), texte), (e.selection.resume(total), texte_doux),
                                (format!("{}", e.selection.copies_effectives()), texte), (txt.to_string(), col), (e.message.clone(), texte_doux), ("Supprimer".to_string(), accent),
                            ];
                            if Self::ligne_tableau(ui, &largeurs, &cellules, fond).is_some() { a_supprimer = Some(e.id.clone()); }
                        }
                    });
                }
                if let Some(id) = a_supprimer { self.supprimer_entree(&id); }
            });
        self.montrer_historique = ouvert;
    }

    fn ui_panneau_parametres_popup(&mut self, ctx: &egui::Context) {
        if !self.panneau_parametres_ouvert { return; }
        let carte = self.couleur_carte(); let bordure = self.couleur_bordure(); let texte = self.couleur_texte(); let texte_doux = self.couleur_texte_doux();
        let mut ouvert = self.panneau_parametres_ouvert;
        egui::Window::new("Paramètres").collapsible(false).resizable(false).open(&mut ouvert)
            .frame(egui::Frame::none().fill(carte).stroke(egui::Stroke::new(1.0_f32, bordure)).rounding(egui::Rounding::same(14.0)).inner_margin(egui::Margin::same(24.0)))
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_min_width(420.0);
                ui.vertical(|ui| {
                    ui.label(egui::RichText::new("Rôle").size(14.0).strong().color(texte));
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new("Hôte (poste connecté à l'imprimante)").size(12.0).color(texte_doux));
                    ui.add_space(16.0);
                    ui.label(egui::RichText::new("Port du serveur").size(14.0).strong().color(texte));
                    ui.add_space(4.0);
                    let ports = crate::config::PORTS_PROPOSES;
                    let mut port_choisi = self.session.port;
                    egui::ComboBox::from_id_source("combo_port_popup").selected_text(format!("{}", port_choisi)).width(150.0).show_ui(ui, |ui| {
                        for p in ports.iter() { if ui.selectable_label(port_choisi == *p, p.to_string()).clicked() { port_choisi = *p; } }
                    });
                    if port_choisi != self.session.port {
                        self.session.port = port_choisi;
                        self.session.sauvegarder();
                        self.message_statut = "Port changé. Redémarrez l'application pour appliquer.".into();
                    }
                    ui.add_space(16.0);
                    ui.label(egui::RichText::new("Partage Windows de l'imprimante").size(14.0).strong().color(texte));
                    ui.add_space(4.0);
                    if self.chemin_partage.is_empty() {
                        ui.label(egui::RichText::new("Sélectionnez une imprimante.").size(12.0).color(texte_doux));
                    } else {
                        ui.label(egui::RichText::new(&self.chemin_partage).size(13.0).color(texte));
                        ui.label(egui::RichText::new(&self.etat_partage).size(11.0).color(texte_doux));
                    }
                    ui.add_space(6.0);
                    if ui.button("Réparer le partage Windows").clicked() { self.reparer_partage(); }
                    ui.add_space(16.0);
                    if ui.button("Réparer l'accès réseau (pare-feu)").clicked() { self.reparer_reseau(); }
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new(&self.etat_pare_feu).size(12.0).color(texte_doux));
                });
            });
        self.panneau_parametres_ouvert = ouvert;
    }

    fn ui_apercu_popup(&mut self, ctx: &egui::Context) {
        if !self.montrer_apercu { return; }
        let carte = self.couleur_carte(); let bordure = self.couleur_bordure(); let texte_doux = self.couleur_texte_doux(); let texte = self.couleur_texte(); let erreur = self.couleur_erreur(); let accent_fonce = self.couleur_accent_fonce();
        let mut ouvert = self.montrer_apercu;
        let page_affichee = self.apercu_page;
        let mut demander_précédente = false;
        let mut demander_suivante = false;
        egui::Window::new("Aperçu").collapsible(false).resizable(true).open(&mut ouvert)
            .frame(egui::Frame::none().fill(carte).stroke(egui::Stroke::new(1.0_f32, bordure)).rounding(egui::Rounding::same(14.0)).inner_margin(egui::Margin::same(20.0)))
            .default_width(600.0).default_height(700.0).anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("Page {}", page_affichee)).size(13.0).color(texte));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add(egui::Button::new(egui::RichText::new("Suivante").size(12.0).color(egui::Color32::WHITE)).fill(accent_fonce)).clicked() { demander_suivante = true; }
                        if ui.button("Précédente").clicked() { demander_précédente = true; }
                    });
                });
                ui.separator();
                if let Some(err) = &self.apercu_erreur {
                    ui.label(egui::RichText::new(err).color(erreur).size(14.0));
                } else if let Some(tex) = &self.apercu_texture {
                    let max_size = ui.available_size();
                    let aspect = tex.size_vec2().x / tex.size_vec2().y;
                    let display_size = if max_size.x / aspect < max_size.y { egui::vec2(max_size.x, max_size.x / aspect) } else { egui::vec2(max_size.y * aspect, max_size.y) };
                    let texture_id = tex.id();
                    ui.image((texture_id, display_size));
                } else {
                    ui.label(egui::RichText::new("Génération de l'aperçu...").color(texte_doux));
                }
            });
        self.montrer_apercu = ouvert;
        if demander_suivante {
            if let Some(cible) = self.apercu_cible.clone() { self.demander_apercu(cible, self.apercu_page + 1, ctx); }
        }
        if demander_précédente && self.apercu_page > 1 {
            if let Some(cible) = self.apercu_cible.clone() { self.demander_apercu(cible, self.apercu_page - 1, ctx); }
        }
        if !ouvert {
            if let Some(chemin) = self.apercu_chemin.take() { let _ = std::fs::remove_file(chemin); }
            self.apercu_texture = None;
            self.apercu_cible = None;
        }
    }

    fn ui_selection_pages(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let texte_doux = self.couleur_texte_doux(); let accent_fonce = self.couleur_accent_fonce();
        let mot_page = if self.selection_locale.est_feuille { "Feuilles" } else { "Pages" };
        ui.label(egui::RichText::new(format!("{} à imprimer", mot_page)).size(12.0).color(texte_doux));
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let mut nouveau_mode = self.selection_locale.mode.clone();
            let label_mode = match self.selection_locale.mode { ModePages::Toutes => format!("Toutes les {}", mot_page.to_lowercase()), ModePages::Personnalise => "Sélection par case".to_string() };
            egui::ComboBox::from_id_source("combo_mode_pages").selected_text(label_mode).width(220.0).show_ui(ui, |ui| {
                if ui.selectable_label(matches!(nouveau_mode, ModePages::Toutes), format!("Toutes les {}", mot_page.to_lowercase())).clicked() { nouveau_mode = ModePages::Toutes; }
                if ui.selectable_label(matches!(nouveau_mode, ModePages::Personnalise), "Sélection par case").clicked() { nouveau_mode = ModePages::Personnalise; }
            });
            if self.selection_locale.mode != nouveau_mode {
                self.selection_locale.mode = nouveau_mode;
                if self.selection_locale.mode == ModePages::Toutes { self.selection_locale.pages_cochees.clear(); }
            }
            if self.selection_locale.mode == ModePages::Personnalise {
                if ui.add(egui::Button::new(egui::RichText::new(format!("Choisir les {}…", mot_page.to_lowercase())).size(13.0).color(egui::Color32::WHITE)).fill(accent_fonce)).clicked() {
                    let cible = self.fichiers_choisis.first().cloned().or_else(|| {
                        let j = self.journal.lock().unwrap();
                        j.entrees.iter().rev().find(|e| e.statut == Statut::EnAttente).map(|e| PathBuf::from(&e.message))
                    });
                    match cible { Some(c) if c.exists() => self.ouvrir_popup_selection(c), _ => { self.message_statut = "Ajoutez d'abord un fichier.".into(); } }
                }
            }
            if ui.button("Aperçu").clicked() {
                let page_a_voir = if self.selection_locale.mode == ModePages::Personnalise { self.selection_locale.pages_cochees.first().copied().unwrap_or(1) } else { 1 };
                let fichier = self.fichiers_choisis.first().cloned().or_else(|| {
                    let j = self.journal.lock().unwrap();
                    j.entrees.iter().rev().find(|e| e.statut == Statut::EnAttente).map(|e| PathBuf::from(&e.message))
                });
                if let Some(f) = fichier { self.demander_apercu(f, page_a_voir, ctx); } else { self.message_statut = "Aucun fichier sélectionné pour l'aperçu.".into(); }
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Copies").size(12.0).color(texte_doux));
            let mut copies = self.selection_locale.copies_effectives();
            if ui.add(egui::DragValue::new(&mut copies).range(1..=999)).changed() { self.selection_locale.copies = copies; }
        });
    }

    fn ui_options_impression(&mut self, ui: &mut egui::Ui) {
        let texte_doux = self.couleur_texte_doux(); let texte = self.couleur_texte();
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Mode d'impression").size(12.0).color(texte_doux));
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    let mut couleur = matches!(self.session.mode_couleur, ModeCouleur::Couleur);
                    if ui.radio_value(&mut couleur, true, egui::RichText::new("Couleur").size(13.0).color(texte)).changed() { self.session.mode_couleur = ModeCouleur::Couleur; self.session.sauvegarder(); }
                    if ui.radio_value(&mut couleur, false, egui::RichText::new("Noir et blanc").size(13.0).color(texte)).changed() { self.session.mode_couleur = ModeCouleur::NoirBlanc; self.session.sauvegarder(); }
                });
            });
            ui.add_space(24.0);
            ui.vertical(|ui| {
                ui.label(egui::RichText::new("Orientation").size(12.0).color(texte_doux));
                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    let mut portrait = matches!(self.session.orientation, Orientation::Portrait);
                    if ui.radio_value(&mut portrait, true, egui::RichText::new("Portrait").size(13.0).color(texte)).changed() { self.session.orientation = Orientation::Portrait; self.session.sauvegarder(); }
                    if ui.radio_value(&mut portrait, false, egui::RichText::new("Paysage").size(13.0).color(texte)).changed() { self.session.orientation = Orientation::Paysage; self.session.sauvegarder(); }
                });
            });
        });
    }

    fn ui_popup_selection_pages(&mut self, ctx: &egui::Context) {
        if !self.popup_selection_ouverte { return; }
        let chemin_opt = self.popup_cible.clone();
        let (total, pret, erreur_pages) = if let Some(ref cible) = chemin_opt {
            let j = self.journal.lock().unwrap();
            match j.entrees.iter().find(|e| e.message == cible.to_string_lossy()) {
                Some(e) => (e.pages_reelles, e.pages_reelles > 0, e.erreur_pages.clone()),
                None => (self.popup_total, false, None),
            }
        } else { (self.popup_total, false, None) };
        self.popup_total = total;
        let carte = self.couleur_carte(); let bordure = self.couleur_bordure(); let accent_fonce = self.couleur_accent_fonce(); let texte = self.couleur_texte(); let texte_doux = self.couleur_texte_doux(); let couleur_erreur = self.couleur_erreur();
        let mot_page = if self.selection_locale.est_feuille { "feuille(s)" } else { "page(s)" };
        let mut ouvert = self.popup_selection_ouverte;
        let mut valider = false; let mut annuler = false;
        egui::Window::new(format!("Choisir les {}", if self.selection_locale.est_feuille { "feuilles" } else { "pages" })).collapsible(false).resizable(true).open(&mut ouvert)
            .frame(egui::Frame::none().fill(carte).stroke(egui::Stroke::new(1.0_f32, bordure)).rounding(egui::Rounding::same(14.0)).inner_margin(egui::Margin::same(20.0)))
            .default_width(520.0).default_height(460.0).anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_min_width(480.0);
                ui.horizontal(|ui| {
                    if pret { ui.label(egui::RichText::new(format!("Document : {} {}", total, mot_page)).size(14.0).color(texte)); }
                    else if let Some(erreur) = erreur_pages.as_ref() { ui.label(egui::RichText::new(format!("Analyse impossible : {}", erreur)).size(13.0).color(couleur_erreur)); }
                    else { ui.label(egui::RichText::new("Analyse du document en cours…").size(14.0).color(texte_doux)); }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(pret, egui::Button::new("Tout cocher")).clicked() { self.popup_cochees = (1..=total).collect(); }
                        if ui.add_enabled(pret, egui::Button::new("Tout décocher")).clicked() { self.popup_cochees.clear(); }
                    });
                });
                ui.separator();
                egui::ScrollArea::vertical().id_source("popup_pages_scroll").max_height(320.0).auto_shrink([false, true]).show(ui, |ui| {
                    if !pret {
                        if let Some(erreur) = erreur_pages.as_ref() { ui.label(egui::RichText::new(erreur).size(12.0).color(couleur_erreur)); }
                        else { ui.label(egui::RichText::new("Le nombre est en cours de calcul.").size(12.0).color(texte_doux)); }
                    } else {
                        let par_ligne = 8usize; let mut i = 1u32;
                        while i <= total {
                            ui.horizontal(|ui| {
                                for _ in 0..par_ligne {
                                    if i > total { break; }
                                    let mut coche = self.popup_cochees.contains(&i);
                                    if ui.checkbox(&mut coche, format!("{}", i)).changed() {
                                        if coche { if !self.popup_cochees.contains(&i) { self.popup_cochees.push(i); } }
                                        else { self.popup_cochees.retain(|p| *p != i); }
                                    }
                                    i += 1;
                                }
                            });
                        }
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(format!("{} {} sélectionnée(s) — {}", self.popup_cochees.len(), mot_page, if self.popup_cochees.is_empty() { "aucune".to_string() } else { formater_pages(&self.popup_cochees) })).size(12.0).color(texte_doux));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(pret && !self.popup_cochees.is_empty(), egui::Button::new(egui::RichText::new("Valider").size(13.0).color(egui::Color32::WHITE)).fill(accent_fonce)).clicked() { valider = true; }
                        if ui.button("Annuler").clicked() { annuler = true; }
                    });
                });
            });
        if valider {
            self.selection_locale.mode = ModePages::Personnalise;
            self.selection_locale.pages_cochees = self.popup_cochees.clone();
            self.popup_selection_ouverte = false;
            self.message_statut = format!("Sélection enregistrée : {} {}.", self.popup_cochees.len(), mot_page);
        }
        if annuler || !ouvert { self.popup_selection_ouverte = false; }
    }

    fn ui_depot_local(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        ui.label(egui::RichText::new("Déposer un ou plusieurs fichiers depuis cet ordinateur").size(12.0).color(self.couleur_texte_doux()));
        ui.add_space(4.0);
        let mut retirer_idx: Option<usize> = None;
        ui.horizontal(|ui| {
            if ui.button("Ouvrir et sélectionner").clicked() {
                if let Some(fichiers) = rfd::FileDialog::new().add_filter("Documents", &["pdf", "txt", "png", "jpg", "jpeg", "docx", "doc", "xlsx", "xls", "pptx", "ppt", "odt", "ods", "odp", "rtf"]).pick_files() {
                    for f in fichiers { self.ajouter_fichier(f); }
                }
            }
            if ui.add_enabled(!self.fichiers_choisis.is_empty(), egui::Button::new("Ajouter à la file")).clicked() { self.imprimer_choisis(); }
            if ui.add_enabled(!self.fichiers_choisis.is_empty(), egui::Button::new("Vider")).clicked() { self.fichiers_choisis.clear(); }
        });
        ui.add_space(6.0);
        self.ui_options_impression(ui);
        ui.add_space(6.0);
        self.ui_selection_pages(ui, ctx);
        ui.add_space(6.0);
        if !self.fichiers_choisis.is_empty() {
            let texte = self.couleur_texte();
            for (i, f) in self.fichiers_choisis.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(f.file_name().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()).size(12.0).color(texte));
                    if ui.small_button("Retirer").clicked() { retirer_idx = Some(i); }
                });
            }
        }
        if let Some(i) = retirer_idx { self.fichiers_choisis.remove(i); }
    }

    fn ligne_tableau(ui: &mut egui::Ui, largeurs: &[f32], cellules: &[(String, egui::Color32)], fond_alterne: Option<egui::Color32>) -> Option<usize> {
        let hauteur = 22.0; let largeur_totale: f32 = largeurs.iter().sum();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(largeur_totale, hauteur), egui::Sense::hover());
        if let Some(c) = fond_alterne { ui.painter().rect_filled(rect, egui::Rounding::ZERO, c); }
        let mut action_clic = None; let mut x = rect.left();
        for (idx, (texte, couleur)) in cellules.iter().enumerate() {
            let w = largeurs[idx];
            let r = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(w, hauteur));
            let mut enfant = ui.child_ui(r, egui::Layout::left_to_right(egui::Align::Center), None);
            if idx + 1 == cellules.len() && cellules.len() > 1 {
                if enfant.small_button(texte).clicked() { action_clic = Some(idx); }
            } else {
                enfant.add(egui::Label::new(egui::RichText::new(texte).size(12.0).color(*couleur)).truncate());
            }
            x += w;
        }
        action_clic
    }

    fn ligne_entete(ui: &mut egui::Ui, largeurs: &[f32], titres: &[&str], couleur: egui::Color32) {
        let hauteur = 22.0; let largeur_totale: f32 = largeurs.iter().sum();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(largeur_totale, hauteur), egui::Sense::hover());
        let mut x = rect.left();
        for (idx, titre) in titres.iter().enumerate() {
            let w = largeurs[idx];
            let r = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(w, hauteur));
            let mut enfant = ui.child_ui(r, egui::Layout::left_to_right(egui::Align::Center), None);
            enfant.add(egui::Label::new(egui::RichText::new(*titre).strong().size(12.0).color(couleur)).truncate());
            x += w;
        }
    }

    fn ui_file(&mut self, ui: &mut egui::Ui) {
        let texte = self.couleur_texte(); let texte_doux = self.couleur_texte_doux(); let avert = self.couleur_avert(); let accent = self.couleur_accent(); let succes = self.couleur_succes(); let erreur = self.couleur_erreur(); let accent_fonce = self.couleur_accent_fonce();
        let en_attente = { let j = self.journal.lock().unwrap(); j.entrees.iter().filter(|e| e.statut == Statut::EnAttente || e.statut == Statut::EnCours).count() };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("File d'attente").size(15.0).strong().color(texte));
            badge(ui, &format!("{} en attente", en_attente), avert);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.add(egui::Button::new(egui::RichText::new("Tout imprimer").size(13.0).color(egui::Color32::WHITE)).fill(accent_fonce)).clicked() { self.imprimer_file(); }
                if ui.add_enabled(en_attente > 0, egui::Button::new("Vider la file")).clicked() { self.vider_file(); }
            });
        });
        ui.add_space(4.0);
        let entrees: Vec<Entree> = { let j = self.journal.lock().unwrap(); j.entrees.iter().filter(|e| e.statut == Statut::EnAttente || e.statut == Statut::EnCours).rev().cloned().collect() };
        let largeur = ui.available_width();
        let cols = [0.16, 0.22, 0.07, 0.22, 0.07, 0.10, 0.16];
        let largeurs: Vec<f32> = cols.iter().map(|c| c * largeur).collect();
        Self::ligne_entete(ui, &largeurs, &["Expéditeur", "Fichier", "Pages", "Sélection", "Copies", "Statut", "Action"], texte);
        ui.separator();
        let mut a_supprimer: Option<String> = None;
        egui::ScrollArea::vertical().id_source("file_scroll").max_height(240.0).auto_shrink([false, true]).show(ui, |ui| {
            if entrees.is_empty() {
                ui.label(egui::RichText::new("Aucun document en attente.").italics().color(texte_doux));
            } else {
                for (i, e) in entrees.iter().enumerate() {
                    let fond = if i % 2 == 1 { Some(accent.gamma_multiply(0.06)) } else { None };
                    let (txt, col) = match e.statut { Statut::EnAttente => ("en attente", avert), Statut::EnCours => ("en cours", accent), Statut::Imprime => ("imprimé", succes), Statut::Erreur => ("erreur", erreur) };
                    let label_pages = if e.pages_reelles > 0 { format!("{}", e.pages_reelles) } else if e.erreur_pages.is_some() { "Erreur".to_string() } else { "Analyse…".to_string() };
                    let cellules = vec![
                        (e.expediteur.clone(), texte), (e.nom_fichier.clone(), texte), (label_pages, texte),
                        (e.selection.resume(e.pages_reelles), texte_doux), (format!("{}", e.selection.copies_effectives()), texte),
                        (txt.to_string(), col), ("Retirer".to_string(), accent),
                    ];
                    if Self::ligne_tableau(ui, &largeurs, &cellules, fond).is_some() { a_supprimer = Some(e.id.clone()); }
                }
            }
        });
        if let Some(id) = a_supprimer { self.supprimer_entree(&id); }
    }

    fn ui_imprimante_principale(&mut self, ui: &mut egui::Ui) {
        ui.label(egui::RichText::new("Imprimante connectée en USB à cet ordinateur").size(12.0).color(self.couleur_texte_doux()));
        ui.add_space(4.0);
        let mut changer_imp = false;
        let mut nouveau_nom = String::new();
        ui.horizontal(|ui| {
            let courant = if self.session.nom_imprimante.is_empty() { "Choisir une imprimante".to_string() } else { self.session.nom_imprimante.clone() };
            egui::ComboBox::from_id_source("combo_imprimante_principal").selected_text(courant).width(340.0).show_ui(ui, |ui| {
                for nom in &self.imprimantes {
                    if ui.selectable_label(self.session.nom_imprimante == *nom, nom).clicked() {
                        nouveau_nom = nom.clone();
                        changer_imp = true;
                    }
                }
            });
            if ui.button("Rafraîchir la liste").clicked() { self.rafraichir_imprimantes(); }
        });
        if changer_imp {
            self.session.nom_imprimante = nouveau_nom.clone();
            *self.imprimante_partagee.lock().unwrap() = nouveau_nom.clone();
            *self.imprimante_ipp.write() = nouveau_nom.clone();
            self.session.sauvegarder();
            self.lancer_partage_auto();
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.traiter_evenements();
        self.traiter_apercus(ctx);
        let carte = self.couleur_carte(); let bordure = self.couleur_bordure(); let fond = self.couleur_fond(); let texte_doux = self.couleur_texte_doux();
        egui::TopBottomPanel::top("entete").frame(egui::Frame::none().fill(carte).inner_margin(egui::Margin::symmetric(16.0, 10.0)).stroke(egui::Stroke::new(1.0_f32, bordure))).show(ctx, |ui| { self.ui_entete(ui, ctx); });
        egui::TopBottomPanel::bottom("pied").frame(egui::Frame::none().fill(carte).inner_margin(egui::Margin::symmetric(16.0, 8.0)).stroke(egui::Stroke::new(1.0_f32, bordure))).show(ctx, |ui| {
            ui.label(egui::RichText::new(&self.message_statut).size(12.0).color(texte_doux));
        });
        egui::CentralPanel::default().frame(egui::Frame::none().fill(fond).inner_margin(egui::Margin::symmetric(20.0, 16.0))).show(ctx, |ui| {
            egui::ScrollArea::vertical().id_source("main_scroll").auto_shrink([false, true]).show(ui, |ui| {
                self.ui_imprimante_principale(ui);
                separateur(ui);
                self.ui_depot_local(ui, ctx);
                separateur(ui);
                self.ui_file(ui);
            });
        });
        let taille = ctx.screen_rect().size();
        self.session.taille_fenetre = Some((taille.x, taille.y));
        self.ui_qr_popup(ctx);
        self.ui_historique_popup(ctx);
        self.ui_panneau_parametres_popup(ctx);
        self.ui_apercu_popup(ctx);
        self.ui_popup_selection_pages(ctx);
        ctx.request_repaint_after(Duration::from_millis(250));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(d) = self.daemon_mdns.take() { let _ = d.shutdown(); }
        printer::nettoyer_processus_office();
        self.session.sauvegarder();
        self.journal.lock().unwrap().sauvegarder();
    }
}
