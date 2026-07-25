//! Journal d'audit (§ traçabilité) : enregistre « qui a fait quoi, et quand ».
//!
//! Chaque action sensible (création/validation/suppression de pièce, paiement,
//! gestion des utilisateurs…) écrit une entrée. Le nom de l'utilisateur est
//! **figé** au moment de l'action (snapshot), pour que le journal reste lisible
//! même si le compte est renommé ou désactivé ensuite.

use crate::error::Result;
use crate::now;
use rusqlite::{params, Connection};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct EntreeAudit {
    pub id: String,
    pub date: String,
    pub utilisateur_id: Option<String>,
    pub utilisateur_nom: String,
    pub action: String,
    pub entite: String,
    pub entite_id: Option<String>,
    pub detail: Option<String>,
}

/// Enregistre une action dans le journal. `acteur_id` = utilisateur courant
/// (None si non authentifié / système). Le nom est résolu depuis la table
/// `utilisateur` et figé dans l'entrée. Ne fait jamais échouer l'appelant sur
/// une erreur de journalisation critique n'est pas souhaité : on remonte l'erreur
/// SQL éventuelle pour rester cohérent avec la transaction en cours.
pub fn enregistrer(
    conn: &Connection,
    acteur_id: Option<&str>,
    action: &str,
    entite: &str,
    entite_id: Option<&str>,
    detail: Option<&str>,
) -> Result<()> {
    let nom = match acteur_id {
        Some(id) => conn
            .query_row("SELECT nom FROM utilisateur WHERE id = ?1", params![id], |r| r.get::<_, String>(0))
            .unwrap_or_else(|_| "—".into()),
        None => "Système".into(),
    };
    conn.execute(
        "INSERT INTO journal_audit
            (id, date, utilisateur_id, utilisateur_nom, action, entite, entite_id, detail)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![Uuid::new_v4().to_string(), now(), acteur_id, nom, action, entite, entite_id, detail],
    )?;
    Ok(())
}

/// Liste les dernières entrées du journal, de la plus récente à la plus ancienne.
/// `limite` bornée à 1..1000 (défaut 300).
pub fn lister(conn: &Connection, limite: Option<i64>) -> Result<Vec<EntreeAudit>> {
    let limite = limite.unwrap_or(300).clamp(1, 1000);
    let mut stmt = conn.prepare(
        "SELECT id, date, utilisateur_id, utilisateur_nom, action, entite, entite_id, detail
         FROM journal_audit ORDER BY date DESC LIMIT ?1",
    )?;
    let rows = stmt.query_map(params![limite], |r| {
        Ok(EntreeAudit {
            id: r.get(0)?,
            date: r.get(1)?,
            utilisateur_id: r.get(2)?,
            utilisateur_nom: r.get(3)?,
            action: r.get(4)?,
            entite: r.get(5)?,
            entite_id: r.get(6)?,
            detail: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn enregistre_et_liste() {
        let conn = db::open_in_memory().unwrap();
        // l'utilisateur par défaut djigui existe
        let id: String = conn.query_row("SELECT id FROM utilisateur LIMIT 1", [], |r| r.get(0)).unwrap();
        enregistrer(&conn, Some(&id), "creation", "document", Some("doc-1"), Some("FA-2026-0001")).unwrap();
        enregistrer(&conn, None, "recalcul", "caisse", None, None).unwrap();
        let entrees = lister(&conn, None).unwrap();
        assert_eq!(entrees.len(), 2);
        // la plus récente d'abord
        assert_eq!(entrees[0].utilisateur_nom, "Système");
        assert_eq!(entrees[1].action, "creation");
        assert_eq!(entrees[1].utilisateur_nom, "Administrateur");
    }
}
