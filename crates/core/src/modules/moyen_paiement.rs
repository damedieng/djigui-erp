//! Moyens de paiement configurables (migration 0018).
//!
//! L'utilisateur définit ses propres moyens (Orange Money, Wave, Free Money…)
//! avec une **image + un texte** dans les Paramètres ; ils s'affichent à
//! l'encaissement. Chaque moyen appartient à une **famille** parmi les 4 valeurs
//! historiques de `paiement.mode` (`espece`, `mobile_money`, `virement`,
//! `cheque`) : la famille pilote le comportement (le rendu de monnaie n'a de
//! sens que pour l'espèce). Le moyen concret est tracé sur le paiement via
//! `moyen_paiement_id`, sans toucher la contrainte CHECK de `mode`.

use crate::error::{CoreError, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Familles autorisées (miroir de `ModePaiement` / du CHECK sur `paiement.mode`).
const FAMILLES: [&str; 4] = ["espece", "mobile_money", "virement", "cheque"];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MoyenPaiement {
    pub id: String,
    pub nom: String,
    pub famille: String,
    /// Image (data-URI base64 embarqué, hors-ligne), optionnelle.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// Couleur de repli (pastille + initiales) quand aucune image.
    pub couleur: String,
    /// Calcule-t-on le rendu de monnaie pour ce moyen ? (typiquement l'espèce).
    pub rendu_monnaie: bool,
    pub actif: bool,
    pub ordre: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouveauMoyen {
    pub nom: String,
    pub famille: String,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub couleur: Option<String>,
    #[serde(default)]
    pub rendu_monnaie: bool,
    #[serde(default = "vrai")]
    pub actif: bool,
    #[serde(default)]
    pub ordre: Option<i64>,
}

fn vrai() -> bool {
    true
}

fn valider_famille(f: &str) -> Result<()> {
    if !FAMILLES.contains(&f) {
        return Err(CoreError::Rule(format!(
            "famille de moyen de paiement inconnue : « {f} »"
        )));
    }
    Ok(())
}

fn ligne(r: &rusqlite::Row) -> rusqlite::Result<MoyenPaiement> {
    Ok(MoyenPaiement {
        id: r.get(0)?,
        nom: r.get(1)?,
        famille: r.get(2)?,
        image: r.get(3)?,
        couleur: r.get(4)?,
        rendu_monnaie: r.get::<_, i64>(5)? != 0,
        actif: r.get::<_, i64>(6)? != 0,
        ordre: r.get(7)?,
    })
}

const COLS: &str = "SELECT id, nom, famille, image, couleur, rendu_monnaie, actif, ordre FROM moyen_paiement";

/// Tous les moyens (actifs et inactifs), pour l'écran de gestion.
pub fn lister(conn: &Connection) -> Result<Vec<MoyenPaiement>> {
    let mut stmt = conn.prepare(&format!("{COLS} ORDER BY ordre, nom"))?;
    let rows = stmt.query_map([], ligne)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Seuls les moyens actifs, pour l'encaissement (caisse).
pub fn lister_actifs(conn: &Connection) -> Result<Vec<MoyenPaiement>> {
    let mut stmt = conn.prepare(&format!("{COLS} WHERE actif = 1 ORDER BY ordre, nom"))?;
    let rows = stmt.query_map([], ligne)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn lire(conn: &Connection, id: &str) -> Result<MoyenPaiement> {
    conn.query_row(&format!("{COLS} WHERE id = ?1"), params![id], ligne)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => CoreError::NotFound(format!("moyen de paiement {id}")),
            autre => autre.into(),
        })
}

pub fn creer(conn: &Connection, m: &NouveauMoyen) -> Result<MoyenPaiement> {
    let nom = m.nom.trim();
    if nom.is_empty() {
        return Err(CoreError::Rule("le nom du moyen de paiement est requis".into()));
    }
    valider_famille(&m.famille)?;
    let doublon: bool = conn
        .query_row("SELECT 1 FROM moyen_paiement WHERE nom = ?1", params![nom], |_| Ok(true))
        .unwrap_or(false);
    if doublon {
        return Err(CoreError::Rule(format!("un moyen « {nom} » existe déjà")));
    }
    let image = m.image.as_deref().filter(|s| !s.trim().is_empty());
    let couleur = m.couleur.as_deref().filter(|s| !s.trim().is_empty()).unwrap_or("#64748b");
    let ordre = m.ordre.unwrap_or_else(|| {
        conn.query_row("SELECT COALESCE(MAX(ordre)+1, 0) FROM moyen_paiement", [], |r| r.get(0))
            .unwrap_or(0)
    });
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO moyen_paiement (id, nom, famille, image, couleur, rendu_monnaie, actif, ordre)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![id, nom, m.famille, image, couleur, m.rendu_monnaie as i64, m.actif as i64, ordre],
    )?;
    lire(conn, &id)
}

pub fn modifier(conn: &Connection, id: &str, m: &NouveauMoyen) -> Result<MoyenPaiement> {
    let nom = m.nom.trim();
    if nom.is_empty() {
        return Err(CoreError::Rule("le nom du moyen de paiement est requis".into()));
    }
    valider_famille(&m.famille)?;
    let doublon: bool = conn
        .query_row("SELECT 1 FROM moyen_paiement WHERE nom = ?1 AND id <> ?2", params![nom, id], |_| Ok(true))
        .unwrap_or(false);
    if doublon {
        return Err(CoreError::Rule(format!("un autre moyen « {nom} » existe déjà")));
    }
    let image = m.image.as_deref().filter(|s| !s.trim().is_empty());
    let couleur = m.couleur.as_deref().filter(|s| !s.trim().is_empty()).unwrap_or("#64748b");
    let n = conn.execute(
        "UPDATE moyen_paiement SET nom=?2, famille=?3, image=?4, couleur=?5, rendu_monnaie=?6, actif=?7 WHERE id=?1",
        params![id, nom, m.famille, image, couleur, m.rendu_monnaie as i64, m.actif as i64],
    )?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("moyen de paiement {id}")));
    }
    lire(conn, id)
}

/// Active/désactive (traitement par lot). Renvoie le nombre de moyens touchés.
pub fn definir_actif(conn: &Connection, ids: &[String], actif: bool) -> Result<usize> {
    let mut n = 0;
    for id in ids {
        n += conn.execute(
            "UPDATE moyen_paiement SET actif = ?2 WHERE id = ?1",
            params![id, actif as i64],
        )?;
    }
    Ok(n)
}

/// Réordonne selon la liste d'ids fournie (position = ordre).
pub fn reordonner(conn: &Connection, ids: &[String]) -> Result<()> {
    for (i, id) in ids.iter().enumerate() {
        conn.execute("UPDATE moyen_paiement SET ordre = ?2 WHERE id = ?1", params![id, i as i64])?;
    }
    Ok(())
}

/// Supprime un moyen **jamais utilisé** (aucun paiement rattaché) ; sinon on le
/// désactive pour préserver l'historique — le repli est géré côté appelant.
pub fn supprimer(conn: &Connection, id: &str) -> Result<()> {
    let utilise: i64 = conn
        .query_row("SELECT COUNT(*) FROM paiement WHERE moyen_paiement_id = ?1", params![id], |r| r.get(0))?;
    if utilise > 0 {
        return Err(CoreError::Rule(
            "ce moyen a déjà servi à des encaissements : désactivez-le plutôt que de le supprimer".into(),
        ));
    }
    let n = conn.execute("DELETE FROM moyen_paiement WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("moyen de paiement {id}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn seed_par_defaut_present() {
        let conn = db::open_in_memory().unwrap();
        let actifs = lister_actifs(&conn).unwrap();
        assert!(actifs.iter().any(|m| m.nom == "Orange Money"));
        assert!(actifs.iter().any(|m| m.nom == "Wave"));
        // l'espèce calcule le rendu, pas les autres
        let espece = actifs.iter().find(|m| m.famille == "espece").unwrap();
        assert!(espece.rendu_monnaie);
    }

    #[test]
    fn crud_et_famille_invalide() {
        let conn = db::open_in_memory().unwrap();
        let m = creer(&conn, &NouveauMoyen {
            nom: "Kpay".into(), famille: "mobile_money".into(), image: None,
            couleur: Some("#123456".into()), rendu_monnaie: false, actif: true, ordre: None,
        }).unwrap();
        assert_eq!(m.couleur, "#123456");

        let mauvais = creer(&conn, &NouveauMoyen {
            nom: "X".into(), famille: "bitcoin".into(), image: None, couleur: None,
            rendu_monnaie: false, actif: true, ordre: None,
        });
        assert!(mauvais.is_err());

        definir_actif(&conn, &[m.id.clone()], false).unwrap();
        assert!(!lire(&conn, &m.id).unwrap().actif);

        supprimer(&conn, &m.id).unwrap();
        assert!(lire(&conn, &m.id).is_err());
    }

    #[test]
    fn suppression_refusee_si_utilise() {
        let conn = db::open_in_memory().unwrap();
        // un tiers + une caisse + un paiement rattaché au moyen espèce
        conn.execute(
            "INSERT INTO tiers (id, code, type_role, nom, solde, actif, cree_le)
             VALUES ('t1','t1','client','C',0,1,'2026-01-01')", []).unwrap();
        let caisse = crate::modules::paiement::caisse_defaut(&conn).unwrap();
        conn.execute(
            "INSERT INTO paiement (id, tiers_id, caisse_id, sens, montant, mode, date, moyen_paiement_id)
             VALUES ('p1','t1',?1,'encaissement',100,'espece','2026-01-01','mp-espece')",
            params![caisse]).unwrap();
        assert!(supprimer(&conn, "mp-espece").is_err());
    }
}
