//! Rapports d'analyse (§7). Calculs dérivés des journaux — aucune donnée stockée.
//!
//! Le **bénéfice** d'une vente = chiffre d'affaires HT − coût d'achat des articles
//! vendus (prix_achat × quantité). On agrège par **mois** (date du document) et
//! par **caisse** (celle de l'encaissement). Seules les factures de vente
//! **validées** comptent — les ventes **annulées** (statut `annule`) sont exclues.

use crate::error::Result;
use rusqlite::Connection;
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
pub struct LigneBenefice {
    /// Mois au format « AAAA-MM ».
    pub mois: String,
    pub caisse_id: Option<String>,
    pub caisse_nom: String,
    /// Chiffre d'affaires HT (somme des lignes).
    pub ca_ht: f64,
    /// Chiffre d'affaires TTC (total_ttc du document).
    pub ca_ttc: f64,
    /// Coût d'achat des articles vendus (prix_achat × quantité).
    pub cout_achat: f64,
    /// Bénéfice = CA HT − coût d'achat.
    pub benefice: f64,
    pub nb_ventes: i64,
}

fn arrondi(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Bénéfices agrégés par mois × caisse (ventes validées uniquement), bornés à la
/// période `[du, au]` (dates « AAAA-MM-JJ » incluses ; `None` = sans borne).
pub fn benefices_par_mois_caisse(
    conn: &Connection,
    du: Option<&str>,
    au: Option<&str>,
) -> Result<Vec<LigneBenefice>> {
    // Un enregistrement par facture de vente validée, avec sa caisse et ses coûts.
    let mut stmt = conn.prepare(
        "SELECT substr(d.date, 1, 7) AS mois,
                (SELECT p.caisse_id FROM paiement p
                   WHERE p.document_id = d.id AND p.sens = 'encaissement' LIMIT 1) AS caisse_id,
                d.total_ttc,
                (SELECT COALESCE(SUM(dl.total_ligne_ht), 0)
                   FROM document_ligne dl WHERE dl.document_id = d.id) AS ca_ht,
                (SELECT COALESCE(SUM(COALESCE(a.prix_achat, 0) * dl.quantite), 0)
                   FROM document_ligne dl JOIN article a ON a.id = dl.article_id
                   WHERE dl.document_id = d.id) AS cout
         FROM document d
         WHERE d.type_document = 'facture' AND d.sens = 'vente' AND d.statut = 'valide'
           AND (?1 IS NULL OR d.date >= ?1) AND (?2 IS NULL OR d.date <= ?2)",
    )?;
    let rows = stmt.query_map(rusqlite::params![du, au], |r| {
        Ok((
            r.get::<_, String>(0)?,          // mois
            r.get::<_, Option<String>>(1)?,  // caisse_id
            r.get::<_, f64>(2)?,             // ttc
            r.get::<_, f64>(3)?,             // ca_ht
            r.get::<_, f64>(4)?,             // cout
        ))
    })?;

    // Agrégation en mémoire par (mois, caisse) — clé triée pour un rendu stable.
    let mut agg: BTreeMap<(String, Option<String>), LigneBenefice> = BTreeMap::new();
    for row in rows {
        let (mois, caisse_id, ttc, ht, cout) = row?;
        let e = agg.entry((mois.clone(), caisse_id.clone())).or_insert_with(|| LigneBenefice {
            mois, caisse_id, caisse_nom: String::new(),
            ca_ht: 0.0, ca_ttc: 0.0, cout_achat: 0.0, benefice: 0.0, nb_ventes: 0,
        });
        e.ca_ht += ht;
        e.ca_ttc += ttc;
        e.cout_achat += cout;
        e.nb_ventes += 1;
    }

    // Noms de caisses.
    let mut noms: BTreeMap<String, String> = BTreeMap::new();
    {
        let mut s = conn.prepare("SELECT id, nom FROM caisse")?;
        let it = s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        for x in it { let (id, nom) = x?; noms.insert(id, nom); }
    }

    let mut out: Vec<LigneBenefice> = agg.into_values().map(|mut l| {
        l.ca_ht = arrondi(l.ca_ht);
        l.ca_ttc = arrondi(l.ca_ttc);
        l.cout_achat = arrondi(l.cout_achat);
        l.benefice = arrondi(l.ca_ht - l.cout_achat);
        l.caisse_nom = l.caisse_id.as_ref()
            .and_then(|id| noms.get(id).cloned())
            .unwrap_or_else(|| "Sans caisse".into());
        l
    }).collect();
    out.sort_by(|a, b| a.mois.cmp(&b.mois).then(a.caisse_nom.cmp(&b.caisse_nom)));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::domain::{SensDocument, SensPaiement, ModePaiement, TypeDocument, TypeArticle};
    use crate::modules::{article::{self, NouvelArticle}, document::{self, NouveauDocument, NouvelleLigne}, paiement::{self, NouveauPaiement}};
    use rusqlite::params;

    #[test]
    fn benefice_ca_moins_cout_ventes_validees() {
        let conn = db::open_in_memory().unwrap();
        conn.execute("INSERT INTO tiers (id, code, type_role, nom, solde, actif, cree_le)
                      VALUES ('t1','t1','client','C',0,1,'2026-01-01')", []).unwrap();
        // article vendu 1000, acheté 600
        let a = article::creer(&conn, &NouvelArticle {
            code: "A1".into(), r#type: TypeArticle::Bien, designation: "X".into(),
            prix_vente: 1000.0, prix_achat: Some(600.0), taux_tva: 0.0, gere_stock: true,
            stock_alerte: None, categorie_id: None, image: None, code_barre: None, taxes: None,
        }).unwrap().id;
        let doc = document::creer(&conn, &NouveauDocument {
            type_document: TypeDocument::Facture, sens: SensDocument::Vente,
            tiers_id: "t1".into(), depot_id: None, date: Some("2026-07-10".into()),
            note: None, document_source_id: None, reference_dossier: None, objet: None,
            lignes: vec![NouvelleLigne { article_id: a, designation: "X".into(),
                quantite: 2.0, prix_unitaire: 1000.0, taux_tva: 0.0, remise: 0.0, taxes: vec![] }],
        }).unwrap();
        document::valider(&conn, &doc.id).unwrap();
        let caisse = paiement::caisse_defaut(&conn).unwrap();
        paiement::enregistrer(&conn, &NouveauPaiement {
            tiers_id: "t1".into(), caisse_id: Some(caisse.clone()), document_id: Some(doc.id.clone()),
            sens: SensPaiement::Encaissement, montant: 2000.0, mode: ModePaiement::Espece, moyen_paiement_id: None,
        }).unwrap();

        let r = benefices_par_mois_caisse(&conn, None, None).unwrap();
        assert_eq!(r.len(), 1);
        let l = &r[0];
        assert_eq!(l.mois, "2026-07");
        assert_eq!(l.ca_ht, 2000.0);
        assert_eq!(l.cout_achat, 1200.0);
        assert_eq!(l.benefice, 800.0);
        assert_eq!(l.caisse_id.as_deref(), Some(caisse.as_str()));

        // annulée ⇒ exclue
        document::annuler(&conn, &doc.id, "test", Some("admin")).unwrap();
        let _ = params![]; // (garde l'import params utilisé)
        assert!(benefices_par_mois_caisse(&conn, None, None).unwrap().is_empty());
    }
}
