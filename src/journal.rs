use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

use crate::config;
use crate::plage_pages::SelectionPages;

pub const VERSION_ANALYSE_PAGES: u32 = 2;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum Statut { EnAttente, EnCours, Imprime, Erreur }

impl Default for Statut { fn default() -> Self { Statut::EnAttente } }

fn selection_defaut() -> SelectionPages { SelectionPages::default() }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entree {
    pub id: String,
    pub expediteur: String,
    pub nom_fichier: String,
    pub pages: u32,
    #[serde(default)]
    pub pages_reelles: u32,
    #[serde(default)]
    pub erreur_pages: Option<String>,
    #[serde(default)]
    pub version_analyse: u32,
    pub horodatage: DateTime<Local>,
    pub statut: Statut,
    pub message: String,
    #[serde(default = "selection_defaut")]
    pub selection: SelectionPages,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Journal { pub entrees: Vec<Entree> }

impl Journal {
    pub fn charger() -> Self {
        let chemin = config::chemin_journal();
        match std::fs::read_to_string(&chemin) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn sauvegarder(&self) {
        let chemin = config::chemin_journal();
        if let Ok(s) = serde_json::to_string_pretty(self) { let _ = std::fs::write(chemin, s); }
    }

    pub fn ajouter(&mut self, e: Entree) {
        self.entrees.push(e);
        if self.entrees.len() > 5000 { let surplus = self.entrees.len() - 5000; self.entrees.drain(0..surplus); }
        self.sauvegarder();
    }

    pub fn mettre_a_jour_statut(&mut self, id: &str, statut: Statut, message: String) {
        if let Some(e) = self.entrees.iter_mut().find(|e| e.id == id) { e.statut = statut; e.message = message; }
        self.sauvegarder();
    }

    pub fn supprimer(&mut self, id: &str) -> bool {
        let avant = self.entrees.len();
        self.entrees.retain(|e| e.id != id);
        let change = self.entrees.len() != avant;
        if change { self.sauvegarder(); }
        change
    }

    pub fn vider_file(&mut self) -> usize {
        let avant = self.entrees.len();
        self.entrees.retain(|e| e.statut != Statut::EnAttente);
        let supprime = avant - self.entrees.len();
        if supprime > 0 { self.sauvegarder(); }
        supprime
    }

    pub fn supprimer_historique(&mut self) -> usize {
        let avant = self.entrees.len();
        self.entrees.retain(|e| e.statut == Statut::EnAttente || e.statut == Statut::EnCours);
        let supprime = avant - self.entrees.len();
        if supprime > 0 { self.sauvegarder(); }
        supprime
    }

    pub fn pages_imprimees(&self) -> u64 {
        self.entrees
            .iter()
            .filter(|e| e.statut == Statut::Imprime)
            .map(|e| {
                let total = if e.pages_reelles > 0 { e.pages_reelles } else { e.pages };
                (e.selection.pages_effectives(total) as u64)
                    * (e.selection.copies_effectives() as u64)
            })
            .sum()
    }
}
