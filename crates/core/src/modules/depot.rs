//! Dépôts (spec §5.3). Un seul dépôt par défaut à la fois.

use crate::error::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Depot {
    pub id: String,
    pub nom: String,
    pub par_defaut: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouveauDepot {
    pub nom: String,
    #[serde(default)]
    pub par_defaut: bool,
}

pub fn creer(conn: &Connection, d: &NouveauDepot) -> Result<Depot> {
    let id = Uuid::new_v4().to_string();
    if d.par_defaut {
        conn.execute("UPDATE depot SET par_defaut = 0", [])?;
    }
    conn.execute(
        "INSERT INTO depot (id, nom, par_defaut) VALUES (?1, ?2, ?3)",
        params![id, d.nom.trim(), d.par_defaut as i64],
    )?;
    Ok(Depot { id, nom: d.nom.trim().to_string(), par_defaut: d.par_defaut })
}

pub fn lister(conn: &Connection) -> Result<Vec<Depot>> {
    let mut stmt = conn.prepare("SELECT id, nom, par_defaut FROM depot ORDER BY nom")?;
    let rows = stmt.query_map([], |r| {
        Ok(Depot { id: r.get(0)?, nom: r.get(1)?, par_defaut: r.get::<_, i64>(2)? != 0 })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Renomme un dépôt (magasin).
pub fn renommer(conn: &Connection, id: &str, nom: &str) -> Result<Depot> {
    let nom = nom.trim();
    if nom.is_empty() {
        return Err(crate::error::CoreError::Rule("le nom du magasin est requis".into()));
    }
    let n = conn.execute("UPDATE depot SET nom = ?2 WHERE id = ?1", params![id, nom])?;
    if n == 0 {
        return Err(crate::error::CoreError::NotFound(format!("dépôt {id}")));
    }
    let pd: i64 = conn.query_row("SELECT par_defaut FROM depot WHERE id = ?1", params![id], |r| r.get(0))?;
    Ok(Depot { id: id.to_string(), nom: nom.to_string(), par_defaut: pd != 0 })
}

/// Définit le dépôt par défaut (un seul à la fois).
pub fn definir_defaut(conn: &Connection, id: &str) -> Result<()> {
    let existe: bool = conn.query_row("SELECT 1 FROM depot WHERE id = ?1", params![id], |_| Ok(true)).unwrap_or(false);
    if !existe {
        return Err(crate::error::CoreError::NotFound(format!("dépôt {id}")));
    }
    conn.execute("UPDATE depot SET par_defaut = 0", [])?;
    conn.execute("UPDATE depot SET par_defaut = 1 WHERE id = ?1", params![id])?;
    Ok(())
}

/// Renvoie l'id du dépôt par défaut, en en créant un (« Principal ») si aucun
/// n'existe. Utile comme dépôt implicite d'un document sans dépôt explicite.
pub fn defaut(conn: &Connection) -> Result<String> {
    let existant: Option<String> = conn
        .query_row("SELECT id FROM depot WHERE par_defaut = 1 LIMIT 1", [], |r| r.get(0))
        .ok();
    if let Some(id) = existant {
        return Ok(id);
    }
    // sinon, premier dépôt trouvé, sinon on en crée un
    if let Ok(id) = conn.query_row("SELECT id FROM depot LIMIT 1", [], |r| r.get::<_, String>(0)) {
        conn.execute("UPDATE depot SET par_defaut = 1 WHERE id = ?1", params![id])?;
        return Ok(id);
    }
    let d = creer(conn, &NouveauDepot { nom: "Principal".into(), par_defaut: true })?;
    Ok(d.id)
}
