//! Djigui Desktop — cœur métier (spec §2.2 : backend Rust dans le processus serveur).
//!
//! Trois paris structurants (§3), à préserver dans toute évolution :
//!   1. un seul `tiers` (rôle client/fournisseur/les_deux) ;
//!   2. un seul `document` (type + sens) ;
//!   3. le stock est un journal (`mouvement_stock`), jamais une valeur stockée.
//!
//! Toutes les écritures passent par ce cœur, appelé uniquement par le serveur.

pub mod authorization;
pub mod db;
pub mod domain;
pub mod error;
pub mod modules;

pub use error::{CoreError, Result};

/// Horodatage ISO-8601 UTC, format unique pour toutes les colonnes datetime.
pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
