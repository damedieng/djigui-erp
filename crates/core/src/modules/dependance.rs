//! Liens de précédence entre activités — les « flèches » du Gantt (mig. 0029).
//!
//! ⚠️ **Règle décidée avec l'utilisateur** : la propagation en cascade existe,
//! mais elle n'est **jamais automatique**. On détecte les incohérences
//! (`violations`), on propose un aperçu (`plan_harmonisation`), et les dates ne
//! bougent que si l'utilisateur applique explicitement (`harmoniser`).
//!
//! v1 : seul le lien **fin → début** est calculé. Les autres types sont stockés
//! mais traités comme fin → début tant que le besoin n'est pas confirmé.

use crate::error::{CoreError, Result};
use crate::now;
use chrono::{Duration, NaiveDate};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize)]
pub struct Dependance {
    pub id: String,
    /// Le successeur.
    pub tache_id: String,
    pub tache_nom: String,
    /// Le prédécesseur : celui qui doit finir avant.
    pub predecesseur_id: String,
    pub predecesseur_nom: String,
    pub r#type: String,
    pub decalage: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouvelleDependance {
    pub tache_id: String,
    pub predecesseur_id: String,
    #[serde(default)]
    pub decalage: i64,
}

const DEP_COLS: &str = "SELECT d.id, d.tache_id, d.predecesseur_id, d.type, d.decalage,
        s.nom AS succ_nom, p.nom AS pred_nom
     FROM dependance d
     JOIN tache s ON s.id = d.tache_id
     JOIN tache p ON p.id = d.predecesseur_id";

fn ligne_dep(r: &rusqlite::Row) -> rusqlite::Result<Dependance> {
    Ok(Dependance {
        id: r.get(0)?,
        tache_id: r.get(1)?,
        predecesseur_id: r.get(2)?,
        r#type: r.get(3)?,
        decalage: r.get(4)?,
        tache_nom: r.get(5)?,
        predecesseur_nom: r.get(6)?,
    })
}

pub fn lister(conn: &Connection, projet_id: &str) -> Result<Vec<Dependance>> {
    let sql = format!("{DEP_COLS} WHERE s.projet_id = ?1 ORDER BY s.ordre");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![projet_id], ligne_dep)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn lire(conn: &Connection, id: &str) -> Result<Dependance> {
    let sql = format!("{DEP_COLS} WHERE d.id = ?1");
    conn.query_row(&sql, params![id], ligne_dep).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => CoreError::NotFound(format!("dépendance {id}")),
        autre => autre.into(),
    })
}

/// Crée un lien. Refuse les vrais non-sens : une tâche liée à elle-même, un
/// cycle, ou deux activités de projets différents. Ce sont des invariants, pas
/// des préférences — un cycle rendrait l'harmonisation impossible.
pub fn creer(conn: &Connection, n: &NouvelleDependance) -> Result<Dependance> {
    if n.tache_id == n.predecesseur_id {
        return Err(CoreError::Rule(
            "une activité ne peut pas dépendre d'elle-même".into(),
        ));
    }
    let projet = |id: &str| -> Result<String> {
        conn.query_row("SELECT projet_id FROM tache WHERE id = ?1", params![id], |r| r.get(0))
            .map_err(|_| CoreError::NotFound(format!("activité {id}")))
    };
    if projet(&n.tache_id)? != projet(&n.predecesseur_id)? {
        return Err(CoreError::Rule(
            "les deux activités doivent appartenir au même projet".into(),
        ));
    }
    // Anti-cycle : le prédécesseur ne doit pas déjà dépendre (directement ou
    // non) du successeur.
    if depend_de(conn, &n.predecesseur_id, &n.tache_id)? {
        return Err(CoreError::Rule(
            "ce lien créerait une boucle : ces deux activités dépendraient l'une de l'autre".into(),
        ));
    }
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO dependance (id, tache_id, predecesseur_id, type, decalage, cree_le)
         VALUES (?1,?2,?3,'fin_debut',?4,?5)",
        params![id, n.tache_id, n.predecesseur_id, n.decalage, now()],
    )
    .map_err(|e| match e {
        rusqlite::Error::SqliteFailure(err, _) if err.extended_code == 2067 => {
            CoreError::Rule("ce lien existe déjà".into())
        }
        autre => autre.into(),
    })?;
    lire(conn, &id)
}

/// `depart` dépend-il de `cible`, directement ou en remontant la chaîne ?
fn depend_de(conn: &Connection, depart: &str, cible: &str) -> Result<bool> {
    let mut a_voir = vec![depart.to_string()];
    let mut vus = std::collections::HashSet::new();
    while let Some(courant) = a_voir.pop() {
        if courant == cible {
            return Ok(true);
        }
        if !vus.insert(courant.clone()) {
            continue;
        }
        let mut stmt = conn.prepare("SELECT predecesseur_id FROM dependance WHERE tache_id = ?1")?;
        let rows = stmt.query_map(params![courant], |r| r.get::<_, String>(0))?;
        for p in rows {
            a_voir.push(p?);
        }
    }
    Ok(false)
}

pub fn supprimer(conn: &Connection, id: &str) -> Result<()> {
    let nb = conn.execute("DELETE FROM dependance WHERE id = ?1", params![id])?;
    if nb == 0 {
        return Err(CoreError::NotFound(format!("dépendance {id}")));
    }
    Ok(())
}

// ===========================================================================
// Cohérence : détection et harmonisation
// ===========================================================================

/// Un lien non respecté : le successeur démarre trop tôt.
#[derive(Debug, Clone, Serialize)]
pub struct Violation {
    pub dependance_id: String,
    pub tache_id: String,
    pub tache_nom: String,
    pub predecesseur_id: String,
    pub predecesseur_nom: String,
    /// Début actuel du successeur.
    pub debut_actuel: String,
    /// Début qu'il devrait avoir : fin du prédécesseur + 1 jour + décalage.
    pub debut_attendu: String,
    /// Nombre de jours de décalage à rattraper.
    pub jours: i64,
}

/// Une date qui changerait si l'utilisateur applique l'harmonisation.
#[derive(Debug, Clone, Serialize)]
pub struct Changement {
    pub tache_id: String,
    pub tache_nom: String,
    pub debut_avant: String,
    pub debut_apres: String,
    pub fin_avant: String,
    pub fin_apres: String,
    pub jours: i64,
}

/// Dates d'une activité *feuille* (les parents sont calculés, on n'y touche pas).
struct Bornes {
    nom: String,
    debut: NaiveDate,
    fin: NaiveDate,
    a_enfants: bool,
}

fn jour(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.get(0..10)?, "%Y-%m-%d").ok()
}

fn charger_bornes(conn: &Connection, projet_id: &str) -> Result<HashMap<String, Bornes>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.nom, t.date_debut_prevue, t.date_fin_prevue,
                EXISTS (SELECT 1 FROM tache c WHERE c.tache_parente_id = t.id)
         FROM tache t WHERE t.projet_id = ?1",
    )?;
    let rows = stmt.query_map(params![projet_id], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, Option<String>>(2)?,
            r.get::<_, Option<String>>(3)?,
            r.get::<_, i64>(4)? != 0,
        ))
    })?;
    let mut map = HashMap::new();
    for l in rows {
        let (id, nom, d, f, a_enfants) = l?;
        // Sans dates saisies, l'activité ne participe pas à l'harmonisation :
        // on n'invente pas un planning à la place de l'utilisateur.
        if let (Some(d), Some(f)) = (d.as_deref().and_then(jour), f.as_deref().and_then(jour)) {
            map.insert(id, Bornes { nom, debut: d, fin: f, a_enfants });
        }
    }
    Ok(map)
}

/// Liens actuellement non respectés. **Signalement seulement**, rien n'est modifié.
pub fn violations(conn: &Connection, projet_id: &str) -> Result<Vec<Violation>> {
    let bornes = charger_bornes(conn, projet_id)?;
    let mut out = Vec::new();
    for d in lister(conn, projet_id)? {
        let (Some(s), Some(p)) = (bornes.get(&d.tache_id), bornes.get(&d.predecesseur_id)) else {
            continue; // dates incomplètes : rien à dire
        };
        let attendu = p.fin + Duration::days(1 + d.decalage);
        if s.debut < attendu {
            out.push(Violation {
                dependance_id: d.id,
                tache_id: d.tache_id.clone(),
                tache_nom: s.nom.clone(),
                predecesseur_id: d.predecesseur_id.clone(),
                predecesseur_nom: p.nom.clone(),
                debut_actuel: s.debut.to_string(),
                debut_attendu: attendu.to_string(),
                jours: (attendu - s.debut).num_days(),
            });
        }
    }
    Ok(out)
}

/// Calcule le décalage à appliquer, **sans rien écrire**. C'est l'aperçu montré
/// à l'utilisateur avant qu'il ne valide.
///
/// Chaque successeur en retard est poussé après son prédécesseur, en conservant
/// sa durée. La propagation est répétée jusqu'à stabilité (une chaîne A→B→C se
/// résout en cascade), avec une borne de sécurité contre les surprises.
pub fn plan_harmonisation(conn: &Connection, projet_id: &str) -> Result<Vec<Changement>> {
    let bornes = charger_bornes(conn, projet_id)?;
    let deps = lister(conn, projet_id)?;
    // Dates de travail, modifiées en mémoire uniquement.
    let mut dates: HashMap<String, (NaiveDate, NaiveDate)> = bornes
        .iter()
        .filter(|(_, b)| !b.a_enfants) // on ne déplace que les feuilles
        .map(|(id, b)| (id.clone(), (b.debut, b.fin)))
        .collect();

    // Le nombre de passes est borné par la longueur de la plus longue chaîne ;
    // l'anti-cycle de `creer` garantit qu'on converge.
    let max_passes = deps.len() + 1;
    for _ in 0..max_passes {
        let mut bouge = false;
        for d in &deps {
            let (Some(&(_, fin_p)), Some(&(deb_s, fin_s))) =
                (dates.get(&d.predecesseur_id), dates.get(&d.tache_id))
            else {
                continue;
            };
            let attendu = fin_p + Duration::days(1 + d.decalage);
            if deb_s < attendu {
                let duree = fin_s - deb_s;
                dates.insert(d.tache_id.clone(), (attendu, attendu + duree));
                bouge = true;
            }
        }
        if !bouge {
            break;
        }
    }

    let mut out: Vec<Changement> = dates
        .iter()
        .filter_map(|(id, &(d, f))| {
            let b = bornes.get(id)?;
            if b.debut == d && b.fin == f {
                return None;
            }
            Some(Changement {
                tache_id: id.clone(),
                tache_nom: b.nom.clone(),
                debut_avant: b.debut.to_string(),
                debut_apres: d.to_string(),
                fin_avant: b.fin.to_string(),
                fin_apres: f.to_string(),
                jours: (d - b.debut).num_days(),
            })
        })
        .collect();
    out.sort_by(|a, b| a.debut_apres.cmp(&b.debut_apres));
    Ok(out)
}

/// Applique l'harmonisation. **Jamais appelé automatiquement** : uniquement sur
/// action explicite de l'utilisateur, après qu'il a vu l'aperçu.
pub fn harmoniser(conn: &Connection, projet_id: &str) -> Result<Vec<Changement>> {
    let plan = plan_harmonisation(conn, projet_id)?;
    for c in &plan {
        conn.execute(
            "UPDATE tache SET date_debut_prevue = ?2, date_fin_prevue = ?3 WHERE id = ?1",
            params![c.tache_id, c.debut_apres, c.fin_apres],
        )?;
    }
    Ok(plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::modules::projet::{self, NouveauProjet, NouvelleTache};

    fn projet_test(conn: &Connection) -> String {
        projet::creer(conn, &NouveauProjet {
            nom: "Chantier".into(), client_id: None, chef_de_projet_id: None,
            date_debut_prevue: Some("2026-08-01".into()),
            date_fin_prevue: Some("2026-12-31".into()),
            date_debut_reelle: None, date_fin_reelle: None, statut: None,
            budget_global: 0.0, note: None,
        }, Some("u1")).unwrap().id
    }

    fn tache(conn: &Connection, p: &str, nom: &str, d: &str, f: &str) -> String {
        projet::creer_tache(conn, &NouvelleTache {
            projet_id: p.into(), tache_parente_id: None, nom: nom.into(),
            description: None, date_debut_prevue: Some(d.into()), date_fin_prevue: Some(f.into()),
            date_debut_reelle: None, date_fin_reelle: None, statut: None,
            avancement: None, budget: 0.0,
        }).unwrap().id
    }

    #[test]
    fn lien_viole_est_signale_puis_harmonise_en_cascade() {
        let conn = db::open_in_memory().unwrap();
        let p = projet_test(&conn);
        // A finit le 10, B démarre le 5 (trop tôt), C démarre le 8 (trop tôt).
        let a = tache(&conn, &p, "A", "2026-08-01", "2026-08-10");
        let b = tache(&conn, &p, "B", "2026-08-05", "2026-08-09"); // durée 4 j
        let c = tache(&conn, &p, "C", "2026-08-08", "2026-08-12");
        creer(&conn, &NouvelleDependance { tache_id: b.clone(), predecesseur_id: a.clone(), decalage: 0 }).unwrap();
        creer(&conn, &NouvelleDependance { tache_id: c.clone(), predecesseur_id: b.clone(), decalage: 0 }).unwrap();

        // Signalement : deux liens non respectés, aucune date touchée.
        assert_eq!(violations(&conn, &p).unwrap().len(), 2);
        let avant = projet::lire_tache(&conn, &b).unwrap();
        assert_eq!(avant.date_debut_prevue.as_deref(), Some("2026-08-05"));

        // Aperçu : ne modifie toujours rien.
        let plan = plan_harmonisation(&conn, &p).unwrap();
        assert_eq!(plan.len(), 2);
        assert_eq!(projet::lire_tache(&conn, &b).unwrap().date_debut_prevue.as_deref(), Some("2026-08-05"));

        // Application explicite : B passe au 11 (durée conservée), C suit.
        harmoniser(&conn, &p).unwrap();
        let nb = projet::lire_tache(&conn, &b).unwrap();
        assert_eq!(nb.date_debut_prevue.as_deref(), Some("2026-08-11"));
        assert_eq!(nb.date_fin_prevue.as_deref(), Some("2026-08-15"), "durée de 4 jours conservée");
        let nc = projet::lire_tache(&conn, &c).unwrap();
        assert_eq!(nc.date_debut_prevue.as_deref(), Some("2026-08-16"), "cascade appliquée à C");
        assert!(violations(&conn, &p).unwrap().is_empty());
    }

    #[test]
    fn boucle_et_auto_dependance_refusees() {
        let conn = db::open_in_memory().unwrap();
        let p = projet_test(&conn);
        let a = tache(&conn, &p, "A", "2026-08-01", "2026-08-05");
        let b = tache(&conn, &p, "B", "2026-08-06", "2026-08-10");

        assert!(creer(&conn, &NouvelleDependance {
            tache_id: a.clone(), predecesseur_id: a.clone(), decalage: 0 }).is_err());

        creer(&conn, &NouvelleDependance {
            tache_id: b.clone(), predecesseur_id: a.clone(), decalage: 0 }).unwrap();
        // A dépendrait de B qui dépend de A → boucle refusée.
        assert!(creer(&conn, &NouvelleDependance {
            tache_id: a.clone(), predecesseur_id: b.clone(), decalage: 0 }).is_err());
        // Doublon refusé.
        assert!(creer(&conn, &NouvelleDependance {
            tache_id: b, predecesseur_id: a, decalage: 0 }).is_err());
    }

    #[test]
    fn decalage_respecte_et_lien_deja_correct_ne_bouge_pas() {
        let conn = db::open_in_memory().unwrap();
        let p = projet_test(&conn);
        let a = tache(&conn, &p, "A", "2026-08-01", "2026-08-10");
        let b = tache(&conn, &p, "B", "2026-08-20", "2026-08-25");
        // B doit commencer 3 jours après la fin de A, soit le 14 au plus tôt.
        // Il commence le 20 : c'est plus tard, donc aucune violation.
        creer(&conn, &NouvelleDependance { tache_id: b.clone(), predecesseur_id: a, decalage: 3 }).unwrap();
        assert!(violations(&conn, &p).unwrap().is_empty());
        assert!(plan_harmonisation(&conn, &p).unwrap().is_empty(), "on ne ramène jamais une tâche en arrière");
    }
}
