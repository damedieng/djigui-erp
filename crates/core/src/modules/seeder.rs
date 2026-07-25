//! Seeder de catalogues métier (SEEDER-CATALOGUES.md).
//!
//! Pré-remplit catégories + articles selon le **type de commerce**, à partir de
//! **données JSON embarquées** (jamais de catalogue en dur). Idempotent et
//! strictement additif : `code_seed` sert de clé, `INSERT OR IGNORE`, jamais
//! d'`UPDATE` (une saisie renommée par l'utilisateur n'est pas écrasée), le tout
//! dans une seule transaction. Les prix sont créés « à compléter » (§5).

use crate::error::{CoreError, Result};
use crate::now;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// --- Données embarquées -----------------------------------------------------

const INDEX: &str = include_str!("../../assets/catalogue/types.json");

/// JSON d'un catalogue par code de type (embarqué). `None` = type non encore fourni.
fn catalogue_json(code: &str) -> Option<&'static str> {
    match code {
        "alimentation_generale" => Some(include_str!("../../assets/catalogue/types/alimentation_generale.json")),
        "restaurant_fast_food" => Some(include_str!("../../assets/catalogue/types/restaurant_fast_food.json")),
        _ => None,
    }
}

// --- Modèle (serde) ---------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TypeCommerce {
    pub code: String,
    pub libelle: String,
    #[serde(default)]
    pub description: String,
    pub icone: String,
    pub ordre: i64,
    #[serde(default)]
    pub disponible: bool,
}

#[derive(Debug, Deserialize)]
struct IndexTypes {
    types: Vec<TypeCommerce>,
}

#[derive(Debug, Deserialize)]
struct Catalogue {
    #[allow(dead_code)]
    code: String,
    version: i64,
    categories: Vec<CategorieSeed>,
}

#[derive(Debug, Deserialize)]
struct CategorieSeed {
    code: String,
    libelle: String,
    icone: String,
    couleur: String,
    ordre: i64,
    articles: Vec<ArticleSeed>,
}

#[derive(Debug, Deserialize)]
struct ArticleSeed {
    code: String,
    libelle: String,
    unite: String,
    gere_stock: bool,
    #[serde(default)]
    tva: f64,
    #[serde(default)]
    #[allow(dead_code)]
    prix_vente: Option<f64>,
    #[serde(default)]
    image: Option<String>,
}

/// Type de commerce enrichi de son état (déjà appliqué ?), pour l'UI de choix.
#[derive(Debug, Clone, Serialize)]
pub struct TypeStatut {
    #[serde(flatten)]
    pub type_commerce: TypeCommerce,
    pub applique: bool,
}

/// Résumé d'une application (retour utilisateur).
#[derive(Debug, Serialize)]
pub struct ResultatSeed {
    pub categories_creees: usize,
    pub articles_crees: usize,
}

/// Sélection de l'utilisateur : un type + les codes d'articles retenus.
#[derive(Debug, Clone, Deserialize)]
pub struct SelectionCatalogue {
    pub code: String,
    pub articles: Vec<String>,
}

// --- Détail d'un catalogue (pour l'écran de choix des articles) -------------

#[derive(Debug, Serialize)]
pub struct CatalogueDetail {
    pub code: String,
    pub libelle: String,
    pub categories: Vec<CategorieDetail>,
}

#[derive(Debug, Serialize)]
pub struct CategorieDetail {
    pub code: String,
    pub libelle: String,
    pub icone: String,
    pub couleur: String,
    pub articles: Vec<ArticleDetail>,
}

#[derive(Debug, Serialize)]
pub struct ArticleDetail {
    pub code: String,
    pub libelle: String,
    pub unite: String,
    pub gere_stock: bool,
    /// Déjà présent en base (déjà seedé) : on le montre coché mais informatif.
    pub existe: bool,
}

/// Détail d'un catalogue (catégories + articles) pour l'écran de sélection.
pub fn detail(conn: &Connection, code: &str) -> Result<CatalogueDetail> {
    let json = catalogue_json(code)
        .ok_or_else(|| CoreError::Rule(format!("catalogue « {code} » indisponible")))?;
    let cat: Catalogue = serde_json::from_str(json)
        .map_err(|e| CoreError::Rule(format!("catalogue « {code} » illisible : {e}")))?;
    let libelle = lire_index()?
        .into_iter()
        .find(|t| t.code == code)
        .map(|t| t.libelle)
        .unwrap_or_else(|| code.to_string());

    // Codes d'articles déjà présents en base (par code_seed).
    let existants: std::collections::HashSet<String> = {
        let mut stmt = conn.prepare("SELECT code_seed FROM article WHERE code_seed IS NOT NULL")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };

    let categories = cat.categories.into_iter().map(|c| CategorieDetail {
        code: c.code,
        libelle: c.libelle,
        icone: c.icone,
        couleur: c.couleur,
        articles: c.articles.into_iter().map(|a| ArticleDetail {
            existe: existants.contains(&a.code),
            code: a.code,
            libelle: a.libelle,
            unite: a.unite,
            gere_stock: a.gere_stock,
        }).collect(),
    }).collect();

    Ok(CatalogueDetail { code: code.to_string(), libelle, categories })
}

/// Tous les codes d'articles d'un type (utilitaire / tests).
pub fn tous_les_articles(code: &str) -> Result<Vec<String>> {
    let json = catalogue_json(code)
        .ok_or_else(|| CoreError::Rule(format!("catalogue « {code} » indisponible")))?;
    let cat: Catalogue = serde_json::from_str(json)
        .map_err(|e| CoreError::Rule(format!("catalogue « {code} » illisible : {e}")))?;
    Ok(cat.categories.into_iter().flat_map(|c| c.articles).map(|a| a.code).collect())
}

// --- Lecture de l'index -----------------------------------------------------

fn lire_index() -> Result<Vec<TypeCommerce>> {
    let idx: IndexTypes = serde_json::from_str(INDEX)
        .map_err(|e| CoreError::Rule(format!("index des catalogues illisible : {e}")))?;
    Ok(idx.types)
}

/// Liste les types de commerce (triés) avec leur état « déjà appliqué ».
pub fn types_disponibles(conn: &Connection) -> Result<Vec<TypeStatut>> {
    let mut types = lire_index()?;
    types.sort_by_key(|t| t.ordre);
    let appliques: std::collections::HashSet<String> = {
        let mut stmt = conn.prepare("SELECT code_type FROM seed_applique")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    Ok(types
        .into_iter()
        .map(|t| TypeStatut { applique: appliques.contains(&t.code), type_commerce: t })
        .collect())
}

// --- Application (idempotente, transactionnelle) ----------------------------

pub fn appliquer(conn: &Connection, selections: &[SelectionCatalogue]) -> Result<ResultatSeed> {
    // Entreprise assujettie à la TVA ? sinon on force tva = 0 (§5).
    let assujetti: bool = conn
        .query_row(
            "SELECT assujetti_tva FROM parametres_entreprise WHERE singleton = 1",
            [],
            |r| Ok(r.get::<_, i64>(0)? != 0),
        )
        .unwrap_or(true);

    let tx = conn.unchecked_transaction()?;
    let mut cat_creees = 0usize;
    let mut art_crees = 0usize;

    for sel in selections {
        let code = &sel.code;
        let json = match catalogue_json(code) {
            Some(j) => j,
            None => return Err(CoreError::Rule(format!("catalogue « {code} » indisponible"))),
        };
        let cat: Catalogue = serde_json::from_str(json)
            .map_err(|e| CoreError::Rule(format!("catalogue « {code} » illisible : {e}")))?;

        // Articles retenus par l'utilisateur pour ce catalogue.
        let choisis: std::collections::HashSet<&str> =
            sel.articles.iter().map(|s| s.as_str()).collect();

        for c in &cat.categories {
            // On ne traite que les articles cochés ; catégorie ignorée si aucun.
            let articles_retenus: Vec<&ArticleSeed> =
                c.articles.iter().filter(|a| choisis.contains(a.code.as_str())).collect();
            if articles_retenus.is_empty() {
                continue;
            }
            // Catégorie : on réutilise l'existante (par code_seed, sinon par nom —
            // `nom` est UNIQUE et 4 catégories par défaut préexistent), sinon on crée.
            let existant: Option<String> = tx
                .query_row("SELECT id FROM categorie WHERE code_seed = ?1", params![c.code], |r| r.get(0))
                .optional()?
                .or(tx
                    .query_row("SELECT id FROM categorie WHERE nom = ?1", params![c.libelle], |r| r.get(0))
                    .optional()?);
            let cat_id = match existant {
                Some(id) => {
                    // Adoption d'une catégorie sans marque de seed : on complète ses
                    // métadonnées (jamais le nom). Idempotent : ne touche rien ensuite.
                    tx.execute(
                        "UPDATE categorie
                         SET code_seed = ?2, icone = ?3, couleur = ?4, ordre = ?5
                         WHERE id = ?1 AND code_seed IS NULL",
                        params![id, c.code, c.icone, c.couleur, c.ordre],
                    )?;
                    id
                }
                None => {
                    let id = Uuid::new_v4().to_string();
                    tx.execute(
                        "INSERT INTO categorie (id, nom, code_seed, icone, couleur, ordre)
                         VALUES (?1,?2,?3,?4,?5,?6)",
                        params![id, c.libelle, c.code, c.icone, c.couleur, c.ordre],
                    )?;
                    cat_creees += 1;
                    id
                }
            };

            for a in &articles_retenus {
                let type_article = if a.gere_stock { "bien" } else { "service" };
                let tva = if assujetti { a.tva } else { 0.0 };
                // Prix « à compléter » : prix_vente à 0 + drapeau (§5).
                let n = tx.execute(
                    "INSERT OR IGNORE INTO article
                        (id, code, type, designation, prix_vente, taux_tva, gere_stock,
                         actif, categorie_id, code_seed, unite, prix_a_completer)
                     VALUES (?1,?2,?3,?4,0,?5,?6,1,?7,?8,?9,1)",
                    params![
                        Uuid::new_v4().to_string(), a.code, type_article, a.libelle,
                        tva, a.gere_stock as i64, cat_id, a.code, a.unite,
                    ],
                )?;
                art_crees += n;
                // (image embarquée : extraction prévue à un incrément suivant)
                let _ = &a.image;
            }
        }

        // Trace du type appliqué (met à jour la version si déjà là).
        tx.execute(
            "INSERT INTO seed_applique (code_type, version, applique_le)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(code_type) DO UPDATE SET version = ?2, applique_le = ?3",
            params![code, cat.version, now()],
        )?;
    }

    tx.commit()?;
    Ok(ResultatSeed { categories_creees: cat_creees, articles_crees: art_crees })
}

// --- Tests ------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn compter(conn: &Connection, sql: &str) -> i64 {
        conn.query_row(sql, [], |r| r.get(0)).unwrap()
    }

    /// Sélection « tout le catalogue » (tous les articles cochés).
    fn sel(code: &str) -> SelectionCatalogue {
        SelectionCatalogue { code: code.into(), articles: tous_les_articles(code).unwrap() }
    }

    #[test]
    fn index_liste_les_types() {
        let conn = db::open_in_memory().unwrap();
        let types = types_disponibles(&conn).unwrap();
        assert!(types.len() >= 9);
        // les deux prioritaires sont disponibles et pas encore appliqués
        let ali = types.iter().find(|t| t.type_commerce.code == "alimentation_generale").unwrap();
        assert!(ali.type_commerce.disponible);
        assert!(!ali.applique);
    }

    #[test]
    fn application_cree_categories_et_articles() {
        let conn = db::open_in_memory().unwrap();
        let r = appliquer(&conn, &[sel("alimentation_generale")]).unwrap();
        assert_eq!(r.articles_crees, 42);
        // les 6 catégories du catalogue sont présentes (créées ou catégorie par défaut adoptée)
        assert_eq!(compter(&conn, "SELECT COUNT(*) FROM categorie WHERE code_seed IS NOT NULL"), 6);
        // articles créés « à compléter », sans prix
        assert_eq!(compter(&conn, "SELECT COUNT(*) FROM article WHERE prix_a_completer = 1"), 42);
        assert_eq!(compter(&conn, "SELECT COUNT(*) FROM article WHERE prix_vente = 0"), 42);
        // type appliqué tracé
        assert_eq!(compter(&conn, "SELECT COUNT(*) FROM seed_applique WHERE code_type='alimentation_generale'"), 1);
    }

    #[test]
    fn rejeu_identique_nadd_rien() {
        let conn = db::open_in_memory().unwrap();
        appliquer(&conn, &[sel("alimentation_generale")]).unwrap();
        let r2 = appliquer(&conn, &[sel("alimentation_generale")]).unwrap();
        assert_eq!(r2.categories_creees, 0);
        assert_eq!(r2.articles_crees, 0);
        assert_eq!(compter(&conn, "SELECT COUNT(*) FROM article WHERE code_seed IS NOT NULL"), 42);
    }

    #[test]
    fn dedoublonnage_entre_types() {
        let conn = db::open_in_memory().unwrap();
        // eau_minerale_50cl et soda_33cl existent dans les deux catalogues.
        appliquer(&conn, &[sel("alimentation_generale"), sel("restaurant_fast_food")]).unwrap();
        assert_eq!(compter(&conn, "SELECT COUNT(*) FROM article WHERE code_seed='eau_minerale_50cl'"), 1);
        assert_eq!(compter(&conn, "SELECT COUNT(*) FROM article WHERE code_seed='soda_33cl'"), 1);
    }

    #[test]
    fn renommage_utilisateur_conserve_au_rejeu() {
        let conn = db::open_in_memory().unwrap();
        appliquer(&conn, &[sel("alimentation_generale")]).unwrap();
        conn.execute("UPDATE article SET designation='Mon riz' WHERE code_seed='riz_brise_parfume_kg'", []).unwrap();
        appliquer(&conn, &[sel("alimentation_generale")]).unwrap();
        let nom: String = conn.query_row(
            "SELECT designation FROM article WHERE code_seed='riz_brise_parfume_kg'", [], |r| r.get(0)).unwrap();
        assert_eq!(nom, "Mon riz"); // jamais écrasé
    }

    #[test]
    fn entreprise_non_assujettie_force_tva_zero() {
        let conn = db::open_in_memory().unwrap();
        conn.execute(
            "INSERT INTO parametres_entreprise (id, singleton, assujetti_tva) VALUES ('p', 1, 0)",
            [],
        ).unwrap();
        appliquer(&conn, &[sel("alimentation_generale")]).unwrap();
        assert_eq!(compter(&conn, "SELECT COUNT(*) FROM article WHERE code_seed IS NOT NULL AND taux_tva <> 0"), 0);
    }
}
