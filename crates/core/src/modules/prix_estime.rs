//! Prix d'achat estimés (migration 0035) — des chiffres de démonstration
//! **assumés comme tels**.
//!
//! # Le problème réel
//!
//! Vérifié sur les données de l'utilisateur le 2026-07-27 : **25 articles sur
//! 25 n'avaient aucun prix d'achat**. Le rapport de bénéfices affichait donc un
//! coût de zéro, et une marge égale au chiffre d'affaires. C'était faux, et
//! rien à l'écran ne permettait de s'en apercevoir.
//!
//! # Le principe retenu
//!
//! L'utilisateur a demandé de « seeder des données test », faute de vrais
//! chiffres. On le fait — mais **jamais en silence** : chaque prix posé ici est
//! marqué `prix_achat_estime`, se voit à l'écran, est rappelé sur les rapports
//! de marge, et s'efface dès que le commerçant saisit son vrai prix.
//!
//! Un chiffre inventé sans étiquette est plus dangereux qu'une case vide,
//! parce qu'il a l'air juste.
//!
//! # Comment le prix est estimé
//!
//! Deux méthodes, dans cet ordre :
//!
//! 1. **Un prix de référence**, pour les denrées et produits courants d'Afrique
//!    de l'Ouest reconnus par leur désignation (riz, huile, sucre…). Employé
//!    uniquement pour un article **acheté tel quel** (marchandise ou matière
//!    première suivie en stock) — sinon on risquerait de confondre le sac de riz
//!    avec le plat de riz.
//! 2. **Un pourcentage du prix de vente**, sinon. Ce sont des ratios de métier :
//!    un plat de restaurant coûte environ un tiers de son prix de vente, une
//!    marchandise revendue en l'état autour de 70 %.
//!
//! Le résultat est toujours **plafonné à 90 % du prix de vente** : une
//! estimation ne doit jamais faire apparaître une marge négative sur une démo.

use crate::error::{CoreError, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Prix de référence indicatifs, en francs CFA, pour des conditionnements
/// courants (le kilo, le litre, l'unité). **Ce sont des ordres de grandeur de
/// marché, pas des tarifs** : ils servent à rendre une démonstration crédible.
///
/// Le mot-clé est cherché dans la désignation, en minuscules. L'ordre compte :
/// le premier mot-clé trouvé gagne, donc les plus précis d'abord.
const REFERENCES: &[(&str, f64)] = &[
    // Céréales, féculents
    ("riz", 500.0),
    ("mil", 450.0),
    ("mais", 400.0),
    ("maïs", 400.0),
    ("farine", 450.0),
    ("pate", 600.0),
    ("pâte", 600.0),
    ("spaghetti", 700.0),
    ("couscous", 800.0),
    ("pomme de terre", 500.0),
    ("manioc", 350.0),
    ("igname", 600.0),
    ("pain", 150.0),
    // Corps gras, condiments
    ("huile", 1200.0),
    ("beurre", 2500.0),
    ("sucre", 700.0),
    ("sel", 150.0),
    ("vinaigre", 700.0),
    ("moutarde", 900.0),
    ("mayonnaise", 1200.0),
    ("bouillon", 50.0),
    ("piment", 800.0),
    ("ail", 1200.0),
    ("gingembre", 1000.0),
    // Protéines
    ("boeuf", 3500.0),
    ("bœuf", 3500.0),
    ("mouton", 4000.0),
    ("agneau", 4000.0),
    ("viande", 3500.0),
    ("poulet", 2500.0),
    ("volaille", 2500.0),
    ("poisson", 1500.0),
    ("thiof", 3000.0),
    ("crevette", 4500.0),
    ("oeuf", 100.0),
    ("œuf", 100.0),
    // Légumes et fruits
    ("oignon", 500.0),
    ("tomate", 600.0),
    ("carotte", 600.0),
    ("chou", 500.0),
    ("aubergine", 500.0),
    ("gombo", 700.0),
    ("citron", 500.0),
    ("mangue", 600.0),
    ("banane", 700.0),
    ("orange", 600.0),
    // Boissons et épicerie
    ("lait", 2000.0),
    ("yaourt", 400.0),
    ("cafe", 100.0),
    ("café", 100.0),
    ("the", 100.0),
    ("thé", 100.0),
    ("eau", 300.0),
    ("jus", 400.0),
    ("soda", 350.0),
    ("boisson", 350.0),
    ("biscuit", 300.0),
    ("chocolat", 1500.0),
    // Quincaillerie et fournitures courantes
    ("ciment", 4000.0),
    ("peinture", 8000.0),
    ("clou", 1000.0),
    ("vis", 1200.0),
    ("cable", 1500.0),
    ("câble", 1500.0),
    ("ampoule", 800.0),
    ("savon", 400.0),
    ("detergent", 900.0),
    ("détergent", 900.0),
    ("papier", 500.0),
];

/// Part du prix de vente que représente le coût d'achat, selon ce que l'article
/// **est**. Ratios de métier, volontairement prudents.
fn ratio_par_nature(nature: &str) -> f64 {
    match nature {
        // Revendue en l'état : la marge du négoce est mince.
        "marchandise" => 0.70,
        // Consommée en fabrication : proche de son coût d'achat.
        "matiere_premiere" => 0.75,
        // Fabriquée : le coût matière d'un plat tourne autour du tiers du prix.
        "produit_fini" => 0.35,
        // Prestation : l'essentiel est de la main-d'œuvre, peu de matière.
        "service" => 0.30,
        _ => 0.60,
    }
}

/// Plafond de sécurité : une estimation ne doit jamais produire une marge
/// négative, ce qui ferait douter de toute la démonstration.
const PART_MAX_DU_PRIX_VENTE: f64 = 0.90;

/// Arrondi au multiple de 5 francs — un prix d'achat à deux décimales n'a aucun
/// sens sur un marché où la plus petite pièce vaut 5 F.
fn arrondir_fcfa(v: f64) -> f64 {
    (v / 5.0).round() * 5.0
}

/// Enlève les accents et met en minuscules, pour que « Café » trouve « cafe ».
fn simplifier(s: &str) -> String {
    s.chars()
        .flat_map(|c| c.to_lowercase())
        .map(|c| match c {
            'á' | 'à' | 'â' | 'ä' => 'a',
            'é' | 'è' | 'ê' | 'ë' => 'e',
            'í' | 'ì' | 'î' | 'ï' => 'i',
            'ó' | 'ò' | 'ô' | 'ö' => 'o',
            'ú' | 'ù' | 'û' | 'ü' => 'u',
            'ç' => 'c',
            autre => autre,
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct PrixPropose {
    pub article_id: String,
    pub code: String,
    pub designation: String,
    pub prix_vente: f64,
    pub prix_estime: f64,
    /// Comment le chiffre a été trouvé, en clair — le commerçant doit pouvoir
    /// juger s'il y croit.
    pub methode: String,
}

/// Calcule (sans rien écrire) le prix qu'on proposerait pour cet article.
/// `None` quand on ne sait pas proposer quoi que ce soit de sensé.
fn proposer(
    designation: &str,
    prix_vente: f64,
    nature: &str,
    gere_stock: bool,
) -> Option<(f64, String)> {
    let achete_tel_quel = gere_stock && matches!(nature, "marchandise" | "matiere_premiere");
    let d = simplifier(designation);

    let (brut, methode) = if achete_tel_quel {
        match REFERENCES.iter().find(|(mot, _)| d.contains(&simplifier(mot))) {
            Some((mot, prix)) => (*prix, format!("prix de marché indicatif pour « {mot} »")),
            None if prix_vente > 0.0 => (
                prix_vente * ratio_par_nature(nature),
                format!("{} % du prix de vente", (ratio_par_nature(nature) * 100.0).round()),
            ),
            None => return None,
        }
    } else if prix_vente > 0.0 {
        (
            prix_vente * ratio_par_nature(nature),
            format!("{} % du prix de vente", (ratio_par_nature(nature) * 100.0).round()),
        )
    } else {
        // Ni prix de vente, ni référence : on n'invente pas.
        return None;
    };

    let plafond = if prix_vente > 0.0 { prix_vente * PART_MAX_DU_PRIX_VENTE } else { brut };
    let retenu = arrondir_fcfa(brut.min(plafond)).max(5.0);
    Some((retenu, methode))
}

/// Aperçu : ce que Djigui proposerait, **sans rien écrire**. L'écran le montre
/// avant d'appliquer — pas de chiffre posé dans le dos de l'utilisateur.
pub fn apercu(conn: &Connection) -> Result<Vec<PrixPropose>> {
    let mut st = conn.prepare(
        "SELECT id, code, designation, prix_vente, nature_comptable, gere_stock
           FROM article
          WHERE actif = 1 AND (prix_achat IS NULL OR prix_achat = 0)
          ORDER BY designation",
    )?;
    let lignes = st
        .query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
                r.get::<_, f64>(3)?, r.get::<_, String>(4)?, r.get::<_, i64>(5)? != 0,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut out = Vec::new();
    for (id, code, designation, prix_vente, nature, gere_stock) in lignes {
        if let Some((prix, methode)) = proposer(&designation, prix_vente, &nature, gere_stock) {
            out.push(PrixPropose {
                article_id: id, code, designation, prix_vente,
                prix_estime: prix, methode,
            });
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RapportEstimation {
    pub estimes: usize,
    /// Articles qu'on n'a pas su estimer (ni prix de vente, ni référence).
    pub ignores: usize,
}

/// Pose les prix estimés sur les articles qui n'en ont pas. Les prix **saisis**
/// ne sont jamais touchés : on ne remplace pas un vrai chiffre par une devinette.
pub fn appliquer(conn: &Connection) -> Result<RapportEstimation> {
    let propositions = apercu(conn)?;
    let total_sans_prix: i64 = conn.query_row(
        "SELECT COUNT(*) FROM article WHERE actif = 1 AND (prix_achat IS NULL OR prix_achat = 0)",
        [],
        |r| r.get(0),
    )?;
    let mut n = 0;
    for p in &propositions {
        conn.execute(
            "UPDATE article SET prix_achat = ?2, prix_achat_estime = 1
              WHERE id = ?1 AND (prix_achat IS NULL OR prix_achat = 0)",
            params![p.article_id, p.prix_estime],
        )?;
        n += 1;
    }
    Ok(RapportEstimation {
        estimes: n,
        ignores: (total_sans_prix as usize).saturating_sub(n),
    })
}

/// Efface toutes les estimations et remet ces articles sans prix d'achat.
/// Indispensable : une démonstration doit pouvoir être **défaite**, sinon les
/// chiffres inventés finiraient par s'installer.
pub fn effacer_estimations(conn: &Connection) -> Result<usize> {
    let n = conn.execute(
        "UPDATE article SET prix_achat = NULL, prix_achat_estime = 0
          WHERE prix_achat_estime = 1",
        [],
    )?;
    Ok(n)
}

#[derive(Debug, Clone, Serialize)]
pub struct ArticleACompleter {
    pub id: String,
    pub code: String,
    pub designation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub categorie_nom: Option<String>,
    pub prix_vente: f64,
    /// Prix actuellement en base : soit une estimation, soit rien.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prix_achat: Option<f64>,
    pub estime: bool,
    /// Marge que donnerait le prix actuel — pour que le commerçant voie tout de
    /// suite si le chiffre est plausible.
    pub marge_pct: Option<f64>,
    /// Quantité déjà vendue : on complète en priorité ce qui tourne.
    pub quantite_vendue: f64,
}

/// La liste de l'écran « Compléter mes prix » : ce qui est estimé ou vide,
/// **le plus vendu d'abord** — c'est là que l'erreur de marge coûte le plus cher.
pub fn a_completer(conn: &Connection) -> Result<Vec<ArticleACompleter>> {
    let mut st = conn.prepare(
        "SELECT a.id, a.code, a.designation, c.nom, a.prix_vente, a.prix_achat,
                a.prix_achat_estime,
                (SELECT COALESCE(SUM(dl.quantite), 0)
                   FROM document_ligne dl
                   JOIN document d ON d.id = dl.document_id
                  WHERE dl.article_id = a.id AND d.sens = 'vente' AND d.statut = 'valide')
           FROM article a
           LEFT JOIN categorie c ON c.id = a.categorie_id
          WHERE a.actif = 1
            AND (a.prix_achat_estime = 1 OR a.prix_achat IS NULL OR a.prix_achat = 0)
          ORDER BY 8 DESC, a.designation",
    )?;
    let v = st
        .query_map([], |r| {
            let prix_vente: f64 = r.get(4)?;
            let prix_achat: Option<f64> = r.get(5)?;
            let marge = match prix_achat {
                Some(pa) if prix_vente > 0.0 => Some(((prix_vente - pa) / prix_vente * 100.0).round()),
                _ => None,
            };
            Ok(ArticleACompleter {
                id: r.get(0)?,
                code: r.get(1)?,
                designation: r.get(2)?,
                categorie_nom: r.get(3)?,
                prix_vente,
                prix_achat,
                estime: r.get::<_, i64>(6)? != 0,
                marge_pct: marge,
                quantite_vendue: r.get(7)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

#[derive(Debug, Clone, Deserialize)]
pub struct PrixReel {
    pub article_id: String,
    pub prix_achat: f64,
}

/// Saisie des vrais prix, un ou plusieurs à la fois. Le drapeau « estimé »
/// tombe : c'est désormais le chiffre du commerçant.
pub fn definir_prix_reels(conn: &Connection, prix: &[PrixReel]) -> Result<usize> {
    let mut n = 0;
    for p in prix {
        if p.prix_achat < 0.0 {
            return Err(CoreError::Rule("un prix d'achat ne peut pas être négatif".into()));
        }
        n += conn.execute(
            "UPDATE article SET prix_achat = ?2, prix_achat_estime = 0 WHERE id = ?1",
            params![p.article_id, p.prix_achat],
        )?;
    }
    Ok(n)
}

/// Combien d'articles reposent encore sur une estimation. Sert au bandeau
/// d'avertissement des rapports de marge.
pub fn nb_estimes(conn: &Connection) -> Result<i64> {
    Ok(conn.query_row(
        "SELECT COUNT(*) FROM article WHERE actif = 1 AND prix_achat_estime = 1",
        [],
        |r| r.get(0),
    )?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn base() -> Connection {
        let conn = db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO article (id, code, type, designation, prix_vente, prix_achat,
                                  taux_tva, gere_stock, actif, nature_comptable)
             VALUES
               ('a1','RIZ','bien','Riz parfumé 1kg', 800, NULL, 0, 1, 1, 'marchandise'),
               ('a2','YASSA','service','Poulet yassa', 3000, NULL, 0, 0, 1, 'produit_fini'),
               ('a3','COUPE','service','Coupe de cheveux', 2000, NULL, 0, 0, 1, 'service'),
               ('a4','VRAI','bien','Article deja tarifé', 1000, 640, 0, 1, 1, 'marchandise'),
               ('a5','RIEN','bien','Sans prix de vente', 0, NULL, 0, 1, 1, 'marchandise');",
        )
        .unwrap();
        conn
    }

    #[test]
    fn une_marchandise_connue_prend_le_prix_de_marche() {
        let conn = base();
        let vus = apercu(&conn).unwrap();
        let riz = vus.iter().find(|p| p.code == "RIZ").unwrap();
        assert_eq!(riz.prix_estime, 500.0);
        assert!(riz.methode.contains("riz"), "{}", riz.methode);
    }

    #[test]
    fn un_plat_est_estime_au_tiers_de_son_prix_de_vente() {
        let conn = base();
        let vus = apercu(&conn).unwrap();
        // 3000 × 0,35 = 1050, arrondi au multiple de 5.
        let plat = vus.iter().find(|p| p.code == "YASSA").unwrap();
        assert_eq!(plat.prix_estime, 1050.0);
        // Un service, lui, est estimé plus bas : 2000 × 0,30 = 600.
        let coupe = vus.iter().find(|p| p.code == "COUPE").unwrap();
        assert_eq!(coupe.prix_estime, 600.0);
    }

    #[test]
    fn on_ninvente_rien_sans_prix_de_vente_ni_reference() {
        let conn = base();
        let vus = apercu(&conn).unwrap();
        assert!(vus.iter().all(|p| p.code != "RIEN"));
        let r = appliquer(&conn).unwrap();
        assert_eq!(r.ignores, 1, "l'article sans aucune base reste sans prix");
    }

    #[test]
    fn un_prix_saisi_nest_jamais_ecrase() {
        let conn = base();
        appliquer(&conn).unwrap();
        let (prix, estime): (f64, i64) = conn
            .query_row("SELECT prix_achat, prix_achat_estime FROM article WHERE id = 'a4'", [],
                       |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(prix, 640.0, "le vrai prix du commerçant reste intact");
        assert_eq!(estime, 0, "et il n'est pas marqué comme estimé");
    }

    #[test]
    fn les_estimations_sont_marquees_puis_effacables() {
        let conn = base();
        let r = appliquer(&conn).unwrap();
        assert_eq!(r.estimes, 3);
        assert_eq!(nb_estimes(&conn).unwrap(), 3);

        // Elles se voient dans la liste « à compléter ».
        let liste = a_completer(&conn).unwrap();
        assert!(liste.iter().filter(|a| a.estime).count() == 3);

        // Et la démonstration se défait entièrement.
        let n = effacer_estimations(&conn).unwrap();
        assert_eq!(n, 3);
        assert_eq!(nb_estimes(&conn).unwrap(), 0);
        let sans: i64 = conn
            .query_row("SELECT COUNT(*) FROM article WHERE prix_achat IS NULL", [], |r| r.get(0))
            .unwrap();
        assert_eq!(sans, 4, "les 3 estimés redeviennent vides, plus celui d'origine");
    }

    #[test]
    fn saisir_le_vrai_prix_retire_letiquette_estime() {
        let conn = base();
        appliquer(&conn).unwrap();
        definir_prix_reels(&conn, &[PrixReel { article_id: "a1".into(), prix_achat: 620.0 }]).unwrap();
        let (prix, estime): (f64, i64) = conn
            .query_row("SELECT prix_achat, prix_achat_estime FROM article WHERE id = 'a1'", [],
                       |r| Ok((r.get(0)?, r.get(1)?)))
            .unwrap();
        assert_eq!(prix, 620.0);
        assert_eq!(estime, 0);
        assert_eq!(nb_estimes(&conn).unwrap(), 2);
    }

    #[test]
    fn une_estimation_ne_produit_jamais_de_marge_negative() {
        let conn = db::open_in_memory().unwrap();
        // Un sac de riz revendu 300 F alors que la référence en vaut 500 :
        // le plafond doit ramener l'estimation sous le prix de vente.
        conn.execute(
            "INSERT INTO article (id, code, type, designation, prix_vente, taux_tva,
                                  gere_stock, actif, nature_comptable)
             VALUES ('x','RIZ2','bien','Riz cassé', 300, 0, 1, 1, 'marchandise')",
            [],
        )
        .unwrap();
        let p = &apercu(&conn).unwrap()[0];
        assert!(p.prix_estime < 300.0, "estimé {} pour un prix de vente de 300", p.prix_estime);
        assert_eq!(p.prix_estime, 270.0, "plafonné à 90 % du prix de vente");
    }
}
