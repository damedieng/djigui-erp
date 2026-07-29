//! Module Gestion de Projet — Incrément 1 : projets + tâches (migration 0021).
//!
//! Gestion **par projet** (v1 cloisonnée). Ce module couvre le projet et ses
//! tâches (hiérarchie à un niveau). Jalons, dépendances, assignations,
//! ressources et journal viendront dans les incréments suivants. **Aucun lien
//! agenda** ici (à valider séparément, cf. spec). CRUD complet + traitement par
//! lot ; les incohérences (dates, %) sont signalées côté UI, non bloquantes.

use crate::domain::{StatutProjet, StatutTache};
use crate::error::{CoreError, Result};
use crate::now;
use chrono::NaiveDate;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use uuid::Uuid;

/// Profondeur maximale de la hiérarchie des tâches (garde-fou).
const NIVEAU_MAX: i64 = 4;

// ===========================================================================
// Projet
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct Projet {
    pub id: String,
    pub nom: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chef_de_projet_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_debut_prevue: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_fin_prevue: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_debut_reelle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_fin_reelle: Option<String>,
    pub statut: String,
    pub budget_global: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    pub cree_le: String,
    // Champs dérivés (jointures / agrégats) — non stockés.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client_nom: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chef_nom: Option<String>,
    #[serde(default)]
    pub nb_taches: i64,
    #[serde(default)]
    pub nb_terminees: i64,
    /// Avancement global = moyenne des `avancement` des tâches (0 si aucune).
    #[serde(default)]
    pub avancement: i64,
    /// Budget des tâches = Σ des tâches feuilles (remontée bas→haut).
    #[serde(default)]
    pub budget_taches: f64,
    /// Budget **planifié total** = budget tâches + main-d'œuvre + ressources.
    #[serde(default)]
    pub budget_planifie: f64,
    /// Coût des ressources rattachées (Σ coût_unitaire × quantité).
    #[serde(default)]
    pub cout_ressources: f64,
    /// Coût de la main-d'œuvre (Σ heures × taux des intervenants assignés).
    #[serde(default)]
    pub cout_main_oeuvre: f64,
    /// Début/fin **calculés** depuis les tâches (min début / max fin).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_debut_calculee: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_fin_calculee: Option<String>,
    /// Vrai si la fin calculée des tâches dépasse la fin prévue du projet.
    #[serde(default)]
    pub depasse_fin: bool,
    /// Avancement **physique** global = moyenne des tâches **pondérée par leur
    /// budget** (repli sur moyenne simple si aucun budget).
    #[serde(default)]
    pub avancement_physique: i64,
    /// Avancement **budgétaire** = dépenses (coût ressources) ÷ budget prévu, en %.
    #[serde(default)]
    pub avancement_budgetaire: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouveauProjet {
    pub nom: String,
    #[serde(default)]
    pub client_id: Option<String>,
    #[serde(default)]
    pub chef_de_projet_id: Option<String>,
    #[serde(default)]
    pub date_debut_prevue: Option<String>,
    #[serde(default)]
    pub date_fin_prevue: Option<String>,
    #[serde(default)]
    pub date_debut_reelle: Option<String>,
    #[serde(default)]
    pub date_fin_reelle: Option<String>,
    #[serde(default)]
    pub statut: Option<StatutProjet>,
    #[serde(default)]
    pub budget_global: f64,
    #[serde(default)]
    pub note: Option<String>,
}

fn vide(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|x| !x.is_empty())
}

const PROJ_COLS: &str = "SELECT p.id, p.nom, p.client_id, p.chef_de_projet_id,
        p.date_debut_prevue, p.date_fin_prevue, p.date_debut_reelle, p.date_fin_reelle,
        p.statut, p.budget_global, p.note, p.cree_le,
        t.nom AS client_nom, u.nom AS chef_nom,
        (SELECT COUNT(*) FROM tache k WHERE k.projet_id = p.id) AS nb_taches,
        (SELECT COUNT(*) FROM tache k WHERE k.projet_id = p.id AND k.statut='terminee') AS nb_term,
        (SELECT CAST(COALESCE(ROUND(AVG(k.avancement)),0) AS INTEGER) FROM tache k WHERE k.projet_id = p.id) AS avg_av,
        (SELECT COALESCE(SUM(k.budget),0) FROM tache k WHERE k.projet_id = p.id
           AND NOT EXISTS (SELECT 1 FROM tache c WHERE c.tache_parente_id = k.id)) AS budget_plan,
        (SELECT COALESCE(SUM(r.cout_unitaire * r.quantite),0) FROM ressource r WHERE r.projet_id = p.id) AS cout_res,
        (SELECT MIN(date_debut_prevue) FROM tache k WHERE k.projet_id = p.id AND date_debut_prevue IS NOT NULL) AS dmin,
        (SELECT MAX(date_fin_prevue)   FROM tache k WHERE k.projet_id = p.id AND date_fin_prevue   IS NOT NULL) AS dmax,
        (SELECT CASE WHEN COALESCE(SUM(k.budget),0) > 0
                     THEN CAST(ROUND(SUM(k.budget * k.avancement) / SUM(k.budget)) AS INTEGER)
                     ELSE CAST(COALESCE(ROUND(AVG(k.avancement)),0) AS INTEGER) END
         FROM tache k WHERE k.projet_id = p.id
           AND NOT EXISTS (SELECT 1 FROM tache c WHERE c.tache_parente_id = k.id)) AS av_phys,
        (SELECT COALESCE(SUM(CASE WHEN i.type_taux='forfait' THEN i.taux
                                  ELSE a.heures_allouees * i.taux END),0)
         FROM assignation a JOIN tache k ON k.id = a.tache_id
           LEFT JOIN intervenant i ON i.id = a.intervenant_id
         WHERE k.projet_id = p.id) AS cout_mo
     FROM projet p
     LEFT JOIN tiers t ON t.id = p.client_id
     LEFT JOIN utilisateur u ON u.id = p.chef_de_projet_id";

fn ligne_projet(r: &rusqlite::Row) -> rusqlite::Result<Projet> {
    Ok(Projet {
        id: r.get(0)?,
        nom: r.get(1)?,
        client_id: r.get(2)?,
        chef_de_projet_id: r.get(3)?,
        date_debut_prevue: r.get(4)?,
        date_fin_prevue: r.get(5)?,
        date_debut_reelle: r.get(6)?,
        date_fin_reelle: r.get(7)?,
        statut: r.get(8)?,
        budget_global: r.get(9)?,
        note: r.get(10)?,
        cree_le: r.get(11)?,
        client_nom: r.get(12)?,
        chef_nom: r.get(13)?,
        nb_taches: r.get(14)?,
        nb_terminees: r.get(15)?,
        avancement: r.get(16)?,
        budget_taches: r.get(17)?,
        budget_planifie: 0.0,          // calculé dans poser_calculs
        cout_ressources: r.get(18)?,
        date_debut_calculee: r.get::<_, Option<String>>(19)?,
        date_fin_calculee: r.get::<_, Option<String>>(20)?,
        depasse_fin: false,           // calculés juste après (dépendent d'autres champs)
        avancement_physique: r.get(21)?,
        avancement_budgetaire: 0,
        cout_main_oeuvre: r.get(22)?,
    })
}

/// Calculs dérivés dépendant de plusieurs champs : dépassement de fin et
/// avancement budgétaire (dépenses ÷ budget prévu).
fn poser_calculs(p: &mut Projet) {
    p.depasse_fin = match (&p.date_fin_calculee, &p.date_fin_prevue) {
        (Some(calc), Some(prev)) => calc.as_str() > prev.as_str(),
        _ => false,
    };
    // Budget planifié TOTAL = budget des tâches + main-d'œuvre + ressources.
    p.budget_planifie = ((p.budget_taches + p.cout_main_oeuvre + p.cout_ressources) * 100.0).round() / 100.0;
    // Budget de référence : celui saisi au projet, sinon le planifié total.
    let budget_ref = if p.budget_global > 0.0 { p.budget_global } else { p.budget_planifie };
    // Dépenses = coût ressources + coût main-d'œuvre (heures × taux).
    let depenses = p.cout_ressources + p.cout_main_oeuvre;
    p.avancement_budgetaire = if budget_ref > 0.0 {
        (depenses / budget_ref * 100.0).round() as i64
    } else {
        0
    };
}

pub fn lister(conn: &Connection, statut: Option<StatutProjet>) -> Result<Vec<Projet>> {
    let sql = format!(
        "{PROJ_COLS} WHERE (?1 IS NULL OR p.statut = ?1) ORDER BY p.cree_le DESC"
    );
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![statut.map(|s| s.as_str())], ligne_projet)?;
    let mut v = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    v.iter_mut().for_each(poser_calculs);
    Ok(v)
}

pub fn lire(conn: &Connection, id: &str) -> Result<Projet> {
    let sql = format!("{PROJ_COLS} WHERE p.id = ?1");
    let mut p = conn.query_row(&sql, params![id], ligne_projet).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => CoreError::NotFound(format!("projet {id}")),
        autre => autre.into(),
    })?;
    poser_calculs(&mut p);
    Ok(p)
}

fn valider_projet(n: &NouveauProjet) -> Result<()> {
    if n.nom.trim().is_empty() {
        return Err(CoreError::Rule("le nom du projet est requis".into()));
    }
    Ok(())
}

pub fn creer(conn: &Connection, n: &NouveauProjet, cree_par: Option<&str>) -> Result<Projet> {
    valider_projet(n)?;
    let id = Uuid::new_v4().to_string();
    let statut = n.statut.unwrap_or(StatutProjet::Planifie);
    conn.execute(
        "INSERT INTO projet (id, nom, client_id, chef_de_projet_id, date_debut_prevue, date_fin_prevue,
                date_debut_reelle, date_fin_reelle, statut, budget_global, note, cree_par, cree_le)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13)",
        params![
            id, n.nom.trim(), vide(&n.client_id), vide(&n.chef_de_projet_id),
            vide(&n.date_debut_prevue), vide(&n.date_fin_prevue), vide(&n.date_debut_reelle),
            vide(&n.date_fin_reelle), statut.as_str(), n.budget_global, vide(&n.note), cree_par, now(),
        ],
    )?;
    lire(conn, &id)
}

pub fn modifier(conn: &Connection, id: &str, n: &NouveauProjet) -> Result<Projet> {
    valider_projet(n)?;
    let statut = n.statut.unwrap_or(StatutProjet::Planifie);
    let nb = conn.execute(
        "UPDATE projet SET nom=?2, client_id=?3, chef_de_projet_id=?4, date_debut_prevue=?5,
                date_fin_prevue=?6, date_debut_reelle=?7, date_fin_reelle=?8, statut=?9,
                budget_global=?10, note=?11 WHERE id=?1",
        params![
            id, n.nom.trim(), vide(&n.client_id), vide(&n.chef_de_projet_id),
            vide(&n.date_debut_prevue), vide(&n.date_fin_prevue), vide(&n.date_debut_reelle),
            vide(&n.date_fin_reelle), statut.as_str(), n.budget_global, vide(&n.note),
        ],
    )?;
    if nb == 0 {
        return Err(CoreError::NotFound(format!("projet {id}")));
    }
    lire(conn, id)
}

pub fn changer_statut(conn: &Connection, id: &str, statut: StatutProjet) -> Result<Projet> {
    let nb = conn.execute("UPDATE projet SET statut = ?2 WHERE id = ?1", params![id, statut.as_str()])?;
    if nb == 0 {
        return Err(CoreError::NotFound(format!("projet {id}")));
    }
    lire(conn, id)
}

/// Supprime un projet **et tout ce qui en dépend** (tâches, observations,
/// ressources) — le projet est l'unité de vie.
pub fn supprimer(conn: &Connection, id: &str) -> Result<()> {
    conn.execute(
        "DELETE FROM tache_action WHERE tache_id IN (SELECT id FROM tache WHERE projet_id = ?1)",
        params![id])?;
    conn.execute(
        "DELETE FROM assignation WHERE tache_id IN (SELECT id FROM tache WHERE projet_id = ?1)",
        params![id])?;
    conn.execute("DELETE FROM ressource WHERE projet_id = ?1", params![id])?;
    // Ordre imposé par les clés étrangères : documents → livrables → jalons,
    // puis les tâches.
    conn.execute("DELETE FROM dependance WHERE tache_id IN (SELECT id FROM tache WHERE projet_id = ?1)",
        params![id])?;
    conn.execute("DELETE FROM document_joint WHERE projet_id = ?1", params![id])?;
    conn.execute("DELETE FROM livrable WHERE projet_id = ?1", params![id])?;
    conn.execute("DELETE FROM jalon WHERE projet_id = ?1", params![id])?;
    conn.execute("DELETE FROM tache WHERE projet_id = ?1", params![id])?;
    let nb = conn.execute("DELETE FROM projet WHERE id = ?1", params![id])?;
    if nb == 0 {
        return Err(CoreError::NotFound(format!("projet {id}")));
    }
    Ok(())
}

// ===========================================================================
// Tâche
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct Tache {
    pub id: String,
    pub projet_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tache_parente_id: Option<String>,
    pub nom: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_debut_prevue: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_fin_prevue: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_debut_reelle: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date_fin_reelle: Option<String>,
    pub statut: String,
    pub avancement: i64,
    /// Budget **propre** (utilisé si la tâche est une feuille).
    pub budget: f64,
    pub ordre: i64,
    // ---- Champs dérivés (calcul bas→haut), non stockés ----
    /// Profondeur dans la hiérarchie (1 = tâche principale).
    #[serde(default)]
    pub niveau: i64,
    /// A-t-elle des sous-tâches ?
    #[serde(default)]
    pub a_enfants: bool,
    /// Budget effectif : propre si feuille, sinon Σ des enfants.
    #[serde(default)]
    pub budget_calcule: f64,
    /// Début effectif : le sien si feuille, sinon le plus tôt des enfants.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub debut_calcule: Option<String>,
    /// Fin effective : la sienne si feuille, sinon la plus tardive des enfants.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fin_calcule: Option<String>,
    /// Avancement effectif : le sien si feuille, sinon moyenne des enfants.
    #[serde(default)]
    pub avancement_calcule: i64,
    /// Nombre de jours (fin_calcule − debut_calcule, inclusif) ; 0 si indéfini.
    #[serde(default)]
    pub nb_jours: i64,
    /// Jours de retard. Pour une **activité feuille** : sa fin prévue est passée
    /// et elle n'est pas terminée. Pour une **activité parente** : le plus grand
    /// retard de sa descendance — sans quoi replier une branche ferait
    /// disparaître le retard de l'écran.
    ///
    /// ⚠️ Définition **strictement alignée** sur `notification::activites_en_retard`
    /// (feuille, `statut <> terminee`, `date_fin_prevue < aujourd'hui`) : la
    /// cloche et le planning ne doivent jamais se contredire.
    /// **Signalement seulement** — aucune date n'est recalculée (barrière « cascade »).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retard_jours: Option<i64>,
    /// Combien d'activités feuilles en retard sous celle-ci (elle-même comprise
    /// si c'est une feuille). Sert à dire « 3 activités en retard » au survol.
    #[serde(default)]
    pub nb_en_retard: i64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouvelleTache {
    pub projet_id: String,
    #[serde(default)]
    pub tache_parente_id: Option<String>,
    pub nom: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub date_debut_prevue: Option<String>,
    #[serde(default)]
    pub date_fin_prevue: Option<String>,
    #[serde(default)]
    pub date_debut_reelle: Option<String>,
    #[serde(default)]
    pub date_fin_reelle: Option<String>,
    #[serde(default)]
    pub statut: Option<StatutTache>,
    #[serde(default)]
    pub avancement: Option<i64>,
    #[serde(default)]
    pub budget: f64,
}

const TACHE_COLS: &str = "SELECT id, projet_id, tache_parente_id, nom, description,
        date_debut_prevue, date_fin_prevue, date_debut_reelle, date_fin_reelle, statut, avancement, budget, ordre
     FROM tache";

fn ligne_tache(r: &rusqlite::Row) -> rusqlite::Result<Tache> {
    Ok(Tache {
        id: r.get(0)?,
        projet_id: r.get(1)?,
        tache_parente_id: r.get(2)?,
        nom: r.get(3)?,
        description: r.get(4)?,
        date_debut_prevue: r.get(5)?,
        date_fin_prevue: r.get(6)?,
        date_debut_reelle: r.get(7)?,
        date_fin_reelle: r.get(8)?,
        statut: r.get(9)?,
        avancement: r.get(10)?,
        budget: r.get(11)?,
        ordre: r.get(12)?,
        niveau: 1,
        a_enfants: false,
        budget_calcule: 0.0,
        debut_calcule: None,
        fin_calcule: None,
        avancement_calcule: 0,
        nb_jours: 0,
        // Le retard se calcule dans `enrichir` : il dépend de la hiérarchie
        // entière, pas d'une ligne isolée.
        retard_jours: None,
        nb_en_retard: 0,
    })
}

pub fn lister_taches(conn: &Connection, projet_id: &str) -> Result<Vec<Tache>> {
    let sql = format!("{TACHE_COLS} WHERE projet_id = ?1 ORDER BY ordre, cree_le");
    let mut stmt = conn.prepare(&sql)?;
    let rows = stmt.query_map(params![projet_id], ligne_tache)?;
    let mut taches = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    enrichir(&mut taches);
    Ok(taches)
}

fn min_opt(a: Option<String>, b: Option<String>) -> Option<String> {
    match (a, b) {
        (Some(x), Some(y)) => Some(if x <= y { x } else { y }),
        (v, None) | (None, v) => v,
    }
}
fn max_opt(a: Option<String>, b: Option<String>) -> Option<String> {
    match (a, b) {
        (Some(x), Some(y)) => Some(if x >= y { x } else { y }),
        (v, None) | (None, v) => v,
    }
}
/// Nombre de jours inclusif entre deux dates « AAAA-MM-JJ » (0 si indéfini).
fn jours_entre(d: &Option<String>, f: &Option<String>) -> i64 {
    match (d.as_deref(), f.as_deref()) {
        (Some(a), Some(b)) => {
            let pa = NaiveDate::parse_from_str(&a[..a.len().min(10)], "%Y-%m-%d");
            let pb = NaiveDate::parse_from_str(&b[..b.len().min(10)], "%Y-%m-%d");
            match (pa, pb) {
                (Ok(x), Ok(y)) => ((y - x).num_days() + 1).max(0),
                _ => 0,
            }
        }
        _ => 0,
    }
}

/// Calcul bas→haut : budget/dates/avancement d'une tâche parente = agrégat de
/// ses enfants ; d'une feuille = ses propres valeurs. Remplit aussi niveau,
/// a_enfants et nb_jours.
fn enrichir(taches: &mut [Tache]) {
    let n = taches.len();
    if n == 0 {
        return;
    }
    let idx: HashMap<String, usize> =
        taches.iter().enumerate().map(|(i, t)| (t.id.clone(), i)).collect();
    let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
    let mut roots: Vec<usize> = Vec::new();
    for i in 0..n {
        match taches[i].tache_parente_id.as_ref().and_then(|p| idx.get(p)) {
            Some(&p) => children[p].push(i),
            None => roots.push(i),
        }
    }
    // Niveau (profondeur) par parcours descendant depuis les racines.
    let mut niveau = vec![1i64; n];
    let mut pile: Vec<(usize, i64)> = roots.iter().map(|&r| (r, 1)).collect();
    while let Some((i, lv)) = pile.pop() {
        niveau[i] = lv;
        for &c in &children[i] {
            pile.push((c, lv + 1));
        }
    }
    // Remontée (post-ordre) des valeurs agrégées.
    let mut budget = vec![0.0f64; n];
    let mut debut: Vec<Option<String>> = vec![None; n];
    let mut fin: Vec<Option<String>> = vec![None; n];
    let mut av = vec![0i64; n];
    for &r in &roots {
        calc_rollup(r, &children, taches, &mut budget, &mut debut, &mut fin, &mut av);
    }
    // --- Retard ---
    // Il se calcule sur les FEUILLES, puis remonte : une parente porte le plus
    // grand retard de sa descendance, sinon replier une branche escamoterait le
    // retard. Le tri par niveau décroissant garantit qu'un enfant est traité
    // avant son parent, sans récursion supplémentaire.
    let today = crate::now()[..10].to_string();
    let mut retard = vec![None::<i64>; n];
    let mut nb_retard = vec![0i64; n];
    for i in 0..n {
        if !children[i].is_empty() {
            continue;
        }
        let en_retard = taches[i].statut != StatutTache::Terminee.as_str()
            && taches[i]
                .date_fin_prevue
                .as_deref()
                .is_some_and(|f| f < today.as_str());
        if en_retard {
            // `jours_entre` compte les bornes ; le retard s'exprime en jours
            // écoulés DEPUIS l'échéance : une fin prévue hier = 1 jour.
            let f = taches[i].date_fin_prevue.clone();
            retard[i] = Some((jours_entre(&f, &Some(today.clone())) - 1).max(0));
            nb_retard[i] = 1;
        }
    }
    let mut ordre: Vec<usize> = (0..n).collect();
    ordre.sort_by_key(|&i| std::cmp::Reverse(niveau[i]));
    for &i in &ordre {
        if let Some(&p) = taches[i].tache_parente_id.as_ref().and_then(|p| idx.get(p)) {
            if let Some(r) = retard[i] {
                retard[p] = Some(retard[p].map_or(r, |x: i64| x.max(r)));
            }
            nb_retard[p] += nb_retard[i];
        }
    }

    for i in 0..n {
        taches[i].niveau = niveau[i];
        taches[i].a_enfants = !children[i].is_empty();
        taches[i].budget_calcule = (budget[i] * 100.0).round() / 100.0;
        taches[i].nb_jours = jours_entre(&debut[i], &fin[i]);
        taches[i].debut_calcule = debut[i].take();
        taches[i].fin_calcule = fin[i].take();
        taches[i].avancement_calcule = av[i];
        taches[i].retard_jours = retard[i];
        taches[i].nb_en_retard = nb_retard[i];
    }
}

fn calc_rollup(
    i: usize,
    children: &[Vec<usize>],
    taches: &[Tache],
    budget: &mut [f64],
    debut: &mut [Option<String>],
    fin: &mut [Option<String>],
    av: &mut [i64],
) {
    if children[i].is_empty() {
        budget[i] = taches[i].budget;
        debut[i] = taches[i].date_debut_prevue.clone();
        fin[i] = taches[i].date_fin_prevue.clone();
        av[i] = taches[i].avancement;
        return;
    }
    let (mut b, mut somme, mut cnt) = (0.0f64, 0i64, 0i64);
    let (mut d, mut f): (Option<String>, Option<String>) = (None, None);
    for &c in &children[i] {
        calc_rollup(c, children, taches, budget, debut, fin, av);
        b += budget[c];
        d = min_opt(d, debut[c].clone());
        f = max_opt(f, fin[c].clone());
        somme += av[c];
        cnt += 1;
    }
    budget[i] = b;
    debut[i] = d;
    fin[i] = f;
    av[i] = if cnt > 0 { ((somme as f64) / (cnt as f64)).round() as i64 } else { 0 };
}

pub fn lire_tache(conn: &Connection, id: &str) -> Result<Tache> {
    let sql = format!("{TACHE_COLS} WHERE id = ?1");
    conn.query_row(&sql, params![id], ligne_tache).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => CoreError::NotFound(format!("tâche {id}")),
        autre => autre.into(),
    })
}

fn borne_avancement(v: Option<i64>) -> i64 {
    v.unwrap_or(0).clamp(0, 100)
}

/// Chaîne d'ancêtres d'une tâche (du parent direct jusqu'à la racine).
fn ancetres(conn: &Connection, id: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur = id.to_string();
    for _ in 0..(NIVEAU_MAX as usize + 2) {
        let p: Option<String> = conn
            .query_row("SELECT tache_parente_id FROM tache WHERE id = ?1", params![cur], |r| r.get(0))
            .ok()
            .flatten();
        match p {
            Some(pp) => { out.push(pp.clone()); cur = pp; }
            None => break,
        }
    }
    out
}

/// Vérifie qu'ajouter une tâche sous `parente_id` respecte la profondeur max (4)
/// et ne crée pas de cycle (`enfant_id` ne doit pas être un ancêtre de la parente).
fn verifier_parente(conn: &Connection, parente_id: &str, enfant_id: Option<&str>) -> Result<()> {
    let chaine = ancetres(conn, parente_id);
    // profondeur de la parente = nb d'ancêtres + 1 ; l'enfant serait à +1.
    if (chaine.len() as i64 + 1) >= NIVEAU_MAX {
        return Err(CoreError::Rule(format!(
            "profondeur maximale de {NIVEAU_MAX} niveaux atteinte"
        )));
    }
    if let Some(e) = enfant_id {
        if e == parente_id || chaine.iter().any(|a| a == e) {
            return Err(CoreError::Rule("une tâche ne peut pas être sa propre sous-tâche".into()));
        }
    }
    Ok(())
}

pub fn creer_tache(conn: &Connection, n: &NouvelleTache) -> Result<Tache> {
    if n.nom.trim().is_empty() {
        return Err(CoreError::Rule("le nom de la tâche est requis".into()));
    }
    if let Some(p) = vide(&n.tache_parente_id) {
        verifier_parente(conn, p, None)?;
    }
    let id = Uuid::new_v4().to_string();
    let statut = n.statut.unwrap_or(StatutTache::AFaire);
    let ordre: i64 = conn
        .query_row("SELECT COALESCE(MAX(ordre)+1,0) FROM tache WHERE projet_id = ?1", params![n.projet_id], |r| r.get(0))
        .unwrap_or(0);
    conn.execute(
        "INSERT INTO tache (id, projet_id, tache_parente_id, nom, description, date_debut_prevue,
                date_fin_prevue, date_debut_reelle, date_fin_reelle, statut, avancement, budget, ordre, cree_le)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14)",
        params![
            id, n.projet_id, vide(&n.tache_parente_id), n.nom.trim(), vide(&n.description),
            vide(&n.date_debut_prevue), vide(&n.date_fin_prevue), vide(&n.date_debut_reelle),
            vide(&n.date_fin_reelle), statut.as_str(), borne_avancement(n.avancement), n.budget.max(0.0), ordre, now(),
        ],
    )?;
    lire_tache(conn, &id)
}

pub fn modifier_tache(conn: &Connection, id: &str, n: &NouvelleTache) -> Result<Tache> {
    if n.nom.trim().is_empty() {
        return Err(CoreError::Rule("le nom de la tâche est requis".into()));
    }
    if let Some(p) = vide(&n.tache_parente_id) {
        if p != id { verifier_parente(conn, p, Some(id))?; }
    }
    let statut = n.statut.unwrap_or(StatutTache::AFaire);
    let nb = conn.execute(
        "UPDATE tache SET tache_parente_id=?2, nom=?3, description=?4, date_debut_prevue=?5,
                date_fin_prevue=?6, date_debut_reelle=?7, date_fin_reelle=?8, statut=?9, avancement=?10, budget=?11 WHERE id=?1",
        params![
            id, vide(&n.tache_parente_id).filter(|p| *p != id), n.nom.trim(), vide(&n.description),
            vide(&n.date_debut_prevue), vide(&n.date_fin_prevue), vide(&n.date_debut_reelle),
            vide(&n.date_fin_reelle), statut.as_str(), borne_avancement(n.avancement), n.budget.max(0.0),
        ],
    )?;
    if nb == 0 {
        return Err(CoreError::NotFound(format!("tâche {id}")));
    }
    lire_tache(conn, id)
}

/// Change le statut d'une ou plusieurs tâches (traitement par lot). Passer une
/// tâche à « terminée » met son avancement à 100 %.
pub fn changer_statut_taches(conn: &Connection, ids: &[String], statut: StatutTache) -> Result<usize> {
    let mut n = 0;
    for id in ids {
        n += if statut == StatutTache::Terminee {
            conn.execute("UPDATE tache SET statut=?2, avancement=100 WHERE id=?1", params![id, statut.as_str()])?
        } else {
            conn.execute("UPDATE tache SET statut=?2 WHERE id=?1", params![id, statut.as_str()])?
        };
    }
    Ok(n)
}

/// Supprime récursivement une tâche, ses sous-tâches, leurs observations, et
/// détache les ressources qui y étaient rattachées (elles restent au projet).
fn purger_tache(conn: &Connection, id: &str) -> Result<usize> {
    // enfants d'abord (récursif)
    let enfants: Vec<String> = {
        let mut stmt = conn.prepare("SELECT id FROM tache WHERE tache_parente_id = ?1")?;
        let rows = stmt.query_map(params![id], |r| r.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()?
    };
    let mut n = 0;
    for e in &enfants {
        n += purger_tache(conn, e)?;
    }
    conn.execute("DELETE FROM tache_action WHERE tache_id = ?1", params![id])?;
    conn.execute("DELETE FROM assignation WHERE tache_id = ?1", params![id])?;
    conn.execute("UPDATE ressource SET tache_id = NULL WHERE tache_id = ?1", params![id])?;
    // Jalons, livrables et documents survivent à l'activité : on les DÉTACHE
    // (sinon la FK bloquerait, et surtout le travail produit ne doit pas
    // disparaître avec une ligne de planning).
    // Les liens de précédence n'ont plus de sens sans leur activité : ils sont
    // supprimés des deux côtés (successeur comme prédécesseur).
    conn.execute("DELETE FROM dependance WHERE tache_id = ?1 OR predecesseur_id = ?1", params![id])?;
    conn.execute("UPDATE jalon SET tache_id = NULL WHERE tache_id = ?1", params![id])?;
    conn.execute("UPDATE livrable SET tache_id = NULL WHERE tache_id = ?1", params![id])?;
    conn.execute("UPDATE document_joint SET tache_id = NULL WHERE tache_id = ?1", params![id])?;
    n += conn.execute("DELETE FROM tache WHERE id = ?1", params![id])?;
    Ok(n)
}

/// Supprime une ou plusieurs tâches **et leurs sous-tâches** (traitement par lot).
pub fn supprimer_taches(conn: &Connection, ids: &[String]) -> Result<usize> {
    let mut n = 0;
    for id in ids {
        n += purger_tache(conn, id)?;
    }
    Ok(n)
}

// ===========================================================================
// Ressources (matériel / budget / sous-traitance) — projet ou tâche
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct Ressource {
    pub id: String,
    pub projet_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tache_id: Option<String>,
    pub r#type: String,
    pub libelle: String,
    pub cout_unitaire: f64,
    pub quantite: f64,
    /// Coût = coût_unitaire × quantité.
    pub cout: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouvelleRessource {
    pub projet_id: String,
    #[serde(default)]
    pub tache_id: Option<String>,
    pub r#type: crate::domain::TypeRessource,
    pub libelle: String,
    #[serde(default)]
    pub cout_unitaire: f64,
    #[serde(default = "un")]
    pub quantite: f64,
}
fn un() -> f64 { 1.0 }

fn ligne_ressource(r: &rusqlite::Row) -> rusqlite::Result<Ressource> {
    let cu: f64 = r.get(4)?;
    let q: f64 = r.get(5)?;
    Ok(Ressource {
        id: r.get(0)?,
        projet_id: r.get(1)?,
        tache_id: r.get(2)?,
        r#type: r.get(3)?,
        libelle: r.get(6)?,
        cout_unitaire: cu,
        quantite: q,
        cout: (cu * q * 100.0).round() / 100.0,
    })
}

pub fn lister_ressources(conn: &Connection, projet_id: &str) -> Result<Vec<Ressource>> {
    let mut stmt = conn.prepare(
        "SELECT id, projet_id, tache_id, type, cout_unitaire, quantite, libelle
         FROM ressource WHERE projet_id = ?1 ORDER BY cree_le")?;
    let rows = stmt.query_map(params![projet_id], ligne_ressource)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn creer_ressource(conn: &Connection, n: &NouvelleRessource) -> Result<Ressource> {
    if n.libelle.trim().is_empty() {
        return Err(CoreError::Rule("le libellé de la ressource est requis".into()));
    }
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO ressource (id, projet_id, tache_id, type, libelle, cout_unitaire, quantite, cree_le)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
        params![id, n.projet_id, vide(&n.tache_id), n.r#type.as_str(), n.libelle.trim(),
                n.cout_unitaire.max(0.0), n.quantite.max(0.0), now()],
    )?;
    lire_ressource(conn, &id)
}

pub fn modifier_ressource(conn: &Connection, id: &str, n: &NouvelleRessource) -> Result<Ressource> {
    if n.libelle.trim().is_empty() {
        return Err(CoreError::Rule("le libellé de la ressource est requis".into()));
    }
    let nb = conn.execute(
        "UPDATE ressource SET tache_id=?2, type=?3, libelle=?4, cout_unitaire=?5, quantite=?6 WHERE id=?1",
        params![id, vide(&n.tache_id), n.r#type.as_str(), n.libelle.trim(),
                n.cout_unitaire.max(0.0), n.quantite.max(0.0)],
    )?;
    if nb == 0 {
        return Err(CoreError::NotFound(format!("ressource {id}")));
    }
    lire_ressource(conn, id)
}

pub fn lire_ressource(conn: &Connection, id: &str) -> Result<Ressource> {
    conn.query_row(
        "SELECT id, projet_id, tache_id, type, cout_unitaire, quantite, libelle FROM ressource WHERE id = ?1",
        params![id], ligne_ressource,
    ).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => CoreError::NotFound(format!("ressource {id}")),
        autre => autre.into(),
    })
}

pub fn supprimer_ressource(conn: &Connection, id: &str) -> Result<()> {
    let nb = conn.execute("DELETE FROM ressource WHERE id = ?1", params![id])?;
    if nb == 0 {
        return Err(CoreError::NotFound(format!("ressource {id}")));
    }
    Ok(())
}

// ===========================================================================
// Journal d'avancement / observations d'une tâche
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct Action {
    pub id: String,
    pub tache_id: String,
    pub date: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avancement: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub observation: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auteur_nom: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouvelleAction {
    #[serde(default)]
    pub avancement: Option<i64>,
    #[serde(default)]
    pub observation: Option<String>,
}

/// Enregistre une observation d'avancement. Si un avancement est fourni, il met
/// **aussi** à jour l'avancement de la tâche (et la termine si 100 %).
pub fn creer_action(conn: &Connection, tache_id: &str, a: &NouvelleAction, par: Option<&str>) -> Result<Action> {
    let obs = a.observation.as_deref().map(str::trim).filter(|s| !s.is_empty());
    if a.avancement.is_none() && obs.is_none() {
        return Err(CoreError::Rule("saisissez un avancement ou une observation".into()));
    }
    // Refuse sur une tâche parente (avancement calculé).
    let a_enfants: bool = conn
        .query_row("SELECT EXISTS(SELECT 1 FROM tache WHERE tache_parente_id = ?1)", params![tache_id],
            |r| Ok(r.get::<_, i64>(0)? != 0))
        .unwrap_or(false);
    if a_enfants && a.avancement.is_some() {
        return Err(CoreError::Rule(
            "l'avancement d'une tâche avec sous-tâches est calculé automatiquement".into()));
    }
    let av = a.avancement.map(|v| v.clamp(0, 100));
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO tache_action (id, tache_id, utilisateur_id, date, avancement, observation)
         VALUES (?1,?2,?3,?4,?5,?6)",
        params![id, tache_id, par, now(), av, obs],
    )?;
    if let Some(v) = av {
        // met à jour l'avancement ; 100 % ⇒ statut terminée.
        if v >= 100 {
            conn.execute("UPDATE tache SET avancement=100, statut='terminee' WHERE id=?1", params![tache_id])?;
        } else {
            conn.execute("UPDATE tache SET avancement=?2 WHERE id=?1", params![tache_id, v])?;
        }
    }
    lire_action(conn, &id)
}

fn ligne_action(r: &rusqlite::Row) -> rusqlite::Result<Action> {
    Ok(Action {
        id: r.get(0)?,
        tache_id: r.get(1)?,
        date: r.get(2)?,
        avancement: r.get(3)?,
        observation: r.get(4)?,
        auteur_nom: r.get(5)?,
    })
}

pub fn lire_action(conn: &Connection, id: &str) -> Result<Action> {
    conn.query_row(
        "SELECT a.id, a.tache_id, a.date, a.avancement, a.observation, u.nom
         FROM tache_action a LEFT JOIN utilisateur u ON u.id = a.utilisateur_id
         WHERE a.id = ?1",
        params![id], ligne_action,
    ).map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => CoreError::NotFound(format!("action {id}")),
        autre => autre.into(),
    })
}

/// Historique des observations d'une tâche, de la plus récente à la plus ancienne.
pub fn lister_actions(conn: &Connection, tache_id: &str) -> Result<Vec<Action>> {
    let mut stmt = conn.prepare(
        "SELECT a.id, a.tache_id, a.date, a.avancement, a.observation, u.nom
         FROM tache_action a LEFT JOIN utilisateur u ON u.id = a.utilisateur_id
         WHERE a.tache_id = ?1 ORDER BY a.date DESC")?;
    let rows = stmt.query_map(params![tache_id], ligne_action)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

// ===========================================================================
// Intervenants (ressources humaines : interne compte, ou externe consultant)
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct Intervenant {
    pub id: String,
    pub nom: String,
    pub r#type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub utilisateur_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub societe: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    pub type_taux: String,
    pub taux: f64,
    pub actif: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouvelIntervenant {
    pub nom: String,
    pub r#type: crate::domain::TypeIntervenant,
    #[serde(default)]
    pub utilisateur_id: Option<String>,
    #[serde(default)]
    pub societe: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
    #[serde(default = "taux_horaire")]
    pub type_taux: crate::domain::TypeTaux,
    #[serde(default)]
    pub taux: f64,
    #[serde(default = "vrai_b")]
    pub actif: bool,
}
fn taux_horaire() -> crate::domain::TypeTaux { crate::domain::TypeTaux::Horaire }
fn vrai_b() -> bool { true }

fn ligne_intervenant(r: &rusqlite::Row) -> rusqlite::Result<Intervenant> {
    Ok(Intervenant {
        id: r.get(0)?, nom: r.get(1)?, r#type: r.get(2)?, utilisateur_id: r.get(3)?,
        societe: r.get(4)?, role: r.get(5)?, type_taux: r.get(6)?, taux: r.get(7)?,
        actif: r.get::<_, i64>(8)? != 0,
    })
}
const INTERV_COLS: &str = "SELECT id, nom, type, utilisateur_id, societe, role, type_taux, taux, actif FROM intervenant";

pub fn lister_intervenants(conn: &Connection) -> Result<Vec<Intervenant>> {
    let mut stmt = conn.prepare(&format!("{INTERV_COLS} ORDER BY actif DESC, type, nom"))?;
    let rows = stmt.query_map([], ligne_intervenant)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}
pub fn lire_intervenant(conn: &Connection, id: &str) -> Result<Intervenant> {
    conn.query_row(&format!("{INTERV_COLS} WHERE id = ?1"), params![id], ligne_intervenant)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => CoreError::NotFound(format!("intervenant {id}")),
            autre => autre.into(),
        })
}
pub fn creer_intervenant(conn: &Connection, n: &NouvelIntervenant) -> Result<Intervenant> {
    if n.nom.trim().is_empty() {
        return Err(CoreError::Rule("le nom de l'intervenant est requis".into()));
    }
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO intervenant (id, nom, type, utilisateur_id, societe, role, type_taux, taux, actif, cree_le)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
        params![id, n.nom.trim(), n.r#type.as_str(), vide(&n.utilisateur_id), vide(&n.societe),
                vide(&n.role), n.type_taux.as_str(), n.taux.max(0.0), n.actif as i64, now()],
    )?;
    lire_intervenant(conn, &id)
}
pub fn modifier_intervenant(conn: &Connection, id: &str, n: &NouvelIntervenant) -> Result<Intervenant> {
    if n.nom.trim().is_empty() {
        return Err(CoreError::Rule("le nom de l'intervenant est requis".into()));
    }
    let nb = conn.execute(
        "UPDATE intervenant SET nom=?2, type=?3, utilisateur_id=?4, societe=?5, role=?6, type_taux=?7, taux=?8, actif=?9 WHERE id=?1",
        params![id, n.nom.trim(), n.r#type.as_str(), vide(&n.utilisateur_id), vide(&n.societe),
                vide(&n.role), n.type_taux.as_str(), n.taux.max(0.0), n.actif as i64],
    )?;
    if nb == 0 { return Err(CoreError::NotFound(format!("intervenant {id}"))); }
    lire_intervenant(conn, id)
}
/// Supprime un intervenant **jamais assigné** ; sinon on invite à le désactiver.
pub fn supprimer_intervenant(conn: &Connection, id: &str) -> Result<()> {
    let utilise: i64 = conn
        .query_row("SELECT COUNT(*) FROM assignation WHERE intervenant_id = ?1", params![id], |r| r.get(0))?;
    if utilise > 0 {
        return Err(CoreError::Rule("cet intervenant est assigné à des tâches : désactivez-le plutôt".into()));
    }
    let nb = conn.execute("DELETE FROM intervenant WHERE id = ?1", params![id])?;
    if nb == 0 { return Err(CoreError::NotFound(format!("intervenant {id}"))); }
    Ok(())
}

// ===========================================================================
// Assignations (intervenant ↔ tâche) — planification par ressource humaine
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct Assignation {
    pub id: String,
    pub tache_id: String,
    pub intervenant_id: String,
    pub heures_allouees: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intervenant_nom: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intervenant_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_taux: Option<String>,
    #[serde(default)]
    pub taux: f64,
    /// Coût de cette assignation = heures × taux (journalier : heures/8 × taux).
    #[serde(default)]
    pub cout: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tache_nom: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouvelleAssignation {
    pub tache_id: String,
    pub intervenant_id: String,
    #[serde(default)]
    pub heures_allouees: f64,
}

/// Coût d'une charge = quantité × taux, sauf forfait (montant fixe). La
/// **quantité est dans l'unité du taux** : jours si journalier, heures si
/// horaire (`heures_allouees` sert de quantité générique).
fn cout_charge(quantite: f64, type_taux: &str, taux: f64) -> f64 {
    let brut = if type_taux == "forfait" { taux } else { quantite * taux };
    (brut * 100.0).round() / 100.0
}

fn ligne_assignation(r: &rusqlite::Row) -> rusqlite::Result<Assignation> {
    let heures: f64 = r.get(3)?;
    let type_taux: Option<String> = r.get(6)?;
    let taux: f64 = r.get::<_, Option<f64>>(7)?.unwrap_or(0.0);
    let cout = cout_charge(heures, type_taux.as_deref().unwrap_or("horaire"), taux);
    Ok(Assignation {
        id: r.get(0)?,
        tache_id: r.get(1)?,
        intervenant_id: r.get(2)?,
        heures_allouees: heures,
        intervenant_nom: r.get(4)?,
        intervenant_type: r.get(5)?,
        type_taux,
        taux,
        cout,
        tache_nom: r.get(8)?,
    })
}

const ASSIGN_COLS: &str = "SELECT a.id, a.tache_id, a.intervenant_id, a.heures_allouees,
        i.nom, i.type, i.type_taux, i.taux, t.nom
     FROM assignation a
     JOIN tache t ON t.id = a.tache_id
     LEFT JOIN intervenant i ON i.id = a.intervenant_id";

pub fn lister_assignations(conn: &Connection, projet_id: &str) -> Result<Vec<Assignation>> {
    let mut stmt = conn.prepare(&format!("{ASSIGN_COLS} WHERE t.projet_id = ?1 ORDER BY t.ordre"))?;
    let rows = stmt.query_map(params![projet_id], ligne_assignation)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn lire_assignation(conn: &Connection, id: &str) -> Result<Assignation> {
    conn.query_row(&format!("{ASSIGN_COLS} WHERE a.id = ?1"), params![id], ligne_assignation)
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => CoreError::NotFound(format!("assignation {id}")),
            autre => autre.into(),
        })
}

pub fn creer_assignation(conn: &Connection, n: &NouvelleAssignation) -> Result<Assignation> {
    if n.intervenant_id.trim().is_empty() {
        return Err(CoreError::Rule("choisissez un intervenant".into()));
    }
    let existe: bool = conn
        .query_row("SELECT 1 FROM assignation WHERE tache_id = ?1 AND intervenant_id = ?2",
            params![n.tache_id, n.intervenant_id], |_| Ok(true))
        .unwrap_or(false);
    if existe {
        return Err(CoreError::Rule("cet intervenant est déjà assigné à la tâche".into()));
    }
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO assignation (id, tache_id, intervenant_id, heures_allouees) VALUES (?1,?2,?3,?4)",
        params![id, n.tache_id, n.intervenant_id, n.heures_allouees.max(0.0)],
    )?;
    lire_assignation(conn, &id)
}

/// Met à jour la quantité allouée (jours ou heures selon le taux) d'une assignation.
pub fn modifier_assignation(conn: &Connection, id: &str, heures_allouees: f64) -> Result<Assignation> {
    let nb = conn.execute(
        "UPDATE assignation SET heures_allouees = ?2 WHERE id = ?1",
        params![id, heures_allouees.max(0.0)],
    )?;
    if nb == 0 {
        return Err(CoreError::NotFound(format!("assignation {id}")));
    }
    lire_assignation(conn, id)
}

pub fn supprimer_assignation(conn: &Connection, id: &str) -> Result<()> {
    let nb = conn.execute("DELETE FROM assignation WHERE id = ?1", params![id])?;
    if nb == 0 {
        return Err(CoreError::NotFound(format!("assignation {id}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn projet_simple(conn: &Connection) -> String {
        creer(conn, &NouveauProjet {
            nom: "Site vitrine".into(), client_id: None, chef_de_projet_id: None,
            date_debut_prevue: Some("2026-08-01".into()), date_fin_prevue: Some("2026-09-30".into()),
            date_debut_reelle: None, date_fin_reelle: None, statut: None, budget_global: 500000.0, note: None,
        }, Some("u1")).unwrap().id
    }
    fn nouvelle(projet: &str, nom: &str) -> NouvelleTache {
        NouvelleTache {
            projet_id: projet.into(), tache_parente_id: None, nom: nom.into(), description: None,
            date_debut_prevue: None, date_fin_prevue: None, date_debut_reelle: None, date_fin_reelle: None,
            statut: None, avancement: None, budget: 0.0,
        }
    }

    #[test]
    fn remontee_budget_dates_bas_vers_haut() {
        let conn = db::open_in_memory().unwrap();
        let p = projet_simple(&conn);
        // parent avec 2 sous-tâches feuilles portant budget + dates
        let parent = creer_tache(&conn, &nouvelle(&p, "Développement")).unwrap();
        let mut s1 = nouvelle(&p, "Front"); s1.tache_parente_id = Some(parent.id.clone());
        s1.budget = 300000.0; s1.date_debut_prevue = Some("2026-08-05".into()); s1.date_fin_prevue = Some("2026-08-10".into());
        creer_tache(&conn, &s1).unwrap();
        let mut s2 = nouvelle(&p, "Back"); s2.tache_parente_id = Some(parent.id.clone());
        s2.budget = 200000.0; s2.date_debut_prevue = Some("2026-08-01".into()); s2.date_fin_prevue = Some("2026-08-20".into());
        creer_tache(&conn, &s2).unwrap();

        let taches = lister_taches(&conn, &p).unwrap();
        let par = taches.iter().find(|t| t.id == parent.id).unwrap();
        assert_eq!(par.budget_calcule, 500000.0);                 // 300k + 200k
        assert_eq!(par.debut_calcule.as_deref(), Some("2026-08-01")); // plus tôt
        assert_eq!(par.fin_calcule.as_deref(), Some("2026-08-20"));   // plus tard
        assert!(par.a_enfants);
        assert_eq!(par.niveau, 1);

        // budget planifié projet = Σ feuilles = 500k (le parent ne compte pas)
        assert_eq!(lire(&conn, &p).unwrap().budget_planifie, 500000.0);
    }

    /// Le retard remonte aux parentes : sans cela, replier une branche ferait
    /// disparaître le retard du planning — exactement ce que l'utilisateur
    /// reprochait au Gantt (les barres sont colorées par niveau, pas par état).
    #[test]
    fn le_retard_remonte_aux_activites_parentes() {
        let conn = db::open_in_memory().unwrap();
        let p = projet_simple(&conn);
        let parent = creer_tache(&conn, &nouvelle(&p, "Gros œuvre")).unwrap();

        // Une feuille largement dépassée, non terminée.
        let mut r = nouvelle(&p, "Fondations"); r.tache_parente_id = Some(parent.id.clone());
        r.date_debut_prevue = Some("2020-01-01".into());
        r.date_fin_prevue = Some("2020-03-01".into());
        let retardee = creer_tache(&conn, &r).unwrap();

        // Une feuille tout aussi dépassée, mais TERMINÉE : ce n'est pas un retard.
        let mut f = nouvelle(&p, "Terrassement"); f.tache_parente_id = Some(parent.id.clone());
        f.date_fin_prevue = Some("2020-02-01".into());
        f.statut = Some(StatutTache::Terminee);
        creer_tache(&conn, &f).unwrap();

        // Une feuille sans date : rien à signaler, et surtout pas d'erreur.
        let mut s = nouvelle(&p, "Réserve"); s.tache_parente_id = Some(parent.id.clone());
        creer_tache(&conn, &s).unwrap();

        let taches = lister_taches(&conn, &p).unwrap();
        let t = |id: &str| taches.iter().find(|x| x.id == id).unwrap().clone();

        let ret = t(&retardee.id);
        assert!(ret.retard_jours.unwrap() > 2000, "retard réel : {:?}", ret.retard_jours);
        assert_eq!(ret.nb_en_retard, 1);

        let par = t(&parent.id);
        assert_eq!(par.retard_jours, ret.retard_jours, "la parente porte le plus grand retard");
        assert_eq!(par.nb_en_retard, 1, "seule la non terminée compte");

        // Terminer l'activité fait disparaître le retard, partout.
        changer_statut_taches(&conn, &[retardee.id.clone()], StatutTache::Terminee).unwrap();
        let taches = lister_taches(&conn, &p).unwrap();
        assert!(taches.iter().all(|x| x.retard_jours.is_none()));
        assert!(taches.iter().all(|x| x.nb_en_retard == 0));
    }

    #[test]
    fn profondeur_max_4_niveaux() {
        let conn = db::open_in_memory().unwrap();
        let p = projet_simple(&conn);
        let n1 = creer_tache(&conn, &nouvelle(&p, "N1")).unwrap();
        let mut t = nouvelle(&p, "N2"); t.tache_parente_id = Some(n1.id.clone());
        let n2 = creer_tache(&conn, &t).unwrap();
        let mut t = nouvelle(&p, "N3"); t.tache_parente_id = Some(n2.id.clone());
        let n3 = creer_tache(&conn, &t).unwrap();
        let mut t = nouvelle(&p, "N4"); t.tache_parente_id = Some(n3.id.clone());
        let n4 = creer_tache(&conn, &t).unwrap();       // niveau 4 OK
        // niveau 5 refusé
        let mut t = nouvelle(&p, "N5"); t.tache_parente_id = Some(n4.id.clone());
        assert!(creer_tache(&conn, &t).is_err());
    }

    #[test]
    fn action_maj_avancement_et_journal() {
        let conn = db::open_in_memory().unwrap();
        let p = projet_simple(&conn);
        let t = creer_tache(&conn, &nouvelle(&p, "Maquette")).unwrap();
        creer_action(&conn, &t.id, &NouvelleAction {
            avancement: Some(60), observation: Some("Première version".into()),
        }, Some("u1")).unwrap();
        assert_eq!(lire_tache(&conn, &t.id).unwrap().avancement, 60);
        // 100 % ⇒ terminée
        creer_action(&conn, &t.id, &NouvelleAction { avancement: Some(100), observation: None }, None).unwrap();
        assert_eq!(lire_tache(&conn, &t.id).unwrap().statut, "terminee");
        assert_eq!(lister_actions(&conn, &t.id).unwrap().len(), 2);
        // ni avancement ni observation ⇒ refus
        assert!(creer_action(&conn, &t.id, &NouvelleAction { avancement: None, observation: None }, None).is_err());
    }

    #[test]
    fn cout_main_oeuvre_interne_et_externe() {
        use crate::domain::{TypeIntervenant, TypeTaux};
        let conn = db::open_in_memory().unwrap();
        let p = projet_simple(&conn);
        let t = creer_tache(&conn, &nouvelle(&p, "Étude")).unwrap();
        // consultant externe, taux journalier 100 000 ; 2 jours alloués → 200 000
        let ext = creer_intervenant(&conn, &NouvelIntervenant {
            nom: "Consultant X".into(), r#type: TypeIntervenant::Externe, utilisateur_id: None,
            societe: Some("Cabinet".into()), role: None, type_taux: TypeTaux::Journalier, taux: 100_000.0, actif: true,
        }).unwrap();
        assert_eq!(ext.r#type, "externe");
        let a0 = creer_assignation(&conn, &NouvelleAssignation {
            tache_id: t.id.clone(), intervenant_id: ext.id.clone(), heures_allouees: 2.0,
        }).unwrap();
        let a = &lister_assignations(&conn, &p).unwrap()[0];
        assert_eq!(a.cout, 200_000.0);
        // modifier la quantité (3 jours → 300 000)
        modifier_assignation(&conn, &a0.id, 3.0).unwrap();
        assert_eq!(a.intervenant_nom.as_deref(), Some("Consultant X"));
        assert_eq!(lire(&conn, &p).unwrap().cout_main_oeuvre, 300_000.0);
    }

    #[test]
    fn ressources_cout() {
        use crate::domain::TypeRessource;
        let conn = db::open_in_memory().unwrap();
        let p = projet_simple(&conn);
        creer_ressource(&conn, &NouvelleRessource {
            projet_id: p.clone(), tache_id: None, r#type: TypeRessource::Materiel,
            libelle: "Ciment".into(), cout_unitaire: 4500.0, quantite: 10.0,
        }).unwrap();
        let r = lister_ressources(&conn, &p).unwrap();
        assert_eq!(r[0].cout, 45000.0);
        assert_eq!(lire(&conn, &p).unwrap().cout_ressources, 45000.0);
    }

    #[test]
    fn crud_projet_et_avancement() {
        let conn = db::open_in_memory().unwrap();
        let p = projet_simple(&conn);
        let t1 = creer_tache(&conn, &nouvelle(&p, "Maquette")).unwrap();
        let mut n2 = nouvelle(&p, "Intégration"); n2.avancement = Some(50);
        creer_tache(&conn, &n2).unwrap();

        // avancement projet = moyenne (0 + 50) = 25
        let proj = lire(&conn, &p).unwrap();
        assert_eq!(proj.nb_taches, 2);
        assert_eq!(proj.avancement, 25);

        // terminer t1 → avancement 100, projet passe à (100+50)/2 = 75
        changer_statut_taches(&conn, &[t1.id.clone()], StatutTache::Terminee).unwrap();
        assert_eq!(lire_tache(&conn, &t1.id).unwrap().avancement, 100);
        assert_eq!(lire(&conn, &p).unwrap().avancement, 75);
    }

    #[test]
    fn suppression_projet_purge_taches() {
        let conn = db::open_in_memory().unwrap();
        let p = projet_simple(&conn);
        creer_tache(&conn, &nouvelle(&p, "T1")).unwrap();
        supprimer(&conn, &p).unwrap();
        assert!(lire(&conn, &p).is_err());
        assert_eq!(lister_taches(&conn, &p).unwrap().len(), 0);
    }

    #[test]
    fn nom_projet_requis() {
        let conn = db::open_in_memory().unwrap();
        let mut n = NouveauProjet {
            nom: "  ".into(), client_id: None, chef_de_projet_id: None, date_debut_prevue: None,
            date_fin_prevue: None, date_debut_reelle: None, date_fin_reelle: None, statut: None,
            budget_global: 0.0, note: None,
        };
        assert!(creer(&conn, &n, None).is_err());
        n.nom = "OK".into();
        assert!(creer(&conn, &n, None).is_ok());
    }
}
