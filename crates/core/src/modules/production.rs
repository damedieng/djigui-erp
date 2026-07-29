//! Production — nomenclatures (recettes) et ordres de fabrication (§5.7, mig 0031).
//!
//! Trois cas d'usage couverts par le même modèle (validé le 2026-07-26) :
//! cuisine/restauration, atelier/transformation, assemblage de kits.
//!
//! Deux idées structurent tout le module :
//!
//! 1. **La recette est un modèle, pas une contrainte.** Un ordre de production
//!    recopie les composants de la nomenclature au moment de sa création, puis
//!    vit sa vie : on peut ajouter, retirer ou corriger un composant sans
//!    toucher à la recette.
//! 2. **Le prévu et le réel sont deux choses différentes.** On saisit à la
//!    clôture ce qui a *vraiment* été produit et consommé. L'écart est
//!    **signalé** (`alertes`), jamais bloquant : la gestion ne doit pas
//!    empêcher de produire. Même le stock insuffisant n'est qu'une alerte —
//!    la marchandise, elle, a bien été utilisée.
//!
//! Le stock n'est touché **qu'à la clôture**, et uniquement par
//! [`crate::modules::stock::ecrire`] (motif `production`) : sorties pour les
//! composants, entrée pour le produit fini.

use crate::domain::{MotifMouvement, SensMouvement, StatutOrdreProduction};
use crate::error::{CoreError, Result};
use crate::modules::stock;
use crate::now;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn vide(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|x| !x.is_empty())
}

/// Arrondi de présentation des quantités : évite les 0.30000000000000004 dus au
/// prorata (quantité demandée ÷ quantité de la recette).
fn arrondir(v: f64) -> f64 {
    (v * 1_000_000.0).round() / 1_000_000.0
}

// ===========================================================================
// Nomenclature (la « recette »)
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct ComposantRecette {
    pub id: String,
    pub article_id: String,
    pub article_code: String,
    pub article_designation: String,
    /// Quantité pour le lot complet (voir `Nomenclature::quantite_produite`).
    pub quantite: f64,
    /// Perte technique attendue en % (épluchures, chutes…).
    pub perte_pct: f64,
    /// Prix d'achat courant de l'article, pour estimer le coût de la recette.
    pub prix_achat: f64,
    pub ordre: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Nomenclature {
    pub id: String,
    pub article_id: String,
    pub article_code: String,
    pub article_designation: String,
    pub nom: String,
    pub quantite_produite: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub actif: bool,
    pub cree_le: String,
    pub nb_composants: i64,
    /// Coût estimé du lot, aux prix d'achat **du jour** (indicatif : le coût réel
    /// est figé à la clôture de chaque ordre).
    pub cout_estime: f64,
    /// Coût estimé d'une unité produite.
    pub cout_unitaire_estime: f64,
    /// Composants — remplis par [`lire_nomenclature`] seulement.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub composants: Vec<ComposantRecette>,
    /// Incohérences détectées. **Informatif, jamais bloquant.**
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub alertes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouveauComposant {
    pub article_id: String,
    pub quantite: f64,
    #[serde(default)]
    pub perte_pct: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouvelleNomenclature {
    pub article_id: String,
    pub nom: String,
    #[serde(default = "un")]
    pub quantite_produite: f64,
    #[serde(default)]
    pub note: Option<String>,
    #[serde(default = "vrai")]
    pub actif: bool,
    /// Liste complète des composants : elle **remplace** l'existante en
    /// modification (le formulaire envoie toujours la recette entière).
    #[serde(default)]
    pub composants: Vec<NouveauComposant>,
}

fn un() -> f64 {
    1.0
}
fn vrai() -> bool {
    true
}

const NOM_COLS: &str = "SELECT n.id, n.article_id, a.code, a.designation, n.nom,
        n.quantite_produite, n.note, n.actif, n.cree_le,
        (SELECT COUNT(*) FROM nomenclature_composant c WHERE c.nomenclature_id = n.id),
        (SELECT COALESCE(SUM(c.quantite * COALESCE(ca.prix_achat, 0)
                             * (1 + c.perte_pct / 100.0)), 0)
           FROM nomenclature_composant c
           JOIN article ca ON ca.id = c.article_id
          WHERE c.nomenclature_id = n.id)
     FROM nomenclature n JOIN article a ON a.id = n.article_id";

fn ligne_nomenclature(r: &rusqlite::Row) -> rusqlite::Result<Nomenclature> {
    let quantite_produite: f64 = r.get(5)?;
    let cout_estime: f64 = r.get(10)?;
    Ok(Nomenclature {
        id: r.get(0)?,
        article_id: r.get(1)?,
        article_code: r.get(2)?,
        article_designation: r.get(3)?,
        nom: r.get(4)?,
        quantite_produite,
        note: r.get(6)?,
        actif: r.get::<_, i64>(7)? == 1,
        cree_le: r.get(8)?,
        nb_composants: r.get(9)?,
        cout_estime: arrondir(cout_estime),
        cout_unitaire_estime: if quantite_produite > 0.0 {
            arrondir(cout_estime / quantite_produite)
        } else {
            0.0
        },
        composants: Vec::new(),
        alertes: Vec::new(),
    })
}

/// Liste les recettes. `article_id` filtre sur l'article fabriqué.
pub fn lister_nomenclatures(
    conn: &Connection,
    article_id: Option<&str>,
    actives_seulement: bool,
) -> Result<Vec<Nomenclature>> {
    let sql = format!(
        "{NOM_COLS} WHERE (?1 IS NULL OR n.article_id = ?1)
           AND (?2 = 0 OR n.actif = 1)
         ORDER BY a.designation, n.nom"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        params![article_id, if actives_seulement { 1 } else { 0 }],
        ligne_nomenclature,
    )?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn lire_nomenclature(conn: &Connection, id: &str) -> Result<Nomenclature> {
    let sql = format!("{NOM_COLS} WHERE n.id = ?1");
    let mut n = conn.query_row(&sql, params![id], ligne_nomenclature).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => CoreError::NotFound(format!("nomenclature {id}")),
        autre => autre.into(),
    })?;
    n.composants = composants_recette(conn, id)?;
    n.alertes = alertes_recette(&n);
    Ok(n)
}

fn composants_recette(conn: &Connection, nomenclature_id: &str) -> Result<Vec<ComposantRecette>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.article_id, a.code, a.designation, c.quantite, c.perte_pct,
                COALESCE(a.prix_achat, 0), c.ordre
           FROM nomenclature_composant c JOIN article a ON a.id = c.article_id
          WHERE c.nomenclature_id = ?1
          ORDER BY c.ordre, a.designation",
    )?;
    let rows = stmt.query_map(params![nomenclature_id], |r| {
        Ok(ComposantRecette {
            id: r.get(0)?,
            article_id: r.get(1)?,
            article_code: r.get(2)?,
            article_designation: r.get(3)?,
            quantite: r.get(4)?,
            perte_pct: r.get(5)?,
            prix_achat: r.get(6)?,
            ordre: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Contrôles de cohérence d'une recette. **Aucun n'empêche d'enregistrer** :
/// on affiche, l'utilisateur décide (standard maison).
fn alertes_recette(n: &Nomenclature) -> Vec<String> {
    let mut a = Vec::new();
    if n.composants.is_empty() {
        a.push("Cette recette n'a aucun composant : elle ne consommera rien.".into());
    }
    if n.composants.iter().any(|c| c.article_id == n.article_id) {
        a.push(format!(
            "« {} » se trouve dans ses propres composants : la fabrication se consommerait elle-même.",
            n.article_designation
        ));
    }
    let sans_prix: Vec<&str> = n
        .composants
        .iter()
        .filter(|c| c.prix_achat <= 0.0)
        .map(|c| c.article_designation.as_str())
        .collect();
    if !sans_prix.is_empty() {
        a.push(format!(
            "Prix d'achat non renseigné pour : {}. Le coût de revient sera sous-évalué.",
            sans_prix.join(", ")
        ));
    }
    if n.composants.iter().any(|c| c.quantite <= 0.0) {
        a.push("Un composant a une quantité nulle ou négative.".into());
    }
    a
}

pub fn creer_nomenclature(
    conn: &Connection,
    n: &NouvelleNomenclature,
    par: Option<&str>,
) -> Result<Nomenclature> {
    valider_recette(n)?;
    let id = Uuid::new_v4().to_string();
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO nomenclature
            (id, article_id, nom, quantite_produite, note, actif, cree_par, cree_le)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            id,
            n.article_id,
            n.nom.trim(),
            n.quantite_produite,
            vide(&n.note),
            if n.actif { 1 } else { 0 },
            par,
            now()
        ],
    )?;
    ecrire_composants_recette(&tx, &id, &n.composants)?;
    classer_recette(&tx, &n.article_id, &n.composants)?;
    tx.commit()?;
    lire_nomenclature(conn, &id)
}

/// Écrire une recette dit tout : ce qu'on fabrique et ce qu'on consomme.
/// C'est le meilleur moment pour classer, sans rien demander à personne.
fn classer_recette(
    conn: &Connection,
    article_produit: &str,
    composants: &[NouveauComposant],
) -> Result<()> {
    classer_produit_fini(conn, article_produit)?;
    for c in composants {
        if c.article_id != article_produit {
            classer_matiere_premiere(conn, &c.article_id)?;
        }
    }
    Ok(())
}

pub fn modifier_nomenclature(
    conn: &Connection,
    id: &str,
    n: &NouvelleNomenclature,
) -> Result<Nomenclature> {
    valider_recette(n)?;
    let tx = conn.unchecked_transaction()?;
    let maj = tx.execute(
        "UPDATE nomenclature SET article_id = ?2, nom = ?3, quantite_produite = ?4,
                note = ?5, actif = ?6 WHERE id = ?1",
        params![
            id,
            n.article_id,
            n.nom.trim(),
            n.quantite_produite,
            vide(&n.note),
            if n.actif { 1 } else { 0 }
        ],
    )?;
    if maj == 0 {
        return Err(CoreError::NotFound(format!("nomenclature {id}")));
    }
    // Le formulaire envoie toujours la recette entière : on remplace la liste.
    tx.execute("DELETE FROM nomenclature_composant WHERE nomenclature_id = ?1", params![id])?;
    ecrire_composants_recette(&tx, id, &n.composants)?;
    classer_recette(&tx, &n.article_id, &n.composants)?;
    tx.commit()?;
    lire_nomenclature(conn, id)
}

fn valider_recette(n: &NouvelleNomenclature) -> Result<()> {
    if n.nom.trim().is_empty() {
        return Err(CoreError::Rule("le nom de la recette est requis".into()));
    }
    if n.article_id.trim().is_empty() {
        return Err(CoreError::Rule("l'article fabriqué est requis".into()));
    }
    if n.quantite_produite <= 0.0 {
        return Err(CoreError::Rule(
            "la quantité produite par la recette doit être positive".into(),
        ));
    }
    Ok(())
}

fn ecrire_composants_recette(
    conn: &Connection,
    nomenclature_id: &str,
    composants: &[NouveauComposant],
) -> Result<()> {
    // Un même article ne peut apparaître qu'une fois (contrainte UNIQUE) : on
    // additionne les doublons plutôt que de renvoyer une erreur technique.
    let mut fusion: Vec<NouveauComposant> = Vec::new();
    for c in composants {
        if c.article_id.trim().is_empty() {
            continue;
        }
        match fusion.iter_mut().find(|f| f.article_id == c.article_id) {
            Some(f) => f.quantite += c.quantite,
            None => fusion.push(c.clone()),
        }
    }
    for (i, c) in fusion.iter().enumerate() {
        conn.execute(
            "INSERT INTO nomenclature_composant
                (id, nomenclature_id, article_id, quantite, perte_pct, ordre)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                Uuid::new_v4().to_string(),
                nomenclature_id,
                c.article_id,
                c.quantite,
                c.perte_pct,
                i as i64
            ],
        )?;
    }
    Ok(())
}

/// Supprime une recette. Les ordres déjà créés depuis cette recette sont
/// **détachés**, jamais détruits : un ordre de fabrication est de l'historique.
pub fn supprimer_nomenclature(conn: &Connection, id: &str) -> Result<()> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("UPDATE ordre_production SET nomenclature_id = NULL WHERE nomenclature_id = ?1", params![id])?;
    tx.execute("DELETE FROM nomenclature_composant WHERE nomenclature_id = ?1", params![id])?;
    let n = tx.execute("DELETE FROM nomenclature WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("nomenclature {id}")));
    }
    tx.commit()?;
    Ok(())
}

// ===========================================================================
// Ordres de production
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct ComposantOrdre {
    pub id: String,
    pub article_id: String,
    pub article_code: String,
    pub article_designation: String,
    pub quantite_prevue: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantite_reelle: Option<f64>,
    /// Coût unitaire figé à la clôture ; avant, prix d'achat courant.
    pub cout_unitaire: f64,
    /// Coût de la ligne (réel s'il est connu, sinon prévu).
    pub cout: f64,
    /// Stock disponible dans le dépôt de l'ordre, au moment de la lecture.
    pub stock_dispo: f64,
    pub ordre: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct Ordre {
    pub id: String,
    pub numero: String,
    pub article_produit_id: String,
    pub article_code: String,
    pub article_designation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nomenclature_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nomenclature_nom: Option<String>,
    pub depot_id: String,
    pub depot_nom: String,
    pub quantite: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quantite_produite: Option<f64>,
    pub statut: String,
    pub date: String,
    pub frais: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cout_total: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cout_unitaire: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motif_annulation: Option<String>,
    pub cree_le: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cloture_le: Option<String>,
    pub nb_composants: i64,
    /// Coût estimé tant que l'ordre n'est pas clôturé (composants prévus aux
    /// prix d'achat du jour + frais). Après clôture, `cout_total` fait foi.
    pub cout_estime: f64,
    /// Écart entre quantité prévue et quantité réellement produite (clôture).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ecart_quantite: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub composants: Vec<ComposantOrdre>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub alertes: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouvelOrdre {
    pub article_produit_id: String,
    #[serde(default)]
    pub nomenclature_id: Option<String>,
    pub depot_id: String,
    pub quantite: f64,
    #[serde(default)]
    pub date: Option<String>,
    #[serde(default)]
    pub frais: f64,
    #[serde(default)]
    pub note: Option<String>,
    /// Composants explicites. Si la liste est vide et qu'une nomenclature est
    /// fournie, ils sont déduits de la recette (au prorata de la quantité).
    #[serde(default)]
    pub composants: Vec<NouveauComposant>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FiltreOrdres {
    #[serde(default)]
    pub statut: Option<String>,
    #[serde(default)]
    pub article_id: Option<String>,
    #[serde(default)]
    pub du: Option<String>,
    #[serde(default)]
    pub au: Option<String>,
}

const ORDRE_COLS: &str = "SELECT o.id, o.numero, o.article_produit_id, a.code, a.designation,
        o.nomenclature_id, n.nom, o.depot_id, d.nom, o.quantite, o.quantite_produite,
        o.statut, o.date, o.frais, o.cout_total, o.cout_unitaire, o.note,
        o.motif_annulation, o.cree_le, o.cloture_le,
        (SELECT COUNT(*) FROM production_composant c WHERE c.ordre_id = o.id),
        (SELECT COALESCE(SUM(COALESCE(c.quantite_reelle, c.quantite_prevue)
                             * COALESCE(c.cout_unitaire, ca.prix_achat, 0)), 0)
           FROM production_composant c
           JOIN article ca ON ca.id = c.article_id
          WHERE c.ordre_id = o.id)
     FROM ordre_production o
     JOIN article a ON a.id = o.article_produit_id
     JOIN depot d ON d.id = o.depot_id
     LEFT JOIN nomenclature n ON n.id = o.nomenclature_id";

fn ligne_ordre(r: &rusqlite::Row) -> rusqlite::Result<Ordre> {
    let quantite: f64 = r.get(9)?;
    let quantite_produite: Option<f64> = r.get(10)?;
    let frais: f64 = r.get(13)?;
    let cout_composants: f64 = r.get(21)?;
    Ok(Ordre {
        id: r.get(0)?,
        numero: r.get(1)?,
        article_produit_id: r.get(2)?,
        article_code: r.get(3)?,
        article_designation: r.get(4)?,
        nomenclature_id: r.get(5)?,
        nomenclature_nom: r.get(6)?,
        depot_id: r.get(7)?,
        depot_nom: r.get(8)?,
        quantite,
        quantite_produite,
        statut: r.get(11)?,
        date: r.get(12)?,
        frais,
        cout_total: r.get(14)?,
        cout_unitaire: r.get(15)?,
        note: r.get(16)?,
        motif_annulation: r.get(17)?,
        cree_le: r.get(18)?,
        cloture_le: r.get(19)?,
        nb_composants: r.get(20)?,
        cout_estime: arrondir(cout_composants + frais),
        ecart_quantite: quantite_produite.map(|qp| arrondir(qp - quantite)),
        composants: Vec::new(),
        alertes: Vec::new(),
    })
}

pub fn lister_ordres(conn: &Connection, f: &FiltreOrdres) -> Result<Vec<Ordre>> {
    let sql = format!(
        "{ORDRE_COLS}
         WHERE (?1 IS NULL OR o.statut = ?1)
           AND (?2 IS NULL OR o.article_produit_id = ?2)
           AND (?3 IS NULL OR o.date >= ?3)
           AND (?4 IS NULL OR o.date <= ?4)
         ORDER BY o.date DESC, o.numero DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(
        params![
            vide(&f.statut),
            vide(&f.article_id),
            vide(&f.du),
            vide(&f.au)
        ],
        ligne_ordre,
    )?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn lire_ordre(conn: &Connection, id: &str) -> Result<Ordre> {
    let sql = format!("{ORDRE_COLS} WHERE o.id = ?1");
    let mut o = conn.query_row(&sql, params![id], ligne_ordre).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => CoreError::NotFound(format!("ordre {id}")),
        autre => autre.into(),
    })?;
    o.composants = composants_ordre(conn, id, &o.depot_id)?;
    o.alertes = alertes_ordre(&o);
    Ok(o)
}

fn composants_ordre(conn: &Connection, ordre_id: &str, depot_id: &str) -> Result<Vec<ComposantOrdre>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.article_id, a.code, a.designation, c.quantite_prevue,
                c.quantite_reelle, COALESCE(c.cout_unitaire, a.prix_achat, 0), c.ordre,
                COALESCE((SELECT SUM(CASE WHEN m.sens='entree' THEN m.quantite ELSE -m.quantite END)
                            FROM mouvement_stock m
                           WHERE m.article_id = c.article_id AND m.depot_id = ?2), 0)
           FROM production_composant c JOIN article a ON a.id = c.article_id
          WHERE c.ordre_id = ?1
          ORDER BY c.ordre, a.designation",
    )?;
    let rows = stmt.query_map(params![ordre_id, depot_id], |r| {
        let prevue: f64 = r.get(4)?;
        let reelle: Option<f64> = r.get(5)?;
        let cu: f64 = r.get(6)?;
        Ok(ComposantOrdre {
            id: r.get(0)?,
            article_id: r.get(1)?,
            article_code: r.get(2)?,
            article_designation: r.get(3)?,
            quantite_prevue: prevue,
            quantite_reelle: reelle,
            cout_unitaire: cu,
            cout: arrondir(reelle.unwrap_or(prevue) * cu),
            stock_dispo: r.get(8)?,
            ordre: r.get(7)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Contrôles de cohérence d'un ordre. **Signalement seulement** : même un stock
/// insuffisant ne bloque pas la clôture — la matière a bien été consommée dans
/// la réalité, c'est le stock théorique qui était faux.
fn alertes_ordre(o: &Ordre) -> Vec<String> {
    let mut a = Vec::new();
    if o.composants.is_empty() {
        a.push("Cet ordre n'a aucun composant : rien ne sera consommé à la clôture.".into());
    }
    if o.statut != "termine" && o.statut != "annule" {
        let manquants: Vec<String> = o
            .composants
            .iter()
            .filter(|c| c.quantite_prevue > c.stock_dispo)
            .map(|c| {
                format!(
                    "{} (besoin {}, en stock {})",
                    c.article_designation, c.quantite_prevue, c.stock_dispo
                )
            })
            .collect();
        if !manquants.is_empty() {
            a.push(format!(
                "Stock insuffisant dans « {} » pour : {}. Vous pouvez tout de même produire ; le stock passera en négatif.",
                o.depot_nom,
                manquants.join(", ")
            ));
        }
    }
    let sans_prix: Vec<&str> = o
        .composants
        .iter()
        .filter(|c| c.cout_unitaire <= 0.0)
        .map(|c| c.article_designation.as_str())
        .collect();
    if !sans_prix.is_empty() {
        a.push(format!(
            "Prix d'achat non renseigné pour : {}. Le prix de revient calculé sera sous-évalué.",
            sans_prix.join(", ")
        ));
    }
    if let Some(ecart) = o.ecart_quantite {
        if ecart < 0.0 {
            a.push(format!(
                "Production inférieure au prévu : {} unité(s) de moins que les {} annoncées.",
                -ecart, o.quantite
            ));
        } else if ecart > 0.0 {
            a.push(format!(
                "Production supérieure au prévu : {ecart} unité(s) de plus que les {} annoncées.",
                o.quantite
            ));
        }
    }
    a
}

/// Numérotation des ordres : même mécanique que les pièces commerciales
/// (`sequence_numero`, un compteur par exercice), préfixe `OF`.
fn numero_suivant(conn: &Connection, date: &str) -> Result<String> {
    let exercice: i64 = date.get(0..4).and_then(|a| a.parse().ok()).unwrap_or(1970);
    conn.execute(
        "INSERT INTO sequence_numero (type_document, exercice, dernier)
         VALUES ('production', ?1, 1)
         ON CONFLICT(type_document, exercice) DO UPDATE SET dernier = dernier + 1",
        params![exercice],
    )?;
    let n: i64 = conn.query_row(
        "SELECT dernier FROM sequence_numero WHERE type_document = 'production' AND exercice = ?1",
        params![exercice],
        |r| r.get(0),
    )?;
    let prefixe: String = conn
        .query_row(
            "SELECT prefixe FROM config_prefixe_document WHERE type_document = 'production'",
            [],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| "OF".to_string());
    Ok(format!("{prefixe}-{exercice}-{n:04}"))
}

pub fn creer_ordre(conn: &Connection, n: &NouvelOrdre, par: Option<&str>) -> Result<Ordre> {
    if n.article_produit_id.trim().is_empty() {
        return Err(CoreError::Rule("l'article à fabriquer est requis".into()));
    }
    if n.depot_id.trim().is_empty() {
        return Err(CoreError::Rule("le magasin de fabrication est requis".into()));
    }
    if n.quantite <= 0.0 {
        return Err(CoreError::Rule("la quantité à produire doit être positive".into()));
    }
    let date = vide(&n.date).map(str::to_string).unwrap_or_else(|| now()[..10].to_string());

    // Composants : ceux fournis, sinon déduits de la recette au prorata.
    let composants = if !n.composants.is_empty() {
        n.composants.clone()
    } else if let Some(nom_id) = vide(&n.nomenclature_id) {
        composants_depuis_recette(conn, nom_id, n.quantite)?
    } else {
        Vec::new()
    };

    let id = Uuid::new_v4().to_string();
    let tx = conn.unchecked_transaction()?;
    let numero = numero_suivant(&tx, &date)?;
    tx.execute(
        "INSERT INTO ordre_production
            (id, numero, article_produit_id, nomenclature_id, depot_id, quantite,
             statut, date, frais, note, cree_par, cree_le)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'brouillon', ?7, ?8, ?9, ?10, ?11)",
        params![
            id,
            numero,
            n.article_produit_id,
            vide(&n.nomenclature_id),
            n.depot_id,
            n.quantite,
            date,
            n.frais,
            vide(&n.note),
            par,
            now()
        ],
    )?;
    ecrire_composants_ordre(&tx, &id, &composants)?;
    tx.commit()?;
    lire_ordre(conn, &id)
}

/// Déduit les composants d'un ordre depuis une recette : quantité de la recette
/// × (quantité demandée ÷ quantité produite par la recette), majorée de la perte
/// technique attendue.
fn composants_depuis_recette(
    conn: &Connection,
    nomenclature_id: &str,
    quantite: f64,
) -> Result<Vec<NouveauComposant>> {
    let recette = lire_nomenclature(conn, nomenclature_id)?;
    let facteur = if recette.quantite_produite > 0.0 {
        quantite / recette.quantite_produite
    } else {
        0.0
    };
    Ok(recette
        .composants
        .iter()
        .map(|c| NouveauComposant {
            article_id: c.article_id.clone(),
            quantite: arrondir(c.quantite * facteur * (1.0 + c.perte_pct / 100.0)),
            perte_pct: c.perte_pct,
        })
        .collect())
}

fn ecrire_composants_ordre(
    conn: &Connection,
    ordre_id: &str,
    composants: &[NouveauComposant],
) -> Result<()> {
    let mut fusion: Vec<NouveauComposant> = Vec::new();
    for c in composants {
        if c.article_id.trim().is_empty() {
            continue;
        }
        match fusion.iter_mut().find(|f| f.article_id == c.article_id) {
            Some(f) => f.quantite += c.quantite,
            None => fusion.push(c.clone()),
        }
    }
    for (i, c) in fusion.iter().enumerate() {
        conn.execute(
            "INSERT INTO production_composant
                (id, ordre_id, article_id, quantite_prevue, ordre)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                Uuid::new_v4().to_string(),
                ordre_id,
                c.article_id,
                c.quantite,
                i as i64
            ],
        )?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Classement comptable AUTOMATIQUE (migration 0032)
// ---------------------------------------------------------------------------
//
// ⚠️ Décision utilisateur (2026-07-26) : « il ne faut pas compliquer la tâche à
// un commerçant ». Un commerçant n'a pas à savoir ce qu'est une « matière
// première » — c'est du vocabulaire de comptable. Djigui classe donc **tout
// seul**, à partir de ce que l'article fait réellement, et ne demande rien.
//
// Deux garde-fous, parce qu'un mauvais classement ferait disparaître un article
// de la caisse — ce serait bien pire que de ne rien classer :
//   * on ne déclasse **jamais** un article qui a déjà été vendu ou qui a un prix
//     de vente : le sac de ciment vendu entier ET reconditionné reste vendable ;
//   * `marchandise` (le défaut) apparaît **à la fois** à la caisse et dans les
//     recettes. Celui qui ne fait rien garde exactement le comportement d'avant.
//
// Le comptable pourra corriger tout cela depuis son propre écran, plus tard.

/// L'article fabriqué devient un **produit fini** (comptes 702/36, production
/// stockée 73). Sans risque : ce qu'on fabrique se vend, et `produit_fini` reste
/// visible à la caisse.
///
/// ⚠️ On accepte aussi `service` comme point de départ (migration 0033) : dans
/// les catalogues métier, `article.type` vaut `service` dès que l'article ne
/// gère pas le stock — un plat cuisiné est donc souvent typé « service ». Ce que
/// l'article **fait** prime sur la façon dont il a été créé.
fn classer_produit_fini(conn: &Connection, article_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE article SET nature_comptable = 'produit_fini'
          WHERE id = ?1 AND nature_comptable IN ('marchandise','service')",
        params![article_id],
    )?;
    Ok(())
}

/// Un composant devient **matière première** (comptes 602/32) uniquement si
/// Djigui est certain qu'il ne se vend pas : aucun prix de vente ET jamais
/// apparu dans un document. Au moindre doute, on n'y touche pas — un article qui
/// disparaîtrait de la caisse serait bien pire qu'un article mal classé.
fn classer_matiere_premiere(conn: &Connection, article_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE article SET nature_comptable = 'matiere_premiere'
          WHERE id = ?1
            AND nature_comptable IN ('marchandise','service')
            AND prix_vente = 0
            AND NOT EXISTS (SELECT 1 FROM document_ligne WHERE article_id = ?1)",
        params![article_id],
    )?;
    Ok(())
}

fn statut_de(conn: &Connection, id: &str) -> Result<String> {
    conn.query_row("SELECT statut FROM ordre_production WHERE id = ?1", params![id], |r| r.get(0))
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => CoreError::NotFound(format!("ordre {id}")),
            autre => autre.into(),
        })
}

/// Modifie un ordre **non clôturé**. Un ordre terminé est de l'historique : il
/// a produit des mouvements de stock, on ne le réécrit pas (on l'annule).
pub fn modifier_ordre(conn: &Connection, id: &str, n: &NouvelOrdre) -> Result<Ordre> {
    let statut = statut_de(conn, id)?;
    if statut == "termine" || statut == "annule" {
        return Err(CoreError::Rule(
            "un ordre terminé ou annulé ne se modifie plus : il a déjà bougé le stock".into(),
        ));
    }
    if n.quantite <= 0.0 {
        return Err(CoreError::Rule("la quantité à produire doit être positive".into()));
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "UPDATE ordre_production SET article_produit_id = ?2, nomenclature_id = ?3,
                depot_id = ?4, quantite = ?5, date = ?6, frais = ?7, note = ?8
         WHERE id = ?1",
        params![
            id,
            n.article_produit_id,
            vide(&n.nomenclature_id),
            n.depot_id,
            n.quantite,
            vide(&n.date).unwrap_or(&now()[..10]),
            n.frais,
            vide(&n.note)
        ],
    )?;
    tx.execute("DELETE FROM production_composant WHERE ordre_id = ?1", params![id])?;
    ecrire_composants_ordre(&tx, id, &n.composants)?;
    tx.commit()?;
    lire_ordre(conn, id)
}

/// Change le statut sans toucher au stock. Le passage à `termine` est refusé
/// ici : il passe obligatoirement par [`cloturer`], qui écrit les mouvements.
pub fn changer_statut(conn: &Connection, id: &str, statut: StatutOrdreProduction) -> Result<Ordre> {
    if statut == StatutOrdreProduction::Termine {
        return Err(CoreError::Rule(
            "pour terminer un ordre, utilisez la clôture (elle enregistre les mouvements de stock)"
                .into(),
        ));
    }
    let actuel = statut_de(conn, id)?;
    if actuel == "termine" {
        return Err(CoreError::Rule("un ordre terminé ne change plus de statut".into()));
    }
    conn.execute(
        "UPDATE ordre_production SET statut = ?2 WHERE id = ?1",
        params![id, statut],
    )?;
    lire_ordre(conn, id)
}

#[derive(Debug, Clone, Deserialize)]
pub struct ComposantConsomme {
    pub article_id: String,
    /// Quantité réellement consommée.
    pub quantite: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Cloture {
    /// Quantité réellement obtenue. Absente = « comme prévu ».
    #[serde(default)]
    pub quantite_produite: Option<f64>,
    /// Consommation réelle. Les composants absents de la liste sont consommés
    /// tels que prévus.
    #[serde(default)]
    pub composants: Vec<ComposantConsomme>,
    /// Frais à incorporer (remplace ceux de l'ordre s'il est renseigné).
    #[serde(default)]
    pub frais: Option<f64>,
    /// Reporter le prix de revient obtenu sur la fiche de l'article fabriqué.
    /// Coché par défaut côté écran, mais c'est l'utilisateur qui décide : on
    /// n'écrase jamais silencieusement un prix qu'il a saisi.
    #[serde(default)]
    pub maj_prix_revient: bool,
}

/// **Clôture** : le seul endroit où la production touche au stock.
///
/// En une transaction : une sortie par composant (motif `production`), une
/// entrée pour l'article fabriqué, le coût figé sur chaque ligne, puis l'ordre
/// passe `termine`.
pub fn cloturer(conn: &Connection, id: &str, c: &Cloture, par: Option<&str>) -> Result<Ordre> {
    let ordre = lire_ordre(conn, id)?;
    match ordre.statut.as_str() {
        "termine" => return Err(CoreError::Rule("cet ordre est déjà terminé".into())),
        "annule" => return Err(CoreError::Rule("un ordre annulé ne peut pas être clôturé".into())),
        _ => {}
    }
    let quantite_produite = c.quantite_produite.unwrap_or(ordre.quantite);
    if quantite_produite <= 0.0 {
        return Err(CoreError::Rule("la quantité produite doit être positive".into()));
    }
    let frais = c.frais.unwrap_or(ordre.frais);

    let tx = conn.unchecked_transaction()?;
    let mut cout_composants = 0.0;

    for comp in &ordre.composants {
        let quantite = c
            .composants
            .iter()
            .find(|x| x.article_id == comp.article_id)
            .map(|x| x.quantite)
            .unwrap_or(comp.quantite_prevue);
        // Coût figé maintenant : une fabrication passée ne doit pas se
        // revaloriser quand le prix d'achat du composant changera.
        let cout_unitaire = comp.cout_unitaire;
        cout_composants += quantite * cout_unitaire;

        tx.execute(
            "UPDATE production_composant SET quantite_reelle = ?2, cout_unitaire = ?3
             WHERE id = ?1",
            params![comp.id, quantite, cout_unitaire],
        )?;
        if quantite > 0.0 {
            stock::ecrire(
                &tx,
                &comp.article_id,
                &ordre.depot_id,
                None,
                SensMouvement::Sortie,
                quantite,
                MotifMouvement::Production,
            )?;
        }
    }

    // Entrée du produit fini.
    stock::ecrire(
        &tx,
        &ordre.article_produit_id,
        &ordre.depot_id,
        None,
        SensMouvement::Entree,
        quantite_produite,
        MotifMouvement::Production,
    )?;

    let cout_total = arrondir(cout_composants + frais);
    let cout_unitaire = arrondir(cout_total / quantite_produite);
    tx.execute(
        "UPDATE ordre_production
            SET statut = 'termine', quantite_produite = ?2, frais = ?3,
                cout_total = ?4, cout_unitaire = ?5, cloture_par = ?6, cloture_le = ?7
          WHERE id = ?1",
        params![id, quantite_produite, frais, cout_total, cout_unitaire, par, now()],
    )?;

    // La fabrication a réellement eu lieu : on peut classer sans hésiter.
    classer_produit_fini(&tx, &ordre.article_produit_id)?;
    for comp in &ordre.composants {
        if comp.article_id != ordre.article_produit_id {
            classer_matiere_premiere(&tx, &comp.article_id)?;
        }
    }

    if c.maj_prix_revient {
        // Le prix de revient alimente la marge et le rapport bénéfices.
        tx.execute(
            "UPDATE article SET prix_achat = ?2 WHERE id = ?1",
            params![ordre.article_produit_id, cout_unitaire],
        )?;
    }
    tx.commit()?;
    lire_ordre(conn, id)
}

/// Annule un ordre. Un ordre **terminé** ne s'annule pas ici : ses mouvements de
/// stock existent, il faudrait les contre-passer — non couvert en v1, on préfère
/// refuser clairement plutôt que de laisser un stock faux.
pub fn annuler(conn: &Connection, id: &str, motif: &str) -> Result<Ordre> {
    if motif.trim().is_empty() {
        return Err(CoreError::Rule("le motif d'annulation est requis".into()));
    }
    let statut = statut_de(conn, id)?;
    if statut == "termine" {
        return Err(CoreError::Rule(
            "un ordre terminé a déjà bougé le stock : il ne peut pas être annulé".into(),
        ));
    }
    conn.execute(
        "UPDATE ordre_production SET statut = 'annule', motif_annulation = ?2 WHERE id = ?1",
        params![id, motif.trim()],
    )?;
    lire_ordre(conn, id)
}

/// Supprime un ordre **brouillon** uniquement (rien n'a bougé). Un ordre lancé
/// ou terminé s'annule, il ne s'efface pas : c'est de la traçabilité.
pub fn supprimer_ordre(conn: &Connection, id: &str) -> Result<()> {
    let statut = statut_de(conn, id)?;
    if statut != "brouillon" {
        return Err(CoreError::Rule(
            "seul un ordre en brouillon peut être supprimé ; sinon, annulez-le".into(),
        ));
    }
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM production_composant WHERE ordre_id = ?1", params![id])?;
    tx.execute("DELETE FROM ordre_production WHERE id = ?1", params![id])?;
    tx.commit()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Traitement par lot (standard maison : toute liste doit savoir agir en masse)
// ---------------------------------------------------------------------------

/// Change le statut de plusieurs ordres. Renvoie le nombre traité ; les ordres
/// qui refusent (déjà terminés) sont ignorés sans faire échouer le lot.
pub fn changer_statut_lot(
    conn: &Connection,
    ids: &[String],
    statut: StatutOrdreProduction,
) -> Result<usize> {
    let mut n = 0;
    for id in ids {
        if changer_statut(conn, id, statut).is_ok() {
            n += 1;
        }
    }
    Ok(n)
}

/// Supprime plusieurs ordres brouillons. Les autres sont ignorés.
pub fn supprimer_lot(conn: &Connection, ids: &[String]) -> Result<usize> {
    let mut n = 0;
    for id in ids {
        if supprimer_ordre(conn, id).is_ok() {
            n += 1;
        }
    }
    Ok(n)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn article(conn: &Connection, code: &str, prix_achat: f64) -> String {
        article_prix(conn, code, prix_achat, 0.0)
    }

    fn article_prix(conn: &Connection, code: &str, prix_achat: f64, prix_vente: f64) -> String {
        let id = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO article (id, code, type, designation, prix_vente, prix_achat, gere_stock)
             VALUES (?1, ?2, 'bien', ?3, ?5, ?4, 1)",
            params![id, code, format!("Article {code}"), prix_achat, prix_vente],
        )
        .unwrap();
        id
    }

    fn nature(conn: &Connection, id: &str) -> String {
        conn.query_row("SELECT nature_comptable FROM article WHERE id = ?1", params![id], |r| {
            r.get(0)
        })
        .unwrap()
    }

    fn depot(conn: &Connection) -> String {
        let id = Uuid::new_v4().to_string();
        conn.execute("INSERT INTO depot (id, nom) VALUES (?1, 'Cuisine')", params![id]).unwrap();
        id
    }

    fn entrer_stock(conn: &Connection, article_id: &str, depot_id: &str, q: f64) {
        stock::ecrire(
            conn,
            article_id,
            depot_id,
            None,
            SensMouvement::Entree,
            q,
            MotifMouvement::Achat,
        )
        .unwrap();
    }

    #[test]
    fn recette_puis_ordre_au_prorata() {
        let conn = db::open_in_memory().unwrap();
        let pain = article(&conn, "PAIN", 0.0);
        let farine = article(&conn, "FARINE", 500.0);
        let sel = article(&conn, "SEL", 200.0);

        // Une recette produit 20 baguettes avec 10 kg de farine et 0,2 kg de sel.
        let rec = creer_nomenclature(
            &conn,
            &NouvelleNomenclature {
                article_id: pain.clone(),
                nom: "Pâte à baguette".into(),
                quantite_produite: 20.0,
                note: None,
                actif: true,
                composants: vec![
                    NouveauComposant { article_id: farine.clone(), quantite: 10.0, perte_pct: 0.0 },
                    NouveauComposant { article_id: sel.clone(), quantite: 0.2, perte_pct: 0.0 },
                ],
            },
            None,
        )
        .unwrap();
        // 10×500 + 0,2×200 = 5040 pour 20 baguettes → 252 l'unité.
        assert_eq!(rec.cout_estime, 5040.0);
        assert_eq!(rec.cout_unitaire_estime, 252.0);

        // Un ordre de 40 baguettes double les quantités de la recette.
        let d = depot(&conn);
        let o = creer_ordre(
            &conn,
            &NouvelOrdre {
                article_produit_id: pain.clone(),
                nomenclature_id: Some(rec.id.clone()),
                depot_id: d.clone(),
                quantite: 40.0,
                date: Some("2026-07-26".into()),
                frais: 0.0,
                note: None,
                composants: vec![],
            },
            None,
        )
        .unwrap();
        assert!(o.numero.starts_with("OF-2026-"));
        let f = o.composants.iter().find(|c| c.article_id == farine).unwrap();
        assert_eq!(f.quantite_prevue, 20.0);
    }

    #[test]
    fn cloture_sort_les_composants_et_entre_le_produit_fini() {
        let conn = db::open_in_memory().unwrap();
        let plat = article(&conn, "PLAT", 0.0);
        let riz = article(&conn, "RIZ", 400.0);
        let d = depot(&conn);
        entrer_stock(&conn, &riz, &d, 50.0);

        let o = creer_ordre(
            &conn,
            &NouvelOrdre {
                article_produit_id: plat.clone(),
                nomenclature_id: None,
                depot_id: d.clone(),
                quantite: 10.0,
                date: Some("2026-07-26".into()),
                frais: 1000.0, // gaz + main-d'œuvre
                note: None,
                composants: vec![NouveauComposant {
                    article_id: riz.clone(),
                    quantite: 5.0,
                    perte_pct: 0.0,
                }],
            },
            None,
        )
        .unwrap();

        let o = cloturer(
            &conn,
            &o.id,
            &Cloture {
                quantite_produite: Some(10.0),
                composants: vec![],
                frais: None,
                maj_prix_revient: true,
            },
            None,
        )
        .unwrap();

        assert_eq!(o.statut, "termine");
        // 5 kg × 400 + 1000 de frais = 3000 pour 10 plats → 300 l'unité.
        assert_eq!(o.cout_total, Some(3000.0));
        assert_eq!(o.cout_unitaire, Some(300.0));
        assert_eq!(stock::stock_article_depot(&conn, &riz, &d).unwrap(), 45.0);
        assert_eq!(stock::stock_article_depot(&conn, &plat, &d).unwrap(), 10.0);
        // Le prix de revient a été reporté sur la fiche article.
        let pa: f64 = conn
            .query_row("SELECT prix_achat FROM article WHERE id = ?1", params![plat], |r| r.get(0))
            .unwrap();
        assert_eq!(pa, 300.0);
        // Un ordre terminé ne se supprime pas.
        assert!(supprimer_ordre(&conn, &o.id).is_err());
    }

    /// Le classement comptable se fait **tout seul** : le commerçant ne coche
    /// rien. Mais il ne doit JAMAIS faire disparaître de la caisse un article
    /// qui se vend — ce serait pire que de ne rien classer.
    #[test]
    fn classement_automatique_sans_faire_disparaitre_un_vendable() {
        let conn = db::open_in_memory().unwrap();
        let pain = article(&conn, "PAIN", 0.0);
        let farine = article(&conn, "FARINE", 500.0); // jamais vendue, prix de vente 0
        // Le sac de ciment : revendu entier ET reconditionné. Il a un prix de vente.
        let ciment = article_prix(&conn, "CIMENT", 3000.0, 4000.0);

        creer_nomenclature(
            &conn,
            &NouvelleNomenclature {
                article_id: pain.clone(),
                nom: "Pate".into(),
                quantite_produite: 10.0,
                note: None,
                actif: true,
                composants: vec![
                    NouveauComposant { article_id: farine.clone(), quantite: 5.0, perte_pct: 0.0 },
                    NouveauComposant { article_id: ciment.clone(), quantite: 1.0, perte_pct: 0.0 },
                ],
            },
            None,
        )
        .unwrap();

        assert_eq!(nature(&conn, &pain), "produit_fini");
        assert_eq!(nature(&conn, &farine), "matiere_premiere");
        // Le ciment a un prix de vente : on n'y touche pas, il reste vendable.
        assert_eq!(nature(&conn, &ciment), "marchandise");

        // Ce qui compte vraiment : ce qui reste proposé à la caisse.
        use crate::modules::article as art;
        let vendables = art::lister(&conn, art::Filtre::Vendables).unwrap();
        let codes: Vec<&str> = vendables.iter().map(|a| a.code.as_str()).collect();
        assert!(codes.contains(&"CIMENT"), "le ciment doit rester vendable");
        assert!(codes.contains(&"PAIN"), "le produit fabriqué doit être vendable");
        assert!(!codes.contains(&"FARINE"), "la farine n'a rien à faire à la caisse");

        // Et côté recettes, la farine ET le ciment restent proposables.
        let composants = art::lister(&conn, art::Filtre::Composants).unwrap();
        let codes: Vec<&str> = composants.iter().map(|a| a.code.as_str()).collect();
        assert!(codes.contains(&"FARINE") && codes.contains(&"CIMENT"));

        // ⚠️ Modifier la farine depuis l'écran Articles (qui n'envoie PAS la
        // nature) ne doit pas annuler le classement automatique : sinon elle
        // réapparaîtrait à la caisse au premier changement de prix d'achat.
        let f = art::lire(&conn, &farine).unwrap();
        art::modifier(
            &conn,
            &farine,
            &art::NouvelArticle {
                code: f.code,
                r#type: f.r#type,
                nature_comptable: None, // l'écran ne s'occupe pas de comptabilité
                designation: f.designation,
                prix_vente: 0.0,
                prix_achat: Some(600.0), // le prix du sac a augmenté
                taux_tva: f.taux_tva,
                gere_stock: f.gere_stock,
                stock_alerte: None,
                categorie_id: None,
                image: None,
                code_barre: None,
                taxes: None,
            },
        )
        .unwrap();
        assert_eq!(nature(&conn, &farine), "matiere_premiere");
    }

    /// Cas réel rencontré le 2026-07-26 sur la base d'un restaurant : dans les
    /// catalogues métier, un article qui ne gère pas le stock est créé avec
    /// `type = 'service'`. Un plat cuisiné est donc typé « service » — ce qui ne
    /// doit surtout pas empêcher de le reconnaître comme produit fabriqué, ni
    /// laisser ses ingrédients traîner à la caisse.
    #[test]
    fn plat_type_service_est_reconnu_comme_produit_fabrique() {
        let conn = db::open_in_memory().unwrap();
        let plat = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO article (id, code, type, designation, prix_vente, gere_stock)
             VALUES (?1, 'YASSA', 'service', 'Yassa poulet', 2500, 0)",
            params![plat],
        )
        .unwrap();
        let riz = Uuid::new_v4().to_string();
        conn.execute(
            "INSERT INTO article (id, code, type, designation, prix_vente, prix_achat, gere_stock)
             VALUES (?1, 'RIZ', 'service', 'Riz local', 0, 700, 0)",
            params![riz],
        )
        .unwrap();

        creer_nomenclature(
            &conn,
            &NouvelleNomenclature {
                article_id: plat.clone(),
                nom: "Plat yassa".into(),
                quantite_produite: 4.0,
                note: None,
                actif: true,
                composants: vec![NouveauComposant {
                    article_id: riz.clone(),
                    quantite: 1.0,
                    perte_pct: 0.0,
                }],
            },
            None,
        )
        .unwrap();

        assert_eq!(nature(&conn, &plat), "produit_fini");
        assert_eq!(nature(&conn, &riz), "matiere_premiere");

        use crate::modules::article as art;
        let vendables: Vec<String> =
            art::lister(&conn, art::Filtre::Vendables).unwrap().into_iter().map(|a| a.code).collect();
        assert!(vendables.contains(&"YASSA".to_string()), "le plat doit rester vendable");
        assert!(!vendables.contains(&"RIZ".to_string()), "le riz ne se vend pas au client");
    }

    #[test]
    fn ecart_de_production_est_signale_sans_bloquer() {
        let conn = db::open_in_memory().unwrap();
        let gateau = article(&conn, "GATEAU", 0.0);
        let sucre = article(&conn, "SUCRE", 100.0);
        let d = depot(&conn);
        // Volontairement AUCUN stock de sucre : la clôture doit passer quand même.

        let o = creer_ordre(
            &conn,
            &NouvelOrdre {
                article_produit_id: gateau.clone(),
                nomenclature_id: None,
                depot_id: d.clone(),
                quantite: 12.0,
                date: None,
                frais: 0.0,
                note: None,
                composants: vec![NouveauComposant {
                    article_id: sucre.clone(),
                    quantite: 3.0,
                    perte_pct: 0.0,
                }],
            },
            None,
        )
        .unwrap();
        assert!(o.alertes.iter().any(|a| a.contains("Stock insuffisant")));

        // 9 gâteaux seulement, et on a consommé 4 kg au lieu de 3.
        let o = cloturer(
            &conn,
            &o.id,
            &Cloture {
                quantite_produite: Some(9.0),
                composants: vec![ComposantConsomme { article_id: sucre.clone(), quantite: 4.0 }],
                frais: None,
                maj_prix_revient: false,
            },
            None,
        )
        .unwrap();
        assert_eq!(o.ecart_quantite, Some(-3.0));
        assert!(o.alertes.iter().any(|a| a.contains("inférieure au prévu")));
        assert_eq!(o.cout_total, Some(400.0));
        // Le stock du composant passe en négatif : c'est le stock théorique qui
        // était faux, pas la fabrication.
        assert_eq!(stock::stock_article_depot(&conn, &sucre, &d).unwrap(), -4.0);
    }
}
