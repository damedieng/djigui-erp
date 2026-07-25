//! Caisse & paiements (spec §5.6 / §6.4).
//!
//! Un paiement met à jour **en une seule écriture** le solde de la `caisse` et
//! celui du `tiers`. Les soldes sont *dérivés* mais tenus à jour à l'écriture
//! pour un affichage instantané ; l'utilitaire [`recalculer_soldes`] les
//! reconstruit depuis les journaux (documents validés + paiements) en cas de
//! doute — la vérité reste toujours dans les journaux (§6.4).
//!
//! Conventions de signe :
//! - `caisse.solde` : encaissement `+montant`, décaissement `−montant`.
//! - `tiers.solde`  : encours dû (créance client / dette fournisseur, positif).
//!   Valider une **facture** ajoute `+total_ttc` ; un **avoir** retire `−total_ttc` ;
//!   un **paiement** réduit l'encours de `−montant` (quel que soit le sens).

use crate::domain::{ModePaiement, SensPaiement};
use crate::error::{CoreError, Result};
use crate::now;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct Paiement {
    pub id: String,
    pub tiers_id: String,
    pub caisse_id: String,
    pub document_id: Option<String>,
    pub sens: String,
    pub montant: f64,
    pub mode: String,
    pub date: String,
    /// Moyen concret utilisé (id de `moyen_paiement`), si renseigné.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moyen_paiement_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouveauPaiement {
    pub tiers_id: String,
    /// Caisse cible ; à défaut, la caisse par défaut du poste.
    #[serde(default)]
    pub caisse_id: Option<String>,
    /// Pièce rattachée (facture encaissée), optionnel.
    #[serde(default)]
    pub document_id: Option<String>,
    pub sens: SensPaiement,
    pub montant: f64,
    pub mode: ModePaiement,
    /// Moyen concret choisi (Orange Money, Wave…), optionnel — trace le détail
    /// au-delà de la famille portée par `mode`.
    #[serde(default)]
    pub moyen_paiement_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Caisse {
    pub id: String,
    pub nom: String,
    pub solde: f64,
    /// Magasin (dépôt) rattaché : les ventes de cette caisse y déduisent le stock.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depot_id: Option<String>,
    /// A déjà servi (a des sessions ou des paiements) → modif à confirmer, suppression interdite.
    #[serde(default)]
    pub a_activite: bool,
}

fn arrondi(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

// ---------------------------------------------------------------------------
// Caisse
// ---------------------------------------------------------------------------

/// Id de la caisse par défaut (la première) ; en crée une si aucune n'existe,
/// rattachée au dépôt par défaut.
pub fn caisse_defaut(conn: &Connection) -> Result<String> {
    if let Ok(id) = conn.query_row("SELECT id FROM caisse LIMIT 1", [], |r| r.get::<_, String>(0)) {
        return Ok(id);
    }
    let depot = crate::modules::depot::defaut(conn)?;
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO caisse (id, nom, solde, depot_id) VALUES (?1, 'Caisse principale', 0, ?2)",
        params![id, depot],
    )?;
    Ok(id)
}

/// Dépôt (magasin) rattaché à une caisse ; repli sur le dépôt par défaut si non défini.
pub fn depot_de_caisse(conn: &Connection, caisse_id: &str) -> Result<String> {
    let d: Option<String> = conn
        .query_row("SELECT depot_id FROM caisse WHERE id = ?1", params![caisse_id], |r| r.get(0))
        .ok()
        .flatten();
    match d {
        Some(id) if !id.is_empty() => Ok(id),
        _ => crate::modules::depot::defaut(conn),
    }
}

pub fn lister_caisses(conn: &Connection) -> Result<Vec<Caisse>> {
    caisse_defaut(conn)?; // garantit au moins une caisse
    let mut stmt = conn.prepare(
        "SELECT c.id, c.nom, c.solde, c.depot_id,
                (EXISTS(SELECT 1 FROM paiement p WHERE p.caisse_id = c.id)
                 OR EXISTS(SELECT 1 FROM session_caisse s WHERE s.caisse_id = c.id)) AS a_activite
         FROM caisse c ORDER BY c.nom")?;
    let rows = stmt.query_map([], |r| {
        Ok(Caisse {
            id: r.get(0)?, nom: r.get(1)?, solde: r.get(2)?, depot_id: r.get(3)?,
            a_activite: r.get::<_, i64>(4)? != 0,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Une caisse a-t-elle déjà servi (sessions ou paiements) ?
pub fn caisse_a_activite(conn: &Connection, id: &str) -> Result<bool> {
    let p: i64 = conn.query_row("SELECT COUNT(*) FROM paiement WHERE caisse_id = ?1", params![id], |r| r.get(0))?;
    let s: i64 = conn.query_row("SELECT COUNT(*) FROM session_caisse WHERE caisse_id = ?1", params![id], |r| r.get(0))?;
    Ok(p > 0 || s > 0)
}

fn session_ouverte_sur(conn: &Connection, id: &str) -> Result<bool> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM session_caisse WHERE caisse_id = ?1 AND statut = 'ouverte'",
        params![id], |r| r.get(0))?;
    Ok(n > 0)
}

/// Crée une caisse (solde initial 0), rattachée à un magasin. Nom unique.
pub fn creer_caisse(conn: &Connection, nom: &str, depot_id: Option<&str>) -> Result<Caisse> {
    let nom = nom.trim();
    if nom.is_empty() {
        return Err(CoreError::Rule("le nom de la caisse est requis".into()));
    }
    let existe: bool = conn
        .query_row("SELECT 1 FROM caisse WHERE nom = ?1", params![nom], |_| Ok(true))
        .unwrap_or(false);
    if existe {
        return Err(CoreError::Rule(format!("une caisse « {nom} » existe déjà")));
    }
    // dépôt fourni, sinon dépôt par défaut
    let depot = match depot_id.filter(|d| !d.trim().is_empty()) {
        Some(d) => d.to_string(),
        None => crate::modules::depot::defaut(conn)?,
    };
    let id = Uuid::new_v4().to_string();
    conn.execute("INSERT INTO caisse (id, nom, solde, depot_id) VALUES (?1, ?2, 0, ?3)",
                 params![id, nom, depot])?;
    Ok(Caisse { id, nom: nom.to_string(), solde: 0.0, depot_id: Some(depot), a_activite: false })
}

/// Modifie une caisse (nom + magasin). Refuse si une **session est ouverte**
/// dessus (fermer d'abord). La confirmation d'une caisse déjà utilisée est gérée
/// côté UI ; le cœur applique la modification.
pub fn modifier_caisse(conn: &Connection, id: &str, nom: &str, depot_id: Option<&str>) -> Result<Caisse> {
    let nom = nom.trim();
    if nom.is_empty() {
        return Err(CoreError::Rule("le nom de la caisse est requis".into()));
    }
    let existe: bool = conn.query_row("SELECT 1 FROM caisse WHERE id = ?1", params![id], |_| Ok(true)).unwrap_or(false);
    if !existe {
        return Err(CoreError::NotFound(format!("caisse {id}")));
    }
    if session_ouverte_sur(conn, id)? {
        return Err(CoreError::Rule("fermez la session ouverte de cette caisse avant de la modifier".into()));
    }
    // nom unique (hors la caisse elle-même)
    let doublon: bool = conn.query_row(
        "SELECT 1 FROM caisse WHERE nom = ?1 AND id <> ?2", params![nom, id], |_| Ok(true)).unwrap_or(false);
    if doublon {
        return Err(CoreError::Rule(format!("une autre caisse « {nom} » existe déjà")));
    }
    let depot = match depot_id.filter(|d| !d.trim().is_empty()) {
        Some(d) => d.to_string(),
        None => crate::modules::depot::defaut(conn)?,
    };
    conn.execute("UPDATE caisse SET nom = ?2, depot_id = ?3 WHERE id = ?1", params![id, nom, depot])?;
    let solde: f64 = conn.query_row("SELECT solde FROM caisse WHERE id = ?1", params![id], |r| r.get(0))?;
    Ok(Caisse { id: id.to_string(), nom: nom.to_string(), solde, depot_id: Some(depot), a_activite: caisse_a_activite(conn, id)? })
}

/// Supprime une caisse **créée par erreur** : seulement si elle n'a **jamais
/// servi** (aucune session, aucun paiement), qu'il en reste au moins une autre,
/// et qu'aucune session n'y est ouverte. Sinon on préserve l'historique.
pub fn supprimer_caisse(conn: &Connection, id: &str) -> Result<()> {
    let existe: bool = conn.query_row("SELECT 1 FROM caisse WHERE id = ?1", params![id], |_| Ok(true)).unwrap_or(false);
    if !existe {
        return Err(CoreError::NotFound(format!("caisse {id}")));
    }
    let total: i64 = conn.query_row("SELECT COUNT(*) FROM caisse", [], |r| r.get(0))?;
    if total <= 1 {
        return Err(CoreError::Rule("impossible de supprimer la dernière caisse".into()));
    }
    if caisse_a_activite(conn, id)? {
        return Err(CoreError::Rule("cette caisse a déjà servi (sessions/paiements) : suppression impossible".into()));
    }
    conn.execute("DELETE FROM caisse WHERE id = ?1", params![id])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Paiement
// ---------------------------------------------------------------------------

/// Enregistre un paiement et met à jour, dans la même connexion, le solde de la
/// caisse et celui du tiers (§6.4).
pub fn enregistrer(conn: &Connection, p: &NouveauPaiement) -> Result<Paiement> {
    if p.montant <= 0.0 {
        return Err(CoreError::Rule("le montant du paiement doit être positif".into()));
    }
    let caisse_id = match &p.caisse_id {
        Some(c) if !c.trim().is_empty() => c.clone(),
        _ => caisse_defaut(conn)?,
    };
    let montant = arrondi(p.montant);
    let id = Uuid::new_v4().to_string();
    // Rattachement automatique à la session ouverte de la caisse, s'il y en a une.
    let session_id: Option<String> = conn
        .query_row(
            "SELECT id FROM session_caisse WHERE caisse_id=?1 AND statut='ouverte' LIMIT 1",
            params![caisse_id],
            |r| r.get(0),
        )
        .ok();
    let moyen = p.moyen_paiement_id.as_deref().filter(|s| !s.trim().is_empty());
    conn.execute(
        "INSERT INTO paiement (id, tiers_id, caisse_id, document_id, sens, montant, mode, date, session_caisse_id, moyen_paiement_id)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![id, p.tiers_id, caisse_id, p.document_id, p.sens, montant, p.mode, now(), session_id, moyen],
    )?;

    // Solde caisse : encaissement +, décaissement − ; solde tiers : encours réduit.
    let signe_caisse = match p.sens {
        SensPaiement::Encaissement => 1.0,
        SensPaiement::Decaissement => -1.0,
    };
    maj_solde_caisse(conn, &caisse_id, signe_caisse * montant)?;
    maj_solde_tiers(conn, &p.tiers_id, -montant)?;
    lire(conn, &id)
}

pub fn lire(conn: &Connection, id: &str) -> Result<Paiement> {
    conn.query_row(
        "SELECT id, tiers_id, caisse_id, document_id, sens, montant, mode, date, moyen_paiement_id
         FROM paiement WHERE id = ?1",
        params![id],
        ligne_vers_paiement,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => CoreError::NotFound(format!("paiement {id}")),
        autre => autre.into(),
    })
}

/// Historique des paiements, du plus récent au plus ancien. Filtre optionnel par tiers.
pub fn lister(conn: &Connection, tiers_id: Option<&str>) -> Result<Vec<Paiement>> {
    const COLS: &str =
        "SELECT id, tiers_id, caisse_id, document_id, sens, montant, mode, date, moyen_paiement_id FROM paiement";
    let paiements = match tiers_id {
        Some(t) => {
            let mut stmt = conn.prepare(&format!("{COLS} WHERE tiers_id = ?1 ORDER BY date DESC"))?;
            let rows = stmt.query_map(params![t], ligne_vers_paiement)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        }
        None => {
            let mut stmt = conn.prepare(&format!("{COLS} ORDER BY date DESC"))?;
            let rows = stmt.query_map([], ligne_vers_paiement)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()?
        }
    };
    Ok(paiements)
}

fn ligne_vers_paiement(r: &rusqlite::Row) -> rusqlite::Result<Paiement> {
    Ok(Paiement {
        id: r.get(0)?,
        tiers_id: r.get(1)?,
        caisse_id: r.get(2)?,
        document_id: r.get(3)?,
        sens: r.get(4)?,
        montant: r.get(5)?,
        mode: r.get(6)?,
        date: r.get(7)?,
        moyen_paiement_id: r.get(8)?,
    })
}

// ---------------------------------------------------------------------------
// Soldes (§6.4)
// ---------------------------------------------------------------------------

/// Applique une variation au solde d'une caisse (delta signé).
pub fn maj_solde_caisse(conn: &Connection, caisse_id: &str, delta: f64) -> Result<()> {
    conn.execute(
        "UPDATE caisse SET solde = ROUND(solde + ?2, 2) WHERE id = ?1",
        params![caisse_id, delta],
    )?;
    Ok(())
}

/// Applique une variation au solde (encours) d'un tiers (delta signé).
pub fn maj_solde_tiers(conn: &Connection, tiers_id: &str, delta: f64) -> Result<()> {
    conn.execute(
        "UPDATE tiers SET solde = ROUND(solde + ?2, 2) WHERE id = ?1",
        params![tiers_id, delta],
    )?;
    Ok(())
}

/// Recalcule TOUS les soldes (caisses + tiers) depuis les journaux (§6.4).
/// - caisse.solde = Σ(encaissements) − Σ(décaissements) de la caisse.
/// - tiers.solde  = Σ(ttc factures validées) − Σ(ttc avoirs validés) − Σ(paiements du tiers).
/// Retourne le nombre de soldes réparés (caisses + tiers).
pub fn recalculer_soldes(conn: &Connection) -> Result<usize> {
    // Caisses
    conn.execute(
        "UPDATE caisse SET solde = ROUND(COALESCE((
            SELECT SUM(CASE WHEN sens='encaissement' THEN montant ELSE -montant END)
            FROM paiement WHERE paiement.caisse_id = caisse.id), 0), 2)",
        [],
    )?;
    // Tiers : encours = factures validées − avoirs validés − paiements.
    conn.execute(
        "UPDATE tiers SET solde = ROUND(
            COALESCE((SELECT SUM(CASE WHEN d.type_document='avoir' THEN -d.total_ttc ELSE d.total_ttc END)
                      FROM document d
                      WHERE d.tiers_id = tiers.id
                        AND d.type_document IN ('facture','avoir')
                        AND d.statut IN ('valide','transforme')), 0)
          - COALESCE((SELECT SUM(montant) FROM paiement WHERE paiement.tiers_id = tiers.id), 0)
        , 2)",
        [],
    )?;
    let nb_caisses: i64 = conn.query_row("SELECT COUNT(*) FROM caisse", [], |r| r.get(0))?;
    let nb_tiers: i64 = conn.query_row("SELECT COUNT(*) FROM tiers", [], |r| r.get(0))?;
    Ok((nb_caisses + nb_tiers) as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn creer_tiers(conn: &Connection, id: &str) {
        conn.execute(
            "INSERT INTO tiers (id, code, type_role, nom, solde, actif, cree_le)
             VALUES (?1, ?1, 'client', 'Client', 0, 1, '2026-01-01')",
            params![id],
        )
        .unwrap();
    }

    #[test]
    fn encaissement_met_a_jour_caisse_et_tiers() {
        let conn = db::open_in_memory().unwrap();
        creer_tiers(&conn, "t1");
        // le tiers doit 10 000 (créance simulée)
        maj_solde_tiers(&conn, "t1", 10_000.0).unwrap();

        let p = enregistrer(&conn, &NouveauPaiement {
            tiers_id: "t1".into(), caisse_id: None, document_id: None,
            sens: SensPaiement::Encaissement, montant: 6_000.0, mode: ModePaiement::Espece, moyen_paiement_id: None,
        }).unwrap();

        let solde_caisse: f64 = conn.query_row(
            "SELECT solde FROM caisse WHERE id = ?1", params![p.caisse_id], |r| r.get(0)).unwrap();
        let solde_tiers: f64 = conn.query_row(
            "SELECT solde FROM tiers WHERE id = 't1'", [], |r| r.get(0)).unwrap();
        assert_eq!(solde_caisse, 6_000.0);   // espèces entrées
        assert_eq!(solde_tiers, 4_000.0);    // créance restante
    }

    #[test]
    fn recalcul_reconstruit_les_soldes() {
        let conn = db::open_in_memory().unwrap();
        creer_tiers(&conn, "t1");
        enregistrer(&conn, &NouveauPaiement {
            tiers_id: "t1".into(), caisse_id: None, document_id: None,
            sens: SensPaiement::Encaissement, montant: 2_500.0, mode: ModePaiement::Espece, moyen_paiement_id: None,
        }).unwrap();

        // on corrompt volontairement les soldes
        conn.execute("UPDATE caisse SET solde = 999999", []).unwrap();
        conn.execute("UPDATE tiers SET solde = 999999", []).unwrap();

        recalculer_soldes(&conn).unwrap();
        let solde_caisse: f64 = conn.query_row("SELECT solde FROM caisse LIMIT 1", [], |r| r.get(0)).unwrap();
        let solde_tiers: f64 = conn.query_row("SELECT solde FROM tiers WHERE id='t1'", [], |r| r.get(0)).unwrap();
        assert_eq!(solde_caisse, 2_500.0);
        assert_eq!(solde_tiers, -2_500.0); // aucun document, juste un encaissement
    }

    #[test]
    fn montant_negatif_refuse() {
        let conn = db::open_in_memory().unwrap();
        creer_tiers(&conn, "t1");
        let r = enregistrer(&conn, &NouveauPaiement {
            tiers_id: "t1".into(), caisse_id: None, document_id: None,
            sens: SensPaiement::Encaissement, montant: 0.0, mode: ModePaiement::Espece, moyen_paiement_id: None,
        });
        assert!(r.is_err());
    }
}
