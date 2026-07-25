//! Catégories d'articles (migration 0002). Table simple : classe les produits
//! pour le filtrage (caisse, articles). Un article peut rester non classé.

use crate::error::{CoreError, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Categorie {
    pub id: String,
    pub nom: String,
    /// Image illustrative (data-URI base64), optionnelle. Sert notamment au fond
    /// des tuiles de catégorie à la caisse.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
    /// Icône Tabler (ex. ti-bottle) — posée par le seeder ; repli côté UI sinon.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icone: Option<String>,
    /// Couleur d'accent (ex. #2563eb) — posée par le seeder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub couleur: Option<String>,
    /// Nombre d'articles classés dans cette catégorie (pour l'affichage/gestion).
    #[serde(default)]
    pub nb_articles: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouvelleCategorie {
    pub nom: String,
    /// Image optionnelle (data-URI). `None`/absent = pas d'image.
    #[serde(default)]
    pub image: Option<String>,
}

pub fn lister(conn: &Connection) -> Result<Vec<Categorie>> {
    let mut stmt = conn.prepare(
        "SELECT c.id, c.nom, c.image, c.icone, c.couleur,
                (SELECT COUNT(*) FROM article a WHERE a.categorie_id = c.id) AS nb
         FROM categorie c ORDER BY c.ordre, c.nom",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(Categorie {
            id: r.get(0)?, nom: r.get(1)?, image: r.get(2)?,
            icone: r.get(3)?, couleur: r.get(4)?, nb_articles: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn creer(conn: &Connection, c: &NouvelleCategorie) -> Result<Categorie> {
    let nom = c.nom.trim();
    if nom.is_empty() {
        return Err(CoreError::Rule("le nom de la catégorie est requis".into()));
    }
    let image = c.image.as_deref().filter(|s| !s.trim().is_empty());
    let id = Uuid::new_v4().to_string();
    conn.execute("INSERT INTO categorie (id, nom, image) VALUES (?1, ?2, ?3)",
                 params![id, nom, image])?;
    Ok(Categorie { id, nom: nom.to_string(), image: image.map(str::to_string), icone: None, couleur: None, nb_articles: 0 })
}

pub fn modifier(conn: &Connection, id: &str, c: &NouvelleCategorie) -> Result<Categorie> {
    let nom = c.nom.trim();
    if nom.is_empty() {
        return Err(CoreError::Rule("le nom de la catégorie est requis".into()));
    }
    let image = c.image.as_deref().filter(|s| !s.trim().is_empty());
    let n = conn.execute("UPDATE categorie SET nom = ?2, image = ?3 WHERE id = ?1",
                         params![id, nom, image])?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("catégorie {id}")));
    }
    let nb: i64 = conn.query_row(
        "SELECT COUNT(*) FROM article WHERE categorie_id = ?1", params![id], |r| r.get(0),
    )?;
    Ok(Categorie { id: id.to_string(), nom: nom.to_string(), image: image.map(str::to_string), icone: None, couleur: None, nb_articles: nb })
}

/// Supprime une catégorie. Les articles qui la référençaient repassent en
/// « non classé » (`categorie_id = NULL`) — on ne perd aucun article.
pub fn supprimer(conn: &Connection, id: &str) -> Result<()> {
    conn.execute("UPDATE article SET categorie_id = NULL WHERE categorie_id = ?1", params![id])?;
    let n = conn.execute("DELETE FROM categorie WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("catégorie {id}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn crud_categorie_et_suppression_declasse_articles() {
        let conn = db::open_in_memory().unwrap();
        let c = creer(&conn, &NouvelleCategorie { nom: "Test".into(), image: None }).unwrap();
        // rattache un article
        conn.execute(
            "INSERT INTO article (id, code, type, designation, prix_vente, taux_tva, gere_stock, actif, categorie_id)
             VALUES ('a1','A','bien','X',100,18,1,1,?1)", params![c.id],
        ).unwrap();
        assert_eq!(lister(&conn).unwrap().iter().find(|x| x.id == c.id).unwrap().nb_articles, 1);

        modifier(&conn, &c.id, &NouvelleCategorie { nom: "Modifié".into(), image: None }).unwrap();
        assert!(lister(&conn).unwrap().iter().any(|x| x.nom == "Modifié"));

        supprimer(&conn, &c.id).unwrap();
        // l'article existe toujours, mais déclassé
        let cat: Option<String> = conn.query_row(
            "SELECT categorie_id FROM article WHERE id='a1'", [], |r| r.get(0)).unwrap();
        assert_eq!(cat, None);
    }
}
