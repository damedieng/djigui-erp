//! Taux de TVA paramétrables (migration 0006). Gérés dans les paramètres et
//! proposés à la création d'un article. Un seul taux « par défaut ».

use crate::error::{CoreError, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TauxTva {
    pub valeur: f64,
    pub libelle: String,
    pub par_defaut: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouveauTaux {
    pub valeur: f64,
    pub libelle: Option<String>,
    #[serde(default)]
    pub par_defaut: bool,
}

pub fn lister(conn: &Connection) -> Result<Vec<TauxTva>> {
    let mut stmt = conn.prepare("SELECT valeur, libelle, par_defaut FROM taux_tva ORDER BY valeur DESC")?;
    let rows = stmt.query_map([], |r| {
        Ok(TauxTva { valeur: r.get(0)?, libelle: r.get(1)?, par_defaut: r.get::<_, i64>(2)? != 0 })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Crée (ou met à jour) un taux. Si `par_defaut`, retire le drapeau des autres.
pub fn creer(conn: &Connection, t: &NouveauTaux) -> Result<TauxTva> {
    if t.valeur < 0.0 {
        return Err(CoreError::Rule("le taux de TVA ne peut pas être négatif".into()));
    }
    let libelle = t.libelle.clone().unwrap_or_else(|| format!("{} %", t.valeur));
    if t.par_defaut {
        conn.execute("UPDATE taux_tva SET par_defaut = 0", [])?;
    }
    conn.execute(
        "INSERT INTO taux_tva (valeur, libelle, par_defaut) VALUES (?1, ?2, ?3)
         ON CONFLICT(valeur) DO UPDATE SET libelle = ?2, par_defaut = ?3",
        params![t.valeur, libelle, t.par_defaut as i64],
    )?;
    Ok(TauxTva { valeur: t.valeur, libelle, par_defaut: t.par_defaut })
}

pub fn supprimer(conn: &Connection, valeur: f64) -> Result<()> {
    let etait_defaut: bool = conn
        .query_row("SELECT par_defaut FROM taux_tva WHERE valeur = ?1", params![valeur],
                   |r| Ok(r.get::<_, i64>(0)? != 0))
        .unwrap_or(false);
    let n = conn.execute("DELETE FROM taux_tva WHERE valeur = ?1", params![valeur])?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("taux {valeur}")));
    }
    // s'il n'y a plus de défaut, promeut le plus élevé restant
    if etait_defaut {
        conn.execute(
            "UPDATE taux_tva SET par_defaut = 1
             WHERE valeur = (SELECT valeur FROM taux_tva ORDER BY valeur DESC LIMIT 1)", [],
        )?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn defaut_unique_et_suppression() {
        let conn = db::open_in_memory().unwrap();
        assert_eq!(lister(&conn).unwrap().len(), 3);
        // définir 10 comme défaut retire le défaut de 18
        creer(&conn, &NouveauTaux { valeur: 10.0, libelle: None, par_defaut: true }).unwrap();
        let defs: Vec<_> = lister(&conn).unwrap().into_iter().filter(|t| t.par_defaut).collect();
        assert_eq!(defs.len(), 1);
        assert_eq!(defs[0].valeur, 10.0);
        // supprimer le défaut en promeut un autre
        supprimer(&conn, 10.0).unwrap();
        assert_eq!(lister(&conn).unwrap().iter().filter(|t| t.par_defaut).count(), 1);
    }
}
