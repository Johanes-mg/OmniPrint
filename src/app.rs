use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use eframe::egui;
use mdns_sd::ServiceDaemon;

use crate::journal::{Entree, Journal, Statut};
use crate::mdns;
use crate::pare_feu;
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
    message_statut: String,
    rx_web: Option<Receiver<()>>,
    serveur_demarre: bool,
    ip_locale: String,
    port_effectif: u16,
    imprimante_partagee: Arc<Mutex<String>>,
    icone_soleil: Option<egui::TextureHandle>,
    icone_lune: Option<egui::TextureHandle>,
    icone_popup: Option<egui::TextureHandle>,
    copie_recente: Option<Instant>,
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
            message_statut: String::new(),
            rx_web: None,
            serveur_demarre: false,
            ip_locale,
            port_effectif: 0,
            imprimante_partagee,
            icone_soleil,
            icone_lune,
            icone_popup,
            copie_recente: None,
            daemon_mdns: None,
            montrer_qr: false,
            qr_texture: None,
            qr_adresse: String::new(),
        };
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
                    let _ = pare_feu::assurer_regle();
                    self.daemon_mdns = mdns::publier(port_reel);
                    self.message_statut = format!("Serveur prêt sur le port {}.", port_reel);
                    return;
                }
                Err(_) => {
                    port_essaye = port_essaye.saturating_add(1);
                }
            }
        }
        self.message_statut = "Aucun port disponible.".into();
    }

    fn rafraichir_imprimantes(&mut self) {
        self.imprimantes = printer::lister_imprimantes();
    }

    fn ajouter_fichier(&mut self, chemin: PathBuf) {
        if !self.fichiers_choisis.contains(&chemin) {
            self.fichiers_choisis.push(chemin);
        }
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
            let pages = printer::compter_pages_pdf(&chemin);
            let nom_fichier = chemin
                .file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default();
            let id = uuid::Uuid::new_v4().to_string();
            {
                let mut j = self.journal.lock().unwrap();
                j.ajouter(Entree {
                    id,
                    expediteur: nom.clone(),
                    nom_fichier,
                    pages,
                    horodatage: chrono::Local::now(),
                    statut: Statut::EnAttente,
                    message: chemin.to_string_lossy().to_string(),
                });
            }
        }
        self.fichiers_choisis.clear();
        self.imprimer_file();
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
        for e in a_imprimer {
            let chemin = PathBuf::from(&e.message);
            if !chemin.exists() {
                let mut j = self.journal.lock().unwrap();
                j.mettre_a_jour_statut(&e.id, Statut::Erreur, "Fichier absent".into());
                continue;
            }
            printer::imprimer_fichier(&imprimante, &chemin, e.id.clone(), self.journal.clone(), mode);
        }
        self.message_statut = "Impression lancée.".into();
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
        if basculer {
            self.basculer_theme(ctx);
        }
        if ouvrir_qr {
            self.montrer_qr = !self.montrer_qr;
        }
    }

    fn ui_qr_popup(&mut self, ctx: &egui::Context) {
        if !self.montrer_qr {
            return;
        }
        if !self.serveur_demarre {
            self.montrer_qr = false;
            return;
        }
        self.assurer_qr(ctx);

        let texte = self.couleur_texte();
        let carte = self.couleur_carte();
        let bordure = self.couleur_bordure();
        let accent_fonce = self.couleur_accent_fonce();
        let ip = self.ip_locale.clone();
        let port = self.port_effectif;

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
                ui.set_min_width(340.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        egui::RichText::new("Scanner ici")
                            .size(18.0)
                            .strong()
                            .color(texte),
                    );
                    ui.add_space(16.0);

                    if let Some(tex) = &self.qr_texture {
                        let taille = egui::vec2(240.0, 240.0);
                        let (rect, _) = ui.allocate_exact_size(taille, egui::Sense::hover());
                        ui.painter().rect_filled(
                            rect,
                            egui::Rounding::same(10.0),
                            egui::Color32::WHITE,
                        );
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
                                    ui.label(
                                        egui::RichText::new(&texte_lien)
                                            .size(14.0)
                                            .color(accent_fonce),
                                    );
                                },
                            );
                            ui.allocate_ui_with_layout(
                                egui::vec2(largeur_bouton, 32.0),
                                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                                |ui| {
                                    if ui.button("Copier").clicked() {
                                        let url = format!("http://omniprint.local:{}", port);
                                        ui.output_mut(|o| o.copied_text = url);
                                    }
                                },
                            );
                        });
                    }
                });
            });
        self.montrer_qr = ouvert;
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
                        &["pdf", "txt", "png", "jpg", "jpeg", "docx", "xlsx"],
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
                    egui::Button::new("Imprimer la sélection"),
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
                            egui::RichText::new("Tout imprimer")
                                .size(13.0)
                                .color(egui::Color32::WHITE),
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
        let cols = [0.18, 0.30, 0.08, 0.10, 0.14, 0.20];
        let largeurs: Vec<f32> = cols.iter().map(|c| c * largeur).collect();

        let titres = ["Expéditeur", "Fichier", "Pages", "Heure", "Statut", "Action"];
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
                        let cellules = vec![
                            (e.expediteur.clone(), texte),
                            (e.nom_fichier.clone(), texte),
                            (format!("{}", e.pages), texte),
                            (e.horodatage.format("%H:%M:%S").to_string(), texte_doux),
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
        let cols = [0.14, 0.16, 0.22, 0.07, 0.10, 0.21, 0.10];
        let largeurs: Vec<f32> = cols.iter().map(|c| c * largeur).collect();

        let titres = ["Date", "Expéditeur", "Fichier", "Pages", "Statut", "Message", "Action"];
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
                let cellules = vec![
                    (e.horodatage.format("%d/%m %H:%M:%S").to_string(), texte_doux),
                    (e.expediteur.clone(), texte),
                    (e.nom_fichier.clone(), texte),
                    (format!("{}", e.pages), texte),
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
        if let Some(t) = self.copie_recente {
            if t.elapsed() >= Duration::from_secs(2) {
                self.copie_recente = None;
            }
        }
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
        ctx.request_repaint_after(Duration::from_millis(200));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if let Some(d) = self.daemon_mdns.take() {
            let _ = d.shutdown();
        }
        self.session.sauvegarder();
        self.journal.lock().unwrap().sauvegarder();
    }
}
