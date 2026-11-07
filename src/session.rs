use serde::{Deserialize, Serialize};

use crate::config;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Host,
    Client,
}

impl Default for Role {
    fn default() -> Self {
        Role::Host
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModeCouleur {
    Couleur,
    NoirBlanc,
}

impl Default for ModeCouleur {
    fn default() -> Self {
        ModeCouleur::Couleur
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub role: Role,
    pub nom_utilisateur: String,
    pub adresse_host: String,
    pub port: u16,
    pub nom_imprimante: String,
    pub taille_fenetre: Option<(f32, f32)>,
    #[serde(default = "mode_sombre_defaut")]
    pub mode_sombre: bool,
    #[serde(default)]
    pub mode_couleur: ModeCouleur,
}

fn mode_sombre_defaut() -> bool {
    true
}

impl Default for Session {
    fn default() -> Self {
        Self {
            role: Role::Host,
            nom_utilisateur: std::env::var("USERNAME").unwrap_or_else(|_| "utilisateur".into()),
            adresse_host: "127.0.0.1".into(),
            port: config::DEFAULT_PORT,
            nom_imprimante: String::new(),
            taille_fenetre: None,
            mode_sombre: true,
            mode_couleur: ModeCouleur::Couleur,
        }
    }
}

impl Session {
    pub fn charger() -> Self {
        let chemin = config::chemin_session();
        match std::fs::read_to_string(&chemin) {
            Ok(s) => serde_json::from_str(&s).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub fn sauvegarder(&self) {
        let chemin = config::chemin_session();
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(chemin, s);
        }
    }
}
