use thiserror::Error;

/// Erreurs du cœur métier. Le serveur les traduit en codes HTTP.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("base de données : {0}")]
    Db(#[from] rusqlite::Error),

    #[error("introuvable : {0}")]
    NotFound(String),

    #[error("règle métier : {0}")]
    Rule(String),

    #[error("capacité non autorisée : {0}")]
    Forbidden(String),

    #[error("authentification : {0}")]
    Unauthorized(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;
