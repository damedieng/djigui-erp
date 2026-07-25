//! Facturation cyclique / abonnements (spec §5.8, migrations 0008/0009).
//!
//! Un abonnement lie un **tiers** à un jeu de **lignes récurrentes** avec une
//! **fréquence** (mensuel, trimestriel, annuel), une **prochaine échéance** et,
//! pour un contrat, un **nombre d'échéances** limité (ex. 3 trimestres), une
//! **référence de dossier** et un **objet**. À chaque échéance atteinte,
//! [`generer_echeances_dues`] crée une **facture (brouillon)** à partir des
//! lignes, avec l'objet libellé « … — Trimestre N », propage le dossier, avance
//! l'échéance et incrémente le compteur ; l'abonnement se **désactive** une fois
//! toutes les échéances émises. Rattrapage si plusieurs périodes manquées.

use crate::domain::{FrequenceAbonnement, SensDocument, TypeDocument};
use crate::error::{CoreError, Result};
use crate::modules::document::{self, Document, NouveauDocument, NouvelleLigne};
use chrono::{Days, Months, NaiveDate};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbonnementLigne {
    pub article_id: String,
    pub designation: String,
    pub quantite: f64,
    pub prix_unitaire: f64,
    #[serde(default)]
    pub taux_tva: f64,
    #[serde(default)]
    pub remise: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Abonnement {
    pub id: String,
    pub tiers_id: String,
    pub tiers_nom: Option<String>,
    pub libelle: Option<String>,
    pub reference_dossier: Option<String>,
    pub objet: Option<String>,
    pub frequence: String,
    /// Date de début de facturation (fixe).
    pub date_debut: String,
    /// Prochaine échéance due (dérivée : début + échéances déjà émises).
    pub prochaine_echeance: String,
    /// Nombre total d'échéances (None = illimité).
    pub nb_echeances: Option<i64>,
    pub echeances_faites: i64,
    pub actif: bool,
    /// Le client est-il exonéré de TVA (pour l'affichage du montant réel).
    pub tiers_exonere: bool,
    pub lignes: Vec<AbonnementLigne>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouvelAbonnement {
    pub tiers_id: String,
    #[serde(default)]
    pub libelle: Option<String>,
    #[serde(default)]
    pub reference_dossier: Option<String>,
    #[serde(default)]
    pub objet: Option<String>,
    pub frequence: FrequenceAbonnement,
    /// Date de début de facturation (format `YYYY-MM-DD`).
    pub date_debut: String,
    #[serde(default)]
    pub nb_echeances: Option<i64>,
    pub lignes: Vec<AbonnementLigne>,
}

fn valider_saisie(a: &NouvelAbonnement) -> Result<()> {
    if NaiveDate::parse_from_str(a.date_debut.trim(), "%Y-%m-%d").is_err() {
        return Err(CoreError::Rule("date de début invalide (attendu AAAA-MM-JJ)".into()));
    }
    if a.lignes.is_empty() {
        return Err(CoreError::Rule("un abonnement doit comporter au moins une ligne".into()));
    }
    if let Some(n) = a.nb_echeances {
        if n <= 0 {
            return Err(CoreError::Rule("le nombre d'échéances doit être positif".into()));
        }
    }
    Ok(())
}

pub fn creer(conn: &Connection, a: &NouvelAbonnement) -> Result<Abonnement> {
    valider_saisie(a)?;
    let id = Uuid::new_v4().to_string();
    let debut = a.date_debut.trim();
    // Première échéance = fin de la 1re période (facturation en fin de période).
    let prochaine = echeance_fin(debut, a.frequence, 1);
    conn.execute(
        "INSERT INTO abonnement
            (id, tiers_id, libelle, reference_dossier, objet, frequence,
             date_debut, prochaine_echeance, nb_echeances, echeances_faites, actif)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,0,1)",
        params![
            id, a.tiers_id, a.libelle, a.reference_dossier, a.objet, a.frequence,
            debut, prochaine, a.nb_echeances,
        ],
    )?;
    ecrire_lignes(conn, &id, &a.lignes)?;
    lire(conn, &id)
}

fn ecrire_lignes(conn: &Connection, abonnement_id: &str, lignes: &[AbonnementLigne]) -> Result<()> {
    conn.execute("DELETE FROM abonnement_ligne WHERE abonnement_id = ?1", params![abonnement_id])?;
    for l in lignes {
        conn.execute(
            "INSERT INTO abonnement_ligne
                (id, abonnement_id, article_id, designation, quantite, prix_unitaire, taux_tva, remise)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                Uuid::new_v4().to_string(), abonnement_id, l.article_id, l.designation,
                l.quantite, l.prix_unitaire, l.taux_tva, l.remise,
            ],
        )?;
    }
    Ok(())
}

fn lignes_de(conn: &Connection, abonnement_id: &str) -> Result<Vec<AbonnementLigne>> {
    let mut stmt = conn.prepare(
        "SELECT article_id, designation, quantite, prix_unitaire, taux_tva, remise
         FROM abonnement_ligne WHERE abonnement_id = ?1",
    )?;
    let rows = stmt.query_map(params![abonnement_id], |r| {
        Ok(AbonnementLigne {
            article_id: r.get(0)?,
            designation: r.get(1)?,
            quantite: r.get(2)?,
            prix_unitaire: r.get(3)?,
            taux_tva: r.get(4)?,
            remise: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

const SELECT: &str = "
    SELECT a.id, a.tiers_id, t.nom, a.libelle, a.reference_dossier, a.objet,
           a.frequence, a.date_debut, a.prochaine_echeance, a.nb_echeances,
           a.echeances_faites, a.actif, COALESCE(t.exonere_tva, 0)
    FROM abonnement a LEFT JOIN tiers t ON t.id = a.tiers_id";

fn ligne_vers_abonnement(r: &rusqlite::Row) -> rusqlite::Result<Abonnement> {
    Ok(Abonnement {
        id: r.get(0)?,
        tiers_id: r.get(1)?,
        tiers_nom: r.get(2)?,
        libelle: r.get(3)?,
        reference_dossier: r.get(4)?,
        objet: r.get(5)?,
        frequence: r.get(6)?,
        date_debut: r.get(7)?,
        prochaine_echeance: r.get(8)?,
        nb_echeances: r.get(9)?,
        echeances_faites: r.get(10)?,
        actif: r.get::<_, i64>(11)? != 0,
        tiers_exonere: r.get::<_, i64>(12)? != 0,
        lignes: Vec::new(),
    })
}

pub fn lire(conn: &Connection, id: &str) -> Result<Abonnement> {
    let mut a = conn
        .query_row(&format!("{SELECT} WHERE a.id = ?1"), params![id], ligne_vers_abonnement)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => CoreError::NotFound(format!("abonnement {id}")),
            autre => autre.into(),
        })?;
    a.lignes = lignes_de(conn, id)?;
    Ok(a)
}

pub fn lister(conn: &Connection) -> Result<Vec<Abonnement>> {
    let mut stmt = conn.prepare(&format!("{SELECT} ORDER BY a.actif DESC, a.prochaine_echeance"))?;
    let rows = stmt.query_map([], ligne_vers_abonnement)?;
    let mut abos = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    for a in &mut abos {
        a.lignes = lignes_de(conn, &a.id)?;
    }
    Ok(abos)
}

/// Met à jour un abonnement (champs + lignes + état actif). Ne réinitialise pas
/// le compteur d'échéances déjà émises.
pub fn modifier(conn: &Connection, id: &str, a: &NouvelAbonnement, actif: bool) -> Result<Abonnement> {
    valider_saisie(a)?;
    // Échéances déjà émises : la prochaine échéance = début + N périodes.
    let faites: i64 = conn
        .query_row("SELECT echeances_faites FROM abonnement WHERE id = ?1", params![id], |r| r.get(0))
        .map_err(|_| CoreError::NotFound(format!("abonnement {id}")))?;
    let debut = a.date_debut.trim();
    // Prochaine échéance = fin de la prochaine période non encore facturée.
    let prochaine = echeance_fin(debut, a.frequence, faites + 1);
    let n = conn.execute(
        "UPDATE abonnement SET tiers_id = ?2, libelle = ?3, reference_dossier = ?4, objet = ?5,
                frequence = ?6, date_debut = ?7, prochaine_echeance = ?8, nb_echeances = ?9,
                actif = ?10 WHERE id = ?1",
        params![
            id, a.tiers_id, a.libelle, a.reference_dossier, a.objet, a.frequence,
            debut, prochaine, a.nb_echeances, actif as i64,
        ],
    )?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("abonnement {id}")));
    }
    ecrire_lignes(conn, id, &a.lignes)?;
    lire(conn, id)
}

pub fn supprimer(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("DELETE FROM abonnement_ligne WHERE abonnement_id = ?1", params![id])?;
    let n = conn.execute("DELETE FROM abonnement WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("abonnement {id}")));
    }
    Ok(())
}

/// Nombre de mois d'une période.
fn mois_par_periode(freq: FrequenceAbonnement) -> u32 {
    match freq {
        FrequenceAbonnement::Mensuel => 1,
        FrequenceAbonnement::Trimestriel => 3,
        FrequenceAbonnement::Annuel => 12,
    }
}

/// Date d'échéance de la **période k** (k ≥ 1) : facturation en **fin de
/// période**, soit `début + k périodes − 1 jour`. Ex. début 01/10/2025,
/// trimestriel : Trimestre 1 → 31/12/2025, Trimestre 4 → 30/09/2026.
fn echeance_fin(debut: &str, freq: FrequenceAbonnement, k: i64) -> String {
    let mois = mois_par_periode(freq) * (k.max(1) as u32);
    NaiveDate::parse_from_str(debut, "%Y-%m-%d")
        .ok()
        .and_then(|d| d.checked_add_months(Months::new(mois)))
        .and_then(|d| d.checked_sub_days(Days::new(1)))
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| debut.to_string())
}

/// Libellé de période selon la fréquence (pour l'objet « … Trimestre N »).
fn label_periode(freq: FrequenceAbonnement) -> &'static str {
    match freq {
        FrequenceAbonnement::Mensuel => "Mois",
        FrequenceAbonnement::Trimestriel => "Trimestre",
        FrequenceAbonnement::Annuel => "Année",
    }
}

/// Génère les factures des abonnements échus (`<= aujourdhui`). Rattrape les
/// périodes manquées, s'arrête au nombre d'échéances prévu (et désactive alors
/// l'abonnement). Retourne les factures créées (brouillon).
pub fn generer_echeances_dues(conn: &Connection, aujourdhui: &str) -> Result<Vec<Document>> {
    let dus = lister(conn)?
        .into_iter()
        .filter(|a| a.actif && !a.lignes.is_empty() && a.prochaine_echeance.as_str() <= aujourdhui)
        .collect::<Vec<_>>();

    let mut crees = Vec::new();
    for ab in dus {
        let freq = FrequenceAbonnement::parse(&ab.frequence).unwrap_or(FrequenceAbonnement::Mensuel);
        let mut faites = ab.echeances_faites;
        let mut garde = 0;
        loop {
            let k = faites + 1; // période à facturer (1-based)
            if let Some(total) = ab.nb_echeances {
                if k > total { break; } // contrat terminé
            }
            if garde >= 480 { break; } // cap de sécurité
            let echeance = echeance_fin(&ab.date_debut, freq, k);
            if echeance.as_str() > aujourdhui { break; } // pas encore échue
            crees.push(generer_facture(conn, &ab, &echeance, freq, k)?);
            faites = k;
            garde += 1;
        }
        // prochaine échéance = fin de la prochaine période non encore facturée
        let prochaine = echeance_fin(&ab.date_debut, freq, faites + 1);
        let termine = ab.nb_echeances.map(|t| faites >= t).unwrap_or(false);
        conn.execute(
            "UPDATE abonnement SET prochaine_echeance = ?2, echeances_faites = ?3, actif = ?4 WHERE id = ?1",
            params![ab.id, prochaine, faites, (!termine) as i64],
        )?;
    }
    Ok(crees)
}

/// Crée une facture brouillon à partir des lignes de l'abonnement, avec objet
/// « objet — Trimestre N » et la référence de dossier propagée.
fn generer_facture(
    conn: &Connection,
    ab: &Abonnement,
    date: &str,
    freq: FrequenceAbonnement,
    numero_echeance: i64,
) -> Result<Document> {
    let lignes: Vec<NouvelleLigne> = ab
        .lignes
        .iter()
        .map(|l| NouvelleLigne {
            article_id: l.article_id.clone(),
            designation: l.designation.clone(),
            quantite: l.quantite,
            prix_unitaire: l.prix_unitaire,
            taux_tva: l.taux_tva,
            remise: l.remise,
            taxes: vec![],
        })
        .collect();

    let periode = format!("{} {}", label_periode(freq), numero_echeance);
    let objet = match &ab.objet {
        Some(o) if !o.trim().is_empty() => format!("{o} — {periode}"),
        _ => periode.clone(),
    };
    let note = match ab.nb_echeances {
        Some(total) => format!("Facture d'abonnement — {periode}/{total} (échéance {date})"),
        None => format!("Facture d'abonnement — {periode} (échéance {date})"),
    };

    document::creer(conn, &NouveauDocument {
        type_document: TypeDocument::Facture,
        sens: SensDocument::Vente,
        tiers_id: ab.tiers_id.clone(),
        depot_id: None,
        date: Some(date.to_string()),
        note: Some(note),
        reference_dossier: ab.reference_dossier.clone(),
        objet: Some(objet),
        document_source_id: None,
        lignes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn base() -> Connection {
        let conn = db::open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO tiers (id, code, type_role, nom, solde, actif, cree_le, exonere_tva)
             VALUES ('t1','C1','client','Client Abo',0,1,'2026-01-01',0)",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO article (id, code, type, designation, prix_vente, taux_tva, gere_stock, actif)
             VALUES ('a1','ART','service','Maintenance',100000,18,0,1)",
            [],
        ).unwrap();
        conn
    }

    fn ligne(prix: f64) -> AbonnementLigne {
        AbonnementLigne {
            article_id: "a1".into(), designation: "Maintenance".into(),
            quantite: 1.0, prix_unitaire: prix, taux_tva: 18.0, remise: 0.0,
        }
    }

    #[test]
    fn contrat_trois_trimestres_s_arrete_et_libelle() {
        let conn = base();
        // 300 000 en 3 trimestres → 100 000 / trimestre
        creer(&conn, &NouvelAbonnement {
            tiers_id: "t1".into(), libelle: Some("Maintenance".into()),
            reference_dossier: Some("DOS-2026-007".into()), objet: Some("Maintenance parc".into()),
            frequence: FrequenceAbonnement::Trimestriel, date_debut: "2026-01-10".into(),
            nb_echeances: Some(3), lignes: vec![ligne(100000.0)],
        }).unwrap();

        // bien après la 3e échéance : exactement 3 factures, pas plus
        let crees = generer_echeances_dues(&conn, "2027-12-31").unwrap();
        assert_eq!(crees.len(), 3);
        assert_eq!(crees[0].objet.as_deref(), Some("Maintenance parc — Trimestre 1"));
        assert_eq!(crees[2].objet.as_deref(), Some("Maintenance parc — Trimestre 3"));
        assert_eq!(crees[0].reference_dossier.as_deref(), Some("DOS-2026-007"));
        assert_eq!(crees[0].total_ttc, 118000.0); // 100000 + 18%

        // l'abonnement est terminé (désactivé) → plus rien
        let ab = &lister(&conn).unwrap()[0];
        assert!(!ab.actif);
        assert_eq!(ab.echeances_faites, 3);
        assert_eq!(generer_echeances_dues(&conn, "2028-12-31").unwrap().len(), 0);
    }

    #[test]
    fn facturation_en_fin_de_periode_dates_et_compte() {
        let conn = base();
        // Contrat trimestriel du 01/10/2025, durée 1 an (4 échéances).
        creer(&conn, &NouvelAbonnement {
            tiers_id: "t1".into(), libelle: None, reference_dossier: None,
            objet: Some("Maintenance".into()),
            frequence: FrequenceAbonnement::Trimestriel, date_debut: "2025-10-01".into(),
            nb_echeances: Some(4), lignes: vec![ligne(100000.0)],
        }).unwrap();

        // Au 22/07/2026 : 3 échéances échues (fins de trimestre), la 4e est future.
        let crees = generer_echeances_dues(&conn, "2026-07-22").unwrap();
        assert_eq!(crees.len(), 3);
        assert_eq!(crees[0].date, "2025-12-31"); // fin trimestre 1
        assert_eq!(crees[1].date, "2026-03-31"); // fin trimestre 2
        assert_eq!(crees[2].date, "2026-06-30"); // fin trimestre 3
        let ab = &lister(&conn).unwrap()[0];
        assert_eq!(ab.prochaine_echeance, "2026-09-30"); // fin trimestre 4
        assert!(ab.actif);                    // pas encore terminé (3/4)
        assert_eq!(ab.echeances_faites, 3);

        // Passé le 30/09/2026 : la 4e est générée et le contrat se termine.
        let crees2 = generer_echeances_dues(&conn, "2026-10-05").unwrap();
        assert_eq!(crees2.len(), 1);
        assert_eq!(crees2[0].date, "2026-09-30");
        assert!(!lister(&conn).unwrap()[0].actif);
    }

    #[test]
    fn client_exonere_tva_facture_sans_taxe() {
        let conn = base();
        conn.execute("UPDATE tiers SET exonere_tva = 1 WHERE id = 't1'", []).unwrap();
        creer(&conn, &NouvelAbonnement {
            tiers_id: "t1".into(), libelle: None, reference_dossier: None, objet: None,
            frequence: FrequenceAbonnement::Trimestriel, date_debut: "2026-01-10".into(),
            nb_echeances: Some(1), lignes: vec![ligne(100000.0)],
        }).unwrap();
        let crees = generer_echeances_dues(&conn, "2026-06-01").unwrap();
        assert_eq!(crees.len(), 1);
        assert_eq!(crees[0].total_tva, 0.0);         // exonéré
        assert_eq!(crees[0].total_ttc, 100000.0);    // HT = TTC
    }

    #[test]
    fn sans_ligne_refuse() {
        let conn = base();
        let r = creer(&conn, &NouvelAbonnement {
            tiers_id: "t1".into(), libelle: None, reference_dossier: None, objet: None,
            frequence: FrequenceAbonnement::Mensuel, date_debut: "2026-01-15".into(),
            nb_echeances: None, lignes: vec![],
        });
        assert!(r.is_err());
    }
}
