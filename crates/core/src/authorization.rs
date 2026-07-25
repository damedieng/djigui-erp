//! Point de contrôle **unique** d'autorisation (spec §3.4).
//!
//! Interdiction absolue d'éparpiller des `if payant` dans le code métier :
//! toute vérification de droit passe par `est_autorise`. Aujourd'hui la couche
//! gratuit/payant n'existe pas → la fonction renvoie toujours `Ok(())`. Quand la
//! licence arrivera, seule cette fonction changera, sans toucher au métier.

use crate::error::{CoreError, Result};

/// Capacités à frontière nette (spec §3.4). Chaque module en dépend.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Capacite {
    Caisse,
    Stock,
    Facturation,
    Production,
    Abonnements,
    Rapports,
}

/// Renvoie `Ok(())` si la capacité est autorisée, sinon `Forbidden`.
/// v1 : tout est autorisé.
pub fn est_autorise(_capacite: Capacite) -> Result<()> {
    Ok(())
}

/// Variante ergonomique pour un garde en début de fonction métier.
pub fn exiger(capacite: Capacite) -> Result<()> {
    est_autorise(capacite).map_err(|_| {
        CoreError::Forbidden(format!("{capacite:?}"))
    })
}
