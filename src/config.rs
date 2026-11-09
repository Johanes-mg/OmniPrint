use std::path::PathBuf;

pub const APP_NAME: &str = "OmniPrint";
pub const DEFAULT_PORT: u16 = 7000;
pub const PORTS_PROPOSES: [u16; 6] = [7000, 8080, 8888, 9000, 9090, 10000];
pub const PORT_RECHERCHE_MAX: u16 = 20;

pub fn dossier_appdata() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        let p = PathBuf::from(appdata).join(APP_NAME);
        if std::fs::create_dir_all(&p).is_ok() {
            return p;
        }
    }
    let p = std::env::temp_dir().join(APP_NAME);
    let _ = std::fs::create_dir_all(&p);
    p
}

pub fn chemin_session() -> PathBuf {
    dossier_appdata().join("session.json")
}

pub fn chemin_journal() -> PathBuf {
    dossier_appdata().join("journal.json")
}

pub fn chemin_documents_recus() -> PathBuf {
    let p = dossier_appdata().join("documents");
    let _ = std::fs::create_dir_all(&p);
    p
}
