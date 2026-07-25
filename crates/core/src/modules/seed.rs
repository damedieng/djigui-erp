//! Jeu de démonstration « supermarché » (§ perf / point 2 du suivi).
//!
//! Génère en masse des articles réalistes répartis sur plusieurs rayons, avec
//! code, code-barres, prix, TVA et un stock initial, pour éprouver la
//! pagination / recherche serveur de `/api/articles/page` à l'échelle de
//! plusieurs milliers de références. Insertion dans une transaction pour la
//! vitesse. Idempotent au sens « on ajoute N articles à chaque appel » : les
//! codes sont préfixés `SM-` + numéro de séquence pour rester uniques.

use crate::error::Result;
use crate::modules::depot;
use rusqlite::{params, Connection};
use uuid::Uuid;

/// Rayons de supermarché et quelques bases de désignation par rayon.
const RAYONS: &[(&str, &[&str])] = &[
    ("Fruits & Légumes", &["Tomate", "Pomme", "Banane", "Oignon", "Carotte", "Mangue", "Salade", "Pomme de terre"]),
    ("Boucherie", &["Poulet", "Bœuf", "Mouton", "Poisson", "Merguez", "Escalope"]),
    ("Épicerie", &["Riz", "Huile", "Sucre", "Sel", "Farine", "Pâtes", "Tomate concentrée", "Café", "Thé"]),
    ("Boissons", &["Eau minérale", "Jus d'orange", "Soda cola", "Bissap", "Lait", "Bière sans alcool"]),
    ("Hygiène", &["Savon", "Shampoing", "Dentifrice", "Papier toilette", "Mouchoirs", "Gel douche"]),
    ("Entretien", &["Javel", "Lessive", "Liquide vaisselle", "Éponge", "Sac poubelle"]),
    ("Produits laitiers", &["Yaourt", "Fromage", "Beurre", "Crème fraîche"]),
    ("Boulangerie", &["Pain de mie", "Baguette", "Croissant", "Biscotte"]),
];

const FORMATS: &[&str] = &["250 g", "500 g", "1 kg", "2 kg", "33 cl", "50 cl", "1 L", "1,5 L", "lot de 6", "pack"];
const MARQUES: &[&str] = &["Bon Prix", "Djolof", "Teranga", "Sahel", "Baobab", "Niokolo", "Casamance", "Diamono"];

/// Génère `n` articles de démonstration. Retourne le nombre réellement créé.
pub fn generer(conn: &Connection, n: usize) -> Result<usize> {
    let depot_id = depot::defaut(conn)?;

    // catégories : une par rayon, réutilisées si déjà présentes.
    let mut cat_ids: Vec<String> = Vec::with_capacity(RAYONS.len());
    for (rayon, _) in RAYONS {
        let id: String = match conn.query_row(
            "SELECT id FROM categorie WHERE nom = ?1",
            params![rayon],
            |r| r.get(0),
        ) {
            Ok(id) => id,
            Err(_) => {
                let id = Uuid::new_v4().to_string();
                conn.execute("INSERT INTO categorie (id, nom) VALUES (?1, ?2)", params![id, rayon])?;
                id
            }
        };
        cat_ids.push(id);
    }

    // point de départ de la séquence de code (évite les collisions inter-appels).
    let base: i64 = conn
        .query_row("SELECT COUNT(*) FROM article WHERE code LIKE 'SM-%'", [], |r| r.get(0))
        .unwrap_or(0);

    let tx = conn.unchecked_transaction()?;
    let now = crate::now();
    for i in 0..n {
        let seq = base + i as i64 + 1;
        let ri = i % RAYONS.len();
        let (_rayon, bases) = RAYONS[ri];
        let designation = format!(
            "{} {} {} {}",
            bases[(i / RAYONS.len()) % bases.len()],
            MARQUES[i % MARQUES.len()],
            FORMATS[i % FORMATS.len()],
            // suffixe numérique pour garantir des désignations distinctes
            seq,
        );
        let code = format!("SM-{seq:05}");
        let code_barre = format!("{:013}", 6_000_000_000_000i64 + seq);
        // prix pseudo-aléatoire déterministe (25 F à ~5000 F, multiples de 25).
        let prix_vente = 25.0 * (((seq.wrapping_mul(2654435761)) & 0xFF) as f64 + 1.0);
        let prix_achat = (prix_vente * 0.72).round();
        let taux_tva = if ri == 0 || ri == 6 { 0.0 } else { 18.0 }; // frais/laitier exonérés (démo)
        let stock_alerte = 5 + (seq % 10);
        let article_id = Uuid::new_v4().to_string();

        tx.execute(
            "INSERT INTO article
                (id, code, type, designation, prix_vente, prix_achat, taux_tva,
                 gere_stock, stock_alerte, actif, categorie_id, code_barre)
             VALUES (?1,?2,'bien',?3,?4,?5,?6,1,?7,1,?8,?9)",
            params![
                article_id, code, designation, prix_vente, prix_achat, taux_tva,
                stock_alerte, cat_ids[ri], code_barre,
            ],
        )?;

        // stock initial (entrée) : 1 à ~256 unités (quantite > 0 exigé par le schéma).
        let q = ((seq.wrapping_mul(40503)) & 0xFF) as f64 + 1.0;
        tx.execute(
            "INSERT INTO mouvement_stock
                (id, article_id, depot_id, document_id, sens, quantite, motif, date)
             VALUES (?1,?2,?3,NULL,'entree',?4,'inventaire',?5)",
            params![Uuid::new_v4().to_string(), article_id, depot_id, q, now],
        )?;
    }
    tx.commit()?;
    Ok(n)
}
