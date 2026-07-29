//! Articles & services (spec §5.2).
//!
//! Règle du pari §3.2 : un `service` ne gère jamais le stock (`gere_stock=false`),
//! garanti à l'écriture et par un CHECK du schéma. Le stock affiché est **dérivé
//! du journal** `mouvement_stock` (§3.3) : Σ(entrées) − Σ(sorties), jamais stocké.

use crate::domain::{NatureComptable, TypeArticle, TypeTaxe};
use crate::error::{CoreError, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Référence légère d'une taxe appliquée à un article (multi-taxes, §migration 0007).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxeRef {
    pub id: String,
    pub nom: String,
    pub taux: f64,
    pub r#type: TypeTaxe,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Article {
    pub id: String,
    pub code: String,
    pub r#type: TypeArticle,
    /// Nature comptable OHADA (migration 0032) : décide où l'article apparaît
    /// (caisse ou recettes) et quels comptes seront employés.
    pub nature_comptable: NatureComptable,
    pub designation: String,
    pub prix_vente: f64,
    pub prix_achat: Option<f64>,
    /// Vrai quand `prix_achat` est une **estimation** posée par Djigui faute de
    /// vrai prix (migration 0035), et non un chiffre saisi par le commerçant.
    /// Un chiffre inventé sans étiquette est plus dangereux qu'une case vide :
    /// l'écran doit toujours le dire.
    #[serde(default)]
    pub prix_achat_estime: bool,
    pub taux_tva: f64,
    pub gere_stock: bool,
    pub stock_alerte: Option<f64>,
    pub actif: bool,
    pub categorie_id: Option<String>,
    /// Nom de la catégorie (joint), pour l'affichage sans second appel.
    pub categorie_nom: Option<String>,
    /// Image du produit (data-URI base64), optionnelle.
    pub image: Option<String>,
    /// Code-barres (EAN/UPC) pour le scan et la recherche en caisse, optionnel.
    pub code_barre: Option<String>,
    /// Taxes appliquées à l'article (0..n). Utilisées pour construire les lignes.
    #[serde(default)]
    pub taxes: Vec<TaxeRef>,
    /// Stock courant tous dépôts confondus (Σ journal). `None` si `gere_stock=false`.
    pub stock: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouvelArticle {
    pub code: String,
    pub r#type: TypeArticle,
    /// Absente = on déduit du type (service → service, sinon marchandise), ce qui
    /// garde compatibles les appels qui ignorent encore ce champ.
    #[serde(default)]
    pub nature_comptable: Option<NatureComptable>,
    pub designation: String,
    #[serde(default)]
    pub prix_vente: f64,
    pub prix_achat: Option<f64>,
    #[serde(default)]
    pub taux_tva: f64,
    #[serde(default)]
    pub gere_stock: bool,
    pub stock_alerte: Option<f64>,
    pub categorie_id: Option<String>,
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub code_barre: Option<String>,
    /// Ids des taxes à appliquer (multi-taxes). Si absent, on ne touche pas aux liens.
    #[serde(default)]
    pub taxes: Option<Vec<String>>,
}

/// Filtre de la liste, aligné sur les chips de la maquette articles.html.
#[derive(Debug, Clone, Copy, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum Filtre {
    #[default]
    Tous,
    Biens,
    Services,
    EnRupture,
    /// Ce qui se **vend** : marchandises, produits fabriqués, services.
    /// Une matière première n'a rien à faire à la caisse.
    Vendables,
    /// Ce qui se **consomme** en fabrication : matières premières et
    /// marchandises (le sac de ciment qu'on revend entier *et* qu'on
    /// reconditionne). Un produit fini reste utilisable via `Tous` si l'on
    /// fabrique en plusieurs étapes.
    Composants,
}

/// Nature comptable à la **création** : celle demandée, sinon déduite du type.
/// Un service ne peut être que de nature `service` (il n'est ni stocké ni
/// transformé) — on corrige silencieusement plutôt que de refuser.
fn nature_de(a: &NouvelArticle) -> NatureComptable {
    nature_maj(a).unwrap_or(NatureComptable::Marchandise)
}

/// Nature comptable à la **modification**. `None` = « ne touche pas au
/// classement existant ».
///
/// ⚠️ Indispensable : le classement est calculé automatiquement par la
/// production (une farine mise dans une recette devient matière première). Si
/// une simple modification de prix depuis l'écran Articles — qui n'envoie pas ce
/// champ — remettait la nature à `marchandise`, la farine réapparaîtrait à la
/// caisse sans que personne ne comprenne pourquoi.
fn nature_maj(a: &NouvelArticle) -> Option<NatureComptable> {
    if a.r#type == TypeArticle::Service {
        return Some(NatureComptable::Service);
    }
    match a.nature_comptable {
        None => None,
        // `service` est incohérent avec un bien : on retombe sur le défaut.
        Some(NatureComptable::Service) => Some(NatureComptable::Marchandise),
        Some(n) => Some(n),
    }
}

pub fn creer(conn: &Connection, a: &NouvelArticle) -> Result<Article> {
    // Garde-fou du pari §3.2 : service ⇒ pas de gestion de stock.
    let gere_stock = a.gere_stock && a.r#type == TypeArticle::Bien;
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO article
            (id, code, type, designation, prix_vente, prix_achat, taux_tva,
             gere_stock, stock_alerte, actif, categorie_id, image, code_barre,
             nature_comptable)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10, ?11, ?12, ?13)",
        params![
            id, a.code, a.r#type, a.designation, a.prix_vente, a.prix_achat,
            a.taux_tva, gere_stock as i64, a.stock_alerte, a.categorie_id, a.image, a.code_barre,
            nature_de(a),
        ],
    )?;
    if let Some(taxes) = &a.taxes {
        ecrire_taxes(conn, &id, taxes)?;
    }
    lire(conn, &id)
}

/// Remplace les taxes liées à un article.
fn ecrire_taxes(conn: &Connection, article_id: &str, taxe_ids: &[String]) -> Result<()> {
    conn.execute("DELETE FROM article_taxe WHERE article_id = ?1", params![article_id])?;
    for tid in taxe_ids {
        conn.execute(
            "INSERT OR IGNORE INTO article_taxe (article_id, taxe_id) VALUES (?1, ?2)",
            params![article_id, tid],
        )?;
    }
    Ok(())
}

/// Lit les taxes d'un article (jointure article_taxe → taxe active).
fn taxes_de(conn: &Connection, article_id: &str) -> Result<Vec<TaxeRef>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.nom, t.taux, t.type FROM article_taxe at
         JOIN taxe t ON t.id = at.taxe_id AND t.actif = 1
         WHERE at.article_id = ?1 ORDER BY t.taux DESC",
    )?;
    let rows = stmt.query_map(params![article_id], |r| {
        let ty: String = r.get(3)?;
        Ok(TaxeRef {
            id: r.get(0)?, nom: r.get(1)?, taux: r.get(2)?,
            r#type: TypeTaxe::parse(&ty).unwrap_or(TypeTaxe::Pourcentage),
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Modifie un article existant (CRUD complet). Conserve le garde-fou §3.2.
pub fn modifier(conn: &Connection, id: &str, a: &NouvelArticle) -> Result<Article> {
    // s'assure que l'article existe (message d'erreur propre sinon)
    lire(conn, id)?;
    let gere_stock = a.gere_stock && a.r#type == TypeArticle::Bien;
    let n = conn.execute(
        "UPDATE article SET
            code = ?2, type = ?3, designation = ?4, prix_vente = ?5, prix_achat = ?6,
            taux_tva = ?7, gere_stock = ?8, stock_alerte = ?9, categorie_id = ?10,
            image = ?11, code_barre = ?12,
            nature_comptable = COALESCE(?13, nature_comptable),
            -- Dès que le commerçant saisit un prix d'achat, ce n'est plus une
            -- estimation de Djigui : le badge « prix estimé » doit disparaître.
            prix_achat_estime = CASE WHEN COALESCE(?6, 0) > 0 THEN 0 ELSE prix_achat_estime END
         WHERE id = ?1",
        params![
            id, a.code, a.r#type, a.designation, a.prix_vente, a.prix_achat,
            a.taux_tva, gere_stock as i64, a.stock_alerte, a.categorie_id, a.image, a.code_barre,
            nature_maj(a),
        ],
    )?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("article {id}")));
    }
    if let Some(taxes) = &a.taxes {
        ecrire_taxes(conn, id, taxes)?;
    }
    lire(conn, id)
}

/// Désactive un article (soft delete : `actif = 0`). On ne supprime jamais un
/// article référencé par des documents/mouvements — on le rend inactif.
pub fn desactiver(conn: &Connection, id: &str) -> Result<()> {
    let n = conn.execute("UPDATE article SET actif = 0 WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("article {id}")));
    }
    Ok(())
}

/// Un article est **supprimable définitivement** seulement s'il n'a **aucune
/// histoire** : jamais employé dans un document, et aucun mouvement de stock
/// (ce qui couvre aussi « stock ≠ 0 »). Bonne pratique : on ne détruit jamais
/// une donnée référencée par la compta ou l'inventaire.
pub fn est_supprimable(conn: &Connection, id: &str) -> Result<bool> {
    let dans_doc: bool = conn
        .query_row("SELECT 1 FROM document_ligne WHERE article_id = ?1 LIMIT 1", params![id], |_| Ok(true))
        .optional()?
        .unwrap_or(false);
    let a_du_stock: bool = conn
        .query_row("SELECT 1 FROM mouvement_stock WHERE article_id = ?1 LIMIT 1", params![id], |_| Ok(true))
        .optional()?
        .unwrap_or(false);
    Ok(!dans_doc && !a_du_stock)
}

/// Supprime **définitivement** un article (et ses taxes) — seulement s'il est
/// supprimable. Sinon renvoie une erreur métier (l'appelant peut désactiver).
pub fn supprimer(conn: &Connection, id: &str) -> Result<()> {
    if !est_supprimable(conn, id)? {
        return Err(CoreError::Rule(
            "article utilisé (documents ou stock) : suppression impossible, désactivez-le".into(),
        ));
    }
    conn.execute("DELETE FROM article_taxe WHERE article_id = ?1", params![id])?;
    let n = conn.execute("DELETE FROM article WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("article {id}")));
    }
    Ok(())
}

/// Résultat d'un traitement par lot de suppression.
#[derive(Debug, Serialize)]
pub struct ResultatSuppressionLot {
    pub supprimes: usize,
    pub archives: usize,
}

/// Suppression par lot « intelligente » : supprime définitivement les articles
/// sans histoire, **archive (désactive)** les autres. Retourne le compte-rendu.
pub fn supprimer_lot(conn: &Connection, ids: &[String]) -> Result<ResultatSuppressionLot> {
    let tx = conn.unchecked_transaction()?;
    let mut supprimes = 0;
    let mut archives = 0;
    for id in ids {
        if est_supprimable(&tx, id)? {
            tx.execute("DELETE FROM article_taxe WHERE article_id = ?1", params![id])?;
            tx.execute("DELETE FROM article WHERE id = ?1", params![id])?;
            supprimes += 1;
        } else {
            tx.execute("UPDATE article SET actif = 0 WHERE id = ?1", params![id])?;
            archives += 1;
        }
    }
    tx.commit()?;
    Ok(ResultatSuppressionLot { supprimes, archives })
}

/// Désactive (archive) un lot d'articles. Toujours autorisé.
pub fn desactiver_lot(conn: &Connection, ids: &[String]) -> Result<usize> {
    let tx = conn.unchecked_transaction()?;
    let mut n = 0;
    for id in ids {
        n += tx.execute("UPDATE article SET actif = 0 WHERE id = ?1", params![id])?;
    }
    tx.commit()?;
    Ok(n)
}

/// Affecte une catégorie (ou la retire si `None`) à un lot d'articles.
pub fn affecter_categorie_lot(conn: &Connection, ids: &[String], categorie_id: Option<&str>) -> Result<usize> {
    let tx = conn.unchecked_transaction()?;
    let mut n = 0;
    for id in ids {
        n += tx.execute(
            "UPDATE article SET categorie_id = ?2 WHERE id = ?1",
            params![id, categorie_id],
        )?;
    }
    tx.commit()?;
    Ok(n)
}

pub fn lire(conn: &Connection, id: &str) -> Result<Article> {
    let mut a = conn.query_row(
        &format!("{BASE_SELECT} WHERE a.id = ?1"),
        params![id],
        ligne_vers_article,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => CoreError::NotFound(format!("article {id}")),
        autre => autre.into(),
    })?;
    a.taxes = taxes_de(conn, id)?;
    Ok(a)
}

/// Clause WHERE (sans le mot-clé) correspondant à un filtre de chips.
fn clause_filtre(filtre: Filtre) -> &'static str {
    match filtre {
        Filtre::Tous => "a.actif = 1",
        Filtre::Biens => "a.actif = 1 AND a.type = 'bien'",
        Filtre::Services => "a.actif = 1 AND a.type = 'service'",
        // en rupture : gère le stock et stock <= seuil d'alerte (ou <= 0 sans seuil)
        Filtre::EnRupture => {
            "a.actif = 1 AND a.gere_stock = 1 \
             AND stock <= COALESCE(a.stock_alerte, 0)"
        }
        Filtre::Vendables => {
            "a.actif = 1 AND a.nature_comptable IN ('marchandise','produit_fini','service')"
        }
        Filtre::Composants => {
            "a.actif = 1 AND a.nature_comptable IN ('matiere_premiere','marchandise','produit_fini')"
        }
    }
}

pub fn lister(conn: &Connection, filtre: Filtre) -> Result<Vec<Article>> {
    let sql = format!("{BASE_SELECT} WHERE {} ORDER BY a.designation", clause_filtre(filtre));
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map([], ligne_vers_article)?;
    let mut arts = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    for a in &mut arts {
        a.taxes = taxes_de(conn, &a.id)?;
    }
    Ok(arts)
}

/// Requête de liste paginée avec recherche serveur (§5.2, échelle « supermarché »).
#[derive(Debug, Clone, Default, Deserialize)]
pub struct RequeteListe {
    #[serde(default)]
    pub filtre: Filtre,
    /// Recherche libre sur désignation / code / code-barres (LIKE insensible à la casse).
    #[serde(default)]
    pub recherche: Option<String>,
    /// Taille de page (défaut 50, borné à 200). `None` conserve le défaut.
    pub limite: Option<i64>,
    #[serde(default)]
    pub offset: Option<i64>,
}

/// Page de résultats + total pour piloter le pager côté UI.
#[derive(Debug, Serialize)]
pub struct PageArticles {
    pub items: Vec<Article>,
    pub total: i64,
    pub limite: i64,
    pub offset: i64,
}

pub fn lister_page(conn: &Connection, req: &RequeteListe) -> Result<PageArticles> {
    use rusqlite::types::Value;

    let mut where_ = clause_filtre(req.filtre).to_string();
    let mut binds: Vec<Value> = Vec::new();
    if let Some(q) = req.recherche.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        where_.push_str(" AND (a.designation LIKE ? OR a.code LIKE ? OR a.code_barre LIKE ?)");
        let motif = Value::Text(format!("%{q}%"));
        binds.push(motif.clone());
        binds.push(motif.clone());
        binds.push(motif);
    }

    // Total (enveloppe le SELECT pour que l'alias `stock` soit disponible).
    let sql_total = format!("SELECT COUNT(*) FROM ({BASE_SELECT} WHERE {where_})");
    let total: i64 = conn.query_row(
        &sql_total,
        rusqlite::params_from_iter(binds.iter()),
        |r| r.get(0),
    )?;

    let limite = req.limite.unwrap_or(50).clamp(1, 200);
    let offset = req.offset.unwrap_or(0).max(0);
    let mut binds_page = binds.clone();
    binds_page.push(Value::Integer(limite));
    binds_page.push(Value::Integer(offset));

    let sql = format!(
        "{BASE_SELECT} WHERE {where_} ORDER BY a.designation LIMIT ? OFFSET ?"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(rusqlite::params_from_iter(binds_page.iter()), ligne_vers_article)?;
    let mut items = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    for a in &mut items {
        a.taxes = taxes_de(conn, &a.id)?;
    }
    Ok(PageArticles { items, total, limite, offset })
}

/// SELECT commun : calcule le stock depuis le journal (§3.3) en sous-requête.
/// Le stock reste `NULL` tant que `gere_stock=0` (services et biens non suivis).
const BASE_SELECT: &str = "
    SELECT a.id, a.code, a.type, a.designation, a.prix_vente, a.prix_achat,
           a.taux_tva, a.gere_stock, a.stock_alerte, a.actif,
           a.categorie_id, c.nom AS categorie_nom, a.image, a.code_barre,
           a.nature_comptable, a.prix_achat_estime,
           CASE WHEN a.gere_stock = 1 THEN (
               SELECT COALESCE(SUM(CASE WHEN m.sens='entree' THEN m.quantite
                                        ELSE -m.quantite END), 0)
               FROM mouvement_stock m WHERE m.article_id = a.id
           ) END AS stock
    FROM article a
    LEFT JOIN categorie c ON c.id = a.categorie_id";

fn ligne_vers_article(r: &rusqlite::Row) -> rusqlite::Result<Article> {
    let t: String = r.get(2)?;
    let nat: String = r.get(14)?;
    Ok(Article {
        id: r.get(0)?,
        code: r.get(1)?,
        r#type: TypeArticle::parse(&t).unwrap_or(TypeArticle::Bien),
        nature_comptable: NatureComptable::parse(&nat).unwrap_or(NatureComptable::Marchandise),
        designation: r.get(3)?,
        prix_vente: r.get(4)?,
        prix_achat: r.get(5)?,
        prix_achat_estime: r.get::<_, i64>(15)? != 0,
        taux_tva: r.get(6)?,
        gere_stock: r.get::<_, i64>(7)? != 0,
        stock_alerte: r.get(8)?,
        actif: r.get::<_, i64>(9)? != 0,
        categorie_id: r.get(10)?,
        categorie_nom: r.get(11)?,
        image: r.get(12)?,
        code_barre: r.get(13)?,
        taxes: Vec::new(), // rempli séparément (jointure) par lire/lister
        stock: r.get(16)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn service_ne_gere_jamais_le_stock() {
        let conn = db::open_in_memory().unwrap();
        let a = creer(&conn, &NouvelArticle {
            code: "SRV-01".into(),
            r#type: TypeArticle::Service,
            designation: "Livraison".into(),
            prix_vente: 1500.0,
            prix_achat: None,
            taux_tva: 0.0,
            gere_stock: true, // demandé mais doit être forcé à false
            stock_alerte: None,
            categorie_id: None, image: None, code_barre: None, taxes: None, nature_comptable: None,
        }).unwrap();
        assert!(!a.gere_stock);
        assert_eq!(a.stock, None);
    }

    #[test]
    fn stock_derive_du_journal() {
        let conn = db::open_in_memory().unwrap();
        let a = creer(&conn, &NouvelArticle {
            code: "ART-01".into(),
            r#type: TypeArticle::Bien,
            designation: "Riz 1kg".into(),
            prix_vente: 650.0, prix_achat: Some(500.0), taux_tva: 18.0,
            gere_stock: true, stock_alerte: Some(10.0), categorie_id: None, image: None, code_barre: None, taxes: None, nature_comptable: None,
        }).unwrap();
        assert_eq!(a.stock, Some(0.0));

        // dépôt + mouvements directement dans le journal
        conn.execute("INSERT INTO depot (id, nom, par_defaut) VALUES ('d1','Principal',1)", []).unwrap();
        for (sens, q) in [("entree", 100.0), ("sortie", 30.0)] {
            conn.execute(
                "INSERT INTO mouvement_stock (id, article_id, depot_id, sens, quantite, motif, date)
                 VALUES (?1, ?2, 'd1', ?3, ?4, 'inventaire', '2026-07-21')",
                params![Uuid::new_v4().to_string(), a.id, sens, q],
            ).unwrap();
        }
        let a = lire(&conn, &a.id).unwrap();
        assert_eq!(a.stock, Some(70.0));

        // filtre rupture : 70 > seuil 10 ⇒ absent
        assert!(lister(&conn, Filtre::EnRupture).unwrap().is_empty());
    }

    #[test]
    fn modifier_puis_desactiver() {
        let conn = db::open_in_memory().unwrap();
        let a = creer(&conn, &NouvelArticle {
            code: "ART-9".into(), r#type: TypeArticle::Bien, designation: "Sucre".into(),
            prix_vente: 1000.0, prix_achat: Some(800.0), taux_tva: 18.0,
            gere_stock: true, stock_alerte: None, categorie_id: None, image: None, code_barre: None, taxes: None, nature_comptable: None,
        }).unwrap();

        let m = modifier(&conn, &a.id, &NouvelArticle {
            code: "ART-9".into(), r#type: TypeArticle::Bien, designation: "Sucre 1kg".into(),
            prix_vente: 1100.0, prix_achat: Some(850.0), taux_tva: 18.0,
            gere_stock: true, stock_alerte: Some(5.0), categorie_id: None, image: None, code_barre: None, taxes: None, nature_comptable: None,
        }).unwrap();
        assert_eq!(m.designation, "Sucre 1kg");
        assert_eq!(m.prix_vente, 1100.0);

        desactiver(&conn, &a.id).unwrap();
        // désactivé ⇒ absent de la liste (qui ne montre que les actifs)
        assert!(lister(&conn, Filtre::Tous).unwrap().is_empty());
    }
}
