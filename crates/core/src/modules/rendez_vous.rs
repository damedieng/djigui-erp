//! Agenda / rendez-vous (migration 0020).
//!
//! Un rendez-vous porte un titre, une plage horaire (`debut` / `fin` optionnelle,
//! format « AAAA-MM-JJ HH:MM »), un statut, et des rattachements optionnels :
//! tiers, responsable (utilisateur), lieu, note. CRUD complet + filtres (période,
//! statut, tiers, responsable) + traitement par lot (statut / suppression), selon
//! les standards Djigui. Les incohérences sont signalées **sans bloquer**
//! l'enregistrement — l'appelant (UI) affiche les alertes.

use crate::domain::StatutRendezVous;
use crate::error::{CoreError, Result};
use crate::now;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct RendezVous {
    pub id: String,
    pub titre: String,
    pub debut: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fin: Option<String>,
    pub journee_entiere: bool,
    pub statut: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tiers_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responsable_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lieu: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Noms « joints » pour l'affichage (jamais stockés).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tiers_nom: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub responsable_nom: Option<String>,
    pub cree_le: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouveauRendezVous {
    pub titre: String,
    pub debut: String,
    #[serde(default)]
    pub fin: Option<String>,
    #[serde(default)]
    pub journee_entiere: bool,
    #[serde(default)]
    pub statut: Option<StatutRendezVous>,
    #[serde(default)]
    pub tiers_id: Option<String>,
    #[serde(default)]
    pub responsable_id: Option<String>,
    #[serde(default)]
    pub lieu: Option<String>,
    #[serde(default)]
    pub note: Option<String>,
}

/// Filtre de liste (toutes les bornes sont optionnelles).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FiltreRendezVous {
    /// Bornes sur `debut` (préfixe date « AAAA-MM-JJ », comparaison lexicale).
    #[serde(default)]
    pub du: Option<String>,
    #[serde(default)]
    pub au: Option<String>,
    #[serde(default)]
    pub statut: Option<StatutRendezVous>,
    #[serde(default)]
    pub tiers_id: Option<String>,
    #[serde(default)]
    pub responsable_id: Option<String>,
}

fn vide_en_none(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|x| !x.is_empty())
}

const COLS: &str = "SELECT r.id, r.titre, r.debut, r.fin, r.journee_entiere, r.statut,
        r.tiers_id, r.responsable_id, r.lieu, r.note, r.cree_le,
        t.nom AS tiers_nom, u.nom AS responsable_nom
     FROM rendez_vous r
     LEFT JOIN tiers t ON t.id = r.tiers_id
     LEFT JOIN utilisateur u ON u.id = r.responsable_id";

fn ligne(r: &rusqlite::Row) -> rusqlite::Result<RendezVous> {
    Ok(RendezVous {
        id: r.get(0)?,
        titre: r.get(1)?,
        debut: r.get(2)?,
        fin: r.get(3)?,
        journee_entiere: r.get::<_, i64>(4)? != 0,
        statut: r.get(5)?,
        tiers_id: r.get(6)?,
        responsable_id: r.get(7)?,
        lieu: r.get(8)?,
        note: r.get(9)?,
        cree_le: r.get(10)?,
        tiers_nom: r.get(11)?,
        responsable_nom: r.get(12)?,
    })
}

pub fn lister(conn: &Connection, f: &FiltreRendezVous) -> Result<Vec<RendezVous>> {
    // Borne haute inclusive : « AAAA-MM-JJ » est complété à la fin de journée.
    let au = vide_en_none(&f.au).map(|s| if s.len() <= 10 { format!("{s} 23:59") } else { s.to_string() });
    let sql = format!(
        "{COLS}
         WHERE (?1 IS NULL OR r.debut >= ?1)
           AND (?2 IS NULL OR r.debut <= ?2)
           AND (?3 IS NULL OR r.statut = ?3)
           AND (?4 IS NULL OR r.tiers_id = ?4)
           AND (?5 IS NULL OR r.responsable_id = ?5)
         ORDER BY r.debut"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        params![
            vide_en_none(&f.du),
            au,
            f.statut.map(|s| s.as_str()),
            vide_en_none(&f.tiers_id),
            vide_en_none(&f.responsable_id),
        ],
        ligne,
    )?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn lire(conn: &Connection, id: &str) -> Result<RendezVous> {
    let sql = format!("{COLS} WHERE r.id = ?1");
    conn.query_row(&sql, params![id], ligne).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => CoreError::NotFound(format!("rendez-vous {id}")),
        autre => autre.into(),
    })
}

fn valider(n: &NouveauRendezVous) -> Result<()> {
    if n.titre.trim().is_empty() {
        return Err(CoreError::Rule("le titre du rendez-vous est requis".into()));
    }
    let debut = n.debut.trim();
    if debut.is_empty() {
        return Err(CoreError::Rule("la date de début est requise".into()));
    }
    // Contrôle : la fin (si fournie) doit être après le début. Les horodatages
    // « AAAA-MM-JJ HH:MM » se comparent lexicalement.
    if let Some(fin) = n.fin.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if fin < debut {
            return Err(CoreError::Rule("la date de fin doit être après la date de début".into()));
        }
    }
    Ok(())
}

pub fn creer(conn: &Connection, n: &NouveauRendezVous, cree_par: Option<&str>) -> Result<RendezVous> {
    valider(n)?;
    let id = Uuid::new_v4().to_string();
    let statut = n.statut.unwrap_or(StatutRendezVous::Planifie);
    conn.execute(
        "INSERT INTO rendez_vous
         (id, titre, debut, fin, journee_entiere, statut, tiers_id, responsable_id, lieu, note, cree_par, cree_le)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
        params![
            id, n.titre.trim(), n.debut.trim(), vide_en_none(&n.fin), n.journee_entiere as i64,
            statut.as_str(), vide_en_none(&n.tiers_id), vide_en_none(&n.responsable_id),
            vide_en_none(&n.lieu), vide_en_none(&n.note), cree_par, now(),
        ],
    )?;
    lire(conn, &id)
}

pub fn modifier(conn: &Connection, id: &str, n: &NouveauRendezVous) -> Result<RendezVous> {
    valider(n)?;
    let statut = n.statut.unwrap_or(StatutRendezVous::Planifie);
    let nb = conn.execute(
        "UPDATE rendez_vous SET titre=?2, debut=?3, fin=?4, journee_entiere=?5, statut=?6,
                tiers_id=?7, responsable_id=?8, lieu=?9, note=?10 WHERE id=?1",
        params![
            id, n.titre.trim(), n.debut.trim(), vide_en_none(&n.fin), n.journee_entiere as i64,
            statut.as_str(), vide_en_none(&n.tiers_id), vide_en_none(&n.responsable_id),
            vide_en_none(&n.lieu), vide_en_none(&n.note),
        ],
    )?;
    if nb == 0 {
        return Err(CoreError::NotFound(format!("rendez-vous {id}")));
    }
    lire(conn, id)
}

/// Change le statut d'un ou plusieurs rendez-vous (traitement par lot).
pub fn changer_statut_lot(conn: &Connection, ids: &[String], statut: StatutRendezVous) -> Result<usize> {
    let mut n = 0;
    for id in ids {
        n += conn.execute(
            "UPDATE rendez_vous SET statut = ?2 WHERE id = ?1",
            params![id, statut.as_str()],
        )?;
    }
    Ok(n)
}

/// Supprime un ou plusieurs rendez-vous (traitement par lot). Un RDV n'a pas
/// d'impact comptable : la suppression est franche.
pub fn supprimer_lot(conn: &Connection, ids: &[String]) -> Result<usize> {
    let mut n = 0;
    for id in ids {
        n += conn.execute("DELETE FROM rendez_vous WHERE id = ?1", params![id])?;
    }
    Ok(n)
}

pub fn supprimer(conn: &Connection, id: &str) -> Result<()> {
    let n = conn.execute("DELETE FROM rendez_vous WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("rendez-vous {id}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn nouveau(titre: &str, debut: &str) -> NouveauRendezVous {
        NouveauRendezVous {
            titre: titre.into(), debut: debut.into(), fin: None, journee_entiere: false,
            statut: None, tiers_id: None, responsable_id: None, lieu: None, note: None,
        }
    }

    #[test]
    fn crud_et_filtre_periode() {
        let conn = db::open_in_memory().unwrap();
        let a = creer(&conn, &nouveau("Visite client", "2026-07-10 09:00"), Some("u1")).unwrap();
        creer(&conn, &nouveau("Livraison", "2026-08-02 14:00"), None).unwrap();
        assert_eq!(a.statut, "planifie");

        // filtre sur juillet uniquement
        let f = FiltreRendezVous { du: Some("2026-07-01".into()), au: Some("2026-07-31".into()), ..Default::default() };
        let juillet = lister(&conn, &f).unwrap();
        assert_eq!(juillet.len(), 1);
        assert_eq!(juillet[0].titre, "Visite client");

        // modification + statut
        let mut m = nouveau("Visite client (report)", "2026-07-11 10:00");
        m.statut = Some(StatutRendezVous::Confirme);
        let mod_ = modifier(&conn, &a.id, &m).unwrap();
        assert_eq!(mod_.statut, "confirme");

        // lot : honoré puis suppression
        changer_statut_lot(&conn, &[a.id.clone()], StatutRendezVous::Honore).unwrap();
        assert_eq!(lire(&conn, &a.id).unwrap().statut, "honore");
        assert_eq!(supprimer_lot(&conn, &[a.id.clone()]).unwrap(), 1);
        assert!(lire(&conn, &a.id).is_err());
    }

    #[test]
    fn titre_requis() {
        let conn = db::open_in_memory().unwrap();
        assert!(creer(&conn, &nouveau("  ", "2026-07-10 09:00"), None).is_err());
    }

    #[test]
    fn fin_avant_debut_refusee() {
        let conn = db::open_in_memory().unwrap();
        let mut n = nouveau("Réunion", "2026-07-10 10:00");
        n.fin = Some("2026-07-10 09:00".into()); // fin avant début
        assert!(creer(&conn, &n, None).is_err());
        // fin après début : OK
        n.fin = Some("2026-07-10 11:00".into());
        assert!(creer(&conn, &n, None).is_ok());
    }

    #[test]
    fn jointure_tiers_nom() {
        let conn = db::open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO tiers (id, code, type_role, nom, solde, actif, cree_le)
             VALUES ('t1','t1','client','ACME',0,1,'2026-01-01')", []).unwrap();
        let mut n = nouveau("RDV ACME", "2026-07-10 09:00");
        n.tiers_id = Some("t1".into());
        let rdv = creer(&conn, &n, None).unwrap();
        assert_eq!(rdv.tiers_nom.as_deref(), Some("ACME"));
    }
}
