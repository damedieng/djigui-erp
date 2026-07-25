//! Sessions de caisse (§ caisse) : ouverture avec un fond, fermeture après
//! comptage, calcul de l'écart.
//!
//! Théorique = fond + Σ(encaissements) − Σ(décaissements) des paiements rattachés
//! à la session. Écart = montant compté − théorique. Les paiements sont rattachés
//! automatiquement à la session **ouverte** de la caisse (voir `paiement`).

use crate::error::{CoreError, Result};
use crate::modules::paiement;
use crate::now;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct SessionCaisse {
    pub id: String,
    pub caisse_id: String,
    pub utilisateur_id: Option<String>,
    pub fond_ouverture: f64,
    pub ouvert_le: String,
    pub ferme_le: Option<String>,
    pub montant_compte: Option<f64>,
    pub ecart: Option<f64>,
    pub statut: String,
    pub note: Option<String>,
    /// Totaux dérivés (pour l'affichage), calculés depuis les paiements rattachés.
    pub encaissements: f64,
    pub decaissements: f64,
    pub theorique: f64,
}

fn arrondi(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Encaissements / décaissements d'une session (depuis les paiements rattachés).
fn totaux(conn: &Connection, session_id: &str) -> Result<(f64, f64)> {
    let enc: f64 = conn.query_row(
        "SELECT COALESCE(SUM(montant),0) FROM paiement WHERE session_caisse_id=?1 AND sens='encaissement'",
        params![session_id], |r| r.get(0))?;
    let dec: f64 = conn.query_row(
        "SELECT COALESCE(SUM(montant),0) FROM paiement WHERE session_caisse_id=?1 AND sens='decaissement'",
        params![session_id], |r| r.get(0))?;
    Ok((arrondi(enc), arrondi(dec)))
}

fn ligne_vers_session(conn: &Connection, r: &rusqlite::Row) -> rusqlite::Result<SessionCaisse> {
    let id: String = r.get(0)?;
    let fond: f64 = r.get(3)?;
    let (enc, dec) = totaux(conn, &id).unwrap_or((0.0, 0.0));
    Ok(SessionCaisse {
        id,
        caisse_id: r.get(1)?,
        utilisateur_id: r.get(2)?,
        fond_ouverture: fond,
        ouvert_le: r.get(4)?,
        ferme_le: r.get(5)?,
        montant_compte: r.get(6)?,
        ecart: r.get(7)?,
        statut: r.get(8)?,
        note: r.get(9)?,
        encaissements: enc,
        decaissements: dec,
        theorique: arrondi(fond + enc - dec),
    })
}

const COLS: &str = "SELECT id, caisse_id, utilisateur_id, fond_ouverture, ouvert_le, ferme_le,
                           montant_compte, ecart, statut, note FROM session_caisse";

/// Session actuellement **ouverte** d'une caisse, s'il y en a une.
pub fn session_ouverte(conn: &Connection, caisse_id: &str) -> Result<Option<SessionCaisse>> {
    let sql = format!("{COLS} WHERE caisse_id=?1 AND statut='ouverte' LIMIT 1");
    let s = conn
        .query_row(&sql, params![caisse_id], |r| ligne_vers_session(conn, r))
        .optional()?;
    Ok(s)
}

pub fn lire(conn: &Connection, id: &str) -> Result<SessionCaisse> {
    let sql = format!("{COLS} WHERE id=?1");
    conn.query_row(&sql, params![id], |r| ligne_vers_session(conn, r))
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => CoreError::NotFound(format!("session {id}")),
            autre => autre.into(),
        })
}

#[derive(Debug, Deserialize)]
pub struct Ouverture {
    /// Caisse ; à défaut la caisse par défaut du poste.
    #[serde(default)]
    pub caisse_id: Option<String>,
    pub fond_ouverture: f64,
    #[serde(default)]
    pub note: Option<String>,
}

/// Ouvre une session sur une caisse. Refuse si une session y est déjà ouverte.
pub fn ouvrir(conn: &Connection, o: &Ouverture, utilisateur_id: Option<&str>) -> Result<SessionCaisse> {
    if o.fond_ouverture < 0.0 {
        return Err(CoreError::Rule("le fond de caisse ne peut pas être négatif".into()));
    }
    let caisse_id = match &o.caisse_id {
        Some(c) if !c.trim().is_empty() => c.clone(),
        _ => paiement::caisse_defaut(conn)?,
    };
    if session_ouverte(conn, &caisse_id)?.is_some() {
        return Err(CoreError::Rule("une session est déjà ouverte sur cette caisse".into()));
    }
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO session_caisse (id, caisse_id, utilisateur_id, fond_ouverture, ouvert_le, statut, note)
         VALUES (?1,?2,?3,?4,?5,'ouverte',?6)",
        params![id, caisse_id, utilisateur_id, arrondi(o.fond_ouverture), now(), o.note],
    )?;
    lire(conn, &id)
}

#[derive(Debug, Deserialize)]
pub struct Fermeture {
    pub montant_compte: f64,
    #[serde(default)]
    pub note: Option<String>,
}

/// Ferme une session : calcule l'écart (compté − théorique) et clôt.
pub fn fermer(conn: &Connection, session_id: &str, f: &Fermeture) -> Result<SessionCaisse> {
    let s = lire(conn, session_id)?;
    if s.statut != "ouverte" {
        return Err(CoreError::Rule("cette session est déjà fermée".into()));
    }
    let ecart = arrondi(f.montant_compte - s.theorique);
    conn.execute(
        "UPDATE session_caisse
         SET statut='fermee', ferme_le=?2, montant_compte=?3, ecart=?4,
             note = COALESCE(?5, note)
         WHERE id=?1",
        params![session_id, now(), arrondi(f.montant_compte), ecart, f.note],
    )?;
    lire(conn, session_id)
}

/// Historique des sessions (les plus récentes d'abord). Filtre optionnel par caisse.
pub fn lister(conn: &Connection, caisse_id: Option<&str>) -> Result<Vec<SessionCaisse>> {
    let (where_, id): (&str, &str) = match caisse_id {
        Some(c) => (" WHERE caisse_id=?1", c),
        None => ("", ""),
    };
    let sql = format!("{COLS}{where_} ORDER BY ouvert_le DESC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = if caisse_id.is_some() {
        stmt.query_map(params![id], |r| ligne_vers_session(conn, r))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        stmt.query_map([], |r| ligne_vers_session(conn, r))?
            .collect::<rusqlite::Result<Vec<_>>>()?
    };
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ModePaiement, SensPaiement};
    use crate::modules::paiement::NouveauPaiement;
    use crate::db;

    fn tiers(conn: &Connection) -> String {
        let id = Uuid::new_v4().to_string();
        conn.execute("INSERT INTO tiers (id, code, type_role, nom, solde, actif, cree_le)
             VALUES (?1,?1,'client','C',0,1,'2026-01-01')", params![id]).unwrap();
        id
    }

    #[test]
    fn ouvrir_encaisser_fermer_calcule_ecart() {
        let conn = db::open_in_memory().unwrap();
        let caisse = paiement::caisse_defaut(&conn).unwrap();
        let t = tiers(&conn);
        let s = ouvrir(&conn, &Ouverture { caisse_id: Some(caisse.clone()), fond_ouverture: 10_000.0, note: None }, None).unwrap();
        // double ouverture refusée
        assert!(ouvrir(&conn, &Ouverture { caisse_id: Some(caisse.clone()), fond_ouverture: 0.0, note: None }, None).is_err());
        // un encaissement de 6000 (rattaché automatiquement via paiement::enregistrer)
        paiement::enregistrer(&conn, &NouveauPaiement {
            tiers_id: t, caisse_id: Some(caisse.clone()), document_id: None,
            sens: SensPaiement::Encaissement, montant: 6_000.0, mode: ModePaiement::Espece, moyen_paiement_id: None,
        }).unwrap();
        let s = lire(&conn, &s.id).unwrap();
        assert_eq!(s.encaissements, 6_000.0);
        assert_eq!(s.theorique, 16_000.0); // 10000 + 6000
        // fermeture : compté 15500 -> écart -500
        let f = fermer(&conn, &s.id, &Fermeture { montant_compte: 15_500.0, note: None }).unwrap();
        assert_eq!(f.statut, "fermee");
        assert_eq!(f.ecart, Some(-500.0));
        // plus de session ouverte
        assert!(session_ouverte(&conn, &caisse).unwrap().is_none());
    }
}
