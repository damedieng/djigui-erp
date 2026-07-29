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

// ===========================================================================
// Les rapports de l'écran « Rapports » (§7)
//
// Tout est **dérivé des journaux** : aucune donnée n'est stockée, rien n'est
// recalculé à l'avance. Ce sont les mêmes faits que le commerçant a déjà saisis,
// simplement regardés sous un autre angle.
//
// Ces rapports parlent le langage du COMMERÇANT (« ce que j'ai vendu », « ce
// qu'on me doit »), là où l'écran comptable parle celui du comptable.
// ===========================================================================

/// Bornes de période, communes à tous les rapports.
#[derive(Debug, Clone, Default, serde::Deserialize)]
pub struct Periode {
    #[serde(default)]
    pub du: Option<String>,
    #[serde(default)]
    pub au: Option<String>,
}

// ---- Journal des ventes / des achats --------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct LigneJournal {
    pub document_id: String,
    pub numero: String,
    pub date: String,
    pub type_document: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiers_nom: Option<String>,
    pub total_ht: f64,
    pub total_tva: f64,
    pub total_ttc: f64,
    /// Retenue à la source prélevée par le client (mig 0043).
    #[serde(default)]
    pub montant_retenue: f64,
    /// Ce que le tiers doit réellement : TTC − retenue.
    pub net_a_payer: f64,
    /// Ce qui a effectivement été réglé sur cette pièce.
    pub regle: f64,
    /// Ce qui reste dû. Négatif = le client a payé plus que la facture.
    pub reste: f64,
    pub statut: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Journal {
    pub lignes: Vec<LigneJournal>,
    pub total_ht: f64,
    pub total_tva: f64,
    pub total_ttc: f64,
    pub total_retenue: f64,
    pub total_net: f64,
    pub total_regle: f64,
    pub total_reste: f64,
}

/// Journal des ventes (`sens = "vente"`) ou des achats (`"achat"`).
///
/// Les pièces **annulées** sont incluses mais marquées : elles font partie de
/// l'histoire, et les masquer donnerait l'impression d'un trou dans la
/// numérotation. En revanche elles ne comptent pas dans les totaux.
pub fn journal(conn: &Connection, sens: &str, p: &Periode) -> Result<Journal> {
    let mut st = conn.prepare(
        "SELECT d.id, d.numero, d.date, d.type_document, t.nom,
                d.total_ht, d.total_tva, d.total_ttc, d.statut,
                d.montant_retenue,
                (SELECT COALESCE(SUM(CASE WHEN pa.sens = 'encaissement'
                                          THEN pa.montant ELSE -pa.montant END), 0)
                   FROM paiement pa WHERE pa.document_id = d.id)
           FROM document d
           LEFT JOIN tiers t ON t.id = d.tiers_id
          WHERE d.sens = ?1
            AND d.type_document IN ('facture','avoir')
            AND d.statut IN ('valide','annule')
            AND (?2 IS NULL OR d.date >= ?2)
            AND (?3 IS NULL OR d.date <= ?3)
          ORDER BY d.date, d.numero",
    )?;
    let lignes = st
        .query_map(rusqlite::params![sens, p.du, p.au], |r| {
            let ttc: f64 = r.get(7)?;
            let statut: String = r.get(8)?;
            // Un encaissement compte positivement pour une vente ; pour un achat
            // c'est le décaissement qui règle la pièce, d'où la valeur absolue.
            let regle: f64 = r.get::<_, f64>(10)?.abs();
            let retenue: f64 = r.get(9)?;
            Ok(LigneJournal {
                document_id: r.get(0)?,
                numero: r.get(1)?,
                date: r.get(2)?,
                type_document: r.get(3)?,
                tiers_nom: r.get(4)?,
                total_ht: arrondi(r.get(5)?),
                total_tva: arrondi(r.get(6)?),
                total_ttc: arrondi(ttc),
                montant_retenue: arrondi(retenue),
                net_a_payer: arrondi(ttc - retenue),
                regle: arrondi(regle),
                // ⚠️ Le reste dû se mesure sur le NET À PAYER (mig 0043). Sur
                // le TTC, une facture avec retenue resterait « impayée » du
                // montant de la retenue même entièrement réglée.
                reste: arrondi(if statut == "annule" { 0.0 } else { ttc - retenue - regle }),
                statut,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let actives = || lignes.iter().filter(|l| l.statut != "annule");
    Ok(Journal {
        total_ht: arrondi(actives().map(|l| l.total_ht).sum()),
        total_tva: arrondi(actives().map(|l| l.total_tva).sum()),
        total_ttc: arrondi(actives().map(|l| l.total_ttc).sum()),
        total_retenue: arrondi(actives().map(|l| l.montant_retenue).sum()),
        total_net: arrondi(actives().map(|l| l.net_a_payer).sum()),
        total_regle: arrondi(actives().map(|l| l.regle).sum()),
        total_reste: arrondi(actives().map(|l| l.reste).sum()),
        lignes,
    })
}

// ---- Marge par article -----------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct LigneMarge {
    pub article_id: String,
    pub code: String,
    pub designation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categorie_nom: Option<String>,
    pub quantite_vendue: f64,
    pub ca_ht: f64,
    pub cout: f64,
    pub marge: f64,
    /// Marge en % du chiffre d'affaires. `None` si aucun chiffre d'affaires.
    pub marge_pct: Option<f64>,
    /// ⚠️ Vrai si le coût repose sur un **prix d'achat estimé** par Djigui, ou
    /// si l'article n'a aucun prix d'achat. Dans les deux cas, la marge affichée
    /// n'est pas fiable et l'écran doit le dire.
    pub cout_incertain: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct RapportMarge {
    pub lignes: Vec<LigneMarge>,
    pub total_ca: f64,
    pub total_cout: f64,
    pub total_marge: f64,
    /// Nombre d'articles vendus dont le coût n'est pas fiable.
    pub nb_cout_incertain: i64,
}

/// Marge par article sur la période, la plus rentable en premier.
///
/// ⚠️ Le coût vaut `prix_achat × quantité` : il ne sait donc que ce que
/// l'article coûte **aujourd'hui**, pas ce qu'il coûtait au moment de la vente.
/// C'est ce que corrigera la valorisation du stock (CUMP).
pub fn marges_par_article(conn: &Connection, p: &Periode) -> Result<RapportMarge> {
    let mut st = conn.prepare(
        "SELECT a.id, a.code, a.designation, c.nom,
                COALESCE(SUM(dl.quantite), 0),
                COALESCE(SUM(dl.total_ligne_ht), 0),
                COALESCE(SUM(COALESCE(a.prix_achat, 0) * dl.quantite), 0),
                a.prix_achat_estime, a.prix_achat
           FROM document_ligne dl
           JOIN document d ON d.id = dl.document_id
           JOIN article a ON a.id = dl.article_id
           LEFT JOIN categorie c ON c.id = a.categorie_id
          WHERE d.sens = 'vente' AND d.statut = 'valide'
            AND d.type_document IN ('facture','avoir')
            AND (?1 IS NULL OR d.date >= ?1)
            AND (?2 IS NULL OR d.date <= ?2)
          GROUP BY a.id, a.code, a.designation, c.nom, a.prix_achat_estime, a.prix_achat
          ORDER BY 6 DESC",
    )?;
    let mut lignes = st
        .query_map(rusqlite::params![p.du, p.au], |r| {
            let ca: f64 = r.get(5)?;
            let cout: f64 = r.get(6)?;
            let estime: bool = r.get::<_, i64>(7)? != 0;
            let prix_achat: Option<f64> = r.get(8)?;
            Ok(LigneMarge {
                article_id: r.get(0)?,
                code: r.get(1)?,
                designation: r.get(2)?,
                categorie_nom: r.get(3)?,
                quantite_vendue: arrondi(r.get(4)?),
                ca_ht: arrondi(ca),
                cout: arrondi(cout),
                marge: arrondi(ca - cout),
                marge_pct: if ca.abs() > 0.005 {
                    Some(((ca - cout) / ca * 100.0).round())
                } else {
                    None
                },
                cout_incertain: estime || prix_achat.unwrap_or(0.0) <= 0.0,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    lignes.sort_by(|a, b| b.marge.partial_cmp(&a.marge).unwrap_or(std::cmp::Ordering::Equal));

    Ok(RapportMarge {
        total_ca: arrondi(lignes.iter().map(|l| l.ca_ht).sum()),
        total_cout: arrondi(lignes.iter().map(|l| l.cout).sum()),
        total_marge: arrondi(lignes.iter().map(|l| l.marge).sum()),
        nb_cout_incertain: lignes.iter().filter(|l| l.cout_incertain).count() as i64,
        lignes,
    })
}

// ---- État du stock et ruptures ---------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct LigneStock {
    pub article_id: String,
    pub code: String,
    pub designation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categorie_nom: Option<String>,
    pub stock: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stock_alerte: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prix_achat: Option<f64>,
    pub prix_achat_estime: bool,
    /// Valeur du stock au prix d'achat. Approximative tant que la valorisation
    /// CUMP n'existe pas — et carrément fausse si le prix d'achat est estimé.
    pub valeur: f64,
    /// `rupture` (stock ≤ 0), `alerte` (sous le seuil), ou `ok`.
    pub etat: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RapportStock {
    pub lignes: Vec<LigneStock>,
    pub valeur_totale: f64,
    pub nb_ruptures: i64,
    pub nb_alertes: i64,
    pub nb_valeur_incertaine: i64,
}

/// État du stock, **les problèmes en premier** : ruptures, puis seuils d'alerte,
/// puis le reste. C'est ce que le commerçant vient chercher.
pub fn etat_stock(conn: &Connection) -> Result<RapportStock> {
    let mut st = conn.prepare(
        "SELECT a.id, a.code, a.designation, c.nom, a.stock_alerte,
                a.prix_achat, a.prix_achat_estime,
                COALESCE((SELECT SUM(CASE WHEN m.sens = 'entree' THEN m.quantite
                                          ELSE -m.quantite END)
                            FROM mouvement_stock m WHERE m.article_id = a.id), 0)
           FROM article a
           LEFT JOIN categorie c ON c.id = a.categorie_id
          WHERE a.actif = 1 AND a.gere_stock = 1
          ORDER BY a.designation",
    )?;
    let mut lignes = st
        .query_map([], |r| {
            let stock: f64 = r.get(7)?;
            let alerte: Option<f64> = r.get(4)?;
            let prix_achat: Option<f64> = r.get(5)?;
            let estime: bool = r.get::<_, i64>(6)? != 0;
            let etat = if stock <= 0.0 {
                "rupture"
            } else if alerte.map(|s| stock <= s).unwrap_or(false) {
                "alerte"
            } else {
                "ok"
            };
            Ok(LigneStock {
                article_id: r.get(0)?,
                code: r.get(1)?,
                designation: r.get(2)?,
                categorie_nom: r.get(3)?,
                stock: arrondi(stock),
                stock_alerte: alerte,
                prix_achat,
                prix_achat_estime: estime,
                valeur: arrondi(stock.max(0.0) * prix_achat.unwrap_or(0.0)),
                etat: etat.into(),
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // Les ennuis d'abord : rupture, puis alerte, puis le reste.
    let rang = |e: &str| match e { "rupture" => 0, "alerte" => 1, _ => 2 };
    lignes.sort_by(|a, b| {
        rang(&a.etat).cmp(&rang(&b.etat)).then(a.designation.cmp(&b.designation))
    });

    Ok(RapportStock {
        valeur_totale: arrondi(lignes.iter().map(|l| l.valeur).sum()),
        nb_ruptures: lignes.iter().filter(|l| l.etat == "rupture").count() as i64,
        nb_alertes: lignes.iter().filter(|l| l.etat == "alerte").count() as i64,
        nb_valeur_incertaine: lignes
            .iter()
            .filter(|l| l.prix_achat_estime || l.prix_achat.unwrap_or(0.0) <= 0.0)
            .count() as i64,
        lignes,
    })
}

// ---- Encours clients et fournisseurs ---------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct LigneEncours {
    pub tiers_id: String,
    pub nom: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telephone: Option<String>,
    /// Solde tenu par le module paiement. Positif = le tiers doit de l'argent.
    pub solde: f64,
    /// Total facturé sur la période retenue (toutes pièces validées).
    pub facture: f64,
    pub regle: f64,
    /// Nombre de pièces non soldées.
    pub nb_pieces_ouvertes: i64,
    /// Date de la pièce ouverte la plus ancienne — le vrai signal d'alerte.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub plus_ancienne: Option<String>,
    /// Jours écoulés depuis cette pièce.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub anciennete_jours: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RapportEncours {
    pub lignes: Vec<LigneEncours>,
    pub total: f64,
}

/// Ce que les clients doivent (`sens = "vente"`) ou ce qu'on doit aux
/// fournisseurs (`"achat"`), **du plus gros débiteur au plus petit**.
///
/// Le solde vient de `tiers.solde`, tenu par le module paiement ; le détail des
/// pièces ouvertes vient des documents. Si les deux divergent un jour, c'est un
/// bug — et c'est exactement le genre de contrôle croisé qu'on veut.
pub fn encours(conn: &Connection, sens: &str) -> Result<RapportEncours> {
    let mut st = conn.prepare(
        "SELECT t.id, t.nom, t.telephone, t.solde,
                COALESCE(SUM(d.total_ttc - d.montant_retenue), 0),
                COALESCE(SUM((SELECT COALESCE(SUM(CASE WHEN pa.sens = 'encaissement'
                                                       THEN pa.montant ELSE -pa.montant END), 0)
                                FROM paiement pa WHERE pa.document_id = d.id)), 0),
                SUM(CASE WHEN (d.total_ttc - d.montant_retenue) > (
                        SELECT COALESCE(SUM(CASE WHEN pa.sens = 'encaissement'
                                                 THEN pa.montant ELSE -pa.montant END), 0)
                          FROM paiement pa WHERE pa.document_id = d.id)
                    THEN 1 ELSE 0 END),
                MIN(CASE WHEN (d.total_ttc - d.montant_retenue) > (
                        SELECT COALESCE(SUM(CASE WHEN pa.sens = 'encaissement'
                                                 THEN pa.montant ELSE -pa.montant END), 0)
                          FROM paiement pa WHERE pa.document_id = d.id)
                    THEN d.date END)
           FROM tiers t
           JOIN document d ON d.tiers_id = t.id
          WHERE d.sens = ?1 AND d.statut = 'valide'
            AND d.type_document IN ('facture','avoir')
          GROUP BY t.id, t.nom, t.telephone, t.solde
         HAVING SUM(CASE WHEN (d.total_ttc - d.montant_retenue) > (
                        SELECT COALESCE(SUM(CASE WHEN pa.sens = 'encaissement'
                                                 THEN pa.montant ELSE -pa.montant END), 0)
                          FROM paiement pa WHERE pa.document_id = d.id)
                    THEN 1 ELSE 0 END) > 0
                -- ⚠️ Un solde peut exister SANS aucune pièce ouverte : un
                -- remboursement ou un acompte non rattaché à une facture. Ne
                -- retenir que les pièces ouvertes ferait disparaître ces
                -- tiers du rapport alors qu'ils ont bel et bien un encours
                -- (constaté sur les données réelles le 2026-07-27).
                OR ABS(t.solde) > 0.005
          ORDER BY t.solde DESC",
    )?;
    let aujourdhui = crate::now();
    let aujourdhui = aujourdhui.get(0..10).unwrap_or("").to_string();
    let mut lignes = st
        .query_map(rusqlite::params![sens], |r| {
            let plus_ancienne: Option<String> = r.get(7)?;
            Ok(LigneEncours {
                tiers_id: r.get(0)?,
                nom: r.get(1)?,
                telephone: r.get(2)?,
                solde: arrondi(r.get(3)?),
                facture: arrondi(r.get(4)?),
                regle: arrondi(r.get::<_, f64>(5)?.abs()),
                nb_pieces_ouvertes: r.get(6)?,
                anciennete_jours: plus_ancienne.as_deref().map(|d| jours_entre(d, &aujourdhui)),
                plus_ancienne,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    lignes.sort_by(|a, b| b.solde.partial_cmp(&a.solde).unwrap_or(std::cmp::Ordering::Equal));

    Ok(RapportEncours {
        total: arrondi(lignes.iter().map(|l| l.solde).sum()),
        lignes,
    })
}

/// Écart en jours entre deux dates « AAAA-MM-JJ ». Renvoie 0 si l'une des deux
/// est illisible : mieux vaut ne rien signaler qu'annoncer un retard imaginaire.
fn jours_entre(debut: &str, fin: &str) -> i64 {
    use chrono::NaiveDate;
    match (
        NaiveDate::parse_from_str(&debut[..debut.len().min(10)], "%Y-%m-%d"),
        NaiveDate::parse_from_str(&fin[..fin.len().min(10)], "%Y-%m-%d"),
    ) {
        (Ok(d), Ok(f)) => (f - d).num_days(),
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// Continuité de la numérotation (N1 OHADA)
// ---------------------------------------------------------------------------
//
// # Ce que dit la règle, et ce qu'elle ne dit pas
//
// La numérotation doit être **continue et sans trou** : c'est ce qui empêche de
// faire disparaître une facture après coup. Un contrôle fiscal commence
// souvent par là, parce qu'un numéro manquant est le signe le plus simple
// d'une pièce escamotée.
//
// ⚠️ **Ce rapport SIGNALE, il ne BLOQUE pas.** Un trou a parfois une
// explication parfaitement légitime (une facture annulée puis supprimée à
// l'état brouillon, une reprise de données, un incident technique). Bloquer la
// saisie sur ce motif empêcherait de travailler sans rien prouver ; c'est à
// l'utilisateur de savoir justifier chaque trou — encore faut-il qu'il les
// connaisse, et aujourd'hui rien ne les lui montre.
//
// ⚠️ On regarde les **documents réellement enregistrés**, pas le compteur
// `sequence_numero`. Le compteur dit combien de numéros ont été TIRÉS ; seuls
// les documents disent lesquels ont SURVÉCU. C'est justement l'écart entre les
// deux qu'on cherche.

#[derive(Debug, Clone, Serialize)]
pub struct TrouNumerotation {
    pub type_document: String,
    pub exercice: i64,
    /// Numéro manquant, tel qu'il aurait dû s'écrire (« FA-2026-0042 »).
    pub numero_attendu: String,
    /// Pièce qui précède le trou — repère pour retrouver ce qui s'est passé.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub precedent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_precedent: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suivant: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_suivant: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SerieNumerotation {
    pub type_document: String,
    pub exercice: i64,
    pub prefixe: String,
    pub premier: i64,
    pub dernier: i64,
    pub nb_documents: i64,
    /// Dernier numéro **tiré** par le compteur. S'il dépasse `dernier`, des
    /// numéros ont été consommés sans qu'aucune pièce n'existe : brouillons
    /// supprimés, le plus souvent.
    pub compteur: i64,
    pub nb_trous: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct RapportNumerotation {
    pub series: Vec<SerieNumerotation>,
    pub trous: Vec<TrouNumerotation>,
    pub nb_trous: i64,
    /// Message en langage clair, destiné à quelqu'un qui n'est pas comptable.
    pub constat: String,
}

/// Cherche les ruptures de séquence, par type de document et par exercice.
pub fn continuite_numerotation(conn: &Connection) -> Result<RapportNumerotation> {
    // Le numéro est de la forme `PREFIXE-EXERCICE-NNNN`. On récupère le rang en
    // découpant après le DERNIER tiret : un préfixe peut lui-même en contenir
    // un (« BL-EXP-2026-0007 »), et découper sur le premier casserait tout.
    let mut st = conn.prepare(
        "SELECT type_document, numero, date
           FROM document
          ORDER BY type_document, numero",
    )?;
    let lignes: Vec<(String, String, String)> = st
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // (type, exercice) → [(rang, numéro, date)]
    let mut series: BTreeMap<(String, i64), Vec<(i64, String, String)>> = BTreeMap::new();
    let mut prefixes: BTreeMap<(String, i64), String> = BTreeMap::new();

    for (type_doc, numero, date) in lignes {
        let Some(pos) = numero.rfind('-') else { continue };
        let Ok(rang) = numero[pos + 1..].parse::<i64>() else { continue };
        let tete = &numero[..pos];
        let Some(pos2) = tete.rfind('-') else { continue };
        let Ok(exercice) = tete[pos2 + 1..].parse::<i64>() else { continue };
        let prefixe = tete[..pos2].to_string();

        prefixes.insert((type_doc.clone(), exercice), prefixe);
        series
            .entry((type_doc, exercice))
            .or_default()
            .push((rang, numero, date));
    }

    let mut sorties = Vec::new();
    let mut trous = Vec::new();

    for ((type_doc, exercice), mut pieces) in series {
        pieces.sort_by_key(|(rang, _, _)| *rang);
        let prefixe = prefixes
            .get(&(type_doc.clone(), exercice))
            .cloned()
            .unwrap_or_else(|| type_doc.to_uppercase());

        let premier = pieces.first().map(|p| p.0).unwrap_or(0);
        let dernier = pieces.last().map(|p| p.0).unwrap_or(0);

        let mut nb_trous_serie = 0;
        // ⚠️ On part de 1 et non du premier numéro trouvé : si la série
        // commence à 3, les numéros 1 et 2 manquent bel et bien.
        let mut attendu = 1;
        let mut precedente: Option<&(i64, String, String)> = None;
        for piece in &pieces {
            while attendu < piece.0 {
                nb_trous_serie += 1;
                trous.push(TrouNumerotation {
                    type_document: type_doc.clone(),
                    exercice,
                    numero_attendu: format!("{prefixe}-{exercice}-{attendu:04}"),
                    precedent: precedente.map(|p| p.1.clone()),
                    date_precedent: precedente.map(|p| p.2.clone()),
                    suivant: Some(piece.1.clone()),
                    date_suivant: Some(piece.2.clone()),
                });
                attendu += 1;
            }
            attendu = piece.0 + 1;
            precedente = Some(piece);
        }

        let compteur: i64 = conn
            .query_row(
                "SELECT dernier FROM sequence_numero WHERE type_document = ?1 AND exercice = ?2",
                rusqlite::params![type_doc, exercice],
                |r| r.get(0),
            )
            .unwrap_or(dernier);

        sorties.push(SerieNumerotation {
            type_document: type_doc,
            exercice,
            prefixe,
            premier,
            dernier,
            nb_documents: pieces.len() as i64,
            compteur,
            nb_trous: nb_trous_serie,
        });
    }

    let nb_trous = trous.len() as i64;
    let constat = if nb_trous == 0 {
        "Aucun trou : toutes vos pièces se suivent sans interruption.".to_string()
    } else {
        format!(
            "{nb_trous} numéro(s) manquant(s) dans vos séries. Un trou n'est pas une faute en soi \
             — une pièce en brouillon supprimée en laisse un —, mais vous devez pouvoir \
             l'expliquer en cas de contrôle."
        )
    };

    Ok(RapportNumerotation { series: sorties, trous, nb_trous, constat })
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
            stock_alerte: None, categorie_id: None, image: None, code_barre: None, taxes: None, nature_comptable: None,
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

    // ---- Continuité de la numérotation (N1 OHADA) --------------------------

    /// Crée `n` factures et renvoie leurs identifiants, dans l'ordre.
    fn factures(conn: &rusqlite::Connection, n: usize) -> Vec<String> {
        conn.execute("INSERT OR IGNORE INTO tiers (id, code, type_role, nom, solde, actif, cree_le)
                      VALUES ('tn','tn','client','Client N',0,1,'2026-01-01')", []).unwrap();
        let a = article::creer(conn, &NouvelArticle {
            code: format!("AN{n}"), r#type: TypeArticle::Bien, designation: "X".into(),
            prix_vente: 1000.0, prix_achat: Some(600.0), taux_tva: 0.0, gere_stock: false,
            stock_alerte: None, categorie_id: None, image: None, code_barre: None,
            taxes: None, nature_comptable: None,
        }).unwrap().id;
        (0..n).map(|_| document::creer(conn, &NouveauDocument {
            type_document: TypeDocument::Facture, sens: SensDocument::Vente,
            tiers_id: "tn".into(), depot_id: None, date: Some("2026-07-10".into()),
            note: None, document_source_id: None, reference_dossier: None, objet: None,
            lignes: vec![NouvelleLigne { article_id: a.clone(), designation: "X".into(),
                quantite: 1.0, prix_unitaire: 1000.0, taux_tva: 0.0, remise: 0.0, taxes: vec![] }],
        }).unwrap().id).collect()
    }

    #[test]
    fn une_serie_continue_ne_signale_rien() {
        let conn = db::open_in_memory().unwrap();
        factures(&conn, 4);
        let r = continuite_numerotation(&conn).unwrap();
        assert_eq!(r.nb_trous, 0);
        assert!(r.trous.is_empty());
        assert!(r.constat.contains("Aucun trou"));
        let s = &r.series[0];
        assert_eq!((s.premier, s.dernier, s.nb_documents), (1, 4, 4));
    }

    /// LE cas qui motive ce rapport : une pièce supprimée laisse un trou, et
    /// rien ne le montrait jusqu'ici.
    #[test]
    fn une_piece_supprimee_laisse_un_trou_visible() {
        let conn = db::open_in_memory().unwrap();
        let ids = factures(&conn, 4);
        document::supprimer(&conn, &ids[1]).unwrap(); // la n° 2 disparaît

        let r = continuite_numerotation(&conn).unwrap();
        assert_eq!(r.nb_trous, 1);
        let t = &r.trous[0];
        assert_eq!(t.numero_attendu, "FA-2026-0002");
        // On donne les pièces qui encadrent le trou : sans elles, impossible de
        // retrouver ce qui s'est passé six mois plus tard.
        assert_eq!(t.precedent.as_deref(), Some("FA-2026-0001"));
        assert_eq!(t.suivant.as_deref(), Some("FA-2026-0003"));
        // ⚠️ Le rapport SIGNALE, il ne condamne pas : le constat doit dire
        // qu'un trou n'est pas une faute en soi.
        assert!(r.constat.contains("pas une faute"));
    }

    /// Le compteur garde la trace des numéros TIRÉS, les documents celle des
    /// numéros SURVIVANTS. L'écart entre les deux est l'information utile.
    #[test]
    fn le_compteur_revele_les_numeros_consommes_sans_piece() {
        let conn = db::open_in_memory().unwrap();
        let ids = factures(&conn, 3);
        document::supprimer(&conn, &ids[2]).unwrap(); // la DERNIÈRE disparaît

        let r = continuite_numerotation(&conn).unwrap();
        let s = &r.series[0];
        assert_eq!(s.dernier, 2, "la dernière pièce existante porte le n° 2");
        assert_eq!(s.compteur, 3, "mais le compteur a bien tiré 3 numéros");
        // Un trou en FIN de série ne se voit pas dans l'intervalle : c'est
        // justement pour ça que `compteur` est exposé.
        assert_eq!(r.nb_trous, 0);
    }

    /// Chaque type de document et chaque exercice a SA propre série : mélanger
    /// factures et devis inventerait des trous qui n'existent pas.
    #[test]
    fn les_series_ne_se_melangent_pas() {
        let conn = db::open_in_memory().unwrap();
        factures(&conn, 2);
        let a = article::creer(&conn, &NouvelArticle {
            code: "AD".into(), r#type: TypeArticle::Bien, designation: "Y".into(),
            prix_vente: 500.0, prix_achat: None, taux_tva: 0.0, gere_stock: false,
            stock_alerte: None, categorie_id: None, image: None, code_barre: None,
            taxes: None, nature_comptable: None,
        }).unwrap().id;
        document::creer(&conn, &NouveauDocument {
            type_document: TypeDocument::Devis, sens: SensDocument::Vente,
            tiers_id: "tn".into(), depot_id: None, date: Some("2026-07-11".into()),
            note: None, document_source_id: None, reference_dossier: None, objet: None,
            lignes: vec![NouvelleLigne { article_id: a, designation: "Y".into(),
                quantite: 1.0, prix_unitaire: 500.0, taux_tva: 0.0, remise: 0.0, taxes: vec![] }],
        }).unwrap();

        let r = continuite_numerotation(&conn).unwrap();
        assert_eq!(r.series.len(), 2, "une série par type de document");
        assert_eq!(r.nb_trous, 0, "un devis n° 1 ne comble ni ne creuse la série des factures");
    }
}
