//! Paramètres entreprise — table singleton (spec §5.9).
//! Sans ces informations, aucune facture n'est valable : on garantit toujours
//! l'existence d'une ligne unique.

use crate::error::Result;
use crate::now;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParametresEntreprise {
    pub raison_sociale: String,
    pub ninea: String,
    pub rccm: Option<String>,
    pub adresse: String,
    pub telephone: String,
    pub email: Option<String>,
    pub logo: Option<String>,
    pub devise: String,
    pub taux_tva_defaut: f64,
    pub pied_facture: Option<String>,
    /// Mentions légales OHADA — toutes facultatives (voir 0027).
    #[serde(default)]
    pub forme_juridique: Option<String>,
    #[serde(default)]
    pub capital: Option<f64>,
    #[serde(default)]
    pub regime_fiscal: Option<String>,
}

impl Default for ParametresEntreprise {
    fn default() -> Self {
        Self {
            raison_sociale: String::new(),
            ninea: String::new(),
            rccm: None,
            adresse: String::new(),
            telephone: String::new(),
            email: None,
            logo: None,
            devise: "FCFA".into(),
            taux_tva_defaut: 18.0,
            pied_facture: None,
            forme_juridique: None,
            capital: None,
            regime_fiscal: None,
        }
    }
}

/// Lit le singleton, en le créant vide au premier appel.
pub fn lire(conn: &Connection) -> Result<ParametresEntreprise> {
    assurer_singleton(conn)?;
    let p = conn.query_row(
        "SELECT raison_sociale, ninea, rccm, adresse, telephone, email, logo,
                devise, taux_tva_defaut, pied_facture,
                forme_juridique, capital, regime_fiscal
         FROM parametres_entreprise WHERE singleton = 1",
        [],
        |r| {
            Ok(ParametresEntreprise {
                raison_sociale: r.get(0)?,
                ninea: r.get(1)?,
                rccm: r.get(2)?,
                adresse: r.get(3)?,
                telephone: r.get(4)?,
                email: r.get(5)?,
                logo: r.get(6)?,
                devise: r.get(7)?,
                taux_tva_defaut: r.get(8)?,
                pied_facture: r.get(9)?,
                forme_juridique: r.get(10)?,
                capital: r.get(11)?,
                regime_fiscal: r.get(12)?,
            })
        },
    )?;
    Ok(p)
}

/// Met à jour le singleton (upsert sur l'unique ligne).
pub fn enregistrer(conn: &Connection, p: &ParametresEntreprise) -> Result<()> {
    assurer_singleton(conn)?;
    conn.execute(
        "UPDATE parametres_entreprise SET
            raison_sociale = ?1, ninea = ?2, rccm = ?3, adresse = ?4,
            telephone = ?5, email = ?6, logo = ?7, devise = ?8,
            taux_tva_defaut = ?9, pied_facture = ?10,
            forme_juridique = ?11, capital = ?12, regime_fiscal = ?13
         WHERE singleton = 1",
        params![
            p.raison_sociale, p.ninea, p.rccm, p.adresse, p.telephone,
            p.email, p.logo, p.devise, p.taux_tva_defaut, p.pied_facture,
            p.forme_juridique, p.capital, p.regime_fiscal,
        ],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Paramètres globaux clé/valeur (table `parametre_global`) — options diverses
// (ex. impression auto du ticket, gestion de stock active…).
// ---------------------------------------------------------------------------

/// Renvoie tous les paramètres globaux sous forme de couples clé → valeur.
pub fn lister_globaux(conn: &Connection) -> Result<std::collections::HashMap<String, String>> {
    let mut stmt = conn.prepare("SELECT cle, valeur FROM parametre_global")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<std::collections::HashMap<_, _>>>()?)
}

/// Écrit (upsert) un paramètre global.
pub fn ecrire_global(conn: &Connection, cle: &str, valeur: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO parametre_global (cle, valeur) VALUES (?1, ?2)
         ON CONFLICT(cle) DO UPDATE SET valeur = ?2",
        rusqlite::params![cle, valeur],
    )?;
    Ok(())
}

fn assurer_singleton(conn: &Connection) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO parametres_entreprise (id, singleton) VALUES (?1, 1)",
        params![Uuid::new_v4().to_string()],
    )?;
    let _ = now(); // horodatage réservé pour un futur champ `maj_le`
    Ok(())
}
