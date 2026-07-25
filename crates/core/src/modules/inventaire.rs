//! Inventaires (§ stock) : comptage **daté et verrouillé** par magasin.
//!
//! À la validation, on fige le détail (théorique / compté / écart) et on crée
//! les ajustements de stock correspondants (motif inventaire). Un inventaire
//! enregistré n'est plus modifiable — on en refait un nouveau si besoin.

use crate::domain::{MotifMouvement, SensMouvement};
use crate::error::{CoreError, Result};
use crate::modules::stock;
use crate::now;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct Inventaire {
    pub id: String,
    pub depot_id: String,
    pub depot_nom: String,
    pub utilisateur_id: Option<String>,
    pub utilisateur_nom: String,
    pub date: String,
    pub statut: String,
    pub note: Option<String>,
    pub nb_lignes: i64,
    pub total_ecart: f64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lignes: Vec<InventaireLigne>,
}

#[derive(Debug, Clone, Serialize)]
pub struct InventaireLigne {
    pub article_id: String,
    pub designation: String,
    pub stock_theorique: f64,
    pub stock_compte: f64,
    pub ecart: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouvelInventaire {
    pub depot_id: String,
    #[serde(default)]
    pub note: Option<String>,
    pub lignes: Vec<NouvelleLigneInv>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouvelleLigneInv {
    pub article_id: String,
    pub stock_compte: f64,
}

fn arrondi(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Enregistre (valide) un inventaire : fige les lignes et crée les ajustements.
pub fn enregistrer(conn: &Connection, ni: &NouvelInventaire, utilisateur_id: Option<&str>) -> Result<Inventaire> {
    if ni.lignes.is_empty() {
        return Err(CoreError::Rule("aucun comptage saisi".into()));
    }
    let tx = conn.unchecked_transaction()?;
    let inv_id = Uuid::new_v4().to_string();
    tx.execute(
        "INSERT INTO inventaire (id, depot_id, utilisateur_id, date, statut, note)
         VALUES (?1,?2,?3,?4,'valide',?5)",
        params![inv_id, ni.depot_id, utilisateur_id, now(), ni.note],
    )?;

    for l in &ni.lignes {
        let theorique = stock::stock_article_depot(&tx, &l.article_id, &ni.depot_id)?;
        let compte = arrondi(l.stock_compte);
        let ecart = arrondi(compte - theorique);
        let designation: String = tx
            .query_row("SELECT designation FROM article WHERE id = ?1", params![l.article_id], |r| r.get(0))
            .unwrap_or_default();
        // Ajustement de stock si écart (un mouvement, motif inventaire).
        if ecart != 0.0 {
            let (sens, q) = if ecart > 0.0 {
                (SensMouvement::Entree, ecart)
            } else {
                (SensMouvement::Sortie, -ecart)
            };
            stock::ecrire(&tx, &l.article_id, &ni.depot_id, None, sens, q, MotifMouvement::Inventaire)?;
        }
        tx.execute(
            "INSERT INTO inventaire_ligne
                (id, inventaire_id, article_id, designation, stock_theorique, stock_compte, ecart)
             VALUES (?1,?2,?3,?4,?5,?6,?7)",
            params![Uuid::new_v4().to_string(), inv_id, l.article_id, designation, theorique, compte, ecart],
        )?;
    }
    tx.commit()?;
    lire(conn, &inv_id)
}

const SELECT_ENTETE: &str = "
    SELECT i.id, i.depot_id, COALESCE(d.nom,'—'), i.utilisateur_id,
           COALESCE(u.nom,'—'), i.date, i.statut, i.note,
           (SELECT COUNT(*) FROM inventaire_ligne l WHERE l.inventaire_id = i.id),
           COALESCE((SELECT SUM(l.ecart) FROM inventaire_ligne l WHERE l.inventaire_id = i.id), 0)
    FROM inventaire i
    LEFT JOIN depot d ON d.id = i.depot_id
    LEFT JOIN utilisateur u ON u.id = i.utilisateur_id";

fn ligne_vers_inventaire(r: &rusqlite::Row) -> rusqlite::Result<Inventaire> {
    Ok(Inventaire {
        id: r.get(0)?,
        depot_id: r.get(1)?,
        depot_nom: r.get(2)?,
        utilisateur_id: r.get(3)?,
        utilisateur_nom: r.get(4)?,
        date: r.get(5)?,
        statut: r.get(6)?,
        note: r.get(7)?,
        nb_lignes: r.get(8)?,
        total_ecart: r.get(9)?,
        lignes: Vec::new(),
    })
}

/// Liste les inventaires (récents d'abord). Filtre optionnel par magasin.
pub fn lister(conn: &Connection, depot_id: Option<&str>) -> Result<Vec<Inventaire>> {
    let (where_, has) = match depot_id {
        Some(_) => (" WHERE i.depot_id = ?1", true),
        None => ("", false),
    };
    let sql = format!("{SELECT_ENTETE}{where_} ORDER BY i.date DESC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = if has {
        stmt.query_map(params![depot_id.unwrap()], ligne_vers_inventaire)?
            .collect::<rusqlite::Result<Vec<_>>>()?
    } else {
        stmt.query_map([], ligne_vers_inventaire)?.collect::<rusqlite::Result<Vec<_>>>()?
    };
    Ok(rows)
}

/// Détail d'un inventaire (avec ses lignes figées).
pub fn lire(conn: &Connection, id: &str) -> Result<Inventaire> {
    let mut inv = conn
        .query_row(&format!("{SELECT_ENTETE} WHERE i.id = ?1"), params![id], ligne_vers_inventaire)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => CoreError::NotFound(format!("inventaire {id}")),
            autre => autre.into(),
        })?;
    let mut stmt = conn.prepare(
        "SELECT article_id, designation, stock_theorique, stock_compte, ecart
         FROM inventaire_ligne WHERE inventaire_id = ?1 ORDER BY designation",
    )?;
    let rows = stmt.query_map(params![id], |r| {
        Ok(InventaireLigne {
            article_id: r.get(0)?,
            designation: r.get(1)?,
            stock_theorique: r.get(2)?,
            stock_compte: r.get(3)?,
            ecart: r.get(4)?,
        })
    })?;
    inv.lignes = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(inv)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::modules::depot;

    fn article_bien(conn: &Connection) -> String {
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO article (id, code, type, designation, prix_vente, taux_tva, gere_stock, actif)
             VALUES (?1, ?1, 'bien', 'Riz', 100, 18, 1, 1)", params![id]).unwrap();
        id
    }

    #[test]
    fn inventaire_fige_et_ajuste() {
        let conn = db::open_in_memory().unwrap();
        let dep = depot::defaut(&conn).unwrap();
        let a = article_bien(&conn);
        // stock théorique 0 -> compté 20 => écart +20, entrée de 20
        let inv = enregistrer(&conn, &NouvelInventaire {
            depot_id: dep.clone(), note: None,
            lignes: vec![NouvelleLigneInv { article_id: a.clone(), stock_compte: 20.0 }],
        }, None).unwrap();
        assert_eq!(inv.statut, "valide");
        assert_eq!(inv.total_ecart, 20.0);
        assert_eq!(inv.lignes.len(), 1);
        assert_eq!(inv.lignes[0].ecart, 20.0);
        // le stock du magasin est passé à 20
        assert_eq!(stock::stock_article_depot(&conn, &a, &dep).unwrap(), 20.0);
        // relire donne le même détail figé
        let relu = lire(&conn, &inv.id).unwrap();
        assert_eq!(relu.lignes[0].stock_compte, 20.0);
        // il apparaît dans la liste du magasin
        assert_eq!(lister(&conn, Some(&dep)).unwrap().len(), 1);
    }
}
