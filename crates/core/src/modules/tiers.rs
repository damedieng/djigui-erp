//! Tiers unifié (spec §3.1) — un partenaire porte un rôle
//! (`client` | `fournisseur` | `les_deux`), jamais deux tables séparées.

use crate::domain::{NatureTiers, TypeRole};
use crate::error::{CoreError, Result};
use crate::now;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Tiers {
    pub id: String,
    pub code: String,
    pub type_role: TypeRole,
    pub nom: String,
    pub telephone: Option<String>,
    pub adresse: Option<String>,
    pub ninea: Option<String>,
    /// Particulier ou entreprise : pilote les mentions de la facture.
    #[serde(default = "nature_defaut")]
    pub nature: NatureTiers,
    /// Particulier — facultatif.
    pub prenom: Option<String>,
    /// Particulier — facultatif, jamais exigée.
    pub cni: Option<String>,
    /// Entreprise — facultatif.
    pub rccm: Option<String>,
    pub solde: f64,
    pub actif: bool,
    pub cree_le: String,
    /// Client exonéré de TVA : ses factures sont émises sans taxe (§ contrat).
    #[serde(default)]
    pub exonere_tva: bool,
    /// Taux de **retenue à la source** que ce tiers prélève sur nos factures
    /// et reverse lui-même au Trésor (mig 0043). `None` = non concerné.
    ///
    /// ⚠️ `None` et `Some(0.0)` ne disent PAS la même chose : « pas concerné »
    /// et « concerné au taux zéro » se justifient différemment dans un dossier.
    #[serde(default)]
    pub retenue_source_taux: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouveauTiers {
    pub code: String,
    pub type_role: TypeRole,
    pub nom: String,
    pub telephone: Option<String>,
    pub adresse: Option<String>,
    pub ninea: Option<String>,
    #[serde(default = "nature_defaut")]
    pub nature: NatureTiers,
    #[serde(default)]
    pub prenom: Option<String>,
    #[serde(default)]
    pub cni: Option<String>,
    #[serde(default)]
    pub rccm: Option<String>,
    #[serde(default)]
    pub exonere_tva: bool,
    #[serde(default)]
    pub retenue_source_taux: Option<f64>,
}

fn nature_defaut() -> NatureTiers {
    NatureTiers::Particulier
}

/// Filtre de liste, aligné sur les rôles (chips de l'écran Tiers).
#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Filtre {
    #[default]
    Tous,
    Client,
    Fournisseur,
}

pub fn creer(conn: &Connection, t: &NouveauTiers) -> Result<Tiers> {
    let id = Uuid::new_v4().to_string();
    let cree_le = now();
    conn.execute(
        "INSERT INTO tiers (id, code, type_role, nom, telephone, adresse, ninea, solde, actif, cree_le,
                            exonere_tva, nature, prenom, cni, rccm, retenue_source_taux)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0, 1, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
        params![id, t.code, t.type_role, t.nom, t.telephone, t.adresse, t.ninea, cree_le,
                t.exonere_tva as i64, t.nature, t.prenom, t.cni, t.rccm, t.retenue_source_taux],
    )?;
    lire(conn, &id)
}

pub fn lire(conn: &Connection, id: &str) -> Result<Tiers> {
    conn.query_row(
        "SELECT id, code, type_role, nom, telephone, adresse, ninea, solde, actif, cree_le, exonere_tva,
                nature, prenom, cni, rccm, retenue_source_taux
         FROM tiers WHERE id = ?1",
        params![id],
        ligne_vers_tiers,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => CoreError::NotFound(format!("tiers {id}")),
        autre => autre.into(),
    })
}

pub fn lister(conn: &Connection, filtre: Filtre) -> Result<Vec<Tiers>> {
    // client et fournisseur incluent le rôle `les_deux`.
    let clause = match filtre {
        Filtre::Tous => "actif = 1",
        Filtre::Client => "actif = 1 AND type_role IN ('client','les_deux')",
        Filtre::Fournisseur => "actif = 1 AND type_role IN ('fournisseur','les_deux')",
    };
    let sql = format!(
        "SELECT id, code, type_role, nom, telephone, adresse, ninea, solde, actif, cree_le, exonere_tva,
                nature, prenom, cni, rccm, retenue_source_taux
         FROM tiers WHERE {clause} ORDER BY nom"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], ligne_vers_tiers)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Modifie un tiers (CRUD complet).
pub fn modifier(conn: &Connection, id: &str, t: &NouveauTiers) -> Result<Tiers> {
    let n = conn.execute(
        "UPDATE tiers SET code = ?2, type_role = ?3, nom = ?4, telephone = ?5,
                          adresse = ?6, ninea = ?7, exonere_tva = ?8,
                          nature = ?9, prenom = ?10, cni = ?11, rccm = ?12,
                          retenue_source_taux = ?13 WHERE id = ?1",
        params![id, t.code, t.type_role, t.nom, t.telephone, t.adresse, t.ninea,
                t.exonere_tva as i64, t.nature, t.prenom, t.cni, t.rccm, t.retenue_source_taux],
    )?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("tiers {id}")));
    }
    lire(conn, id)
}

/// Désactive un tiers (soft delete). On ne supprime jamais un tiers ayant un
/// historique de documents/paiements.
pub fn desactiver(conn: &Connection, id: &str) -> Result<()> {
    let n = conn.execute("UPDATE tiers SET actif = 0 WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("tiers {id}")));
    }
    Ok(())
}

/// Résultat d'une opération par lot : nombre d'éléments traités.
#[derive(Debug, Clone, Serialize)]
pub struct ResultatLot {
    pub traites: usize,
}

/// Désactive plusieurs tiers en une opération (traitement par lot).
pub fn desactiver_lot(conn: &Connection, ids: &[String]) -> Result<ResultatLot> {
    let mut traites = 0;
    for id in ids {
        traites += conn.execute("UPDATE tiers SET actif = 0 WHERE id = ?1", params![id])? ;
    }
    Ok(ResultatLot { traites })
}

/// Change le rôle de plusieurs tiers en une opération (traitement par lot).
pub fn changer_role_lot(conn: &Connection, ids: &[String], role: TypeRole) -> Result<ResultatLot> {
    let mut traites = 0;
    for id in ids {
        traites += conn.execute(
            "UPDATE tiers SET type_role = ?2 WHERE id = ?1",
            params![id, role],
        )?;
    }
    Ok(ResultatLot { traites })
}

fn ligne_vers_tiers(r: &rusqlite::Row) -> rusqlite::Result<Tiers> {
    let role: String = r.get(2)?;
    Ok(Tiers {
        id: r.get(0)?,
        code: r.get(1)?,
        type_role: TypeRole::parse(&role).unwrap_or(TypeRole::Client),
        nom: r.get(3)?,
        telephone: r.get(4)?,
        adresse: r.get(5)?,
        ninea: r.get(6)?,
        solde: r.get(7)?,
        actif: r.get::<_, i64>(8)? != 0,
        cree_le: r.get(9)?,
        exonere_tva: r.get::<_, i64>(10)? != 0,
        nature: {
            let n: String = r.get(11)?;
            NatureTiers::parse(&n).unwrap_or(NatureTiers::Particulier)
        },
        prenom: r.get(12)?,
        cni: r.get(13)?,
        rccm: r.get(14)?,
        retenue_source_taux: r.get(15)?,
    })
}

/// Mentions d'identité manquantes, à afficher en **alerte jaune**.
/// Ne bloque jamais : une vente reste possible avec un tiers sans aucune pièce
/// (voir 0027 — contexte Afrique de l'Ouest).
pub fn alertes_identite(t: &Tiers) -> Vec<String> {
    let vide = |o: &Option<String>| o.as_deref().map(str::trim).unwrap_or("").is_empty();
    let mut a = Vec::new();
    match t.nature {
        NatureTiers::Entreprise => {
            if vide(&t.ninea) {
                a.push("NINEA absent : il est attendu sur la facture d'une entreprise.".into());
            }
            if vide(&t.rccm) {
                a.push("RCCM absent : mention légale attendue pour une entreprise.".into());
            }
        }
        NatureTiers::Particulier => {
            if vide(&t.telephone) {
                a.push("Aucun téléphone : vous ne pourrez pas recontacter ce client.".into());
            }
        }
    }
    a
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn ajouter(conn: &Connection, code: &str, role: TypeRole) -> String {
        creer(conn, &NouveauTiers {
            code: code.into(), type_role: role, nom: code.into(),
            telephone: None, adresse: None, ninea: None, exonere_tva: false, retenue_source_taux: None,
            nature: NatureTiers::Particulier, prenom: None, cni: None, rccm: None,
        }).unwrap().id
    }

    /// Une entreprise sans NINEA ni RCCM s'enregistre quand même : on alerte,
    /// on ne bloque pas.
    #[test]
    fn entreprise_sans_ninea_est_acceptee_mais_alertee() {
        let conn = db::open_in_memory().unwrap();
        let t = creer(&conn, &NouveauTiers {
            code: "ENT".into(), type_role: TypeRole::Client, nom: "Sarl Teranga".into(),
            telephone: None, adresse: None, ninea: None, exonere_tva: false, retenue_source_taux: None,
            nature: NatureTiers::Entreprise, prenom: None, cni: None, rccm: None,
        }).unwrap();
        assert_eq!(t.nature, NatureTiers::Entreprise);
        assert_eq!(alertes_identite(&t).len(), 2); // NINEA + RCCM

        // Un particulier sans CNI n'est pas alerté sur la CNI (jamais exigée).
        let p = creer(&conn, &NouveauTiers {
            code: "PART".into(), type_role: TypeRole::Client, nom: "Diop".into(),
            telephone: Some("77".into()), adresse: None, ninea: None, exonere_tva: false, retenue_source_taux: None,
            nature: NatureTiers::Particulier, prenom: Some("Awa".into()), cni: None, rccm: None,
        }).unwrap();
        assert_eq!(p.prenom.as_deref(), Some("Awa"));
        assert!(alertes_identite(&p).is_empty());
    }

    #[test]
    fn creer_lister_filtre() {
        let conn = db::open_in_memory().unwrap();
        ajouter(&conn, "CLI", TypeRole::Client);
        ajouter(&conn, "FOU", TypeRole::Fournisseur);
        ajouter(&conn, "MIX", TypeRole::LesDeux);
        assert_eq!(lister(&conn, Filtre::Tous).unwrap().len(), 3);
        // client + les_deux
        assert_eq!(lister(&conn, Filtre::Client).unwrap().len(), 2);
        // fournisseur + les_deux
        assert_eq!(lister(&conn, Filtre::Fournisseur).unwrap().len(), 2);
    }

    #[test]
    fn modifier_desactiver_et_lot() {
        let conn = db::open_in_memory().unwrap();
        let a = ajouter(&conn, "A", TypeRole::Client);
        let b = ajouter(&conn, "B", TypeRole::Client);

        let m = modifier(&conn, &a, &NouveauTiers {
            code: "A".into(), type_role: TypeRole::LesDeux, nom: "Aïda".into(),
            telephone: Some("77".into()), adresse: None, ninea: None, exonere_tva: false, retenue_source_taux: None,
            nature: NatureTiers::Particulier, prenom: None, cni: None, rccm: None,
        }).unwrap();
        assert_eq!(m.nom, "Aïda");
        assert_eq!(m.type_role, TypeRole::LesDeux);

        // lot : passer A et B en fournisseur
        let r = changer_role_lot(&conn, &[a.clone(), b.clone()], TypeRole::Fournisseur).unwrap();
        assert_eq!(r.traites, 2);
        assert_eq!(lister(&conn, Filtre::Fournisseur).unwrap().len(), 2);

        // lot : désactiver les deux
        let r = desactiver_lot(&conn, &[a, b]).unwrap();
        assert_eq!(r.traites, 2);
        assert!(lister(&conn, Filtre::Tous).unwrap().is_empty());
    }
}
