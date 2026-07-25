//! Document unifié (spec §3.2 / §5.4).
//!
//! Toutes les pièces commerciales (devis, facture, avoir, commande, livraison,
//! proforma) vivent ici, distinguées par `type_document` + `sens`. Les totaux
//! sont **dérivés** des lignes. Le comportement des types (impact stock,
//! numérotation) est **piloté par la donnée** (tables `config_*`), jamais codé
//! en dur (§9).

use crate::domain::{
    MotifMouvement, SensDocument, SensMouvement, StatutDocument, TypeDocument, TypeTaxe,
};
use crate::error::{CoreError, Result};
use crate::modules::{depot, stock};
use crate::now;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct Document {
    pub id: String,
    pub numero: String,
    pub type_document: TypeDocument,
    pub sens: SensDocument,
    pub tiers_id: String,
    pub depot_id: Option<String>,
    pub date: String,
    pub statut: StatutDocument,
    pub document_source_id: Option<String>,
    pub total_ht: f64,
    pub total_tva: f64,
    pub total_ttc: f64,
    pub note: Option<String>,
    /// Référence de dossier / contrat (§ facturation contrat), affichée sur la facture.
    #[serde(default)]
    pub reference_dossier: Option<String>,
    /// Objet de la facture (ex. « Maintenance — Trimestre 1 »).
    #[serde(default)]
    pub objet: Option<String>,
    pub cree_le: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lignes: Vec<Ligne>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Ligne {
    pub id: String,
    pub article_id: String,
    pub designation: String,
    pub quantite: f64,
    pub prix_unitaire: f64,
    pub taux_tva: f64,
    pub remise: f64,
    pub total_ligne_ht: f64,
    /// Taxes appliquées à la ligne (snapshot figé). Vide = repli sur `taux_tva`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub taxes: Vec<LigneTaxe>,
}

/// Snapshot d'une taxe sur une ligne (nom/taux/type figés + montant calculé).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LigneTaxe {
    pub nom: String,
    #[serde(default = "type_pourcentage")]
    pub r#type: TypeTaxe,
    pub taux: f64,
    #[serde(default)]
    pub montant: f64,
}
fn type_pourcentage() -> TypeTaxe { TypeTaxe::Pourcentage }

#[derive(Debug, Clone, Deserialize)]
pub struct NouveauDocument {
    pub type_document: TypeDocument,
    pub sens: SensDocument,
    pub tiers_id: String,
    pub depot_id: Option<String>,
    #[serde(default)]
    pub date: Option<String>,
    pub note: Option<String>,
    #[serde(default)]
    pub reference_dossier: Option<String>,
    #[serde(default)]
    pub objet: Option<String>,
    #[serde(default)]
    pub document_source_id: Option<String>,
    pub lignes: Vec<NouvelleLigne>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouvelleLigne {
    pub article_id: String,
    pub designation: String,
    pub quantite: f64,
    pub prix_unitaire: f64,
    #[serde(default)]
    pub taux_tva: f64,
    #[serde(default)]
    pub remise: f64,
    /// Taxes explicites de la ligne (multi-taxes). Si vide/absent, repli sur `taux_tva`.
    #[serde(default)]
    pub taxes: Vec<LigneTaxe>,
}

/// Filtre de liste.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct FiltreDocuments {
    pub sens: Option<SensDocument>,
    pub type_document: Option<TypeDocument>,
    pub statut: Option<StatutDocument>,
}

// ---------------------------------------------------------------------------
// Calcul des totaux (dérivés — jamais saisis)
// ---------------------------------------------------------------------------

/// Total HT d'une ligne = quantité × PU × (1 − remise%). Arrondi à 2 décimales.
fn total_ligne_ht(l: &NouvelleLigne) -> f64 {
    arrondi(l.quantite * l.prix_unitaire * (1.0 - l.remise / 100.0))
}

fn arrondi(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Montant d'une taxe sur une ligne : pourcentage sur le HT, ou montant fixe par
/// unité (× quantité). Arrondi à 2 décimales.
fn montant_taxe(ht: f64, quantite: f64, taux: f64, typ: TypeTaxe) -> f64 {
    match typ {
        TypeTaxe::Pourcentage => arrondi(ht * taux / 100.0),
        TypeTaxe::Fixe => arrondi(taux * quantite),
    }
}

/// Taxes effectives d'une ligne, avec montant calculé. Si la ligne fournit des
/// taxes explicites (multi-taxes), on les utilise ; sinon repli sur `taux_tva`
/// (compat saisie simple / documents existants).
fn taxes_ligne(l: &NouvelleLigne, ht: f64) -> Vec<LigneTaxe> {
    if !l.taxes.is_empty() {
        l.taxes.iter().map(|t| LigneTaxe {
            nom: t.nom.clone(), r#type: t.r#type, taux: t.taux,
            montant: montant_taxe(ht, l.quantite, t.taux, t.r#type),
        }).collect()
    } else if l.taux_tva != 0.0 {
        vec![LigneTaxe {
            nom: format!("TVA {} %", l.taux_tva), r#type: TypeTaxe::Pourcentage,
            taux: l.taux_tva, montant: montant_taxe(ht, l.quantite, l.taux_tva, TypeTaxe::Pourcentage),
        }]
    } else {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// Numérotation (§5.4) — pilotée par la donnée (préfixe + compteur par exercice)
// ---------------------------------------------------------------------------

fn numero_suivant(conn: &Connection, type_doc: TypeDocument, date: &str) -> Result<String> {
    let exercice: i64 = date.get(0..4).and_then(|a| a.parse().ok()).unwrap_or(1970);
    conn.execute(
        "INSERT INTO sequence_numero (type_document, exercice, dernier)
         VALUES (?1, ?2, 1)
         ON CONFLICT(type_document, exercice) DO UPDATE SET dernier = dernier + 1",
        params![type_doc.as_str(), exercice],
    )?;
    let n: i64 = conn.query_row(
        "SELECT dernier FROM sequence_numero WHERE type_document = ?1 AND exercice = ?2",
        params![type_doc.as_str(), exercice],
        |r| r.get(0),
    )?;
    let prefixe: String = conn
        .query_row(
            "SELECT prefixe FROM config_prefixe_document WHERE type_document = ?1",
            params![type_doc.as_str()],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| type_doc.as_str().to_uppercase());
    Ok(format!("{prefixe}-{exercice}-{n:04}"))
}

// ---------------------------------------------------------------------------
// Création (statut brouillon)
// ---------------------------------------------------------------------------

pub fn creer(conn: &Connection, d: &NouveauDocument) -> Result<Document> {
    if d.lignes.is_empty() {
        return Err(CoreError::Rule("un document doit avoir au moins une ligne".into()));
    }
    let id = Uuid::new_v4().to_string();
    let date = d.date.clone().unwrap_or_else(|| now()[..10].to_string());
    let numero = numero_suivant(conn, d.type_document, &date)?;

    // Client exonéré de TVA (§ contrat) : aucune taxe sur ses factures.
    let exonere: bool = conn
        .query_row("SELECT exonere_tva FROM tiers WHERE id = ?1", params![d.tiers_id],
                   |r| Ok(r.get::<_, i64>(0)? != 0))
        .unwrap_or(false);

    // Totaux dérivés des lignes (somme de TOUTES les taxes, multi-taxes).
    let mut total_ht = 0.0;
    let mut total_tva = 0.0;
    let mut lignes_calc: Vec<(f64, Vec<LigneTaxe>)> = Vec::with_capacity(d.lignes.len());
    for l in &d.lignes {
        let ht = total_ligne_ht(l);
        let taxes = if exonere { Vec::new() } else { taxes_ligne(l, ht) };
        total_ht += ht;
        total_tva += taxes.iter().map(|t| t.montant).sum::<f64>();
        lignes_calc.push((ht, taxes));
    }
    total_ht = arrondi(total_ht);
    total_tva = arrondi(total_tva);
    let total_ttc = arrondi(total_ht + total_tva);

    conn.execute(
        "INSERT INTO document
            (id, numero, type_document, sens, tiers_id, depot_id, date, statut,
             document_source_id, total_ht, total_tva, total_ttc, note,
             reference_dossier, objet, cree_le)
         VALUES (?1,?2,?3,?4,?5,?6,?7,'brouillon',?8,?9,?10,?11,?12,?13,?14,?15)",
        params![
            id, numero, d.type_document, d.sens, d.tiers_id, d.depot_id, date,
            d.document_source_id, total_ht, total_tva, total_ttc, d.note,
            d.reference_dossier, d.objet, now(),
        ],
    )?;

    for (l, (ht, taxes)) in d.lignes.iter().zip(lignes_calc.iter()) {
        let ligne_id = Uuid::new_v4().to_string();
        // Client exonéré : on force aussi le taux stocké sur la ligne à 0.
        let taux_ligne = if exonere { 0.0 } else { l.taux_tva };
        conn.execute(
            "INSERT INTO document_ligne
                (id, document_id, article_id, designation, quantite, prix_unitaire,
                 taux_tva, remise, total_ligne_ht)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                ligne_id, id, l.article_id, l.designation,
                l.quantite, l.prix_unitaire, taux_ligne, l.remise, ht,
            ],
        )?;
        // snapshot des taxes de la ligne (figé)
        for t in taxes {
            conn.execute(
                "INSERT INTO document_ligne_taxe (id, ligne_id, nom, type, taux, montant)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![Uuid::new_v4().to_string(), ligne_id, t.nom, t.r#type, t.taux, t.montant],
            )?;
        }
    }
    lire(conn, &id)
}

// ---------------------------------------------------------------------------
// Lecture
// ---------------------------------------------------------------------------

pub fn lire(conn: &Connection, id: &str) -> Result<Document> {
    let mut doc = conn
        .query_row(&format!("{SELECT_ENTETE} WHERE id = ?1"), params![id], ligne_vers_doc)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => CoreError::NotFound(format!("document {id}")),
            autre => autre.into(),
        })?;
    doc.lignes = lignes_du_document(conn, id)?;
    Ok(doc)
}

pub fn lister(conn: &Connection, f: &FiltreDocuments) -> Result<Vec<Document>> {
    let mut clauses = Vec::new();
    if let Some(s) = &f.sens { clauses.push(format!("sens = '{}'", s.as_str())); }
    if let Some(t) = &f.type_document { clauses.push(format!("type_document = '{}'", t.as_str())); }
    if let Some(st) = &f.statut { clauses.push(format!("statut = '{}'", st.as_str())); }
    let where_ = if clauses.is_empty() { String::new() } else { format!(" WHERE {}", clauses.join(" AND ")) };
    let sql = format!("{SELECT_ENTETE}{where_} ORDER BY date DESC, cree_le DESC");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], ligne_vers_doc)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

const SELECT_ENTETE: &str = "
    SELECT id, numero, type_document, sens, tiers_id, depot_id, date, statut,
           document_source_id, total_ht, total_tva, total_ttc, note, cree_le,
           reference_dossier, objet
    FROM document";

fn ligne_vers_doc(r: &rusqlite::Row) -> rusqlite::Result<Document> {
    let td: String = r.get(2)?;
    let se: String = r.get(3)?;
    let st: String = r.get(7)?;
    Ok(Document {
        id: r.get(0)?,
        numero: r.get(1)?,
        type_document: TypeDocument::parse(&td).unwrap_or(TypeDocument::Devis),
        sens: SensDocument::parse(&se).unwrap_or(SensDocument::Vente),
        tiers_id: r.get(4)?,
        depot_id: r.get(5)?,
        date: r.get(6)?,
        statut: StatutDocument::parse(&st).unwrap_or(StatutDocument::Brouillon),
        document_source_id: r.get(8)?,
        total_ht: r.get(9)?,
        total_tva: r.get(10)?,
        total_ttc: r.get(11)?,
        note: r.get(12)?,
        cree_le: r.get(13)?,
        reference_dossier: r.get(14)?,
        objet: r.get(15)?,
        lignes: Vec::new(),
    })
}

fn lignes_du_document(conn: &Connection, doc_id: &str) -> Result<Vec<Ligne>> {
    let mut stmt = conn.prepare(
        "SELECT id, article_id, designation, quantite, prix_unitaire, taux_tva, remise, total_ligne_ht
         FROM document_ligne WHERE document_id = ?1",
    )?;
    let rows = stmt.query_map(params![doc_id], |r| {
        Ok(Ligne {
            id: r.get(0)?,
            article_id: r.get(1)?,
            designation: r.get(2)?,
            quantite: r.get(3)?,
            prix_unitaire: r.get(4)?,
            taux_tva: r.get(5)?,
            remise: r.get(6)?,
            total_ligne_ht: r.get(7)?,
            taxes: Vec::new(),
        })
    })?;
    let mut lignes = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    for l in &mut lignes {
        l.taxes = taxes_de_ligne(conn, &l.id)?;
    }
    Ok(lignes)
}

/// Taxes (snapshot) d'une ligne de document.
fn taxes_de_ligne(conn: &Connection, ligne_id: &str) -> Result<Vec<LigneTaxe>> {
    let mut stmt = conn.prepare(
        "SELECT nom, type, taux, montant FROM document_ligne_taxe WHERE ligne_id = ?1",
    )?;
    let rows = stmt.query_map(params![ligne_id], |r| {
        let ty: String = r.get(1)?;
        Ok(LigneTaxe {
            nom: r.get(0)?,
            r#type: TypeTaxe::parse(&ty).unwrap_or(TypeTaxe::Pourcentage),
            taux: r.get(2)?,
            montant: r.get(3)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

// ---------------------------------------------------------------------------
// Validation → impact stock (§6.1)
// ---------------------------------------------------------------------------

/// Passe le document à `valide` et, pour chaque ligne, crée un mouvement de
/// stock **si et seulement si** les trois conditions du §6.1 sont réunies :
///   1. `article.gere_stock = true`, ET
///   2. le type de document est configuré comme impactant le stock, ET
///   3. le paramètre global « gestion de stock active » est vrai.
/// Sens : vente → sortie, achat → entrée ; un avoir inverse le sens.
pub fn valider(conn: &Connection, id: &str) -> Result<Document> {
    let doc = lire(conn, id)?;
    if doc.statut != StatutDocument::Brouillon {
        return Err(CoreError::Rule(format!(
            "seul un brouillon peut être validé (statut actuel : {})",
            doc.statut.as_str()
        )));
    }

    // Condition 2 : le type impacte-t-il le stock ? (piloté par la donnée)
    let (impacte, inverse): (bool, bool) = conn
        .query_row(
            "SELECT impacte_stock, mouvement_inverse FROM config_type_document WHERE type_document = ?1",
            params![doc.type_document.as_str()],
            |r| Ok((r.get::<_, i64>(0)? != 0, r.get::<_, i64>(1)? != 0)),
        )
        .unwrap_or((false, false));

    // Condition 3 : gestion de stock globalement active ?
    let stock_actif = parametre_global_bool(conn, "gestion_stock_active", true)?;

    if impacte && stock_actif {
        let depot_id = match &doc.depot_id {
            Some(d) => d.clone(),
            None => depot::defaut(conn)?,
        };
        // sens de base : vente → sortie, achat → entrée ; avoir inverse
        let sens_base = match doc.sens {
            SensDocument::Vente => SensMouvement::Sortie,
            SensDocument::Achat => SensMouvement::Entree,
        };
        let sens = if inverse { inverser(sens_base) } else { sens_base };
        let motif = match doc.sens {
            SensDocument::Vente => MotifMouvement::Vente,
            SensDocument::Achat => MotifMouvement::Achat,
        };

        for l in &doc.lignes {
            // Condition 1 : l'article gère-t-il le stock ?
            let gere: bool = conn
                .query_row(
                    "SELECT gere_stock FROM article WHERE id = ?1",
                    params![l.article_id],
                    |r| Ok(r.get::<_, i64>(0)? != 0),
                )
                .unwrap_or(false);
            if gere {
                stock::ecrire(conn, &l.article_id, &depot_id, Some(id), sens, l.quantite, motif)?;
            }
        }
    }

    // §6.4 : une facture crée un encours (+ttc), un avoir le réduit (−ttc).
    // Les autres types (devis, commande, livraison, proforma) n'y touchent pas.
    match doc.type_document {
        TypeDocument::Facture => crate::modules::paiement::maj_solde_tiers(conn, &doc.tiers_id, doc.total_ttc)?,
        TypeDocument::Avoir => crate::modules::paiement::maj_solde_tiers(conn, &doc.tiers_id, -doc.total_ttc)?,
        _ => {}
    }

    conn.execute(
        "UPDATE document SET statut = 'valide' WHERE id = ?1",
        params![id],
    )?;
    lire(conn, id)
}

// ---------------------------------------------------------------------------
// Annulation d'une vente déjà validée / encaissée (contre-passation, §6)
// ---------------------------------------------------------------------------

/// Annule une **vente validée** (facture) : bonne pratique de caisse — on
/// n'efface rien, on **contre-passe**. Réservé à l'Admin (garde côté API).
///
/// Effets, tous dans la même connexion :
///   1. réintègre le stock (mouvements inverses des sorties de vente) ;
///   2. inverse l'encours du tiers (annule le `+ttc` posé par la validation) ;
///   3. **contre-passe chaque paiement** rattaché : un décaissement lié qui
///      restitue l'argent (caisse `−montant`) et rétablit l'encours du tiers ;
///   4. passe le document en `annule` en traçant motif / auteur / date.
///
/// Refuse tout statut ≠ `valide` (un brouillon se supprime, un document
/// transformé n'est pas annulable directement).
pub fn annuler(conn: &Connection, id: &str, motif: &str, par: Option<&str>) -> Result<Document> {
    let motif = motif.trim();
    if motif.is_empty() {
        return Err(CoreError::Rule("le motif d'annulation est obligatoire".into()));
    }
    let doc = lire(conn, id)?;
    if doc.statut != StatutDocument::Valide {
        return Err(CoreError::Rule(format!(
            "seule une vente validée peut être annulée (statut actuel : {})",
            doc.statut.as_str()
        )));
    }

    // 1. Réintégration du stock : mêmes conditions que la validation, sens inversé.
    let (impacte, inverse): (bool, bool) = conn
        .query_row(
            "SELECT impacte_stock, mouvement_inverse FROM config_type_document WHERE type_document = ?1",
            params![doc.type_document.as_str()],
            |r| Ok((r.get::<_, i64>(0)? != 0, r.get::<_, i64>(1)? != 0)),
        )
        .unwrap_or((false, false));
    let stock_actif = parametre_global_bool(conn, "gestion_stock_active", true)?;
    if impacte && stock_actif {
        let depot_id = match &doc.depot_id {
            Some(d) => d.clone(),
            None => depot::defaut(conn)?,
        };
        let sens_base = match doc.sens {
            SensDocument::Vente => SensMouvement::Sortie,
            SensDocument::Achat => SensMouvement::Entree,
        };
        // sens de la validation, puis on l'INVERSE pour réintégrer
        let sens_valid = if inverse { inverser(sens_base) } else { sens_base };
        let sens_annul = inverser(sens_valid);
        let motif_mvt = match doc.sens {
            SensDocument::Vente => MotifMouvement::Vente,
            SensDocument::Achat => MotifMouvement::Achat,
        };
        for l in &doc.lignes {
            let gere: bool = conn
                .query_row("SELECT gere_stock FROM article WHERE id = ?1", params![l.article_id],
                    |r| Ok(r.get::<_, i64>(0)? != 0))
                .unwrap_or(false);
            if gere {
                stock::ecrire(conn, &l.article_id, &depot_id, Some(id), sens_annul, l.quantite, motif_mvt)?;
            }
        }
    }

    // 2. Inverse l'encours du tiers posé par la validation.
    match doc.type_document {
        TypeDocument::Facture => crate::modules::paiement::maj_solde_tiers(conn, &doc.tiers_id, -doc.total_ttc)?,
        TypeDocument::Avoir => crate::modules::paiement::maj_solde_tiers(conn, &doc.tiers_id, doc.total_ttc)?,
        _ => {}
    }

    // 3. Contre-passation des paiements encaissés (hors contre-passations déjà faites).
    let mut a_passer: Vec<(String, String, String, f64, String, Option<String>)> = Vec::new();
    {
        let mut stmt = conn.prepare(
            "SELECT id, tiers_id, caisse_id, montant, mode, moyen_paiement_id
             FROM paiement
             WHERE document_id = ?1 AND sens = 'encaissement' AND annulation_de IS NULL
               AND id NOT IN (SELECT annulation_de FROM paiement WHERE annulation_de IS NOT NULL)",
        )?;
        let rows = stmt.query_map(params![id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
                r.get::<_, f64>(3)?, r.get::<_, String>(4)?, r.get::<_, Option<String>>(5)?))
        })?;
        for row in rows { a_passer.push(row?); }
    }
    for (pid, tiers_id, caisse_id, montant, mode, moyen) in a_passer {
        let rev = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO paiement (id, tiers_id, caisse_id, document_id, sens, montant, mode, date, moyen_paiement_id, annulation_de)
             VALUES (?1,?2,?3,?4,'decaissement',?5,?6,?7,?8,?9)",
            params![rev, tiers_id, caisse_id, id, montant, mode, now(), moyen, pid],
        )?;
        // caisse : l'argent sort ; tiers : on rétablit l'encours réduit par le paiement d'origine.
        crate::modules::paiement::maj_solde_caisse(conn, &caisse_id, -montant)?;
        crate::modules::paiement::maj_solde_tiers(conn, &tiers_id, montant)?;
    }

    // 4. Marque le document annulé (motif / auteur / date tracés).
    conn.execute(
        "UPDATE document SET statut = 'annule', motif_annulation = ?2, annule_par = ?3, annule_le = ?4 WHERE id = ?1",
        params![id, motif, par, now()],
    )?;
    lire(conn, id)
}

// ---------------------------------------------------------------------------
// Suppression (ménage) — brouillon uniquement
// ---------------------------------------------------------------------------

/// Supprime définitivement un document **à l'état brouillon** (et ses lignes +
/// snapshots de taxes). Un document validé/transformé/annulé n'est jamais
/// supprimé : il a un impact (stock, solde, numérotation) et doit être conservé.
pub fn supprimer(conn: &Connection, id: &str) -> Result<()> {
    let doc = lire(conn, id)?;
    if doc.statut != StatutDocument::Brouillon {
        return Err(CoreError::Rule(format!(
            "seul un brouillon peut être supprimé (statut actuel : {})",
            doc.statut.as_str()
        )));
    }
    // Cascade explicite : taxes de lignes → lignes → entête.
    conn.execute(
        "DELETE FROM document_ligne_taxe WHERE ligne_id IN
            (SELECT id FROM document_ligne WHERE document_id = ?1)",
        params![id],
    )?;
    conn.execute("DELETE FROM document_ligne WHERE document_id = ?1", params![id])?;
    conn.execute("DELETE FROM document WHERE id = ?1", params![id])?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Transformation de pièce (§6.2) — pilotée par `config_transformation`
// ---------------------------------------------------------------------------

/// Transforme une pièce source en une pièce cible (ex. devis → facture).
/// La source n'est **jamais modifiée sur le fond** : on crée une nouvelle pièce,
/// on copie ses lignes, on relie via `document_source_id`, et la source passe au
/// statut `transforme`. Les couples autorisés vivent dans `config_transformation`.
pub fn transformer(conn: &Connection, source_id: &str, type_cible: TypeDocument) -> Result<Document> {
    let source = lire(conn, source_id)?;

    // Couple (source, cible) autorisé ?
    let existe: bool = conn
        .query_row(
            "SELECT 1 FROM config_transformation WHERE type_source = ?1 AND type_cible = ?2",
            params![source.type_document.as_str(), type_cible.as_str()],
            |_| Ok(true),
        )
        .unwrap_or(false);
    if !existe {
        return Err(CoreError::Rule(format!(
            "transformation {} → {} non autorisée",
            source.type_document.as_str(),
            type_cible.as_str()
        )));
    }

    // La source doit être dans un statut « confirmé » et pas déjà transformée/annulée.
    match source.statut {
        StatutDocument::Valide | StatutDocument::Accepte => {}
        StatutDocument::Transforme => {
            return Err(CoreError::Rule("cette pièce a déjà été transformée".into()))
        }
        autre => {
            return Err(CoreError::Rule(format!(
                "la pièce doit être validée avant transformation (statut : {})",
                autre.as_str()
            )))
        }
    }

    // Nouvelle pièce cible : mêmes tiers/sens/dépôt, lignes copiées.
    let lignes: Vec<NouvelleLigne> = source
        .lignes
        .iter()
        .map(|l| NouvelleLigne {
            article_id: l.article_id.clone(),
            designation: l.designation.clone(),
            quantite: l.quantite,
            prix_unitaire: l.prix_unitaire,
            taux_tva: l.taux_tva,
            remise: l.remise,
            taxes: l.taxes.iter().map(|t| LigneTaxe {
                nom: t.nom.clone(), r#type: t.r#type, taux: t.taux, montant: 0.0,
            }).collect(),
        })
        .collect();

    let cible = creer(conn, &NouveauDocument {
        type_document: type_cible,
        sens: source.sens,
        tiers_id: source.tiers_id.clone(),
        depot_id: source.depot_id.clone(),
        date: None,
        note: source.note.clone(),
        reference_dossier: source.reference_dossier.clone(),
        objet: source.objet.clone(),
        document_source_id: Some(source.id.clone()),
        lignes,
    })?;

    conn.execute(
        "UPDATE document SET statut = 'transforme' WHERE id = ?1",
        params![source_id],
    )?;

    Ok(cible)
}

fn inverser(s: SensMouvement) -> SensMouvement {
    match s {
        SensMouvement::Entree => SensMouvement::Sortie,
        SensMouvement::Sortie => SensMouvement::Entree,
    }
}

fn parametre_global_bool(conn: &Connection, cle: &str, defaut: bool) -> Result<bool> {
    let v: Option<String> = conn
        .query_row("SELECT valeur FROM parametre_global WHERE cle = ?1", params![cle], |r| r.get(0))
        .ok();
    Ok(v.map(|s| s == "1" || s.eq_ignore_ascii_case("true")).unwrap_or(defaut))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::domain::TypeArticle;
    use crate::modules::article::{self, NouvelArticle};
    use crate::modules::stock;

    fn tiers(conn: &Connection) -> String {
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO tiers (id, code, type_role, nom, solde, actif, cree_le)
             VALUES (?1,'T1','client','Client',0,1,?2)",
            params![id, now()],
        ).unwrap();
        id
    }

    fn article_bien(conn: &Connection, gere: bool) -> String {
        article::creer(conn, &NouvelArticle {
            code: format!("A-{}", Uuid::new_v4()),
            r#type: TypeArticle::Bien,
            designation: "Riz".into(),
            prix_vente: 650.0, prix_achat: Some(500.0), taux_tva: 18.0,
            gere_stock: gere, stock_alerte: None, categorie_id: None, image: None, code_barre: None, taxes: None,
        }).unwrap().id
    }

    #[test]
    fn totaux_derives_avec_remise() {
        let conn = db::open_in_memory().unwrap();
        let t = tiers(&conn);
        let a = article_bien(&conn, true);
        let doc = creer(&conn, &NouveauDocument {
            type_document: TypeDocument::Facture, sens: SensDocument::Vente,
            tiers_id: t, depot_id: None, date: Some("2026-07-21".into()),
            note: None, document_source_id: None, reference_dossier: None, objet: None,
            lignes: vec![NouvelleLigne {
                article_id: a, designation: "Riz".into(),
                quantite: 10.0, prix_unitaire: 650.0, taux_tva: 18.0, remise: 10.0, taxes: vec![],
            }],
        }).unwrap();
        // 10 × 650 × 0,9 = 5850 HT ; TVA 18% = 1053 ; TTC = 6903
        assert_eq!(doc.total_ht, 5850.0);
        assert_eq!(doc.total_tva, 1053.0);
        assert_eq!(doc.total_ttc, 6903.0);
        assert!(doc.numero.starts_with("FA-2026-"));
    }

    #[test]
    fn validation_facture_vente_cree_une_sortie() {
        let conn = db::open_in_memory().unwrap();
        let t = tiers(&conn);
        let a = article_bien(&conn, true);
        let depot = depot::defaut(&conn).unwrap();
        let doc = creer(&conn, &NouveauDocument {
            type_document: TypeDocument::Facture, sens: SensDocument::Vente,
            tiers_id: t, depot_id: Some(depot.clone()), date: None,
            note: None, document_source_id: None, reference_dossier: None, objet: None,
            lignes: vec![NouvelleLigne {
                article_id: a.clone(), designation: "Riz".into(),
                quantite: 5.0, prix_unitaire: 650.0, taux_tva: 18.0, remise: 0.0, taxes: vec![],
            }],
        }).unwrap();
        let v = valider(&conn, &doc.id).unwrap();
        assert_eq!(v.statut, StatutDocument::Valide);
        // vente ⇒ sortie de 5 ⇒ stock = -5
        assert_eq!(stock::stock_article_depot(&conn, &a, &depot).unwrap(), -5.0);
    }

    #[test]
    fn devis_nimpacte_pas_le_stock() {
        let conn = db::open_in_memory().unwrap();
        let t = tiers(&conn);
        let a = article_bien(&conn, true);
        let depot = depot::defaut(&conn).unwrap();
        let doc = creer(&conn, &NouveauDocument {
            type_document: TypeDocument::Devis, sens: SensDocument::Vente,
            tiers_id: t, depot_id: Some(depot.clone()), date: None,
            note: None, document_source_id: None, reference_dossier: None, objet: None,
            lignes: vec![NouvelleLigne {
                article_id: a.clone(), designation: "Riz".into(),
                quantite: 5.0, prix_unitaire: 650.0, taux_tva: 18.0, remise: 0.0, taxes: vec![],
            }],
        }).unwrap();
        valider(&conn, &doc.id).unwrap();
        // devis ⇒ aucun mouvement
        assert_eq!(stock::stock_article_depot(&conn, &a, &depot).unwrap(), 0.0);
    }

    #[test]
    fn transformation_devis_en_facture() {
        let conn = db::open_in_memory().unwrap();
        let t = tiers(&conn);
        let a = article_bien(&conn, true);
        let devis = creer(&conn, &NouveauDocument {
            type_document: TypeDocument::Devis, sens: SensDocument::Vente,
            tiers_id: t, depot_id: None, date: None, note: None, document_source_id: None, reference_dossier: None, objet: None,
            lignes: vec![NouvelleLigne {
                article_id: a, designation: "Riz".into(),
                quantite: 4.0, prix_unitaire: 650.0, taux_tva: 18.0, remise: 0.0, taxes: vec![],
            }],
        }).unwrap();
        // transformer sans valider ⇒ refus
        assert!(transformer(&conn, &devis.id, TypeDocument::Facture).is_err());
        valider(&conn, &devis.id).unwrap();
        let facture = transformer(&conn, &devis.id, TypeDocument::Facture).unwrap();
        assert_eq!(facture.type_document, TypeDocument::Facture);
        assert_eq!(facture.document_source_id.as_deref(), Some(devis.id.as_str()));
        assert_eq!(facture.total_ttc, devis.total_ttc);
        // la source est figée en 'transforme'
        assert_eq!(lire(&conn, &devis.id).unwrap().statut, StatutDocument::Transforme);
        // double transformation ⇒ refus
        assert!(transformer(&conn, &devis.id, TypeDocument::Facture).is_err());
    }

    #[test]
    fn suppression_brouillon_ok_mais_pas_valide() {
        let conn = db::open_in_memory().unwrap();
        let t = tiers(&conn);
        let a = article_bien(&conn, true);
        let doc = creer(&conn, &NouveauDocument {
            type_document: TypeDocument::Facture, sens: SensDocument::Vente,
            tiers_id: t, depot_id: None, date: None,
            note: None, document_source_id: None, reference_dossier: None, objet: None,
            lignes: vec![NouvelleLigne {
                article_id: a, designation: "Riz".into(),
                quantite: 2.0, prix_unitaire: 650.0, taux_tva: 18.0, remise: 0.0, taxes: vec![],
            }],
        }).unwrap();
        // brouillon ⇒ suppression OK, plus lisible ensuite
        supprimer(&conn, &doc.id).unwrap();
        assert!(lire(&conn, &doc.id).is_err());
        // lignes + taxes purgées
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM document_ligne WHERE document_id = ?1",
            params![doc.id], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
    }

    #[test]
    fn suppression_document_valide_refusee() {
        let conn = db::open_in_memory().unwrap();
        let t = tiers(&conn);
        let a = article_bien(&conn, true);
        let doc = creer(&conn, &NouveauDocument {
            type_document: TypeDocument::Facture, sens: SensDocument::Vente,
            tiers_id: t, depot_id: None, date: None,
            note: None, document_source_id: None, reference_dossier: None, objet: None,
            lignes: vec![NouvelleLigne {
                article_id: a, designation: "Riz".into(),
                quantite: 2.0, prix_unitaire: 650.0, taux_tva: 18.0, remise: 0.0, taxes: vec![],
            }],
        }).unwrap();
        valider(&conn, &doc.id).unwrap();
        assert!(supprimer(&conn, &doc.id).is_err());
        // toujours là
        assert!(lire(&conn, &doc.id).is_ok());
    }

    #[test]
    fn annulation_vente_reintegre_stock_et_contrepasse_paiement() {
        let conn = db::open_in_memory().unwrap();
        let t = tiers(&conn);
        let a = article_bien(&conn, true);
        let depot = depot::defaut(&conn).unwrap();
        let doc = creer(&conn, &NouveauDocument {
            type_document: TypeDocument::Facture, sens: SensDocument::Vente,
            tiers_id: t.clone(), depot_id: Some(depot.clone()), date: None,
            note: None, document_source_id: None, reference_dossier: None, objet: None,
            lignes: vec![NouvelleLigne {
                article_id: a.clone(), designation: "Riz".into(),
                quantite: 5.0, prix_unitaire: 1000.0, taux_tva: 0.0, remise: 0.0, taxes: vec![],
            }],
        }).unwrap();
        let ttc = doc.total_ttc; // 5000 (TVA 0)
        valider(&conn, &doc.id).unwrap();
        // encaissement complet en caisse
        let pay = enregistrer_pay(&conn, &t, &doc.id, ttc);
        let caisse = pay.caisse_id.clone();
        // état après vente : stock -5, tiers 0, caisse +ttc
        assert_eq!(stock::stock_article_depot(&conn, &a, &depot).unwrap(), -5.0);
        let solde_c0: f64 = conn.query_row("SELECT solde FROM caisse WHERE id=?1", params![caisse], |r| r.get(0)).unwrap();
        assert_eq!(solde_c0, ttc);

        // brouillon/annulation d'une pièce non validée refusée
        assert!(annuler(&conn, &doc.id, "", None).is_err()); // motif vide

        let ann = annuler(&conn, &doc.id, "client a changé d'avis", Some("admin")).unwrap();
        assert_eq!(ann.statut, StatutDocument::Annule);
        // stock réintégré (retour à 0), caisse revenue à 0 (argent rendu), tiers 0
        assert_eq!(stock::stock_article_depot(&conn, &a, &depot).unwrap(), 0.0);
        let solde_c1: f64 = conn.query_row("SELECT solde FROM caisse WHERE id=?1", params![caisse], |r| r.get(0)).unwrap();
        assert_eq!(solde_c1, 0.0);
        let solde_t: f64 = conn.query_row("SELECT solde FROM tiers WHERE id=?1", params![t], |r| r.get(0)).unwrap();
        assert_eq!(solde_t, 0.0);
        // re-annuler ⇒ refus (déjà annulé)
        assert!(annuler(&conn, &doc.id, "x", None).is_err());
        // la contre-passation est traçable
        let rev: i64 = conn.query_row(
            "SELECT COUNT(*) FROM paiement WHERE annulation_de IS NOT NULL", [], |r| r.get(0)).unwrap();
        assert_eq!(rev, 1);
    }

    fn enregistrer_pay(conn: &Connection, tiers_id: &str, doc_id: &str, montant: f64) -> crate::modules::paiement::Paiement {
        use crate::domain::{ModePaiement, SensPaiement};
        use crate::modules::paiement::{enregistrer, NouveauPaiement};
        enregistrer(conn, &NouveauPaiement {
            tiers_id: tiers_id.into(), caisse_id: None, document_id: Some(doc_id.into()),
            sens: SensPaiement::Encaissement, montant, mode: ModePaiement::Espece, moyen_paiement_id: None,
        }).unwrap()
    }

    #[test]
    fn article_sans_gestion_stock_nest_pas_impacte() {
        let conn = db::open_in_memory().unwrap();
        let t = tiers(&conn);
        let a = article_bien(&conn, false); // ne gère pas le stock
        let depot = depot::defaut(&conn).unwrap();
        let doc = creer(&conn, &NouveauDocument {
            type_document: TypeDocument::Facture, sens: SensDocument::Vente,
            tiers_id: t, depot_id: Some(depot.clone()), date: None,
            note: None, document_source_id: None, reference_dossier: None, objet: None,
            lignes: vec![NouvelleLigne {
                article_id: a.clone(), designation: "Riz".into(),
                quantite: 5.0, prix_unitaire: 650.0, taux_tva: 18.0, remise: 0.0, taxes: vec![],
            }],
        }).unwrap();
        valider(&conn, &doc.id).unwrap();
        assert_eq!(stock::stock_article_depot(&conn, &a, &depot).unwrap(), 0.0);
    }
}
