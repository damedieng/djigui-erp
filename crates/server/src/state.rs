//! État partagé du serveur : la connexion SQLite unique, sérialisée.
//!
//! `Mutex` suffit et est correct ici : l'architecture garantit un seul écrivain
//! (spec §2.1), donc pas d'accès concurrent au fichier — le verrou sérialise
//! simplement les appels de l'API.

use djigui_core::db;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub struct AppState {
    pub conn: Arc<Mutex<Connection>>,
    /// Dossier de stockage des pièces jointes, à côté de la base. Les fichiers
    /// vivent sur disque ; la base ne garde que leur chemin relatif (migration
    /// 0028) — mettre des pièces jointes en base64 ferait exploser djigui.db.
    pub dossier_documents: std::path::PathBuf,
}

impl AppState {
    pub fn ouvrir(path: &str) -> anyhow::Result<Self> {
        let conn = db::open(path)?;
        let dossier_documents = std::path::Path::new(path)
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .join("documents");
        std::fs::create_dir_all(&dossier_documents)?;
        Ok(Self { conn: Arc::new(Mutex::new(conn)), dossier_documents })
    }
}
