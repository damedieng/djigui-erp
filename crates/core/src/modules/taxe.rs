//! Catalogue des taxes (migration 0007). Une vente/un article peut porter
//! plusieurs taxes : TVA et autres, en pourcentage ou en montant fixe.

use crate::error::{CoreError, Result};
use crate::domain::TypeTaxe;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Taxe {
    pub id: String,
    pub nom: String,
    pub taux: f64,
    pub r#type: TypeTaxe,
    pub actif: bool,
    pub par_defaut: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouvelleTaxe {
    pub nom: String,
    pub taux: f64,
    #[serde(default)]
    pub r#type: Option<TypeTaxe>,
    #[serde(default)]
    pub par_defaut: bool,
}

/// Taxes **actives** uniquement — celles proposées et comptabilisées.
pub fn lister(conn: &Connection) -> Result<Vec<Taxe>> {
    lister_interne(conn, true)
}

/// Toutes les taxes (actives + inactives) — pour l'écran de paramétrage.
pub fn lister_tous(conn: &Connection) -> Result<Vec<Taxe>> {
    lister_interne(conn, false)
}

fn lister_interne(conn: &Connection, actives_seulement: bool) -> Result<Vec<Taxe>> {
    let sql = if actives_seulement {
        "SELECT id, nom, taux, type, actif, par_defaut FROM taxe WHERE actif = 1 ORDER BY taux DESC, nom"
    } else {
        "SELECT id, nom, taux, type, actif, par_defaut FROM taxe ORDER BY actif DESC, taux DESC, nom"
    };
    let mut stmt = conn.prepare(sql)?;
    let rows = stmt.query_map([], ligne_vers_taxe)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Active/désactive une taxe. Seules les taxes actives sont comptabilisées.
/// Une taxe désactivée perd son statut « par défaut ».
pub fn definir_actif(conn: &Connection, id: &str, actif: bool) -> Result<()> {
    let n = if actif {
        conn.execute("UPDATE taxe SET actif = 1 WHERE id = ?1", params![id])?
    } else {
        conn.execute("UPDATE taxe SET actif = 0, par_defaut = 0 WHERE id = ?1", params![id])?
    };
    if n == 0 {
        return Err(CoreError::NotFound(format!("taxe {id}")));
    }
    Ok(())
}

pub fn creer(conn: &Connection, t: &NouvelleTaxe) -> Result<Taxe> {
    let nom = t.nom.trim();
    if nom.is_empty() {
        return Err(CoreError::Rule("le nom de la taxe est requis".into()));
    }
    if t.par_defaut {
        conn.execute("UPDATE taxe SET par_defaut = 0", [])?;
    }
    let id = Uuid::new_v4().to_string();
    let typ = t.r#type.unwrap_or(TypeTaxe::Pourcentage);
    conn.execute(
        "INSERT INTO taxe (id, nom, taux, type, actif, par_defaut) VALUES (?1,?2,?3,?4,1,?5)",
        params![id, nom, t.taux, typ, t.par_defaut as i64],
    )?;
    Ok(Taxe { id, nom: nom.to_string(), taux: t.taux, r#type: typ, actif: true, par_defaut: t.par_defaut })
}

pub fn modifier(conn: &Connection, id: &str, t: &NouvelleTaxe) -> Result<Taxe> {
    if t.par_defaut {
        conn.execute("UPDATE taxe SET par_defaut = 0", [])?;
    }
    let typ = t.r#type.unwrap_or(TypeTaxe::Pourcentage);
    let n = conn.execute(
        "UPDATE taxe SET nom = ?2, taux = ?3, type = ?4, par_defaut = ?5 WHERE id = ?1",
        params![id, t.nom.trim(), t.taux, typ, t.par_defaut as i64],
    )?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("taxe {id}")));
    }
    Ok(Taxe { id: id.to_string(), nom: t.nom.trim().to_string(), taux: t.taux, r#type: typ, actif: true, par_defaut: t.par_defaut })
}

/// Suppression douce : `actif = 0` (une taxe peut être référencée par l'historique).
pub fn supprimer(conn: &Connection, id: &str) -> Result<()> {
    let n = conn.execute("UPDATE taxe SET actif = 0, par_defaut = 0 WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("taxe {id}")));
    }
    Ok(())
}

fn ligne_vers_taxe(r: &rusqlite::Row) -> rusqlite::Result<Taxe> {
    let ty: String = r.get(3)?;
    Ok(Taxe {
        id: r.get(0)?,
        nom: r.get(1)?,
        taux: r.get(2)?,
        r#type: TypeTaxe::parse(&ty).unwrap_or(TypeTaxe::Pourcentage),
        actif: r.get::<_, i64>(4)? != 0,
        par_defaut: r.get::<_, i64>(5)? != 0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn catalogue_taxes() {
        let conn = db::open_in_memory().unwrap();
        assert_eq!(lister(&conn).unwrap().len(), 3); // reprises de taux_tva
        let t = creer(&conn, &NouvelleTaxe {
            nom: "Taxe tourisme".into(), taux: 2.0,
            r#type: Some(TypeTaxe::Pourcentage), par_defaut: false,
        }).unwrap();
        assert_eq!(lister(&conn).unwrap().len(), 4);
        // désactiver retire des actives mais reste visible dans « tous »
        definir_actif(&conn, &t.id, false).unwrap();
        assert_eq!(lister(&conn).unwrap().len(), 3);
        assert_eq!(lister_tous(&conn).unwrap().len(), 4);
        // réactiver
        definir_actif(&conn, &t.id, true).unwrap();
        assert_eq!(lister(&conn).unwrap().len(), 4);
        supprimer(&conn, &t.id).unwrap();
        assert_eq!(lister(&conn).unwrap().len(), 3); // désactivée
    }
}
