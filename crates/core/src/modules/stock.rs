//! Mouvements de stock — le journal (spec §3.3 / §5.5).
//!
//! Un mouvement n'est **jamais** modifié ni supprimé : une erreur se corrige par
//! un mouvement inverse. Le stock d'un article dans un dépôt est toujours
//! Σ(entrées) − Σ(sorties), calculé à la demande, jamais stocké.

use crate::domain::{MotifMouvement, SensMouvement};
use crate::error::{CoreError, Result};
use crate::now;
use rusqlite::{params, Connection};
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct Mouvement {
    pub id: String,
    pub article_id: String,
    pub depot_id: String,
    pub document_id: Option<String>,
    pub sens: String,
    pub quantite: f64,
    pub motif: String,
    pub date: String,
}

/// Écrit un mouvement dans le journal. Point de passage unique des écritures de
/// stock (validation de document, inventaire, casse, transfert, production).
pub fn ecrire(
    conn: &Connection,
    article_id: &str,
    depot_id: &str,
    document_id: Option<&str>,
    sens: SensMouvement,
    quantite: f64,
    motif: MotifMouvement,
) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO mouvement_stock
            (id, article_id, depot_id, document_id, sens, quantite, motif, date)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![id, article_id, depot_id, document_id, sens, quantite, motif, now()],
    )?;
    Ok(id)
}

/// Stock d'un article dans un dépôt = Σ(entrées) − Σ(sorties).
pub fn stock_article_depot(conn: &Connection, article_id: &str, depot_id: &str) -> Result<f64> {
    let s: f64 = conn.query_row(
        "SELECT COALESCE(SUM(CASE WHEN sens='entree' THEN quantite ELSE -quantite END), 0)
         FROM mouvement_stock WHERE article_id = ?1 AND depot_id = ?2",
        params![article_id, depot_id],
        |r| r.get(0),
    )?;
    Ok(s)
}

/// Transfère `quantite` d'un article d'un magasin (dépôt) source vers un autre.
/// Deux mouvements (sortie source + entrée destination), motif `transfert`.
/// Garde-fou : la quantité doit être positive et disponible dans la source.
pub fn transferer(
    conn: &Connection,
    article_id: &str,
    source_depot: &str,
    dest_depot: &str,
    quantite: f64,
) -> Result<()> {
    if quantite <= 0.0 {
        return Err(CoreError::Rule("la quantité à transférer doit être positive".into()));
    }
    if source_depot == dest_depot {
        return Err(CoreError::Rule("le magasin source et destination doivent différer".into()));
    }
    let dispo = stock_article_depot(conn, article_id, source_depot)?;
    if quantite > dispo {
        return Err(CoreError::Rule(format!(
            "stock insuffisant dans le magasin source (disponible : {dispo})"
        )));
    }
    let tx = conn.unchecked_transaction()?;
    ecrire(&tx, article_id, source_depot, None, SensMouvement::Sortie, quantite, MotifMouvement::Transfert)?;
    ecrire(&tx, article_id, dest_depot, None, SensMouvement::Entree, quantite, MotifMouvement::Transfert)?;
    tx.commit()?;
    Ok(())
}

/// Liste, pour un dépôt, les articles gérés en stock avec leur stock courant.
/// Utile pour l'écran d'inventaire par magasin.
#[derive(Debug, Serialize)]
pub struct StockLigne {
    pub article_id: String,
    pub code: String,
    pub designation: String,
    pub stock: f64,
    pub stock_alerte: Option<f64>,
}

pub fn etat_depot(conn: &Connection, depot_id: &str) -> Result<Vec<StockLigne>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.code, a.designation, a.stock_alerte,
                COALESCE((SELECT SUM(CASE WHEN m.sens='entree' THEN m.quantite ELSE -m.quantite END)
                          FROM mouvement_stock m
                          WHERE m.article_id = a.id AND m.depot_id = ?1), 0) AS stock
         FROM article a
         WHERE a.actif = 1 AND a.gere_stock = 1
         ORDER BY a.designation",
    )?;
    let rows = stmt.query_map(params![depot_id], |r| {
        Ok(StockLigne {
            article_id: r.get(0)?,
            code: r.get(1)?,
            designation: r.get(2)?,
            stock_alerte: r.get(3)?,
            stock: r.get(4)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Ajustement d'inventaire (§6.3) : écart entre stock physique compté et stock
/// théorique, matérialisé par un unique mouvement de motif `inventaire`.
pub fn ajuster_inventaire(
    conn: &Connection,
    article_id: &str,
    depot_id: &str,
    stock_physique: f64,
) -> Result<Option<String>> {
    let theorique = stock_article_depot(conn, article_id, depot_id)?;
    let ecart = stock_physique - theorique;
    if ecart == 0.0 {
        return Ok(None);
    }
    let (sens, q) = if ecart > 0.0 {
        (SensMouvement::Entree, ecart)
    } else {
        (SensMouvement::Sortie, -ecart)
    };
    let id = ecrire(conn, article_id, depot_id, None, sens, q, MotifMouvement::Inventaire)?;
    Ok(Some(id))
}
