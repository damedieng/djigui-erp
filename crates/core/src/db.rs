//! Connexion SQLite et système de migrations versionnées.
//!
//! Rappel architecture (spec §2.1) : **seul le processus serveur écrit**. On
//! garde donc une seule connexion, sérialisée derrière un `Mutex` côté serveur.
//! Le mode WAL et les clés étrangères sont activés à chaque ouverture.

use crate::error::Result;
use rusqlite::Connection;
use std::path::Path;

/// Migrations embarquées dans le binaire, appliquées dans l'ordre du numéro.
/// Ajouter une migration = ajouter un fichier `NNNN_nom.sql` et une ligne ici.
/// On ne modifie JAMAIS une migration déjà publiée : on en ajoute une nouvelle.
const MIGRATIONS: &[Migration] = &[
    Migration {
        version: 1,
        nom: "initial",
        sql: include_str!("../migrations/0001_initial.sql"),
    },
    Migration {
        version: 2,
        nom: "categories",
        sql: include_str!("../migrations/0002_categories.sql"),
    },
    Migration {
        version: 3,
        nom: "sequences",
        sql: include_str!("../migrations/0003_sequences.sql"),
    },
    Migration {
        version: 4,
        nom: "article_image",
        sql: include_str!("../migrations/0004_article_image.sql"),
    },
    Migration {
        version: 5,
        nom: "article_code_barre",
        sql: include_str!("../migrations/0005_article_code_barre.sql"),
    },
    Migration {
        version: 6,
        nom: "taux_tva",
        sql: include_str!("../migrations/0006_taux_tva.sql"),
    },
    Migration {
        version: 7,
        nom: "taxes",
        sql: include_str!("../migrations/0007_taxes.sql"),
    },
    Migration {
        version: 8,
        nom: "abonnement_lignes",
        sql: include_str!("../migrations/0008_abonnement_lignes.sql"),
    },
    Migration {
        version: 9,
        nom: "dossier_objet_exoneration",
        sql: include_str!("../migrations/0009_dossier_objet_exoneration.sql"),
    },
    Migration {
        version: 10,
        nom: "abonnement_date_debut",
        sql: include_str!("../migrations/0010_abonnement_date_debut.sql"),
    },
    Migration {
        version: 11,
        nom: "utilisateurs",
        sql: include_str!("../migrations/0011_utilisateurs.sql"),
    },
    Migration {
        version: 12,
        nom: "audit",
        sql: include_str!("../migrations/0012_audit.sql"),
    },
    Migration {
        version: 13,
        nom: "categorie_image",
        sql: include_str!("../migrations/0013_categorie_image.sql"),
    },
    Migration {
        version: 14,
        nom: "seeder_catalogues",
        sql: include_str!("../migrations/0014_seeder_catalogues.sql"),
    },
    Migration {
        version: 15,
        nom: "session_caisse",
        sql: include_str!("../migrations/0015_session_caisse.sql"),
    },
    Migration {
        version: 16,
        nom: "inventaire",
        sql: include_str!("../migrations/0016_inventaire.sql"),
    },
    Migration {
        version: 17,
        nom: "caisse_depot",
        sql: include_str!("../migrations/0017_caisse_depot.sql"),
    },
    Migration {
        version: 18,
        nom: "moyens_paiement",
        sql: include_str!("../migrations/0018_moyens_paiement.sql"),
    },
    Migration {
        version: 19,
        nom: "document_annulation",
        sql: include_str!("../migrations/0019_document_annulation.sql"),
    },
    Migration {
        version: 20,
        nom: "rendez_vous",
        sql: include_str!("../migrations/0020_rendez_vous.sql"),
    },
    Migration {
        version: 21,
        nom: "projets",
        sql: include_str!("../migrations/0021_projets.sql"),
    },
    Migration {
        version: 22,
        nom: "projet_budget_ressources",
        sql: include_str!("../migrations/0022_projet_budget_ressources.sql"),
    },
    Migration {
        version: 23,
        nom: "tache_action",
        sql: include_str!("../migrations/0023_tache_action.sql"),
    },
    Migration {
        version: 24,
        nom: "assignation",
        sql: include_str!("../migrations/0024_assignation.sql"),
    },
    Migration {
        version: 25,
        nom: "intervenants",
        sql: include_str!("../migrations/0025_intervenants.sql"),
    },
    Migration {
        version: 26,
        nom: "intervenant_forfait",
        sql: include_str!("../migrations/0026_intervenant_forfait.sql"),
    },
    Migration {
        version: 27,
        nom: "identite_tiers_entreprise",
        sql: include_str!("../migrations/0027_identite_tiers_entreprise.sql"),
    },
    Migration {
        version: 28,
        nom: "jalons_livrables_documents",
        sql: include_str!("../migrations/0028_jalons_livrables_documents.sql"),
    },
    Migration {
        version: 29,
        nom: "dependances",
        sql: include_str!("../migrations/0029_dependances.sql"),
    },
    Migration {
        version: 30,
        nom: "notifications",
        sql: include_str!("../migrations/0030_notifications.sql"),
    },
    Migration {
        version: 31,
        nom: "production",
        sql: include_str!("../migrations/0031_production.sql"),
    },
    Migration {
        version: 32,
        nom: "nature_comptable_article",
        sql: include_str!("../migrations/0032_nature_comptable_article.sql"),
    },
    Migration {
        version: 33,
        nom: "reclassement_articles",
        sql: include_str!("../migrations/0033_reclassement_articles.sql"),
    },
    Migration {
        version: 34,
        nom: "comptabilite",
        sql: include_str!("../migrations/0034_comptabilite.sql"),
    },
    Migration {
        version: 35,
        nom: "prix_achat_estime",
        sql: include_str!("../migrations/0035_prix_achat_estime.sql"),
    },
    Migration {
        version: 36,
        nom: "valorisation_stock",
        sql: include_str!("../migrations/0036_valorisation_stock.sql"),
    },
    Migration {
        version: 37,
        nom: "marches",
        sql: include_str!("../migrations/0037_marches.sql"),
    },
    Migration {
        version: 38,
        nom: "marches_enchainement",
        sql: include_str!("../migrations/0038_marches_enchainement.sql"),
    },
    Migration {
        version: 39,
        nom: "marches_phases",
        sql: include_str!("../migrations/0039_marches_phases.sql"),
    },
    Migration {
        version: 40,
        nom: "modules",
        sql: include_str!("../migrations/0040_modules.sql"),
    },
    Migration {
        version: 41,
        nom: "modules_ouverture_existant",
        sql: include_str!("../migrations/0041_modules_ouverture_existant.sql"),
    },
    Migration {
        version: 42,
        nom: "sauvegarde",
        sql: include_str!("../migrations/0042_sauvegarde.sql"),
    },
    Migration {
        version: 43,
        nom: "retenue_source",
        sql: include_str!("../migrations/0043_retenue_source.sql"),
    },
    Migration {
        version: 44,
        nom: "paie_parametres",
        sql: include_str!("../migrations/0044_paie_parametres.sql"),
    },
    Migration {
        version: 45,
        nom: "paie_employes",
        sql: include_str!("../migrations/0045_paie_employes.sql"),
    },
];

struct Migration {
    version: i64,
    nom: &'static str,
    sql: &'static str,
}

/// Ouvre (ou crée) la base au chemin donné, active WAL + FK, puis migre.
pub fn open(path: impl AsRef<Path>) -> Result<Connection> {
    let conn = Connection::open(path)?;
    configurer(&conn)?;
    migrer(&conn)?;
    crate::modules::utilisateur::assurer_defaut(&conn)?;
    Ok(conn)
}

/// Base en mémoire — utile pour les tests.
pub fn open_in_memory() -> Result<Connection> {
    let conn = Connection::open_in_memory()?;
    configurer(&conn)?;
    migrer(&conn)?;
    crate::modules::utilisateur::assurer_defaut(&conn)?;
    Ok(conn)
}

fn configurer(conn: &Connection) -> Result<()> {
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "foreign_keys", "ON")?;
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(())
}

/// Applique toutes les migrations non encore enregistrées, chacune dans une
/// transaction. Idempotent : rejouer ne réapplique rien.
pub fn migrer(conn: &Connection) -> Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version    INTEGER PRIMARY KEY,
            nom        TEXT NOT NULL,
            applique_le TEXT NOT NULL
        );",
    )?;

    let courante: i64 = conn
        .query_row("SELECT COALESCE(MAX(version), 0) FROM schema_migrations", [], |r| r.get(0))?;

    for m in MIGRATIONS {
        if m.version <= courante {
            continue;
        }
        tracing::info!(version = m.version, nom = m.nom, "application migration");
        conn.execute_batch("BEGIN;")?;
        match conn.execute_batch(m.sql) {
            Ok(_) => {
                conn.execute(
                    "INSERT INTO schema_migrations (version, nom, applique_le) VALUES (?1, ?2, ?3)",
                    rusqlite::params![m.version, m.nom, crate::now()],
                )?;
                conn.execute_batch("COMMIT;")?;
            }
            Err(e) => {
                conn.execute_batch("ROLLBACK;")?;
                return Err(e.into());
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migration_cree_les_tables_et_est_idempotente() {
        let conn = open_in_memory().unwrap();
        // La config pilotée par la donnée doit être présente (§6.1).
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM config_type_document", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 6);
        // Rejouer les migrations ne casse rien.
        migrer(&conn).unwrap();
        let v: i64 = conn
            .query_row("SELECT MAX(version) FROM schema_migrations", [], |r| r.get(0))
            .unwrap();
        // Comparé au registre lui-même, et non à un numéro écrit à la main :
        // ce test doit vérifier que TOUTES les migrations déclarées se sont
        // appliquées, pas qu'on a pensé à incrémenter un chiffre ici.
        assert_eq!(v, MIGRATIONS.last().unwrap().version);
        // migration 0007 : les taxes reprennent les taux de TVA
        let nt: i64 = conn.query_row("SELECT COUNT(*) FROM taxe", [], |r| r.get(0)).unwrap();
        assert_eq!(nt, 3);
        // La migration 0002 a bien ajouté les catégories par défaut.
        let cats: i64 = conn
            .query_row("SELECT COUNT(*) FROM categorie", [], |r| r.get(0))
            .unwrap();
        assert_eq!(cats, 4);
    }
}
