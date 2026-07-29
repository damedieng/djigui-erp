//! Calendriers superposés (décision utilisateur du 2026-07-28).
//!
//! L'agenda affiche ses rendez-vous, et **par-dessus** les échéances des autres
//! modules — jalons de projet, étapes de marché, livrables, fins d'activités —
//! comme Google Agenda superpose plusieurs calendriers.
//!
//! # ⚠️ LECTURE SEULE, et c'est structurant
//!
//! Ce module n'écrit **jamais**. Il ne contient que des `SELECT`. Une échéance
//! se modifie dans **son écran d'origine**, où vivent ses garde-fous : le verrou
//! d'ordre des étapes de marché, le contrôle chronologique des dates, la règle
//! qui interdit de rouvrir un jalon sans conséquence. Les laisser contourner
//! depuis l'agenda reviendrait à les supprimer.
//!
//! Cela lève la barrière « jalons ↔ agenda » de la spec Gestion de projet
//! (« aucun lien agenda ; le branchement se décide séparément ») — l'utilisateur
//! l'a tranchée, dans le sens le plus sûr : **on voit, on ne touche pas**.
//!
//! # Ce qui n'est pas proposé n'existe pas
//!
//! Un calendrier n'apparaît que si **son module est visible** (migration 0040).
//! Proposer d'afficher les marchés à un client qui vient de masquer le module
//! serait incohérent.

use crate::error::Result;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

/// Les calendriers superposables, dans l'ordre d'affichage. Le premier est
/// l'agenda lui-même : c'est le seul qui soit modifiable, depuis son écran.
///
/// `(code, libellé, couleur, module requis)`
pub const CALENDRIERS: &[(&str, &str, &str, &str)] = &[
    ("rendez_vous", "Mes rendez-vous", "#16794f", "agenda"),
    ("jalon", "Jalons de projet", "#7c3aed", "projets"),
    ("livrable", "Livrables attendus", "#c8860a", "projets"),
    ("activite", "Fins d'activités", "#3f6fb0", "projets"),
    ("etape_marche", "Étapes de marché", "#b23a2c", "marches"),
];

#[derive(Debug, Clone, Serialize)]
pub struct Calendrier {
    pub code: String,
    pub libelle: String,
    pub couleur: String,
    /// Combien d'échéances sur la période demandée. Sert à dire « Jalons (3) »
    /// et à comprendre d'où vient l'encombrement du mois.
    pub nb: i64,
}

/// Une échéance affichée. **Aucun identifiant modifiable n'est exposé** : on
/// donne de quoi afficher et de quoi ouvrir la bonne fiche, rien de plus.
#[derive(Debug, Clone, Serialize)]
pub struct Evenement {
    pub id: String,
    /// Le calendrier d'origine : c'est lui qui donne la couleur et la forme.
    pub source: String,
    /// « AAAA-MM-JJ ». Les rendez-vous portent aussi une heure, à part.
    pub date: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub heure: Option<String>,
    pub titre: String,
    /// D'où ça vient : le nom du projet ou du marché. Sans lui, une échéance
    /// isolée dans un calendrier ne veut rien dire.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origine: Option<String>,
    pub statut: String,
    /// L'écran à ouvrir pour agir. La modification se fait **là-bas**.
    pub lien: String,
    /// L'échéance est passée sans être honorée.
    pub en_retard: bool,
    /// Complément affiché dans l'aperçu (montant, responsable, réserve…).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FiltreCalendrier {
    /// Bornes de la période, « AAAA-MM-JJ ». Sans elles, on remonterait tout
    /// l'historique pour afficher un mois.
    #[serde(default)]
    pub du: Option<String>,
    #[serde(default)]
    pub au: Option<String>,
    /// Codes des calendriers demandés, séparés par des virgules. Vide = tous
    /// ceux qui sont disponibles.
    #[serde(default)]
    pub sources: Option<String>,
}

/// Les calendriers réellement proposables : ceux dont le module est visible.
pub fn disponibles(conn: &Connection, f: &FiltreCalendrier) -> Result<Vec<Calendrier>> {
    let visibles = crate::modules::activation::visibles(conn)?;
    let evts = evenements(conn, &FiltreCalendrier { sources: None, ..f.clone() })?;
    Ok(CALENDRIERS
        .iter()
        .filter(|(_, _, _, module)| visibles.iter().any(|v| v == module))
        .map(|(code, libelle, couleur, _)| Calendrier {
            code: (*code).to_string(),
            libelle: (*libelle).to_string(),
            couleur: (*couleur).to_string(),
            nb: evts.iter().filter(|e| e.source == *code).count() as i64,
        })
        .collect())
}

fn demande(f: &FiltreCalendrier, code: &str, visibles: &[String]) -> bool {
    // Le module doit être visible : ce qui est masqué du menu ne s'invite pas
    // dans l'agenda.
    let module = CALENDRIERS.iter().find(|(c, ..)| *c == code).map(|(_, _, _, m)| *m);
    if !module.map(|m| visibles.iter().any(|v| v == m)).unwrap_or(false) {
        return false;
    }
    match f.sources.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        None => true,
        Some(s) => s.split(',').map(str::trim).any(|x| x == code),
    }
}

/// Toutes les échéances de la période, tous calendriers demandés confondus.
///
/// **Uniquement des lectures.** Ajouter une écriture ici serait un contresens :
/// l'agenda est une vue.
pub fn evenements(conn: &Connection, f: &FiltreCalendrier) -> Result<Vec<Evenement>> {
    let visibles = crate::modules::activation::visibles(conn)?;
    let today = crate::now()[..10].to_string();
    let du = f.du.clone();
    let au = f.au.clone();
    let mut out: Vec<Evenement> = Vec::new();

    // --- Les rendez-vous : le calendrier propre de l'agenda ---
    if demande(f, "rendez_vous", &visibles) {
        let mut st = conn.prepare(
            "SELECT r.id, r.titre, r.debut, r.statut, r.journee_entiere, t.nom, r.lieu
               FROM rendez_vous r
               LEFT JOIN tiers t ON t.id = r.tiers_id
              WHERE (?1 IS NULL OR substr(r.debut,1,10) >= ?1)
                AND (?2 IS NULL OR substr(r.debut,1,10) <= ?2)",
        )?;
        let v = st.query_map(params![du, au], |r| {
            let debut: String = r.get(2)?;
            let statut: String = r.get(3)?;
            let journee: i64 = r.get(4)?;
            let date = debut[..10.min(debut.len())].to_string();
            Ok(Evenement {
                id: r.get(0)?,
                source: "rendez_vous".into(),
                heure: if journee != 0 || debut.len() < 16 {
                    None
                } else {
                    Some(debut[11..16].to_string())
                },
                titre: r.get(1)?,
                origine: r.get::<_, Option<String>>(5)?,
                en_retard: date < today && matches!(statut.as_str(), "planifie" | "confirme"),
                statut,
                lien: "agenda.html".into(),
                detail: r.get::<_, Option<String>>(6)?,
                date,
            })
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        out.extend(v);
    }

    // --- Jalons de projet : les dates clés, celles qu'on veut voir arriver ---
    if demande(f, "jalon", &visibles) {
        let mut st = conn.prepare(
            "SELECT j.id, j.nom, j.date_prevue, j.statut, p.nom, p.id, j.date_reelle
               FROM jalon j JOIN projet p ON p.id = j.projet_id
              WHERE j.date_prevue IS NOT NULL
                AND (?1 IS NULL OR j.date_prevue >= ?1)
                AND (?2 IS NULL OR j.date_prevue <= ?2)",
        )?;
        let v = st.query_map(params![du, au], |r| {
            let date: String = r.get(2)?;
            let statut: String = r.get(3)?;
            let projet_id: String = r.get(5)?;
            let reelle: Option<String> = r.get(6)?;
            Ok(Evenement {
                id: r.get(0)?,
                source: "jalon".into(),
                heure: None,
                titre: r.get(1)?,
                origine: r.get::<_, Option<String>>(4)?,
                en_retard: date < today && statut != "atteint",
                statut,
                lien: format!("projet-detail.html?id={projet_id}"),
                detail: reelle.map(|d| format!("Atteint le {d}")),
                date,
            })
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        out.extend(v);
    }

    // --- Livrables : ce que le projet doit remettre ---
    if demande(f, "livrable", &visibles) {
        let mut st = conn.prepare(
            "SELECT l.id, l.nom, l.date_attendue, l.statut, p.nom, p.id, l.date_livraison
               FROM livrable l JOIN projet p ON p.id = l.projet_id
              WHERE l.date_attendue IS NOT NULL
                AND (?1 IS NULL OR l.date_attendue >= ?1)
                AND (?2 IS NULL OR l.date_attendue <= ?2)",
        )?;
        let v = st.query_map(params![du, au], |r| {
            let date: String = r.get(2)?;
            let statut: String = r.get(3)?;
            let projet_id: String = r.get(5)?;
            let livre: Option<String> = r.get(6)?;
            Ok(Evenement {
                id: r.get(0)?,
                source: "livrable".into(),
                heure: None,
                titre: r.get(1)?,
                origine: r.get::<_, Option<String>>(4)?,
                en_retard: date < today && !matches!(statut.as_str(), "livre" | "accepte"),
                statut,
                lien: format!("projet-detail.html?id={projet_id}"),
                detail: livre.map(|d| format!("Livré le {d}")),
                date,
            })
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        out.extend(v);
    }

    // --- Fins d'activités : c'est ce qui remplit le plus le calendrier, d'où
    // la case à cocher pour l'éteindre. Seules les activités FEUILLES comptent :
    // une phase parente ferait doublon avec ses propres activités.
    if demande(f, "activite", &visibles) {
        let mut st = conn.prepare(
            "SELECT t.id, t.nom, t.date_fin_prevue, t.statut, p.nom, p.id, t.avancement
               FROM tache t JOIN projet p ON p.id = t.projet_id
              WHERE t.date_fin_prevue IS NOT NULL
                AND NOT EXISTS (SELECT 1 FROM tache c WHERE c.tache_parente_id = t.id)
                AND (?1 IS NULL OR t.date_fin_prevue >= ?1)
                AND (?2 IS NULL OR t.date_fin_prevue <= ?2)",
        )?;
        let v = st.query_map(params![du, au], |r| {
            let date: String = r.get(2)?;
            let statut: String = r.get(3)?;
            let projet_id: String = r.get(5)?;
            let av: i64 = r.get(6)?;
            Ok(Evenement {
                id: r.get(0)?,
                source: "activite".into(),
                heure: None,
                titre: r.get(1)?,
                origine: r.get::<_, Option<String>>(4)?,
                en_retard: date < today && statut != "terminee",
                statut,
                lien: format!("projet-detail.html?id={projet_id}"),
                detail: Some(format!("Avancement {av} %")),
                date,
            })
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        out.extend(v);
    }

    // --- Étapes de marché : de vrais rendez-vous, souvent avec des tiers ---
    if demande(f, "etape_marche", &visibles) {
        let mut st = conn.prepare(
            "SELECT e.id, e.libelle, e.date_prevue, e.statut, m.objet, m.id, m.numero
               FROM marche_etape e JOIN marche m ON m.id = e.marche_id
              WHERE e.date_prevue IS NOT NULL
                AND m.statut <> 'annule'
                AND (?1 IS NULL OR e.date_prevue >= ?1)
                AND (?2 IS NULL OR e.date_prevue <= ?2)",
        )?;
        let v = st.query_map(params![du, au], |r| {
            let date: String = r.get(2)?;
            let statut: String = r.get(3)?;
            let marche_id: String = r.get(5)?;
            let numero: String = r.get(6)?;
            Ok(Evenement {
                id: r.get(0)?,
                source: "etape_marche".into(),
                heure: None,
                titre: r.get(1)?,
                origine: r.get::<_, Option<String>>(4)?,
                en_retard: date < today && !matches!(statut.as_str(), "termine" | "annule"),
                statut,
                lien: format!("marche-detail.html?id={marche_id}"),
                detail: Some(numero),
                date,
            })
        })?.collect::<rusqlite::Result<Vec<_>>>()?;
        out.extend(v);
    }

    // Tri chronologique, puis par heure : c'est l'ordre dans lequel la journée
    // se déroule, donc celui dans lequel on veut la lire.
    out.sort_by(|a, b| {
        a.date.cmp(&b.date).then(
            a.heure.as_deref().unwrap_or("").cmp(b.heure.as_deref().unwrap_or("")),
        )
    });
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::modules::{activation, marche, projet};

    fn base() -> Connection {
        let conn = db::open_in_memory().unwrap();
        // Tout est ouvert par défaut (migration 0041).
        conn
    }

    fn projet_avec_jalon(conn: &Connection) -> String {
        let p = projet::creer(conn, &projet::NouveauProjet {
            nom: "Chantier de l'école".into(), client_id: None, chef_de_projet_id: None,
            date_debut_prevue: None, date_fin_prevue: None, date_debut_reelle: None,
            date_fin_reelle: None, statut: None, budget_global: 0.0, note: None,
        }, None).unwrap();
        conn.execute(
            "INSERT INTO jalon (id, projet_id, nom, date_prevue, statut, ordre, cree_le)
             VALUES ('j1', ?1, 'Réception des travaux', '2026-08-15', 'a_venir', 1, datetime('now'))",
            params![p.id],
        ).unwrap();
        p.id
    }

    #[test]
    fn lagenda_superpose_les_echeances_des_autres_modules() {
        let conn = base();
        let pid = projet_avec_jalon(&conn);
        // Une activité feuille et une activité parente : seule la feuille compte.
        let parent = projet::creer_tache(&conn, &projet::NouvelleTache {
            projet_id: pid.clone(), tache_parente_id: None, nom: "Phase 1".into(),
            description: None, date_debut_prevue: None, date_fin_prevue: Some("2026-08-31".into()),
            date_debut_reelle: None, date_fin_reelle: None, statut: None, avancement: None,
            budget: 0.0,
        }).unwrap();
        projet::creer_tache(&conn, &projet::NouvelleTache {
            projet_id: pid.clone(), tache_parente_id: Some(parent.id.clone()),
            nom: "Terrassement".into(), description: None, date_debut_prevue: None,
            date_fin_prevue: Some("2026-08-20".into()), date_debut_reelle: None,
            date_fin_reelle: None, statut: None, avancement: None, budget: 0.0,
        }).unwrap();

        let m = marche::creer(&conn, &marche::NouveauMarche {
            objet: "Fourniture de tables".into(), type_id: Some("mt-travaux".into()),
            montant_estime: 1_000_000.0, montant_attribue: None, monnaie: None,
            date_lancement: Some("2026-08-01".into()), date_cloture_prevue: None,
            attributaire_id: None, projet_id: None, responsable_id: None,
            lieu_execution: None, observations: None, etapes: Vec::new(),
        }, None).unwrap();

        let f = FiltreCalendrier {
            du: Some("2026-08-01".into()), au: Some("2026-08-31".into()), sources: None,
        };
        let evts = evenements(&conn, &f).unwrap();

        // Le jalon est là, rattaché à son projet et pointant vers son écran.
        let jalon = evts.iter().find(|e| e.source == "jalon").unwrap();
        assert_eq!(jalon.titre, "Réception des travaux");
        assert_eq!(jalon.origine.as_deref(), Some("Chantier de l'école"));
        assert!(jalon.lien.contains("projet-detail.html"), "{}", jalon.lien);

        // ⚠️ Seules les activités FEUILLES : la phase parente ferait doublon
        // avec les activités qu'elle contient.
        let acts: Vec<_> = evts.iter().filter(|e| e.source == "activite").collect();
        assert_eq!(acts.len(), 1, "{:?}", acts.iter().map(|a| &a.titre).collect::<Vec<_>>());
        assert_eq!(acts[0].titre, "Terrassement");

        // Les étapes du marché sont là, avec le numéro en complément.
        let etapes: Vec<_> = evts.iter().filter(|e| e.source == "etape_marche").collect();
        assert!(!etapes.is_empty());
        assert_eq!(etapes[0].detail.as_deref(), Some(m.numero.as_str()));
        assert!(etapes[0].lien.contains("marche-detail.html"));

        // Tout est trié chronologiquement : c'est l'ordre de lecture d'un mois.
        let dates: Vec<_> = evts.iter().map(|e| e.date.clone()).collect();
        let mut triees = dates.clone();
        triees.sort();
        assert_eq!(dates, triees);
    }

    #[test]
    fn on_ne_montre_que_les_calendriers_dont_le_module_est_visible() {
        let conn = base();
        projet_avec_jalon(&conn);
        marche::creer(&conn, &marche::NouveauMarche {
            objet: "Essai".into(), type_id: Some("mt-travaux".into()), montant_estime: 0.0,
            montant_attribue: None, monnaie: None, date_lancement: Some("2026-08-01".into()),
            date_cloture_prevue: None, attributaire_id: None, projet_id: None,
            responsable_id: None, lieu_execution: None, observations: None, etapes: Vec::new(),
        }, None).unwrap();
        let f = FiltreCalendrier { du: None, au: None, sources: None };

        let codes: Vec<String> = disponibles(&conn, &f).unwrap().into_iter().map(|c| c.code).collect();
        assert!(codes.contains(&"jalon".to_string()));
        assert!(codes.contains(&"etape_marche".to_string()));

        // Le client masque les marchés : le calendrier disparaît AUSSI, sinon on
        // proposerait d'afficher ce qu'on vient de retirer du menu.
        activation::changer_actif(&conn, "marches", false).unwrap();
        let codes: Vec<String> = disponibles(&conn, &f).unwrap().into_iter().map(|c| c.code).collect();
        assert!(!codes.contains(&"etape_marche".to_string()), "{codes:?}");
        assert!(codes.contains(&"jalon".to_string()), "les projets restent");
        // Et surtout : plus aucune échéance de marché ne remonte.
        let evts = evenements(&conn, &f).unwrap();
        assert!(evts.iter().all(|e| e.source != "etape_marche"));
    }

    #[test]
    fn on_peut_eteindre_un_calendrier_a_la_demande() {
        let conn = base();
        projet_avec_jalon(&conn);
        let tous = evenements(&conn, &FiltreCalendrier::default()).unwrap();
        assert!(tous.iter().any(|e| e.source == "jalon"));

        // Ne demander que les rendez-vous : plus aucun jalon.
        let f = FiltreCalendrier { sources: Some("rendez_vous".into()), ..Default::default() };
        let filtres = evenements(&conn, &f).unwrap();
        assert!(filtres.iter().all(|e| e.source == "rendez_vous"));
    }

    /// Le retard se voit à l'agenda comme ailleurs : une échéance passée et non
    /// honorée doit sauter aux yeux.
    #[test]
    fn une_echeance_passee_et_non_honoree_est_signalee() {
        let conn = base();
        let p = projet::creer(&conn, &projet::NouveauProjet {
            nom: "Ancien".into(), client_id: None, chef_de_projet_id: None,
            date_debut_prevue: None, date_fin_prevue: None, date_debut_reelle: None,
            date_fin_reelle: None, statut: None, budget_global: 0.0, note: None,
        }, None).unwrap();
        conn.execute(
            "INSERT INTO jalon (id, projet_id, nom, date_prevue, statut, ordre, cree_le)
             VALUES ('jr', ?1, 'Rapport final', '2020-01-01', 'a_venir', 1, datetime('now'))",
            params![p.id]).unwrap();
        conn.execute(
            "INSERT INTO jalon (id, projet_id, nom, date_prevue, statut, ordre, cree_le)
             VALUES ('jo', ?1, 'Lancement', '2020-01-01', 'atteint', 2, datetime('now'))",
            params![p.id]).unwrap();

        let evts = evenements(&conn, &FiltreCalendrier::default()).unwrap();
        let r = evts.iter().find(|e| e.id == "jr").unwrap();
        let o = evts.iter().find(|e| e.id == "jo").unwrap();
        assert!(r.en_retard, "échéance passée et non atteinte");
        assert!(!o.en_retard, "un jalon ATTEINT n'est jamais en retard, même daté d'hier");
    }
}
