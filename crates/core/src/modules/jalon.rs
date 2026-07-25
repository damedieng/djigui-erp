//! Jalons, livrables et documents joints d'un projet (migration 0028).
//!
//! ⚠️ **Barrière spec respectée** : un jalon est *local au projet*, il n'a
//! aucun lien avec l'agenda (`rendez_vous`). Le branchement éventuel se décide
//! séparément avec l'utilisateur.
//!
//! Sorti de `projet.rs` qui dépasse déjà 1200 lignes.

use crate::domain::{StatutJalon, StatutLivrable};
use crate::error::{CoreError, Result};
use crate::now;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn vide(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|x| !x.is_empty())
}

// ===========================================================================
// Jalons
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct Jalon {
    pub id: String,
    pub projet_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tache_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tache_nom: Option<String>,
    pub nom: String,
    pub date_prevue: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_reelle: Option<String>,
    pub statut: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub ordre: i64,
    /// Nombre de livrables rattachés — repère de suivi.
    pub nb_livrables: i64,
    /// `true` si la date prévue est dépassée sans que le jalon soit atteint.
    /// **Signalement seulement** : aucune date n'est recalculée (barrière spec
    /// « retard en cascade »).
    pub en_retard: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouveauJalon {
    pub projet_id: String,
    #[serde(default)]
    pub tache_id: Option<String>,
    pub nom: String,
    pub date_prevue: String,
    #[serde(default)]
    pub date_reelle: Option<String>,
    #[serde(default)]
    pub statut: Option<StatutJalon>,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default)]
    pub ordre: i64,
}

const JALON_COLS: &str = "SELECT j.id, j.projet_id, j.tache_id, j.nom, j.date_prevue,
        j.date_reelle, j.statut, j.note, j.ordre, t.nom AS tache_nom,
        (SELECT COUNT(*) FROM livrable l WHERE l.jalon_id = j.id) AS nb_liv
     FROM jalon j LEFT JOIN tache t ON t.id = j.tache_id";

fn ligne_jalon(r: &rusqlite::Row) -> rusqlite::Result<Jalon> {
    let statut: String = r.get(6)?;
    let date_prevue: String = r.get(4)?;
    let date_reelle: Option<String> = r.get(5)?;
    // Retard = échéance passée et jalon non atteint. Comparaison lexicale,
    // valide car les dates sont en AAAA-MM-JJ.
    let en_retard = statut == "a_venir"
        && date_reelle.is_none()
        && date_prevue.as_str() < &crate::now()[..10];
    Ok(Jalon {
        id: r.get(0)?,
        projet_id: r.get(1)?,
        tache_id: r.get(2)?,
        nom: r.get(3)?,
        date_prevue,
        date_reelle,
        statut,
        note: r.get(7)?,
        ordre: r.get(8)?,
        tache_nom: r.get(9)?,
        nb_livrables: r.get(10)?,
        en_retard,
    })
}

pub fn lister_jalons(conn: &Connection, projet_id: &str) -> Result<Vec<Jalon>> {
    let sql = format!("{JALON_COLS} WHERE j.projet_id = ?1 ORDER BY j.date_prevue, j.ordre");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![projet_id], ligne_jalon)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn lire_jalon(conn: &Connection, id: &str) -> Result<Jalon> {
    let sql = format!("{JALON_COLS} WHERE j.id = ?1");
    conn.query_row(&sql, params![id], ligne_jalon).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => CoreError::NotFound(format!("jalon {id}")),
        autre => autre.into(),
    })
}

pub fn creer_jalon(conn: &Connection, n: &NouveauJalon) -> Result<Jalon> {
    if n.nom.trim().is_empty() {
        return Err(CoreError::Rule("le nom du jalon est requis".into()));
    }
    if n.date_prevue.trim().is_empty() {
        return Err(CoreError::Rule("la date prévue du jalon est requise".into()));
    }
    let id = Uuid::new_v4().to_string();
    let statut = n.statut.unwrap_or(StatutJalon::AVenir);
    conn.execute(
        "INSERT INTO jalon (id, projet_id, tache_id, nom, date_prevue, date_reelle,
                            statut, note, ordre, cree_le)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![id, n.projet_id, vide(&n.tache_id), n.nom.trim(), n.date_prevue.trim(),
                vide(&n.date_reelle), statut.as_str(), vide(&n.note), n.ordre, now()],
    )?;
    lire_jalon(conn, &id)
}

pub fn modifier_jalon(conn: &Connection, id: &str, n: &NouveauJalon) -> Result<Jalon> {
    if n.nom.trim().is_empty() {
        return Err(CoreError::Rule("le nom du jalon est requis".into()));
    }
    let statut = n.statut.unwrap_or(StatutJalon::AVenir);
    let nb = conn.execute(
        "UPDATE jalon SET tache_id=?2, nom=?3, date_prevue=?4, date_reelle=?5,
                          statut=?6, note=?7, ordre=?8 WHERE id=?1",
        params![id, vide(&n.tache_id), n.nom.trim(), n.date_prevue.trim(),
                vide(&n.date_reelle), statut.as_str(), vide(&n.note), n.ordre],
    )?;
    if nb == 0 {
        return Err(CoreError::NotFound(format!("jalon {id}")));
    }
    lire_jalon(conn, id)
}

/// Change le statut de plusieurs jalons (traitement par lot).
/// Passer à « atteint » sans date réelle horodate au jour même.
pub fn changer_statut_jalons(conn: &Connection, ids: &[String], statut: StatutJalon) -> Result<usize> {
    let mut n = 0;
    for id in ids {
        n += conn.execute(
            "UPDATE jalon SET statut = ?2,
                date_reelle = CASE WHEN ?2 = 'atteint' AND date_reelle IS NULL
                                   THEN ?3 ELSE date_reelle END
             WHERE id = ?1",
            params![id, statut.as_str(), &now()[..10]],
        )?;
    }
    Ok(n)
}

/// Supprime des jalons. Les livrables et documents rattachés ne sont pas
/// détruits : ils sont **détachés** (le travail produit survit au jalon).
pub fn supprimer_jalons(conn: &Connection, ids: &[String]) -> Result<usize> {
    let mut n = 0;
    for id in ids {
        conn.execute("UPDATE livrable SET jalon_id = NULL WHERE jalon_id = ?1", params![id])?;
        conn.execute("UPDATE document_joint SET jalon_id = NULL WHERE jalon_id = ?1", params![id])?;
        n += conn.execute("DELETE FROM jalon WHERE id = ?1", params![id])?;
    }
    Ok(n)
}

// ===========================================================================
// Livrables
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct Livrable {
    pub id: String,
    pub projet_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tache_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tache_nom: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jalon_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jalon_nom: Option<String>,
    pub nom: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub statut: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_attendue: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_livraison: Option<String>,
    pub ordre: i64,
    pub nb_documents: i64,
    /// Attendu dépassé et pas encore livré — signalement seulement.
    pub en_retard: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouveauLivrable {
    pub projet_id: String,
    #[serde(default)]
    pub tache_id: Option<String>,
    #[serde(default)]
    pub jalon_id: Option<String>,
    pub nom: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub statut: Option<StatutLivrable>,
    #[serde(default)]
    pub date_attendue: Option<String>,
    #[serde(default)]
    pub date_livraison: Option<String>,
    #[serde(default)]
    pub ordre: i64,
}

const LIV_COLS: &str = "SELECT l.id, l.projet_id, l.tache_id, l.jalon_id, l.nom, l.description,
        l.statut, l.date_attendue, l.date_livraison, l.ordre, t.nom AS tache_nom, j.nom AS jalon_nom,
        (SELECT COUNT(*) FROM document_joint d WHERE d.livrable_id = l.id) AS nb_doc
     FROM livrable l
     LEFT JOIN tache t ON t.id = l.tache_id
     LEFT JOIN jalon j ON j.id = l.jalon_id";

fn ligne_livrable(r: &rusqlite::Row) -> rusqlite::Result<Livrable> {
    let statut: String = r.get(6)?;
    let date_attendue: Option<String> = r.get(7)?;
    let date_livraison: Option<String> = r.get(8)?;
    let livre = matches!(statut.as_str(), "livre" | "accepte");
    let en_retard = !livre
        && date_livraison.is_none()
        && date_attendue.as_deref().is_some_and(|d| d < &crate::now()[..10]);
    Ok(Livrable {
        id: r.get(0)?,
        projet_id: r.get(1)?,
        tache_id: r.get(2)?,
        jalon_id: r.get(3)?,
        nom: r.get(4)?,
        description: r.get(5)?,
        statut,
        date_attendue,
        date_livraison,
        ordre: r.get(9)?,
        tache_nom: r.get(10)?,
        jalon_nom: r.get(11)?,
        nb_documents: r.get(12)?,
        en_retard,
    })
}

pub fn lister_livrables(conn: &Connection, projet_id: &str) -> Result<Vec<Livrable>> {
    let sql = format!("{LIV_COLS} WHERE l.projet_id = ?1 ORDER BY l.ordre, l.nom");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![projet_id], ligne_livrable)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn lire_livrable(conn: &Connection, id: &str) -> Result<Livrable> {
    let sql = format!("{LIV_COLS} WHERE l.id = ?1");
    conn.query_row(&sql, params![id], ligne_livrable).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => CoreError::NotFound(format!("livrable {id}")),
        autre => autre.into(),
    })
}

pub fn creer_livrable(conn: &Connection, n: &NouveauLivrable) -> Result<Livrable> {
    if n.nom.trim().is_empty() {
        return Err(CoreError::Rule("le nom du livrable est requis".into()));
    }
    let id = Uuid::new_v4().to_string();
    let statut = n.statut.unwrap_or(StatutLivrable::AProduire);
    conn.execute(
        "INSERT INTO livrable (id, projet_id, tache_id, jalon_id, nom, description,
                               statut, date_attendue, date_livraison, ordre, cree_le)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![id, n.projet_id, vide(&n.tache_id), vide(&n.jalon_id), n.nom.trim(),
                vide(&n.description), statut.as_str(), vide(&n.date_attendue),
                vide(&n.date_livraison), n.ordre, now()],
    )?;
    lire_livrable(conn, &id)
}

pub fn modifier_livrable(conn: &Connection, id: &str, n: &NouveauLivrable) -> Result<Livrable> {
    if n.nom.trim().is_empty() {
        return Err(CoreError::Rule("le nom du livrable est requis".into()));
    }
    let statut = n.statut.unwrap_or(StatutLivrable::AProduire);
    let nb = conn.execute(
        "UPDATE livrable SET tache_id=?2, jalon_id=?3, nom=?4, description=?5,
                             statut=?6, date_attendue=?7, date_livraison=?8, ordre=?9
         WHERE id=?1",
        params![id, vide(&n.tache_id), vide(&n.jalon_id), n.nom.trim(), vide(&n.description),
                statut.as_str(), vide(&n.date_attendue), vide(&n.date_livraison), n.ordre],
    )?;
    if nb == 0 {
        return Err(CoreError::NotFound(format!("livrable {id}")));
    }
    lire_livrable(conn, id)
}

/// Traitement par lot. Passer à « livré » sans date horodate au jour même.
pub fn changer_statut_livrables(conn: &Connection, ids: &[String], statut: StatutLivrable) -> Result<usize> {
    let mut n = 0;
    for id in ids {
        n += conn.execute(
            "UPDATE livrable SET statut = ?2,
                date_livraison = CASE WHEN ?2 IN ('livre','accepte') AND date_livraison IS NULL
                                      THEN ?3 ELSE date_livraison END
             WHERE id = ?1",
            params![id, statut.as_str(), &now()[..10]],
        )?;
    }
    Ok(n)
}

/// Supprime des livrables ; les documents joints sont **détachés**, pas détruits.
pub fn supprimer_livrables(conn: &Connection, ids: &[String]) -> Result<usize> {
    let mut n = 0;
    for id in ids {
        conn.execute("UPDATE document_joint SET livrable_id = NULL WHERE livrable_id = ?1", params![id])?;
        n += conn.execute("DELETE FROM livrable WHERE id = ?1", params![id])?;
    }
    Ok(n)
}

// ===========================================================================
// Documents joints
//
// Le fichier vit SUR DISQUE ; la base ne stocke que son chemin. C'est le
// serveur qui écrit/supprime le fichier — le cœur ne touche jamais au disque.
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct DocumentJoint {
    pub id: String,
    pub projet_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tache_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jalon_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub livrable_id: Option<String>,
    pub nom: String,
    pub chemin: String,
    pub taille: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_mime: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cree_par: Option<String>,
    pub cree_le: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouveauDocument {
    pub projet_id: String,
    #[serde(default)]
    pub tache_id: Option<String>,
    #[serde(default)]
    pub jalon_id: Option<String>,
    #[serde(default)]
    pub livrable_id: Option<String>,
    pub nom: String,
    /// Chemin relatif au dossier de stockage, calculé par le serveur.
    pub chemin: String,
    #[serde(default)]
    pub taille: i64,
    #[serde(default)]
    pub type_mime: Option<String>,
}

fn ligne_document(r: &rusqlite::Row) -> rusqlite::Result<DocumentJoint> {
    Ok(DocumentJoint {
        id: r.get(0)?,
        projet_id: r.get(1)?,
        tache_id: r.get(2)?,
        jalon_id: r.get(3)?,
        livrable_id: r.get(4)?,
        nom: r.get(5)?,
        chemin: r.get(6)?,
        taille: r.get(7)?,
        type_mime: r.get(8)?,
        cree_par: r.get(9)?,
        cree_le: r.get(10)?,
    })
}

const DOC_COLS: &str = "SELECT id, projet_id, tache_id, jalon_id, livrable_id, nom, chemin,
        taille, type_mime, cree_par, cree_le FROM document_joint";

pub fn lister_documents(conn: &Connection, projet_id: &str) -> Result<Vec<DocumentJoint>> {
    let sql = format!("{DOC_COLS} WHERE projet_id = ?1 ORDER BY cree_le DESC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![projet_id], ligne_document)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn lire_document(conn: &Connection, id: &str) -> Result<DocumentJoint> {
    let sql = format!("{DOC_COLS} WHERE id = ?1");
    conn.query_row(&sql, params![id], ligne_document).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => CoreError::NotFound(format!("document {id}")),
        autre => autre.into(),
    })
}

pub fn creer_document(conn: &Connection, n: &NouveauDocument, par: Option<&str>) -> Result<DocumentJoint> {
    if n.nom.trim().is_empty() {
        return Err(CoreError::Rule("le nom du document est requis".into()));
    }
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO document_joint (id, projet_id, tache_id, jalon_id, livrable_id,
                                     nom, chemin, taille, type_mime, cree_par, cree_le)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
        params![id, n.projet_id, vide(&n.tache_id), vide(&n.jalon_id), vide(&n.livrable_id),
                n.nom.trim(), n.chemin, n.taille.max(0), vide(&n.type_mime), par, now()],
    )?;
    lire_document(conn, &id)
}

/// Retire l'enregistrement et renvoie le chemin du fichier, au serveur d'effacer
/// le fichier lui-même.
pub fn supprimer_document(conn: &Connection, id: &str) -> Result<String> {
    let doc = lire_document(conn, id)?;
    conn.execute("DELETE FROM document_joint WHERE id = ?1", params![id])?;
    Ok(doc.chemin)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::modules::projet::{self, NouveauProjet};

    fn projet_test(conn: &Connection) -> String {
        projet::creer(conn, &NouveauProjet {
            nom: "Chantier".into(), client_id: None, chef_de_projet_id: None,
            date_debut_prevue: Some("2026-08-01".into()),
            date_fin_prevue: Some("2026-12-31".into()),
            date_debut_reelle: None, date_fin_reelle: None, statut: None,
            budget_global: 0.0, note: None,
        }, Some("u1")).unwrap().id
    }

    #[test]
    fn jalon_atteint_horodate_et_signale_le_retard() {
        let conn = db::open_in_memory().unwrap();
        let p = projet_test(&conn);

        // Échéance largement passée, jalon non atteint → signalé en retard.
        let j = creer_jalon(&conn, &NouveauJalon {
            projet_id: p.clone(), tache_id: None, nom: "Livraison phase 1".into(),
            date_prevue: "2020-01-01".into(), date_reelle: None, statut: None,
            note: None, ordre: 0,
        }).unwrap();
        assert_eq!(j.statut, "a_venir");
        assert!(j.en_retard, "une échéance passée non atteinte doit être signalée");

        // Passage à « atteint » : la date réelle est posée automatiquement.
        assert_eq!(changer_statut_jalons(&conn, &[j.id.clone()], StatutJalon::Atteint).unwrap(), 1);
        let j = lire_jalon(&conn, &j.id).unwrap();
        assert_eq!(j.statut, "atteint");
        assert!(j.date_reelle.is_some());
        assert!(!j.en_retard, "un jalon atteint n'est plus en retard");
    }

    #[test]
    fn supprimer_un_jalon_detache_livrables_et_documents() {
        let conn = db::open_in_memory().unwrap();
        let p = projet_test(&conn);
        let j = creer_jalon(&conn, &NouveauJalon {
            projet_id: p.clone(), tache_id: None, nom: "Recette".into(),
            date_prevue: "2026-10-01".into(), date_reelle: None, statut: None,
            note: None, ordre: 0,
        }).unwrap();
        let l = creer_livrable(&conn, &NouveauLivrable {
            projet_id: p.clone(), tache_id: None, jalon_id: Some(j.id.clone()),
            nom: "Rapport final".into(), description: None, statut: None,
            date_attendue: Some("2026-10-01".into()), date_livraison: None, ordre: 0,
        }).unwrap();
        let d = creer_document(&conn, &NouveauDocument {
            projet_id: p.clone(), tache_id: None, jalon_id: Some(j.id.clone()),
            livrable_id: Some(l.id.clone()), nom: "plan.pdf".into(),
            chemin: "projets/plan.pdf".into(), taille: 1024,
            type_mime: Some("application/pdf".into()),
        }, Some("u1")).unwrap();

        // Le jalon compte bien son livrable.
        assert_eq!(lire_jalon(&conn, &j.id).unwrap().nb_livrables, 1);
        assert_eq!(lire_livrable(&conn, &l.id).unwrap().nb_documents, 1);

        // Supprimer le jalon ne détruit ni le livrable ni le document.
        assert_eq!(supprimer_jalons(&conn, &[j.id.clone()]).unwrap(), 1);
        let l = lire_livrable(&conn, &l.id).unwrap();
        assert!(l.jalon_id.is_none(), "le livrable est détaché, pas supprimé");
        let d = lire_document(&conn, &d.id).unwrap();
        assert!(d.jalon_id.is_none());
        assert_eq!(d.livrable_id.as_deref(), Some(l.id.as_str()));
    }

    #[test]
    fn livrable_en_retard_puis_livre() {
        let conn = db::open_in_memory().unwrap();
        let p = projet_test(&conn);
        let l = creer_livrable(&conn, &NouveauLivrable {
            projet_id: p, tache_id: None, jalon_id: None, nom: "Maquette".into(),
            description: None, statut: None, date_attendue: Some("2020-05-01".into()),
            date_livraison: None, ordre: 0,
        }).unwrap();
        assert!(l.en_retard);

        changer_statut_livrables(&conn, &[l.id.clone()], StatutLivrable::Livre).unwrap();
        let l = lire_livrable(&conn, &l.id).unwrap();
        assert_eq!(l.statut, "livre");
        assert!(l.date_livraison.is_some(), "la date de livraison est horodatée");
        assert!(!l.en_retard);
    }
}
