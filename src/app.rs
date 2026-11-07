use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use eframe::egui;
use mdns_sd::ServiceDaemon;

use crate::journal::{Entree, Journal, Statut};
use crate::mdns;
use crate::pare_feu;
use crate::plage_pages::{formater_pages, ModePages, SelectionPages};
use crate::printer;
use crate::reseau_utils;
use crate::session::{ModeCouleur, Session};
use crate::theme::{self, badge, bouton_popup, bouton_theme, separateur, Palette, PaletteClaire};
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
    rx_web: Option<Receiver<()>>,
    serveur_demarre: bool,
    ip_locale: String,
    port_effectif: u16,
    imprimante_partagee: Arc<Mutex<String>>,
    icone_soleil: Option<egui::TextureHandle>,
    icone_lune: Option<egui::TextureHandle>,
    icone_popup: Option<egui::TextureHandle>,
    daemon_mdns: Option<ServiceDaemon>,
    montrer_qr: bool,
    qr_texture: Option<egui::TextureHandle>,
    qr_adresse: String,
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
        let icone_soleil = theme::charger_texture(&cc.egui_ctx, "images/light.png", "icone_soleil");
        let icone_lune = theme::charger_texture(&cc.egui_ctx, "images/dark.png", "icone_lune");
        let icone_popup = theme::charger_texture(&cc.egui_ctx, "images/popup.png", "icone_popup");
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
            rx_web: None,
            serveur_demarre: false,
            ip_locale,
            port_effectif: 0,
            imprimante_partagee,
            icone_soleil,
            icone_lune,
            icone_popup,
            daemon_mdns: None,
            montrer_qr: false,
            qr_texture: None,
            qr_adresse: String::new(),
        };
        app.reanalyser_entrees_anciennes();
        app.demarrer_serveur();
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
        if self.session.mode_sombre {
            theme::appliquer(ctx);
        } else {
            theme::appliquer_clair(ctx);
        }
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
                    match pare_feu::assurer_regle() {
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

    fn reparer_reseau(&mut self) {
        pare_feu::supprimer_toutes_regles();
        match pare_feu::assurer_regle() {
            Ok(_) => {
                self.ip_locale = reseau_utils::adresse_ip_locale();
                self.qr_texture = None;
                self.qr_adresse.clear();
                self.etat_pare_feu = "Pare-feu reconfiguré".into();
                self.message_statut = format!(
                    "Accès réseau réparé : http://{}:{}",
                    self.ip_locale, self.port_effectif
                );
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
        let documents = {
            let mut journal = self.journal.lock().unwrap();
            let mut documents = Vec::new();
            for entree in journal.entrees.iter_mut() {
                if entree.statut == Statut::EnAttente
                    && entree.version_analyse < crate::journal::VERSION_ANALYSE_PAGES
                {
                    let chemin = PathBuf::from(&entree.message);
                    entree.pages = 0;
                    entree.pages_reelles = 0;
                    entree.erreur_pages = None;
                    documents.push((entree.id.clone(), chemin));
                }
            }
            journal.sauvegarder();
            documents
        };

        let imprimante = self.session.nom_imprimante.clone();
        for (id, chemin) in documents {
            let journal_bg = self.journal.clone();
            let imprimante_bg = imprimante.clone();
            std::thread::spawn(move || {
                let resultat = printer::compter_pages_document(&chemin, &imprimante_bg);
                if let Ok(mut journal) = journal_bg.lock() {
                    if let Some(entree) = journal.entrees.iter_mut().find(|e| e.id == id) {
                        match resultat {
                            Ok(pages) => {
                                entree.pages = pages;
                                entree.pages_reelles = pages;
                                entree.erreur_pages = None;
                            }
                            Err(erreur) => {
                                entree.pages = 0;
                                entree.pages_reelles = 0;
                                entree.erreur_pages = Some(erreur);
                            }
                        }
                        entree.version_analyse = crate::journal::VERSION_ANALYSE_PAGES;
                    }
                    journal.sauvegarder();
                }
            });
        }
    }

    fn ajouter_fichier(&mut self, chemin: PathBuf) {
        if !self.fichiers_choisis.contains(&chemin) {
            self.fichiers_choisis.push(chemin);
        }
    }


    fn ouvrir_popup_selection(&mut self, chemin: PathBuf) {
        let nom_fichier = chemin
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();

        let existant = {
            let j = self.journal.lock().unwrap();
            j.entrees
                .iter()
                .find(|e| e.nom_fichier == nom_fichier && (e.message == chemin.to_string_lossy() || PathBuf::from(&e.message).exists()))
                .cloned()
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
                    selection: SelectionPages::default(),
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

        self.popup_cible = Some(chemin.clone());
        self.popup_cochees = if self.selection_locale.mode == ModePages::Personnalise {
            self.selection_locale.pages_cochees.clone()
        } else {
            Vec::new()
        };
        self.popup_total = 0;
        self.message_statut = format!("Analyse du document « {} » en cours…", nom_fichier);
        self.popup_selection_ouverte = true;
    }

    fn imprimer_choisis(&mut self) {
        let imprimante = self.session.nom_imprimante.clone();
        if imprimante.is_empty() {
            self.message_statut = "Choisissez d'abord une imprimante.".into();
            return;
        }
        if self.fichiers_choisis.is_empty() {
            self.message_statut = "Aucun fichier sélectionné.".into();
            return;
        }
        let nom = self.session.nom_utilisateur.clone();
        for chemin in self.fichiers_choisis.clone() {
            let id = uuid::Uuid::new_v4().to_string();
            let selection = SelectionPages {
                mode: self.selection_locale.mode.clone(),
                pages_cochees: self.selection_locale.pages_cochees.clone(),
                copies: self.selection_locale.copies_effectives(),
            };
            let nom_fichier = chemin
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            {
                let mut j = self.journal.lock().unwrap();
                j.ajouter(Entree {
                    id: id.clone(),
                    expediteur: nom.clone(),
                    nom_fichier,
                    pages: 0,
                    pages_reelles: 0,
                    erreur_pages: None,
                    version_analyse: 0,
                    horodatage: chrono::Local::now(),
                    statut: Statut::EnAttente,
                    message: chemin.to_string_lossy().to_string(),
                    selection,
                });
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
        }
        self.fichiers_choisis.clear();
    }

    fn imprimer_file(&mut self) {
        let imprimante = self.session.nom_imprimante.clone();
        let mode = self.session.mode_couleur;
        if imprimante.is_empty() {
            self.message_statut = "Choisissez d'abord une imprimante.".into();
            return;
        }
        let a_imprimer: Vec<Entree> = {
            let j = self.journal.lock().unwrap();
            j.entrees
                .iter()
                .filter(|e| e.statut == Statut::EnAttente)
                .cloned()
                .collect()
        };
        if a_imprimer.is_empty() {
            self.message_statut = "Aucun document en attente.".into();
            return;
        }
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
                message_analyse = Some(e.erreur_pages.clone().unwrap_or_else(|| {
                    format!("Le calcul des pages de « {} » est encore en cours.", e.nom_fichier)
                }));
                continue;
            }
            let total_pages = e.pages_reelles;
            let spec = match e.selection.spec_imprimante(total_pages) {
                Ok(s) => s,
                Err(err) => {
                    let mut j = self.journal.lock().unwrap();
                    j.mettre_a_jour_statut(&e.id, Statut::Erreur, err);
                    continue;
                }
            };
            printer::imprimer_fichier(
                &imprimante,
                &chemin,
                e.id.clone(),
                self.journal.clone(),
                mode,
                spec,
                e.selection.copies_effectives(),
            );
            impression_lancee = true;
        }
        self.message_statut = if impression_lancee {
            "Impression lancée.".into()
        } else {
            message_analyse.unwrap_or_else(|| "Aucun document n'a pu être lancé.".into())
        };
    }

    fn supprimer_entree(&mut self, id: &str) {
        let mut j = self.journal.lock().unwrap();
        j.supprimer(id);
    }

    fn vider_file(&mut self) {
        let mut j = self.journal.lock().unwrap();
        j.vider_file();
    }

    fn supprimer_historique(&mut self) {
        let mut j = self.journal.lock().unwrap();
        j.supprimer_historique();
    }

    fn traiter_evenements(&mut self) {
        if let Some(rx) = &self.rx_web {
            while rx.try_recv().is_ok() {}
        }
    }

    fn assurer_qr(&mut self, ctx: &egui::Context) {
        let ip_fraiche = reseau_utils::adresse_ip_locale();
        if ip_fraiche != self.ip_locale {
            self.ip_locale = ip_fraiche;
            self.qr_texture = None;
            self.qr_adresse.clear();
        }
        let adresse = format!("http://{}:{}", self.ip_locale, self.port_effectif);
        if self.qr_adresse == adresse && self.qr_texture.is_some() {
            return;
        }
        if let Some(img) = theme::generer_qr(&adresse, 512) {
            let handle = ctx.load_texture("qr_popup", img, egui::TextureOptions::NEAREST);
            self.qr_texture = Some(handle);
            self.qr_adresse = adresse;
        }
    }

    fn ui_entete(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        let texte = self.couleur_texte();
        let succes = self.couleur_succes();
        let erreur = self.couleur_erreur();
        let mut basculer = false;
        let mut ouvrir_qr = false;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("OmniPrint").size(22.0).strong().color(texte));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if self.serveur_demarre {
                    badge(ui, "SERVEUR ACTIF", succes);
                } else {
                    badge(ui, "SERVEUR INACTIF", erreur);
                }
                ui.add_space(4.0);
                let soleil = self.icone_soleil.as_ref();
                let lune = self.icone_lune.as_ref();
                if bouton_theme(ui, self.session.mode_sombre, soleil, lune) {
                    basculer = true;
                }
                ui.add_space(4.0);
                let popup = self.icone_popup.as_ref();
                if bouton_popup(ui, self.session.mode_sombre, popup) {
                    ouvrir_qr = true;
                }
            });
        });
        if basculer { self.basculer_theme(ctx); }
        if ouvrir_qr { self.montrer_qr = !self.montrer_qr; }
    }

    fn ui_qr_popup(&mut self, ctx: &egui::Context) {
        if !self.montrer_qr { return; }
        if !self.serveur_demarre { self.montrer_qr = false; return; }
        self.assurer_qr(ctx);

        let texte = self.couleur_texte();
        let texte_doux = self.couleur_texte_doux();
        let carte = self.couleur_carte();
        let bordure = self.couleur_bordure();
        let accent_fonce = self.couleur_accent_fonce();
        let ip = self.ip_locale.clone();
        let port = self.port_effectif;
        let etat_pf = self.etat_pare_feu.clone();
        let mut reparer = false;

        let mut ouvert = self.montrer_qr;
        egui::Window::new("Partage d'accès")
            .collapsible(false)
            .resizable(false)
            .open(&mut ouvert)
            .frame(
                egui::Frame::none()
                    .fill(carte)
                    .stroke(egui::Stroke::new(1.0_f32, bordure))
                    .rounding(egui::Rounding::same(14.0))
                    .inner_margin(egui::Margin::same(24.0)),
            )
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_min_width(360.0);
                ui.vertical_centered(|ui| {
                    ui.label(egui::RichText::new("Scanner ici").size(18.0).strong().color(texte));
                    ui.add_space(16.0);
                    if let Some(tex) = &self.qr_texture {
                        let taille = egui::vec2(240.0, 240.0);
                        let (rect, _) = ui.allocate_exact_size(taille, egui::Sense::hover());
                        ui.painter().rect_filled(rect, egui::Rounding::same(10.0), egui::Color32::WHITE);
                        let marge = 10.0;
                        let rect_img = egui::Rect::from_min_size(
                            rect.min + egui::vec2(marge, marge),
                            egui::vec2(taille.x - marge * 2.0, taille.y - marge * 2.0),
                        );
                        ui.painter().image(
                            tex.id(),
                            rect_img,
                            egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)),
                            egui::Color32::WHITE,
                        );
                    } else {
                        ui.label(egui::RichText::new("QR code indisponible").color(texte));
                    }
                    ui.add_space(18.0);
                    let url_ip = format!("{}:{}", ip, port);
                    egui::Frame::none()
                        .fill(accent_fonce)
                        .rounding(egui::Rounding::same(10.0))
                        .inner_margin(egui::Margin::symmetric(20.0, 14.0))
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    egui::RichText::new(&url_ip)
                                        .size(22.0)
                                        .strong()
                                        .color(egui::Color32::WHITE),
                                );
                            });
                        });
                    if self.daemon_mdns.is_some() {
                        ui.add_space(10.0);
                        let largeur_totale = 340.0;
                        let largeur_lien = largeur_totale * 0.75;
                        let largeur_bouton = largeur_totale * 0.25;
                        ui.horizontal(|ui| {
                            ui.allocate_ui_with_layout(
                                egui::vec2(largeur_lien, 32.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    let texte_lien = format!("omniprint.local:{}", port);
                                    ui.label(egui::RichText::new(&texte_lien).size(14.0).color(accent_fonce));
                                },
                            );
                            ui.allocate_ui_with_layout(
                                egui::vec2(largeur_bouton, 32.0),
                                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                                |ui| {
                                    if ui.button("Copier").clicked() {
                                        let url = format!("http://omniprint.local:{}/", port);
                                        ui.output_mut(|o| o.copied_text = url);
                                    }
                                },
                            );
                        });
                    }
                    ui.add_space(14.0);
                    ui.label(egui::RichText::new(&etat_pf).size(11.0).color(texte_doux));
                    ui.add_space(6.0);
                    if ui.button("Réparer l'accès réseau").clicked() {
                        reparer = true;
                    }
                });
            });
        self.montrer_qr = ouvert;
        if reparer { self.reparer_reseau(); }
    }

    fn ui_imprimante(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("Imprimante connectée en USB à cet ordinateur")
                .size(12.0)
                .color(self.couleur_texte_doux()),
        );
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let courant = if self.session.nom_imprimante.is_empty() {
                "Choisir une imprimante".to_string()
            } else {
                self.session.nom_imprimante.clone()
            };
            egui::ComboBox::from_id_source("combo_imprimante")
                .selected_text(courant)
                .width(340.0)
                .show_ui(ui, |ui| {
                    for nom in &self.imprimantes {
                        if ui
                            .selectable_label(self.session.nom_imprimante == *nom, nom)
                            .clicked()
                        {
                            self.session.nom_imprimante = nom.clone();
                            *self.imprimante_partagee.lock().unwrap() = nom.clone();
                            self.session.sauvegarder();
                        }
                    }
                });
            if ui.button("Rafraîchir la liste").clicked() {
                self.rafraichir_imprimantes();
            }
        });
        ui.add_space(6.0);
        ui.label(
            egui::RichText::new("Mode d'impression")
                .size(12.0)
                .color(self.couleur_texte_doux()),
        );
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            let mut courant = self.session.mode_couleur;
            let btn_couleur = egui::Button::new(
                egui::RichText::new("Couleur")
                    .size(13.0)
                    .color(if courant == ModeCouleur::Couleur {
                        egui::Color32::WHITE
                    } else {
                        self.couleur_texte()
                    }),
            )
            .fill(if courant == ModeCouleur::Couleur {
                self.couleur_accent_fonce()
            } else {
                self.couleur_carte()
            });
            if ui.add(btn_couleur).clicked() {
                courant = ModeCouleur::Couleur;
            }
            let btn_nb = egui::Button::new(
                egui::RichText::new("Noir et blanc")
                    .size(13.0)
                    .color(if courant == ModeCouleur::NoirBlanc {
                        egui::Color32::WHITE
                    } else {
                        self.couleur_texte()
                    }),
            )
            .fill(if courant == ModeCouleur::NoirBlanc {
                self.couleur_accent_fonce()
            } else {
                self.couleur_carte()
            });
            if ui.add(btn_nb).clicked() {
                courant = ModeCouleur::NoirBlanc;
            }
            if courant != self.session.mode_couleur {
                self.session.mode_couleur = courant;
                self.session.sauvegarder();
            }
        });
    }

    fn ui_selection_pages(&mut self, ui: &mut egui::Ui) {
        let texte_doux = self.couleur_texte_doux();
        let accent_fonce = self.couleur_accent_fonce();
        let accent = self.couleur_accent();

        ui.label(
            egui::RichText::new("Pages à imprimer")
                .size(12.0)
                .color(texte_doux),
        );
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            let mut nouveau_mode = self.selection_locale.mode.clone();
            let label_mode = match self.selection_locale.mode {
                ModePages::Toutes => "Toutes les pages",
                ModePages::Personnalise => "Sélection par case",
            };
            egui::ComboBox::from_id_source("combo_mode_pages")
                .selected_text(label_mode)
                .width(220.0)
                .show_ui(ui, |ui| {
                    if ui.selectable_label(matches!(nouveau_mode, ModePages::Toutes), "Toutes les pages").clicked() {
                        nouveau_mode = ModePages::Toutes;
                    }
                    if ui.selectable_label(matches!(nouveau_mode, ModePages::Personnalise), "Sélection par case").clicked() {
                        nouveau_mode = ModePages::Personnalise;
                    }
                });
            if self.selection_locale.mode != nouveau_mode {
                self.selection_locale.mode = nouveau_mode;
                if self.selection_locale.mode == ModePages::Toutes {
                    self.selection_locale.pages_cochees.clear();
                }
            }

            if self.selection_locale.mode == ModePages::Personnalise {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("Choisir les pages…").size(13.0).color(egui::Color32::WHITE),
                        )
                        .fill(accent_fonce),
                    )
                    .clicked()
                {
                    let cible = self.fichiers_choisis.first().cloned().or_else(|| {
                        let j = self.journal.lock().unwrap();
                        j.entrees
                            .iter()
                            .rev()
                            .find(|e| e.statut == Statut::EnAttente)
                            .map(|e| PathBuf::from(&e.message))
                    });
                    match cible {
                        Some(c) if c.exists() => self.ouvrir_popup_selection(c),
                        _ => { self.message_statut = "Ajoutez d'abord un fichier.".into(); }
                    }
                }
            }
        });

        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Copies").size(12.0).color(texte_doux));
            let mut copies = self.selection_locale.copies_effectives();
            if ui.add(egui::DragValue::new(&mut copies).range(1..=999)).changed() {
                self.selection_locale.copies = copies;
            }
        });

        let _ = accent;
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
        } else {
            (self.popup_total, false, None)
        };
        self.popup_total = total;

        let carte = self.couleur_carte();
        let bordure = self.couleur_bordure();
        let accent_fonce = self.couleur_accent_fonce();
        let texte = self.couleur_texte();
        let texte_doux = self.couleur_texte_doux();
        let couleur_erreur = self.couleur_erreur();

        let mut ouvert = self.popup_selection_ouverte;
        let mut valider = false;
        let mut annuler = false;

        egui::Window::new("Choisir les pages")
            .collapsible(false)
            .resizable(true)
            .open(&mut ouvert)
            .frame(
                egui::Frame::none()
                    .fill(carte)
                    .stroke(egui::Stroke::new(1.0_f32, bordure))
                    .rounding(egui::Rounding::same(14.0))
                    .inner_margin(egui::Margin::same(20.0)),
            )
            .default_width(520.0)
            .default_height(460.0)
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.set_min_width(480.0);
                ui.horizontal(|ui| {
                    if pret {
                        ui.label(egui::RichText::new(format!("Document : {} page(s)", total)).size(14.0).color(texte));
                    } else if let Some(erreur) = erreur_pages.as_ref() {
                        ui.label(egui::RichText::new(format!("Analyse impossible : {}", erreur)).size(13.0).color(couleur_erreur));
                    } else {
                        ui.label(egui::RichText::new("Analyse du document en cours…").size(14.0).color(texte_doux));
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(pret, egui::Button::new("Tout cocher")).clicked() {
                            self.popup_cochees = (1..=total).collect();
                        }
                        if ui.add_enabled(pret, egui::Button::new("Tout décocher")).clicked() {
                            self.popup_cochees.clear();
                        }
                    });
                });
                ui.separator();
                egui::ScrollArea::vertical()
                    .id_source("popup_pages_scroll")
                    .max_height(320.0)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if !pret {
                            if let Some(erreur) = erreur_pages.as_ref() {
                                ui.label(egui::RichText::new(erreur).size(12.0).color(couleur_erreur));
                            } else {
                                ui.label(egui::RichText::new("Le nombre de pages est en cours de calcul. La grille s'affichera dès que le document sera analysé.").size(12.0).color(texte_doux));
                            }
                        } else {
                            let par_ligne = 8usize;
                            let mut i = 1u32;
                            while i <= total {
                                ui.horizontal(|ui| {
                                    for _ in 0..par_ligne {
                                        if i > total { break; }
                                        let mut coche = self.popup_cochees.contains(&i);
                                        if ui.checkbox(&mut coche, format!("{}", i)).changed() {
                                            if coche {
                                                if !self.popup_cochees.contains(&i) {
                                                    self.popup_cochees.push(i);
                                                }
                                            } else {
                                                self.popup_cochees.retain(|p| *p != i);
                                            }
                                        }
                                        i += 1;
                                    }
                                });
                            }
                        }
                    });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} page(s) sélectionnée(s) — {}",
                            self.popup_cochees.len(),
                            if self.popup_cochees.is_empty() {
                                "aucune".to_string()
                            } else {
                                formater_pages(&self.popup_cochees)
                            }
                        ))
                        .size(12.0)
                        .color(texte_doux),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(pret && !self.popup_cochees.is_empty(), egui::Button::new(
                                egui::RichText::new("Valider").size(13.0).color(egui::Color32::WHITE),
                            ).fill(accent_fonce))
                            .clicked()
                        {
                            valider = true;
                        }
                        if ui.button("Annuler").clicked() {
                            annuler = true;
                        }
                    });
                });
            });

        if valider {
            self.selection_locale.mode = ModePages::Personnalise;
            self.selection_locale.pages_cochees = self.popup_cochees.clone();
            self.popup_selection_ouverte = false;
            self.message_statut = format!(
                "Sélection enregistrée : {} page(s).",
                self.popup_cochees.len()
            );
        }
        if annuler || !ouvert {
            self.popup_selection_ouverte = false;
        }
    }

    fn ui_depot_local(&mut self, ui: &mut egui::Ui) {
        ui.label(
            egui::RichText::new("Déposer un ou plusieurs fichiers depuis cet ordinateur")
                .size(12.0)
                .color(self.couleur_texte_doux()),
        );
        ui.add_space(4.0);
        let mut retirer_idx: Option<usize> = None;
        ui.horizontal(|ui| {
            if ui.button("Ouvrir et sélectionner").clicked() {
                if let Some(fichiers) = rfd::FileDialog::new()
                    .add_filter(
                        "Documents",
                        &["pdf", "txt", "png", "jpg", "jpeg", "docx", "doc", "xlsx", "xls", "pptx", "ppt", "odt", "ods", "odp", "rtf"],
                    )
                    .pick_files()
                {
                    for f in fichiers {
                        self.ajouter_fichier(f);
                    }
                }
            }
            if ui
                .add_enabled(
                    !self.fichiers_choisis.is_empty(),
                    egui::Button::new("Ajouter à la file"),
                )
                .clicked()
            {
                self.imprimer_choisis();
            }
            if ui
                .add_enabled(!self.fichiers_choisis.is_empty(), egui::Button::new("Vider"))
                .clicked()
            {
                self.fichiers_choisis.clear();
            }
        });
        ui.add_space(6.0);
        self.ui_selection_pages(ui);
        ui.add_space(6.0);
        if !self.fichiers_choisis.is_empty() {
            let texte = self.couleur_texte();
            for (i, f) in self.fichiers_choisis.iter().enumerate() {
                ui.horizontal(|ui| {
                    ui.label(
                        egui::RichText::new(
                            f.file_name()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_default(),
                        )
                        .size(12.0)
                        .color(texte),
                    );
                    if ui.small_button("Retirer").clicked() {
                        retirer_idx = Some(i);
                    }
                });
            }
        }
        if let Some(i) = retirer_idx {
            self.fichiers_choisis.remove(i);
        }
    }

    fn ligne_tableau(
        ui: &mut egui::Ui,
        largeurs: &[f32],
        cellules: &[(String, egui::Color32)],
        fond_alterne: Option<egui::Color32>,
    ) -> Option<usize> {
        let hauteur = 22.0;
        let largeur_totale: f32 = largeurs.iter().sum();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(largeur_totale, hauteur), egui::Sense::hover());
        if let Some(c) = fond_alterne {
            ui.painter().rect_filled(rect, egui::Rounding::ZERO, c);
        }
        let mut action_clic = None;
        let mut x = rect.left();
        for (idx, (texte, couleur)) in cellules.iter().enumerate() {
            let w = largeurs[idx];
            let r = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(w, hauteur));
            let mut enfant = ui.child_ui(r, egui::Layout::left_to_right(egui::Align::Center), None);
            if idx + 1 == cellules.len() && cellules.len() > 1 {
                if enfant.small_button(texte).clicked() {
                    action_clic = Some(idx);
                }
            } else {
                enfant.add(
                    egui::Label::new(egui::RichText::new(texte).size(12.0).color(*couleur))
                        .truncate(),
                );
            }
            x += w;
        }
        action_clic
    }

    fn ligne_entete(ui: &mut egui::Ui, largeurs: &[f32], titres: &[&str], couleur: egui::Color32) {
        let hauteur = 22.0;
        let largeur_totale: f32 = largeurs.iter().sum();
        let (rect, _) = ui.allocate_exact_size(egui::vec2(largeur_totale, hauteur), egui::Sense::hover());
        let mut x = rect.left();
        for (idx, titre) in titres.iter().enumerate() {
            let w = largeurs[idx];
            let r = egui::Rect::from_min_size(egui::pos2(x, rect.top()), egui::vec2(w, hauteur));
            let mut enfant = ui.child_ui(r, egui::Layout::left_to_right(egui::Align::Center), None);
            enfant.add(
                egui::Label::new(egui::RichText::new(*titre).strong().size(12.0).color(couleur))
                    .truncate(),
            );
            x += w;
        }
    }

    fn ui_file(&mut self, ui: &mut egui::Ui) {
        let texte = self.couleur_texte();
        let texte_doux = self.couleur_texte_doux();
        let avert = self.couleur_avert();
        let accent = self.couleur_accent();
        let succes = self.couleur_succes();
        let erreur = self.couleur_erreur();
        let accent_fonce = self.couleur_accent_fonce();

        let en_attente = {
            let j = self.journal.lock().unwrap();
            j.entrees
                .iter()
                .filter(|e| e.statut == Statut::EnAttente || e.statut == Statut::EnCours)
                .count()
        };

        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("File d'attente").size(15.0).strong().color(texte));
            badge(ui, &format!("{} en attente", en_attente), avert);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui
                    .add(
                        egui::Button::new(
                            egui::RichText::new("Tout imprimer").size(13.0).color(egui::Color32::WHITE),
                        )
                        .fill(accent_fonce),
                    )
                    .clicked()
                {
                    self.imprimer_file();
                }
                if ui
                    .add_enabled(en_attente > 0, egui::Button::new("Vider la file"))
                    .clicked()
                {
                    self.vider_file();
                }
            });
        });
        ui.add_space(4.0);

        let entrees: Vec<Entree> = {
            let j = self.journal.lock().unwrap();
            j.entrees
                .iter()
                .filter(|e| e.statut == Statut::EnAttente || e.statut == Statut::EnCours)
                .rev()
                .cloned()
                .collect()
        };

        let largeur = ui.available_width();
        let cols = [0.16, 0.22, 0.07, 0.22, 0.07, 0.10, 0.16];
        let largeurs: Vec<f32> = cols.iter().map(|c| c * largeur).collect();

        let titres = ["Expéditeur", "Fichier", "Pages", "Sélection", "Copies", "Statut", "Action"];
        Self::ligne_entete(ui, &largeurs, &titres, texte);
        ui.separator();

        let mut a_supprimer: Option<String> = None;

        egui::ScrollArea::vertical()
            .id_source("file_scroll")
            .max_height(240.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if entrees.is_empty() {
                    ui.label(egui::RichText::new("Aucun document en attente.").italics().color(texte_doux));
                } else {
                    for (i, e) in entrees.iter().enumerate() {
                        let fond = if i % 2 == 1 { Some(accent.gamma_multiply(0.06)) } else { None };
                        let (txt, col) = match e.statut {
                            Statut::EnAttente => ("en attente", avert),
                            Statut::EnCours => ("en cours", accent),
                            Statut::Imprime => ("imprimé", succes),
                            Statut::Erreur => ("erreur", erreur),
                        };
                        let total = e.pages_reelles;
                        let label_pages = if e.pages_reelles > 0 {
                            format!("{}", total)
                        } else if e.erreur_pages.is_some() {
                            "Erreur".to_string()
                        } else {
                            "Analyse…".to_string()
                        };
                        let cellules = vec![
                            (e.expediteur.clone(), texte),
                            (e.nom_fichier.clone(), texte),
                            (label_pages, texte),
                            (e.selection.resume(total), texte_doux),
                            (format!("{}", e.selection.copies_effectives()), texte),
                            (txt.to_string(), col),
                            ("Retirer".to_string(), accent),
                        ];
                        if Self::ligne_tableau(ui, &largeurs, &cellules, fond).is_some() {
                            a_supprimer = Some(e.id.clone());
                        }
                    }
                }
            });

        if let Some(id) = a_supprimer {
            self.supprimer_entree(&id);
        }
    }

    fn ui_historique(&mut self, ui: &mut egui::Ui) {
        let texte = self.couleur_texte();
        let texte_doux = self.couleur_texte_doux();
        let accent = self.couleur_accent();
        let succes = self.couleur_succes();
        let erreur = self.couleur_erreur();

        let total_pages = { let j = self.journal.lock().unwrap(); j.total_pages() };
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Historique").size(15.0).strong().color(texte));
            badge(ui, &format!("{} pages imprimées", total_pages), accent);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Effacer l'historique").clicked() {
                    self.supprimer_historique();
                }
            });
        });
        ui.add_space(4.0);

        let entrees: Vec<Entree> = {
            let j = self.journal.lock().unwrap();
            j.entrees
                .iter()
                .filter(|e| e.statut == Statut::Imprime || e.statut == Statut::Erreur)
                .rev()
                .take(500)
                .cloned()
                .collect()
        };

        let largeur = ui.available_width();
        let cols = [0.11, 0.13, 0.18, 0.06, 0.18, 0.06, 0.09, 0.11, 0.08];
        let largeurs: Vec<f32> = cols.iter().map(|c| c * largeur).collect();

        let titres = ["Date", "Expéditeur", "Fichier", "Pages", "Sélection", "Copies", "Statut", "Message", "Action"];
        Self::ligne_entete(ui, &largeurs, &titres, texte);
        ui.separator();

        let mut a_supprimer: Option<String> = None;

        if entrees.is_empty() {
            ui.label(egui::RichText::new("Aucun document traité.").italics().color(texte_doux));
        } else {
            for (i, e) in entrees.iter().enumerate() {
                let fond = if i % 2 == 1 { Some(accent.gamma_multiply(0.06)) } else { None };
                let (txt, col) = match e.statut {
                    Statut::Imprime => ("imprimé", succes),
                    Statut::Erreur => ("erreur", erreur),
                    _ => ("", texte),
                };
                let total = if e.pages_reelles > 0 { e.pages_reelles } else { e.pages };
                let cellules = vec![
                    (e.horodatage.format("%d/%m %H:%M:%S").to_string(), texte_doux),
                    (e.expediteur.clone(), texte),
                    (e.nom_fichier.clone(), texte),
                    (format!("{}", total), texte),
                    (e.selection.resume(total), texte_doux),
                    (format!("{}", e.selection.copies_effectives()), texte),
                    (txt.to_string(), col),
                    (e.message.clone(), texte_doux),
                    ("Supprimer".to_string(), accent),
                ];
                if Self::ligne_tableau(ui, &largeurs, &cellules, fond).is_some() {
                    a_supprimer = Some(e.id.clone());
                }
            }
        }

        if let Some(id) = a_supprimer {
            self.supprimer_entree(&id);
        }
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.traiter_evenements();
        let carte = self.couleur_carte();
        let bordure = self.couleur_bordure();
        let fond = self.couleur_fond();
        let texte_doux = self.couleur_texte_doux();

        egui::TopBottomPanel::top("entete")
            .frame(
                egui::Frame::none()
                    .fill(carte)
                    .inner_margin(egui::Margin::symmetric(16.0, 10.0))
                    .stroke(egui::Stroke::new(1.0_f32, bordure)),
            )
            .show(ctx, |ui| {
                self.ui_entete(ui, ctx);
            });
        egui::TopBottomPanel::bottom("pied")
            .frame(
                egui::Frame::none()
                    .fill(carte)
                    .inner_margin(egui::Margin::symmetric(16.0, 8.0))
                    .stroke(egui::Stroke::new(1.0_f32, bordure)),
            )
            .show(ctx, |ui| {
                ui.label(egui::RichText::new(&self.message_statut).size(12.0).color(texte_doux));
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::none()
                    .fill(fond)
                    .inner_margin(egui::Margin::symmetric(20.0, 16.0)),
            )
            .show(ctx, |ui| {
                egui::ScrollArea::vertical()
                    .id_source("main_scroll")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        self.ui_imprimante(ui);
                        separateur(ui);
                        self.ui_depot_local(ui);
                        separateur(ui);
                        self.ui_file(ui);
                        separateur(ui);
                        self.ui_historique(ui);
                        ui.add_space(8.0);
                    });
            });
        self.ui_qr_popup(ctx);
        self.ui_popup_selection_pages(ctx);
        ctx.request_repaint_after(Duration::from_millis(250));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(d) = self.daemon_mdns.take() {
            let _ = d.shutdown();
        }
        printer::nettoyer_processus_office();
        self.session.sauvegarder();
        self.journal.lock().unwrap().sauvegarder();
    }
}

