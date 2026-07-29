//! Passation et suivi des marchés (migration 0037).
//!
//! Transposé du module « Passation de Marché » de l'application OLAC, étendu aux
//! **soumissionnaires**, **avenants** et **réceptions** à la demande de
//! l'utilisateur. Spec : `claude_spec/SPEC_MODULE_MARCHES.md`.
//!
//! # L'idée qui structure tout
//!
//! **Le type de marché porte sa procédure.** On choisit « Travaux » et les
//! étapes s'instancient seules, leurs dates calculées par **cumul des durées**
//! depuis la date de lancement. C'est le rapport modèle → instance déjà employé
//! entre une recette et un ordre de fabrication : le modèle amorce, l'instance
//! vit ensuite sa vie et reste modifiable au cas par cas.
//!
//! Le libellé de l'étape est **recopié**, jamais joint : corriger une procédure
//! ne doit pas réécrire l'histoire des marchés déjà lancés.
//!
//! # Ce que le module ne fait pas
//!
//! **Il ne bloque pas.** Une étape en retard n'empêche pas la suivante, un
//! montant dépassé n'empêche pas d'enregistrer. Tout est signalé en `alertes`,
//! rien n'est refusé — le terrain ne s'arrête pas parce qu'un logiciel n'est pas
//! content. Et **aucune date n'est recalculée en cascade** sans un geste
//! explicite : `plan_replanification` propose, `replanifier` applique.

use crate::error::{CoreError, Result};
use crate::now;
use chrono::{Duration, NaiveDate};
use rusqlite::{params, Connection, OptionalExtension, Row};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

fn vide(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|x| !x.is_empty())
}

fn aujourdhui() -> String {
    now()[..10].to_string()
}

/// Ajoute des jours à une date « AAAA-MM-JJ ». Renvoie la date de départ si elle
/// est illisible : mieux vaut une date approximative qu'une erreur bloquante.
fn ajouter_jours(date: &str, jours: i64) -> String {
    match NaiveDate::parse_from_str(&date[..date.len().min(10)], "%Y-%m-%d") {
        Ok(d) => (d + Duration::days(jours)).format("%Y-%m-%d").to_string(),
        Err(_) => date.to_string(),
    }
}

fn jours_entre(debut: &str, fin: &str) -> i64 {
    match (
        NaiveDate::parse_from_str(&debut[..debut.len().min(10)], "%Y-%m-%d"),
        NaiveDate::parse_from_str(&fin[..fin.len().min(10)], "%Y-%m-%d"),
    ) {
        (Ok(d), Ok(f)) => (f - d).num_days(),
        _ => 0,
    }
}

// ===========================================================================
// Phases de la procédure (migration 0039)
//
// Le dénominateur commun de toutes les procédures : elles n'ont ni le même
// nombre d'étapes ni les mêmes libellés, mais elles se rangent toutes dans ces
// six phases. C'est ce qui permet d'afficher TOUS les marchés côte à côte.
// ===========================================================================

/// Les phases, **dans l'ordre de déroulement**. Cet ordre fait foi : il sert à
/// situer un marché et à mesurer le temps passé dans chacune.
pub const PHASES: &[(&str, &str)] = &[
    ("preparation", "Préparation"),
    ("consultation", "Consultation"),
    ("evaluation", "Évaluation"),
    ("attribution", "Attribution"),
    ("contractualisation", "Contractualisation"),
    ("execution", "Exécution"),
];

/// Libellé lisible d'une phase — sert aussi à l'export Excel.
pub fn libelle_phase(code: &str) -> &str {
    PHASES.iter().find(|(c, _)| *c == code).map(|(_, l)| *l).unwrap_or(code)
}

fn rang_phase(code: &str) -> usize {
    PHASES.iter().position(|(c, _)| *c == code).unwrap_or(usize::MAX)
}

// ===========================================================================
// Types de marché et procédures
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct EtapeModele {
    pub id: String,
    pub type_id: String,
    pub libelle: String,
    /// Phase de la procédure à laquelle cette étape appartient (migration 0039).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub ordre: i64,
    pub duree_prevue_jours: i64,
    pub obligatoire: bool,
    pub actif: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct TypeMarche {
    pub id: String,
    pub code: String,
    pub libelle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub actif: bool,
    /// Durée totale de la procédure, somme des étapes : ce que le type « coûte »
    /// en délai avant même de commencer.
    pub duree_totale_jours: i64,
    pub nb_marches: i64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub etapes: Vec<EtapeModele>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouvelleEtapeModele {
    pub libelle: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub duree_prevue_jours: i64,
    #[serde(default = "vrai")]
    pub obligatoire: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouveauType {
    pub code: String,
    pub libelle: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default = "vrai")]
    pub actif: bool,
    /// Liste complète des étapes : elle **remplace** l'existante (le formulaire
    /// envoie toujours la procédure entière).
    #[serde(default)]
    pub etapes: Vec<NouvelleEtapeModele>,
}

fn vrai() -> bool {
    true
}

const TYPE_COLS: &str = "SELECT t.id, t.code, t.libelle, t.description, t.actif,
        (SELECT COALESCE(SUM(e.duree_prevue_jours), 0) FROM marche_etape_modele e
          WHERE e.type_id = t.id AND e.actif = 1),
        (SELECT COUNT(*) FROM marche m WHERE m.type_id = t.id)
   FROM marche_type t";

fn ligne_type(r: &Row) -> rusqlite::Result<TypeMarche> {
    Ok(TypeMarche {
        id: r.get(0)?,
        code: r.get(1)?,
        libelle: r.get(2)?,
        description: r.get(3)?,
        actif: r.get::<_, i64>(4)? != 0,
        duree_totale_jours: r.get(5)?,
        nb_marches: r.get(6)?,
        etapes: Vec::new(),
    })
}

fn etapes_modele(conn: &Connection, type_id: &str) -> Result<Vec<EtapeModele>> {
    let mut st = conn.prepare(
        "SELECT id, type_id, libelle, description, ordre, duree_prevue_jours, obligatoire, actif, phase
           FROM marche_etape_modele WHERE type_id = ?1 ORDER BY ordre",
    )?;
    let v = st
        .query_map(params![type_id], |r| {
            Ok(EtapeModele {
                id: r.get(0)?,
                type_id: r.get(1)?,
                libelle: r.get(2)?,
                description: r.get(3)?,
                ordre: r.get(4)?,
                duree_prevue_jours: r.get(5)?,
                obligatoire: r.get::<_, i64>(6)? != 0,
                actif: r.get::<_, i64>(7)? != 0,
                phase: r.get(8)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn lister_types(conn: &Connection, actifs_seulement: bool) -> Result<Vec<TypeMarche>> {
    let sql = format!(
        "{TYPE_COLS} {} ORDER BY t.libelle",
        if actifs_seulement { "WHERE t.actif = 1" } else { "" }
    );
    let mut st = conn.prepare(&sql)?;
    let mut types = st.query_map([], ligne_type)?.collect::<rusqlite::Result<Vec<_>>>()?;
    for t in &mut types {
        t.etapes = etapes_modele(conn, &t.id)?;
    }
    Ok(types)
}

pub fn lire_type(conn: &Connection, id: &str) -> Result<TypeMarche> {
    let mut st = conn.prepare(&format!("{TYPE_COLS} WHERE t.id = ?1"))?;
    let mut t = st
        .query_row(params![id], ligne_type)
        .map_err(|_| CoreError::NotFound(format!("type de marché {id}")))?;
    t.etapes = etapes_modele(conn, id)?;
    Ok(t)
}

fn ecrire_etapes_modele(conn: &Connection, type_id: &str, etapes: &[NouvelleEtapeModele]) -> Result<()> {
    // Les étapes déjà instanciées sur un marché pointent sur ces lignes : on
    // détache d'abord, on ne casse jamais une clé étrangère.
    conn.execute(
        "UPDATE marche_etape SET etape_modele_id = NULL
          WHERE etape_modele_id IN (SELECT id FROM marche_etape_modele WHERE type_id = ?1)",
        params![type_id],
    )?;
    conn.execute("DELETE FROM marche_etape_modele WHERE type_id = ?1", params![type_id])?;
    for (i, e) in etapes.iter().enumerate() {
        if e.libelle.trim().is_empty() {
            continue;
        }
        conn.execute(
            "INSERT INTO marche_etape_modele
                (id, type_id, libelle, description, ordre, duree_prevue_jours, obligatoire, actif)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 1)",
            params![
                Uuid::new_v4().to_string(), type_id, e.libelle.trim(),
                vide(&e.description), i as i64 + 1,
                e.duree_prevue_jours.max(0), e.obligatoire as i64
            ],
        )?;
    }
    Ok(())
}

pub fn creer_type(conn: &Connection, t: &NouveauType, par: Option<&str>) -> Result<TypeMarche> {
    if t.code.trim().is_empty() || t.libelle.trim().is_empty() {
        return Err(CoreError::Rule("le code et le libellé du type sont obligatoires".into()));
    }
    let existe: i64 = conn.query_row(
        "SELECT COUNT(*) FROM marche_type WHERE code = ?1",
        params![t.code.trim()],
        |r| r.get(0),
    )?;
    if existe > 0 {
        return Err(CoreError::Rule(format!("le code {} existe déjà", t.code.trim())));
    }
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO marche_type (id, code, libelle, description, actif, cree_par, cree_le)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![id, t.code.trim(), t.libelle.trim(), vide(&t.description), t.actif as i64, par, now()],
    )?;
    ecrire_etapes_modele(conn, &id, &t.etapes)?;
    lire_type(conn, &id)
}

pub fn modifier_type(conn: &Connection, id: &str, t: &NouveauType) -> Result<TypeMarche> {
    lire_type(conn, id)?;
    if t.libelle.trim().is_empty() {
        return Err(CoreError::Rule("le libellé du type est obligatoire".into()));
    }
    conn.execute(
        "UPDATE marche_type SET code = ?2, libelle = ?3, description = ?4, actif = ?5 WHERE id = ?1",
        params![id, t.code.trim(), t.libelle.trim(), vide(&t.description), t.actif as i64],
    )?;
    ecrire_etapes_modele(conn, id, &t.etapes)?;
    lire_type(conn, id)
}

/// Supprimer un type **détache** les marchés qui l'utilisaient : ils gardent
/// leurs étapes déjà instanciées. On ne détruit jamais l'historique.
pub fn supprimer_type(conn: &Connection, id: &str) -> Result<()> {
    lire_type(conn, id)?;
    conn.execute(
        "UPDATE marche_etape SET etape_modele_id = NULL
          WHERE etape_modele_id IN (SELECT id FROM marche_etape_modele WHERE type_id = ?1)",
        params![id],
    )?;
    conn.execute("DELETE FROM marche_etape_modele WHERE type_id = ?1", params![id])?;
    conn.execute("UPDATE marche SET type_id = NULL WHERE type_id = ?1", params![id])?;
    conn.execute("DELETE FROM marche_type WHERE id = ?1", params![id])?;
    Ok(())
}

// ===========================================================================
// Étapes suivies
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct Etape {
    pub id: String,
    pub marche_id: String,
    /// L'étape de procédure dont celle-ci est issue. Traçabilité seulement —
    /// l'instance est autonome — mais c'est elle qui permet d'ALIGNER les
    /// marchés d'un même type dans un tableau comparatif, même si l'un d'eux a
    /// reçu une étape supplémentaire au milieu.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub etape_modele_id: Option<String>,
    pub libelle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub ordre: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_prevue: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_effective: Option<String>,
    pub statut: String,
    pub obligatoire: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observations: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valide_par: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub valide_le: Option<String>,
    /// Jours de retard si l'étape est en cours et sa date prévue dépassée.
    /// **Signalement**, jamais un blocage.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retard_jours: Option<i64>,
    /// L'étape validée reste-t-elle modifiable ? (fenêtre paramétrable, 30 j par
    /// défaut — codée en dur dans OLAC, réglage ici.)
    pub modifiable: bool,
    pub nb_documents: i64,

    // --- Enchaînement (migration 0038) -------------------------------------
    /// Une étape antérieure obligatoire n'est pas franchie : cette étape ne peut
    /// pas l'être non plus, sauf dérogation motivée.
    pub verrouillee: bool,
    /// Ce qui bloque, nommément — « Terminez d'abord : Ouverture des plis ».
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raison_verrou: Option<String>,
    /// L'étape du moment : la première non terminée. Il n'y en a qu'une.
    pub est_courante: bool,
    /// Cette étape a été franchie **hors de son rang**, en l'assumant.
    pub derogation: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motif_derogation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub derogation_par: Option<String>,
    /// Phase de la procédure. `NULL` en base signifie « même phase que l'étape
    /// précédente » : une étape ajoutée au milieu appartient naturellement à la
    /// phase en cours, et l'utilisateur n'a rien à renseigner.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,

    /// **Écart entre le réalisé et le prévu, en jours.**
    /// `+` = retard, `−` = avance, `0` = à l'heure.
    ///
    /// Deux cas le renseignent :
    /// - l'étape est **faite** : réalisé − prévu, c'est un écart constaté ;
    /// - l'étape n'est **pas faite mais son échéance est passée** : le retard
    ///   court depuis l'échéance jusqu'à aujourd'hui. Il existe même si l'acte
    ///   n'est pas posé — c'est justement le dossier qui traîne.
    ///
    /// `None` quand il n'y a rien à dire : pas de date prévue, ou échéance
    /// encore à venir.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ecart_jours: Option<i64>,
    /// L'écart est un retard **en cours** (étape pas encore faite), et non un
    /// écart constaté. L'écran et l'export le distinguent visuellement.
    pub ecart_en_cours: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MajEtape {
    #[serde(default)]
    pub libelle: Option<String>,
    #[serde(default)]
    pub date_prevue: Option<String>,
    #[serde(default)]
    pub date_effective: Option<String>,
    #[serde(default)]
    pub statut: Option<String>,
    #[serde(default)]
    pub observations: Option<String>,
    #[serde(default)]
    pub obligatoire: Option<bool>,
    /// Motif de dérogation, quand on franchit l'étape hors de son rang.
    #[serde(default)]
    pub motif_derogation: Option<String>,
}

fn delai_modification(conn: &Connection) -> i64 {
    conn.query_row(
        "SELECT valeur FROM parametre_global WHERE cle = 'marche_delai_modification_suivi_jours'",
        [],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .and_then(|v| v.trim().parse().ok())
    .unwrap_or(30)
}

fn charger_etapes(conn: &Connection, marche_id: &str) -> Result<Vec<Etape>> {
    let delai = delai_modification(conn);
    let today = aujourdhui();
    let mut st = conn.prepare(
        // ⚠️ `valide_par` et `derogation_par` stockent un IDENTIFIANT technique.
        // On joint `utilisateur` pour rendre un NOM : « 543bbf1d-33e1-… » à
        // l'écran ne veut rien dire pour personne (constaté par l'utilisateur).
        // COALESCE : si le compte a été supprimé, on garde l'identifiant plutôt
        // que d'afficher un vide — la trace prime sur l'esthétique.
        "SELECT e.id, e.marche_id, e.libelle, e.description, e.ordre, e.date_prevue,
                e.date_effective, e.statut, e.obligatoire, e.observations,
                COALESCE(uv.nom, e.valide_par), e.valide_le,
                (SELECT COUNT(*) FROM document_joint d WHERE d.marche_etape_id = e.id),
                e.derogation, e.motif_derogation, COALESCE(ud.nom, e.derogation_par),
                e.phase, e.etape_modele_id
           FROM marche_etape e
           LEFT JOIN utilisateur uv ON uv.id = e.valide_par
           LEFT JOIN utilisateur ud ON ud.id = e.derogation_par
          WHERE e.marche_id = ?1 ORDER BY e.ordre",
    )?;
    let mut v = st
        .query_map(params![marche_id], |r| {
            let statut: String = r.get(7)?;
            let date_prevue: Option<String> = r.get(5)?;
            let valide_le: Option<String> = r.get(11)?;
            // Une étape en cours dont la date prévue est passée est en retard.
            let retard = match (&statut[..], &date_prevue) {
                ("en_cours", Some(d)) if d.as_str() < today.as_str() => {
                    Some(jours_entre(d, &today))
                }
                _ => None,
            };
            // Une étape validée se fige au bout du délai ; les autres restent libres.
            let modifiable = match &valide_le {
                Some(v) => jours_entre(&v[..v.len().min(10)], &today) <= delai,
                None => true,
            };
            // Écart réalisé / prévu. Voir la documentation du champ : un écart
            // constaté quand l'étape est faite, un retard qui court sinon.
            let date_effective: Option<String> = r.get(6)?;
            let (ecart_jours, ecart_en_cours) = match (&date_prevue, &date_effective) {
                (Some(p), Some(e)) => (Some(jours_entre(p, e)), false),
                (Some(p), None) if statut != "termine" && statut != "annule"
                                   && p.as_str() < today.as_str() => {
                    (Some(jours_entre(p, &today)), true)
                }
                _ => (None, false),
            };
            Ok(Etape {
                id: r.get(0)?,
                marche_id: r.get(1)?,
                libelle: r.get(2)?,
                description: r.get(3)?,
                ordre: r.get(4)?,
                date_prevue,
                date_effective,
                statut,
                obligatoire: r.get::<_, i64>(8)? != 0,
                observations: r.get(9)?,
                valide_par: r.get(10)?,
                valide_le,
                retard_jours: retard,
                modifiable,
                nb_documents: r.get(12)?,
                // Posés juste après, en un seul parcours : ils dépendent des
                // AUTRES étapes, pas de la ligne courante.
                verrouillee: false,
                raison_verrou: None,
                est_courante: false,
                derogation: r.get::<_, i64>(13)? != 0,
                motif_derogation: r.get(14)?,
                derogation_par: r.get(15)?,
                phase: r.get(16)?,
                etape_modele_id: r.get(17)?,
                ecart_jours,
                ecart_en_cours,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    poser_enchainement(&mut v);
    Ok(v)
}

/// La dernière étape franchie **avant** un rang donné, avec sa date effective.
/// C'est le plancher chronologique : rien ne peut être daté avant elle.
///
/// On prend la **plus récente** des étapes antérieures, pas la précédente
/// immédiate : si l'étape 2 a été faite après l'étape 3 (cas d'une saisie dans
/// le désordre), c'est bien la plus tardive qui fait plancher.
fn derniere_date_avant(etapes: &[Etape], ordre: i64) -> Option<(String, String)> {
    etapes
        .iter()
        .filter(|e| e.ordre < ordre && e.statut == "termine")
        .filter_map(|e| e.date_effective.clone().map(|d| (e.libelle.clone(), d)))
        .max_by(|a, b| a.1.cmp(&b.1))
}

/// Première étape franchie **après** un rang donné : le plafond chronologique.
/// Corriger la date d'une étape ancienne ne doit pas la faire passer après des
/// actes qui l'ont suivie.
fn premiere_date_apres(etapes: &[Etape], ordre: i64) -> Option<(String, String)> {
    etapes
        .iter()
        .filter(|e| e.ordre > ordre && e.statut == "termine")
        .filter_map(|e| e.date_effective.clone().map(|d| (e.libelle.clone(), d)))
        .min_by(|a, b| a.1.cmp(&b.1))
}

/// Marque l'étape courante et celles qui sont verrouillées.
///
/// **La règle** : une étape est verrouillée tant qu'une étape **obligatoire qui
/// la précède** n'est pas terminée. Une étape déjà franchie ne se verrouille
/// jamais — on ne réécrit pas le passé, on le rouvre explicitement.
///
/// Les étapes **facultatives** ne bloquent pas la suite : les sauter est prévu.
/// Une étape `annule` ne bloque pas non plus — elle a été écartée sciemment.
fn poser_enchainement(etapes: &mut [Etape]) {
    // Phase par CONTINUITÉ : une étape sans phase hérite de la précédente.
    // C'est ce qui permet d'ajouter une étape au milieu d'une procédure sans
    // rien renseigner et sans trouer le tableau de suivi.
    let mut derniere: Option<String> = None;
    for e in etapes.iter_mut() {
        match &e.phase {
            Some(p) if !p.trim().is_empty() => derniere = Some(p.clone()),
            _ => e.phase = derniere.clone(),
        }
    }
    // Une procédure qui commencerait sans phase : tout ce qui précède la
    // première phase connue relève de la préparation.
    if let Some(premiere) = etapes.iter().find(|e| e.phase.is_some()).map(|e| e.phase.clone()) {
        let _ = premiere;
        for e in etapes.iter_mut() {
            if e.phase.is_none() {
                e.phase = Some("preparation".to_string());
            }
        }
    }

    let mut bloquant: Option<String> = None;
    let mut courante_posee = false;
    for e in etapes.iter_mut() {
        let franchie = e.statut == "termine";
        if !franchie {
            // La première étape non franchie est « celle du moment ».
            if !courante_posee && bloquant.is_none() {
                e.est_courante = true;
                courante_posee = true;
            }
            if let Some(b) = &bloquant {
                e.verrouillee = true;
                e.raison_verrou = Some(format!("Terminez d'abord : {b}"));
            }
        }
        // Cette étape devient-elle le verrou des suivantes ?
        if bloquant.is_none() && e.obligatoire && !franchie && e.statut != "annule" {
            bloquant = Some(e.libelle.clone());
        }
    }
}

/// Change le statut d'une étape. Passer à `termine` **horodate** la date
/// effective et enregistre qui a validé — c'est la valeur probante du module.
/// Résultat d'un changement de statut d'étape : l'étape elle-même, et surtout
/// **ce que le geste a entraîné ailleurs**. Rouvrir une étape franchie remet en
/// cause tout ce qui en découle ; l'écran doit pouvoir le dire.
#[derive(Debug, Clone, Serialize)]
pub struct EffetEtape {
    pub etape: Etape,
    /// Étapes repassées « à faire » parce que celle-ci a été rouverte.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub etapes_rouvertes: Vec<String>,
}

pub fn changer_statut_etape(
    conn: &Connection,
    etape_id: &str,
    statut: &str,
    par: Option<&str>,
) -> Result<Etape> {
    changer_statut_etape_avec(conn, etape_id, statut, par, None).map(|e| e.etape)
}

/// Change le statut d'une étape **en respectant la chaîne de la procédure**.
///
/// # Les trois règles
///
/// 1. **On ne franchit pas une étape verrouillée.** Une procédure de passation
///    est une suite d'actes qui se fondent l'un l'autre : évaluer des offres que
///    l'on n'a pas ouvertes ne veut rien dire.
/// 2. **Sauf dérogation motivée.** `motif_derogation` lève le verrou, en le
///    traçant nominativement. Sans cette porte, il deviendrait impossible de
///    saisir un dossier déjà commencé sur papier.
/// 3. **Rouvrir une étape franchie rouvre tout ce qui en découle** (cascade,
///    décision utilisateur du 2026-07-28). Les validations effacées sont
///    consignées dans les observations de chaque étape touchée : on ne fait
///    jamais disparaître une trace en silence.
/// Ce que l'utilisateur saisit en changeant l'état d'une étape : **quand** cela
/// s'est passé et **ce qui s'est dit**. Franchir une étape est un acte daté ;
/// laisser le logiciel mettre « aujourd'hui » d'office fait perdre la date
/// réelle dès qu'on saisit avec un jour de retard.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct SaisieEtape {
    #[serde(default)]
    pub date_effective: Option<String>,
    #[serde(default)]
    pub observations: Option<String>,
    #[serde(default)]
    pub motif_derogation: Option<String>,
}

pub fn changer_statut_etape_avec(
    conn: &Connection,
    etape_id: &str,
    statut: &str,
    par: Option<&str>,
    motif_derogation: Option<&str>,
) -> Result<EffetEtape> {
    changer_statut_etape_saisie(conn, etape_id, statut, par, &SaisieEtape {
        motif_derogation: motif_derogation.map(str::to_string),
        ..Default::default()
    })
}

pub fn changer_statut_etape_saisie(
    conn: &Connection,
    etape_id: &str,
    statut: &str,
    par: Option<&str>,
    saisie: &SaisieEtape,
) -> Result<EffetEtape> {
    let motif_derogation = saisie.motif_derogation.as_deref();
    const STATUTS: &[&str] = &["en_attente", "en_cours", "termine", "annule", "reporte"];
    if !STATUTS.contains(&statut) {
        return Err(CoreError::Rule(format!("statut d'étape inconnu : {statut}")));
    }
    let marche_id: String = conn
        .query_row("SELECT marche_id FROM marche_etape WHERE id = ?1", params![etape_id], |r| r.get(0))
        .map_err(|_| CoreError::NotFound(format!("étape {etape_id}")))?;

    let avant = charger_etapes(conn, &marche_id)?;
    let cette = avant
        .iter()
        .find(|e| e.id == etape_id)
        .ok_or_else(|| CoreError::NotFound(format!("étape {etape_id}")))?
        .clone();

    // Un recours ouvert gèle la procédure : c'est un arrêt subi, pas un oubli.
    if matches!(statut, "termine" | "en_cours") {
        if let Some(r) = recours_ouvert(conn, &marche_id)? {
            if motif_derogation.is_none() {
                return Err(CoreError::Rule(format!(
                    "un recours est en cours ({r}) : la procédure est gelée jusqu'à la décision"
                )));
            }
        }
    }

    let derogation = motif_derogation
        .map(str::trim)
        .filter(|m| !m.is_empty());

    // --- Règle chronologique ---
    // Le pendant temporel du verrou d'ordre : un acte ne peut pas être daté
    // AVANT celui qui le rend possible. « Publication de l'avis » faite le
    // 04/11/2025 alors que la préparation du dossier l'a été le 28/07/2026 est
    // une impossibilité, pas un retard (constaté sur les données réelles).
    if statut == "termine" {
        if let Some(d) = vide(&saisie.date_effective) {
            if let Some((lib, quand)) = derniere_date_avant(&avant, cette.ordre) {
                if d < quand.as_str() {
                    match derogation {
                        None => {
                            return Err(CoreError::Rule(format!(
                                "cette étape ne peut pas être faite le {d} : l'étape précédente \
                                 « {lib} » l'a été le {quand}. Un acte ne peut pas précéder celui \
                                 qui le rend possible. Vous pouvez passer outre en donnant le motif."
                            )))
                        }
                        Some(m) => {
                            conn.execute(
                                "UPDATE marche_etape SET derogation = 1, motif_derogation = ?2,
                                        derogation_par = ?3, derogation_le = ?4 WHERE id = ?1",
                                params![etape_id, m, par, now()],
                            )?;
                        }
                    }
                }
            }
        }
    }

    // --- Règle 1 et 2 : le verrou, et sa dérogation ---
    if statut == "termine" && cette.verrouillee {
        let raison = cette.raison_verrou.clone().unwrap_or_default();
        match derogation {
            None => {
                return Err(CoreError::Rule(format!(
                    "cette étape vient après une autre qui n'est pas terminée. {raison}. \
                     Vous pouvez passer outre, mais il faudra en donner le motif."
                )))
            }
            Some(m) => {
                conn.execute(
                    "UPDATE marche_etape SET derogation = 1, motif_derogation = ?2,
                            derogation_par = ?3, derogation_le = ?4 WHERE id = ?1",
                    params![etape_id, m, par, now()],
                )?;
            }
        }
    }

    // --- Règle 3 : rouvrir une étape franchie rouvre ce qui en découle ---
    let mut rouvertes: Vec<String> = Vec::new();
    if cette.statut == "termine" && statut != "termine" {
        let suivantes: Vec<Etape> = avant
            .iter()
            .filter(|e| e.ordre > cette.ordre && e.statut != "en_attente")
            .cloned()
            .collect();
        for s in suivantes {
            // La validation effacée est consignée : la trace ne disparaît pas.
            let trace = format!(
                "Rouverte le {} : l'étape « {} » a été remise en cause{}.",
                aujourdhui(),
                cette.libelle,
                match (&s.date_effective, &s.valide_par) {
                    (Some(d), Some(v)) => format!(" (était terminée le {d}, validée par {v})"),
                    (Some(d), None) => format!(" (était terminée le {d})"),
                    _ => String::new(),
                }
            );
            conn.execute(
                "UPDATE marche_etape SET statut = 'en_attente', date_effective = NULL,
                        valide_par = NULL, valide_le = NULL,
                        observations = TRIM(COALESCE(observations || char(10), '') || ?2)
                  WHERE id = ?1",
                params![s.id, trace],
            )?;
            rouvertes.push(s.libelle.clone());
        }
    }

    // La date saisie prime : c'est la date RÉELLE de l'acte. Sans saisie, on
    // retombe sur celle déjà connue, puis sur aujourd'hui.
    let date_acte = vide(&saisie.date_effective)
        .map(str::to_string)
        .unwrap_or_else(aujourdhui);
    if statut == "termine" {
        conn.execute(
            "UPDATE marche_etape SET statut = 'termine', date_effective = ?2,
                    valide_par = ?3, valide_le = ?4
              WHERE id = ?1",
            params![etape_id, date_acte, par, now()],
        )?;
    } else {
        conn.execute(
            "UPDATE marche_etape SET statut = ?2 WHERE id = ?1",
            params![etape_id, statut],
        )?;
    }
    // L'observation s'AJOUTE à l'historique de l'étape, elle ne l'écrase pas :
    // ce sont des faits successifs, pas une valeur qu'on corrige.
    if let Some(obs) = vide(&saisie.observations) {
        conn.execute(
            "UPDATE marche_etape
                SET observations = TRIM(COALESCE(observations || char(10), '') || ?2)
              WHERE id = ?1",
            params![etape_id, format!("{date_acte} — {obs}")],
        )?;
    }

    // Une seule étape « en cours » à la fois : c'est l'étape du moment.
    if statut == "en_cours" {
        conn.execute(
            "UPDATE marche_etape SET statut = 'en_attente'
              WHERE marche_id = ?1 AND id <> ?2 AND statut = 'en_cours'",
            params![marche_id, etape_id],
        )?;
    }

    let etape = charger_etapes(conn, &marche_id)?
        .into_iter()
        .find(|e| e.id == etape_id)
        .ok_or_else(|| CoreError::NotFound(format!("étape {etape_id}")))?;
    Ok(EffetEtape { etape, etapes_rouvertes: rouvertes })
}

pub fn modifier_etape(conn: &Connection, etape_id: &str, m: &MajEtape) -> Result<Etape> {
    let marche_id: String = conn
        .query_row("SELECT marche_id FROM marche_etape WHERE id = ?1", params![etape_id], |r| r.get(0))
        .map_err(|_| CoreError::NotFound(format!("étape {etape_id}")))?;

    // ⚠️ Un changement de statut passe par la RÈGLE D'ENCHAÎNEMENT, jamais par
    // un UPDATE direct : sans cela ce formulaire serait une porte dérobée qui
    // laisserait franchir une étape verrouillée. Les autres champs (libellé,
    // dates, observations) restent libres.
    if let Some(s) = vide(&m.statut) {
        let actuel: String = conn.query_row(
            "SELECT statut FROM marche_etape WHERE id = ?1", params![etape_id], |r| r.get(0))?;
        if s != actuel {
            changer_statut_etape_saisie(conn, etape_id, s, None, &SaisieEtape {
                date_effective: m.date_effective.clone(),
                observations: None,
                motif_derogation: m.motif_derogation.clone(),
            })?;
        }
    }

    // Corriger une date effective doit rester chronologiquement possible, DANS
    // LES DEUX SENS : ni avant l'acte qui précède, ni après ceux qui ont suivi.
    if let Some(d) = vide(&m.date_effective) {
        if m.motif_derogation.is_none() {
            let etapes = charger_etapes(conn, &marche_id)?;
            if let Some(e) = etapes.iter().find(|x| x.id == etape_id) {
                if let Some((lib, quand)) = derniere_date_avant(&etapes, e.ordre) {
                    if d < quand.as_str() {
                        return Err(CoreError::Rule(format!(
                            "date impossible : l'étape précédente « {lib} » a été faite le {quand}."
                        )));
                    }
                }
                if let Some((lib, quand)) = premiere_date_apres(&etapes, e.ordre) {
                    if d > quand.as_str() {
                        return Err(CoreError::Rule(format!(
                            "date impossible : l'étape suivante « {lib} » a déjà été faite le {quand}."
                        )));
                    }
                }
            }
        }
    }

    conn.execute(
        "UPDATE marche_etape SET
            libelle = COALESCE(?2, libelle),
            date_prevue = COALESCE(?3, date_prevue),
            date_effective = COALESCE(?4, date_effective),
            observations = COALESCE(?5, observations),
            obligatoire = COALESCE(?6, obligatoire)
         WHERE id = ?1",
        params![
            etape_id, vide(&m.libelle), vide(&m.date_prevue), vide(&m.date_effective),
            vide(&m.observations), m.obligatoire.map(|b| b as i64)
        ],
    )?;
    charger_etapes(conn, &marche_id)?
        .into_iter()
        .find(|e| e.id == etape_id)
        .ok_or_else(|| CoreError::NotFound(format!("étape {etape_id}")))
}

#[derive(Debug, Clone, Serialize)]
pub struct DecalageEtape {
    pub etape_id: String,
    pub libelle: String,
    pub date_actuelle: Option<String>,
    pub date_proposee: String,
    pub decalage_jours: i64,
}

/// **Aperçu sans écriture** : ce que deviendraient les dates des étapes qui
/// suivent, si l'on repartait de la date effective de l'étape donnée.
///
/// ⚠️ Djigui ne recalcule JAMAIS les dates tout seul (barrière « cascade » de la
/// spec Gestion de projet). Cette fonction propose, [`replanifier`] applique —
/// et seulement sur un geste explicite de l'utilisateur.
pub fn plan_replanification(conn: &Connection, etape_id: &str) -> Result<Vec<DecalageEtape>> {
    let (marche_id, ordre, date_ref): (String, i64, Option<String>) = conn
        .query_row(
            "SELECT marche_id, ordre, COALESCE(date_effective, date_prevue)
               FROM marche_etape WHERE id = ?1",
            params![etape_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .map_err(|_| CoreError::NotFound(format!("étape {etape_id}")))?;
    let Some(mut curseur) = date_ref else { return Ok(Vec::new()) };

    let mut st = conn.prepare(
        "SELECT e.id, e.libelle, e.date_prevue,
                COALESCE((SELECT m.duree_prevue_jours FROM marche_etape_modele m
                           WHERE m.id = e.etape_modele_id), 0)
           FROM marche_etape e
          WHERE e.marche_id = ?1 AND e.ordre > ?2 AND e.statut NOT IN ('termine','annule')
          ORDER BY e.ordre",
    )?;
    let suivantes = st
        .query_map(params![marche_id, ordre], |r| {
            Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?, r.get::<_, i64>(3)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut plan = Vec::new();
    for (id, libelle, date_actuelle, duree) in suivantes {
        // Une durée inconnue (étape ajoutée à la main) vaut au moins un jour :
        // sinon toutes les étapes s'empileraient sur la même date.
        curseur = ajouter_jours(&curseur, duree.max(1));
        let decalage = date_actuelle.as_deref().map(|d| jours_entre(d, &curseur)).unwrap_or(0);
        plan.push(DecalageEtape {
            etape_id: id,
            libelle,
            date_actuelle,
            date_proposee: curseur.clone(),
            decalage_jours: decalage,
        });
    }
    Ok(plan)
}

/// Applique le plan ci-dessus. Geste explicite, jamais automatique.
pub fn replanifier(conn: &Connection, etape_id: &str) -> Result<usize> {
    let plan = plan_replanification(conn, etape_id)?;
    for d in &plan {
        conn.execute(
            "UPDATE marche_etape SET date_prevue = ?2 WHERE id = ?1",
            params![d.etape_id, d.date_proposee],
        )?;
    }
    Ok(plan.len())
}

// ===========================================================================
// Soumissionnaires
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct Soumissionnaire {
    pub id: String,
    pub marche_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiers_id: Option<String>,
    pub nom: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ninea: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telephone: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub montant_offre: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub montant_offre_ttc: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delai_jours: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note_technique: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note_financiere: Option<f64>,
    pub statut: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motif: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observations: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_depot: Option<String>,
    /// Écart en % avec le montant estimé du marché : le chiffre qu'on regarde
    /// en premier au dépouillement.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ecart_estime_pct: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouveauSoumissionnaire {
    #[serde(default)]
    pub tiers_id: Option<String>,
    pub nom: String,
    #[serde(default)]
    pub ninea: Option<String>,
    #[serde(default)]
    pub telephone: Option<String>,
    #[serde(default)]
    pub montant_offre: Option<f64>,
    #[serde(default)]
    pub montant_offre_ttc: Option<f64>,
    #[serde(default)]
    pub delai_jours: Option<i64>,
    #[serde(default)]
    pub note_technique: Option<f64>,
    #[serde(default)]
    pub note_financiere: Option<f64>,
    #[serde(default)]
    pub statut: Option<String>,
    #[serde(default)]
    pub motif: Option<String>,
    #[serde(default)]
    pub observations: Option<String>,
    #[serde(default)]
    pub date_depot: Option<String>,
}

fn charger_soumissionnaires(conn: &Connection, marche_id: &str, estime: f64) -> Result<Vec<Soumissionnaire>> {
    let mut st = conn.prepare(
        "SELECT id, marche_id, tiers_id, nom, ninea, telephone, montant_offre,
                montant_offre_ttc, delai_jours, note_technique, note_financiere,
                statut, motif, observations, date_depot
           FROM marche_soumissionnaire WHERE marche_id = ?1
          ORDER BY CASE statut WHEN 'retenu' THEN 0 ELSE 1 END, montant_offre",
    )?;
    let v = st
        .query_map(params![marche_id], |r| {
            let montant: Option<f64> = r.get(6)?;
            Ok(Soumissionnaire {
                id: r.get(0)?,
                marche_id: r.get(1)?,
                tiers_id: r.get(2)?,
                nom: r.get(3)?,
                ninea: r.get(4)?,
                telephone: r.get(5)?,
                montant_offre: montant,
                montant_offre_ttc: r.get(7)?,
                delai_jours: r.get(8)?,
                note_technique: r.get(9)?,
                note_financiere: r.get(10)?,
                statut: r.get(11)?,
                motif: r.get(12)?,
                observations: r.get(13)?,
                date_depot: r.get(14)?,
                ecart_estime_pct: match montant {
                    Some(m) if estime.abs() > 0.005 => {
                        Some(((m - estime) / estime * 100.0 * 10.0).round() / 10.0)
                    }
                    _ => None,
                },
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn ajouter_soumissionnaire(
    conn: &Connection,
    marche_id: &str,
    s: &NouveauSoumissionnaire,
    par: Option<&str>,
) -> Result<Soumissionnaire> {
    if s.nom.trim().is_empty() {
        return Err(CoreError::Rule("le nom du soumissionnaire est obligatoire".into()));
    }
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO marche_soumissionnaire
            (id, marche_id, tiers_id, nom, ninea, telephone, montant_offre,
             montant_offre_ttc, delai_jours, note_technique, note_financiere,
             statut, motif, observations, date_depot, cree_par, cree_le)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,COALESCE(?12,'recu'),?13,?14,?15,?16,?17)",
        params![
            id, marche_id, vide(&s.tiers_id), s.nom.trim(), vide(&s.ninea), vide(&s.telephone),
            s.montant_offre, s.montant_offre_ttc, s.delai_jours, s.note_technique,
            s.note_financiere, vide(&s.statut), vide(&s.motif), vide(&s.observations),
            vide(&s.date_depot), par, now()
        ],
    )?;
    let estime: f64 = conn
        .query_row("SELECT montant_estime FROM marche WHERE id = ?1", params![marche_id], |r| r.get(0))
        .unwrap_or(0.0);
    charger_soumissionnaires(conn, marche_id, estime)?
        .into_iter()
        .find(|x| x.id == id)
        .ok_or_else(|| CoreError::NotFound("soumissionnaire".into()))
}

pub fn modifier_soumissionnaire(
    conn: &Connection,
    id: &str,
    s: &NouveauSoumissionnaire,
) -> Result<Soumissionnaire> {
    let marche_id: String = conn
        .query_row("SELECT marche_id FROM marche_soumissionnaire WHERE id = ?1", params![id], |r| r.get(0))
        .map_err(|_| CoreError::NotFound(format!("soumissionnaire {id}")))?;
    conn.execute(
        "UPDATE marche_soumissionnaire SET
            tiers_id = ?2, nom = ?3, ninea = ?4, telephone = ?5, montant_offre = ?6,
            montant_offre_ttc = ?7, delai_jours = ?8, note_technique = ?9,
            note_financiere = ?10, statut = COALESCE(?11, statut), motif = ?12,
            observations = ?13, date_depot = ?14
         WHERE id = ?1",
        params![
            id, vide(&s.tiers_id), s.nom.trim(), vide(&s.ninea), vide(&s.telephone),
            s.montant_offre, s.montant_offre_ttc, s.delai_jours, s.note_technique,
            s.note_financiere, vide(&s.statut), vide(&s.motif), vide(&s.observations),
            vide(&s.date_depot)
        ],
    )?;
    let estime: f64 = conn
        .query_row("SELECT montant_estime FROM marche WHERE id = ?1", params![marche_id], |r| r.get(0))
        .unwrap_or(0.0);
    charger_soumissionnaires(conn, &marche_id, estime)?
        .into_iter()
        .find(|x| x.id == id)
        .ok_or_else(|| CoreError::NotFound("soumissionnaire".into()))
}

pub fn supprimer_soumissionnaire(conn: &Connection, id: &str) -> Result<()> {
    let n = conn.execute("DELETE FROM marche_soumissionnaire WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("soumissionnaire {id}")));
    }
    Ok(())
}

/// **Attribuer le marché** à un soumissionnaire : il passe `retenu`, les autres
/// `ecarte`, et le marché reçoit son attributaire et son montant attribué.
/// Un seul geste, parce que c'est un seul acte dans la réalité.
pub fn attribuer(conn: &Connection, soumissionnaire_id: &str) -> Result<Marche> {
    let (marche_id, tiers_id, nom, montant): (String, Option<String>, String, Option<f64>) = conn
        .query_row(
            "SELECT marche_id, tiers_id, nom, montant_offre
               FROM marche_soumissionnaire WHERE id = ?1",
            params![soumissionnaire_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .map_err(|_| CoreError::NotFound(format!("soumissionnaire {soumissionnaire_id}")))?;

    conn.execute(
        "UPDATE marche_soumissionnaire SET statut = 'ecarte'
          WHERE marche_id = ?1 AND id <> ?2 AND statut = 'retenu'",
        params![marche_id, soumissionnaire_id],
    )?;
    conn.execute(
        "UPDATE marche_soumissionnaire SET statut = 'retenu' WHERE id = ?1",
        params![soumissionnaire_id],
    )?;
    // Le tiers n'est rattaché que s'il existe : on n'oblige pas à créer une
    // fiche pour attribuer, le nom saisi suffit à tracer la décision.
    conn.execute(
        "UPDATE marche SET attributaire_id = COALESCE(?2, attributaire_id),
                           montant_attribue = COALESCE(?3, montant_attribue)
          WHERE id = ?1",
        params![marche_id, tiers_id, montant],
    )?;
    let _ = nom;
    lire(conn, &marche_id)
}

// ===========================================================================
// Incidents de procédure : infructueux et recours (migration 0038)
//
// Deux évènements qui INTERROMPENT la chaîne, et que le modèle plat ne savait
// pas dire :
//   • **infructueux** : aucune offre, ou aucune conforme. On relance à partir
//     de la publication — mais la première tentative reste au dossier.
//   • **recours** : un candidat conteste. La procédure est gelée jusqu'à
//     décision. C'est un arrêt SUBI : il ne doit pas passer pour un retard
//     de l'administration.
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct Incident {
    pub id: String,
    pub marche_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub etape_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub etape_libelle: Option<String>,
    pub type_incident: String,
    pub date_incident: String,
    pub motif: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auteur_recours: Option<String>,
    pub statut: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decision: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_decision: Option<String>,
    pub tentative: i64,
    pub cree_le: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouvelIncident {
    pub type_incident: String,
    pub motif: String,
    #[serde(default)]
    pub etape_id: Option<String>,
    #[serde(default)]
    pub date_incident: Option<String>,
    #[serde(default)]
    pub auteur_recours: Option<String>,
}

const INCIDENT_COLS: &str = "SELECT i.id, i.marche_id, i.etape_id, e.libelle, i.type_incident,
        i.date_incident, i.motif, i.auteur_recours, i.statut, i.decision,
        i.date_decision, i.tentative, i.cree_le
   FROM marche_incident i
   LEFT JOIN marche_etape e ON e.id = i.etape_id";

fn ligne_incident(r: &Row) -> rusqlite::Result<Incident> {
    Ok(Incident {
        id: r.get(0)?,
        marche_id: r.get(1)?,
        etape_id: r.get(2)?,
        etape_libelle: r.get(3)?,
        type_incident: r.get(4)?,
        date_incident: r.get(5)?,
        motif: r.get(6)?,
        auteur_recours: r.get(7)?,
        statut: r.get(8)?,
        decision: r.get(9)?,
        date_decision: r.get(10)?,
        tentative: r.get(11)?,
        cree_le: r.get(12)?,
    })
}

fn charger_incidents(conn: &Connection, marche_id: &str) -> Result<Vec<Incident>> {
    let mut st = conn.prepare(&format!(
        "{INCIDENT_COLS} WHERE i.marche_id = ?1 ORDER BY i.date_incident DESC, i.cree_le DESC"
    ))?;
    let v = st
        .query_map(params![marche_id], ligne_incident)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

/// Motif du recours encore ouvert, s'il y en a un. Tant qu'il l'est, la
/// procédure ne doit pas avancer.
fn recours_ouvert(conn: &Connection, marche_id: &str) -> Result<Option<String>> {
    let r: Option<String> = conn
        .query_row(
            "SELECT motif FROM marche_incident
              WHERE marche_id = ?1 AND type_incident = 'recours' AND statut = 'ouvert'
              ORDER BY date_incident DESC LIMIT 1",
            params![marche_id],
            |x| x.get(0),
        )
        .optional()?;
    Ok(r)
}

/// Déclarer un incident.
///
/// Un **infructueux** ne se contente pas d'être noté : il **relance la
/// procédure**. Les étapes à partir de la publication repassent « à faire » et
/// le marché change de tentative. Sans cela, on garderait à l'écran une
/// attribution qui n'a jamais eu lieu.
pub fn declarer_incident(
    conn: &Connection,
    marche_id: &str,
    n: &NouvelIncident,
    par: Option<&str>,
) -> Result<Incident> {
    if !matches!(n.type_incident.as_str(), "infructueux" | "recours") {
        return Err(CoreError::Rule(format!(
            "type d'incident inconnu : {}",
            n.type_incident
        )));
    }
    if n.motif.trim().is_empty() {
        return Err(CoreError::Rule(
            "le motif est obligatoire : un incident sans motif ne vaut rien au dossier".into(),
        ));
    }
    let tentative: i64 = conn
        .query_row("SELECT tentative FROM marche WHERE id = ?1", params![marche_id], |r| r.get(0))
        .map_err(|_| CoreError::NotFound(format!("marché {marche_id}")))?;

    let id = Uuid::new_v4().to_string();
    let date = vide(&n.date_incident).map(str::to_string).unwrap_or_else(aujourdhui);
    conn.execute(
        "INSERT INTO marche_incident
            (id, marche_id, etape_id, type_incident, date_incident, motif,
             auteur_recours, statut, tentative, cree_par, cree_le)
         VALUES (?1,?2,?3,?4,?5,?6,?7,'ouvert',?8,?9,?10)",
        params![
            id, marche_id, vide(&n.etape_id), n.type_incident.trim(), date,
            n.motif.trim(), vide(&n.auteur_recours), tentative, par, now()
        ],
    )?;

    if n.type_incident == "infructueux" {
        relancer_apres_infructueux(conn, marche_id, &date)?;
    }

    charger_incidents(conn, marche_id)?
        .into_iter()
        .find(|x| x.id == id)
        .ok_or_else(|| CoreError::NotFound("incident".into()))
}

/// Relance après un appel d'offres infructueux : tout ce qui suit la
/// **publication** repart à zéro, les offres reçues sont écartées, et le marché
/// passe à la tentative suivante. La publication elle-même reste franchie —
/// c'est bien elle qu'on va refaire, mais la première a eu lieu.
fn relancer_apres_infructueux(conn: &Connection, marche_id: &str, date: &str) -> Result<usize> {
    let etapes = charger_etapes(conn, marche_id)?;
    // Le point de reprise : la publication si on la trouve, sinon la 2ᵉ étape.
    let depart = etapes
        .iter()
        .find(|e| {
            let l = e.libelle.to_lowercase();
            l.contains("publication") || l.contains("avis")
        })
        .map(|e| e.ordre)
        .unwrap_or_else(|| etapes.first().map(|e| e.ordre).unwrap_or(0) + 1);

    let mut n = 0;
    for e in etapes.iter().filter(|e| e.ordre > depart && e.statut != "en_attente") {
        conn.execute(
            "UPDATE marche_etape SET statut = 'en_attente', date_effective = NULL,
                    valide_par = NULL, valide_le = NULL,
                    observations = TRIM(COALESCE(observations || char(10), '') || ?2)
              WHERE id = ?1",
            params![e.id, format!("Reprise le {date} : appel d'offres déclaré infructueux.")],
        )?;
        n += 1;
    }
    // Les offres de la tentative ratée sont écartées, jamais effacées : elles
    // prouvent que la consultation a bien eu lieu.
    conn.execute(
        "UPDATE marche_soumissionnaire SET statut = 'ecarte',
                motif = COALESCE(motif || ' — ', '') || 'Appel d''offres infructueux'
          WHERE marche_id = ?1 AND statut <> 'ecarte'",
        params![marche_id],
    )?;
    // L'attribution éventuelle tombe avec la procédure.
    conn.execute(
        "UPDATE marche SET tentative = tentative + 1,
                attributaire_id = NULL, montant_attribue = NULL
          WHERE id = ?1",
        params![marche_id],
    )?;
    Ok(n)
}

/// Clore un incident : le recours est tranché, ou la relance est actée.
pub fn clore_incident(conn: &Connection, id: &str, decision: &str) -> Result<Incident> {
    if decision.trim().is_empty() {
        return Err(CoreError::Rule("la décision est obligatoire pour clore".into()));
    }
    let marche_id: String = conn
        .query_row("SELECT marche_id FROM marche_incident WHERE id = ?1", params![id], |r| r.get(0))
        .map_err(|_| CoreError::NotFound(format!("incident {id}")))?;
    conn.execute(
        "UPDATE marche_incident SET statut = 'clos', decision = ?2, date_decision = ?3
          WHERE id = ?1",
        params![id, decision.trim(), aujourdhui()],
    )?;
    charger_incidents(conn, &marche_id)?
        .into_iter()
        .find(|x| x.id == id)
        .ok_or_else(|| CoreError::NotFound("incident".into()))
}

pub fn supprimer_incident(conn: &Connection, id: &str) -> Result<()> {
    let n = conn.execute("DELETE FROM marche_incident WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("incident {id}")));
    }
    Ok(())
}

// ===========================================================================
// Avenants
//
// Un marché qui gonfle ou s'allonge en cours d'exécution, c'est la règle plus
// que l'exception. Le montant d'origine reste **intact** sur le marché et les
// avenants s'empilent ; le montant courant se déduit. On ne réécrit jamais le
// contrat initial — c'est toute la valeur probante du registre.
//
// Seuls les avenants **approuvés** comptent : un avenant à l'état de projet est
// une intention, pas un engagement.
// ===========================================================================

/// Part du montant initial au-delà de laquelle l'empilement des avenants est
/// signalé. 30 % est le plafond usuel des marchés publics dans l'espace UEMOA ;
/// c'est un **repère affiché**, pas une interdiction : le module ne bloque pas.
const SEUIL_ALERTE_AVENANTS_PCT: f64 = 30.0;

#[derive(Debug, Clone, Serialize)]
pub struct Avenant {
    pub id: String,
    pub marche_id: String,
    pub numero: i64,
    pub objet: String,
    pub montant_variation: f64,
    pub delai_jours: i64,
    pub date_avenant: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motif: Option<String>,
    pub statut: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approuve_par: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approuve_le: Option<String>,
    /// Un avenant approuvé est **figé** : il a produit ses effets sur le montant
    /// et le délai du marché. Pour revenir dessus, on en prend un nouveau.
    pub modifiable: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouvelAvenant {
    pub objet: String,
    #[serde(default)]
    pub montant_variation: f64,
    #[serde(default)]
    pub delai_jours: i64,
    #[serde(default)]
    pub date_avenant: Option<String>,
    #[serde(default)]
    pub motif: Option<String>,
}

fn ligne_avenant(r: &Row) -> rusqlite::Result<Avenant> {
    let statut: String = r.get(8)?;
    Ok(Avenant {
        id: r.get(0)?,
        marche_id: r.get(1)?,
        numero: r.get(2)?,
        objet: r.get(3)?,
        montant_variation: r.get(4)?,
        delai_jours: r.get(5)?,
        date_avenant: r.get(6)?,
        motif: r.get(7)?,
        modifiable: statut == "projet",
        statut,
        approuve_par: r.get(9)?,
        approuve_le: r.get(10)?,
    })
}

const AVENANT_COLS: &str = "SELECT id, marche_id, numero, objet, montant_variation,
        delai_jours, date_avenant, motif, statut, approuve_par, approuve_le
   FROM marche_avenant";

fn charger_avenants(conn: &Connection, marche_id: &str) -> Result<Vec<Avenant>> {
    let mut st = conn.prepare(&format!("{AVENANT_COLS} WHERE marche_id = ?1 ORDER BY numero"))?;
    let v = st
        .query_map(params![marche_id], ligne_avenant)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

fn lire_avenant(conn: &Connection, id: &str) -> Result<Avenant> {
    let mut st = conn.prepare(&format!("{AVENANT_COLS} WHERE id = ?1"))?;
    st.query_row(params![id], ligne_avenant)
        .map_err(|_| CoreError::NotFound(format!("avenant {id}")))
}

pub fn ajouter_avenant(
    conn: &Connection,
    marche_id: &str,
    a: &NouvelAvenant,
    par: Option<&str>,
) -> Result<Avenant> {
    if a.objet.trim().is_empty() {
        return Err(CoreError::Rule("l'objet de l'avenant est obligatoire".into()));
    }
    let statut: String = conn
        .query_row("SELECT statut FROM marche WHERE id = ?1", params![marche_id], |r| r.get(0))
        .map_err(|_| CoreError::NotFound(format!("marché {marche_id}")))?;
    if statut == "annule" {
        return Err(CoreError::Rule(
            "ce marché est annulé : on ne lui ajoute pas d'avenant".into(),
        ));
    }
    // La numérotation est **par marché** : avenant n° 1, n° 2… C'est ainsi qu'on
    // les désigne dans les actes, pas par un identifiant global.
    let numero: i64 = conn.query_row(
        "SELECT COALESCE(MAX(numero), 0) + 1 FROM marche_avenant WHERE marche_id = ?1",
        params![marche_id],
        |r| r.get(0),
    )?;
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO marche_avenant
            (id, marche_id, numero, objet, montant_variation, delai_jours,
             date_avenant, motif, statut, cree_par, cree_le)
         VALUES (?1,?2,?3,?4,?5,?6,COALESCE(?7,?10),?8,'projet',?9,?11)",
        params![
            id, marche_id, numero, a.objet.trim(), a.montant_variation, a.delai_jours,
            vide(&a.date_avenant), vide(&a.motif), par, aujourdhui(), now()
        ],
    )?;
    lire_avenant(conn, &id)
}

/// Modification : **tant qu'il est à l'état de projet**. Une fois approuvé,
/// l'avenant a engagé les parties ; on en prend un nouveau plutôt que de
/// réécrire celui-là.
pub fn modifier_avenant(conn: &Connection, id: &str, a: &NouvelAvenant) -> Result<Avenant> {
    let actuel = lire_avenant(conn, id)?;
    if !actuel.modifiable {
        return Err(CoreError::Rule(format!(
            "l'avenant n° {} est {} : prenez un nouvel avenant pour le corriger",
            actuel.numero,
            if actuel.statut == "approuve" { "approuvé" } else { "rejeté" }
        )));
    }
    if a.objet.trim().is_empty() {
        return Err(CoreError::Rule("l'objet de l'avenant est obligatoire".into()));
    }
    conn.execute(
        "UPDATE marche_avenant SET objet = ?2, montant_variation = ?3, delai_jours = ?4,
                date_avenant = COALESCE(?5, date_avenant), motif = ?6
          WHERE id = ?1",
        params![
            id, a.objet.trim(), a.montant_variation, a.delai_jours,
            vide(&a.date_avenant), vide(&a.motif)
        ],
    )?;
    lire_avenant(conn, id)
}

/// `approuve` ou `rejete`. L'approbation **horodate et nomme** son auteur :
/// c'est l'acte qui change le montant et le délai du marché.
pub fn statut_avenant(conn: &Connection, id: &str, statut: &str, par: Option<&str>) -> Result<Avenant> {
    if !matches!(statut, "projet" | "approuve" | "rejete") {
        return Err(CoreError::Rule(format!("statut d'avenant inconnu : {statut}")));
    }
    let actuel = lire_avenant(conn, id)?;
    if actuel.statut == "approuve" && statut != "approuve" {
        return Err(CoreError::Rule(format!(
            "l'avenant n° {} est déjà approuvé : il a produit ses effets, prenez un avenant en sens inverse",
            actuel.numero
        )));
    }
    if statut == "approuve" {
        conn.execute(
            "UPDATE marche_avenant SET statut = 'approuve', approuve_par = ?2, approuve_le = ?3
              WHERE id = ?1",
            params![id, par, now()],
        )?;
    } else {
        conn.execute(
            "UPDATE marche_avenant SET statut = ?2, approuve_par = NULL, approuve_le = NULL
              WHERE id = ?1",
            params![id, statut],
        )?;
    }
    lire_avenant(conn, id)
}

/// Suppression : réservée aux avenants **encore à l'état de projet**. Un avenant
/// approuvé fait partie de l'histoire du marché.
pub fn supprimer_avenant(conn: &Connection, id: &str) -> Result<()> {
    let a = lire_avenant(conn, id)?;
    if a.statut == "approuve" {
        return Err(CoreError::Rule(format!(
            "l'avenant n° {} est approuvé : il ne se supprime pas",
            a.numero
        )));
    }
    conn.execute("DELETE FROM marche_avenant WHERE id = ?1", params![id])?;
    Ok(())
}

// ===========================================================================
// Réceptions
//
// La fin réelle d'un marché : réception provisoire, réserves, levée des
// réserves, réception définitive. C'est la levée des réserves qui conditionne
// la libération de la retenue de garantie — donc on la trace explicitement.
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct Reception {
    pub id: String,
    pub marche_id: String,
    pub type_reception: String,
    pub date_reception: String,
    pub resultat: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reserves: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_levee_reserves: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub garantie_mois: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub montant_retenue_garantie: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observations: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receptionne_par: Option<String>,
    pub cree_le: String,
    /// Des réserves ont été émises et ne sont pas levées.
    pub reserves_ouvertes: bool,
    /// Fin de la garantie, déduite de la date de réception et de sa durée.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fin_garantie: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouvelleReception {
    #[serde(default)]
    pub type_reception: Option<String>,
    #[serde(default)]
    pub date_reception: Option<String>,
    #[serde(default)]
    pub resultat: Option<String>,
    #[serde(default)]
    pub reserves: Option<String>,
    #[serde(default)]
    pub date_levee_reserves: Option<String>,
    #[serde(default)]
    pub garantie_mois: Option<i64>,
    #[serde(default)]
    pub montant_retenue_garantie: Option<f64>,
    #[serde(default)]
    pub observations: Option<String>,
    #[serde(default)]
    pub receptionne_par: Option<String>,
}

const RECEPTION_COLS: &str = "SELECT id, marche_id, type_reception, date_reception, resultat,
        reserves, date_levee_reserves, garantie_mois, montant_retenue_garantie,
        observations, receptionne_par, cree_le
   FROM marche_reception";

fn ligne_reception(r: &Row) -> rusqlite::Result<Reception> {
    let resultat: String = r.get(4)?;
    let date_reception: String = r.get(3)?;
    let levee: Option<String> = r.get(6)?;
    let garantie_mois: Option<i64> = r.get(7)?;
    Ok(Reception {
        id: r.get(0)?,
        marche_id: r.get(1)?,
        type_reception: r.get(2)?,
        reserves_ouvertes: resultat == "avec_reserves" && levee.is_none(),
        // 30 jours par mois : la garantie se compte en mois pleins et cette
        // date n'est qu'un repère affiché, pas une échéance contractuelle.
        fin_garantie: garantie_mois.map(|m| ajouter_jours(&date_reception, m * 30)),
        date_reception,
        resultat,
        reserves: r.get(5)?,
        date_levee_reserves: levee,
        garantie_mois,
        montant_retenue_garantie: r.get(8)?,
        observations: r.get(9)?,
        receptionne_par: r.get(10)?,
        cree_le: r.get(11)?,
    })
}

fn charger_receptions(conn: &Connection, marche_id: &str) -> Result<Vec<Reception>> {
    let mut st = conn.prepare(&format!(
        "{RECEPTION_COLS} WHERE marche_id = ?1 ORDER BY date_reception, cree_le"
    ))?;
    let v = st
        .query_map(params![marche_id], ligne_reception)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

fn lire_reception(conn: &Connection, id: &str) -> Result<Reception> {
    let mut st = conn.prepare(&format!("{RECEPTION_COLS} WHERE id = ?1"))?;
    st.query_row(params![id], ligne_reception)
        .map_err(|_| CoreError::NotFound(format!("réception {id}")))
}

/// Les deux seules exigences dures : un type et un résultat connus, et des
/// **réserves écrites** quand le procès-verbal en comporte. Une réception « avec
/// réserves » sans le texte des réserves ne veut rien dire et laisserait le
/// marché dans un état invérifiable.
fn valider_reception(r: &NouvelleReception) -> Result<()> {
    if let Some(t) = vide(&r.type_reception) {
        if !matches!(t, "provisoire" | "definitive" | "partielle") {
            return Err(CoreError::Rule(format!("type de réception inconnu : {t}")));
        }
    }
    let resultat = vide(&r.resultat).unwrap_or("prononcee");
    if !matches!(resultat, "prononcee" | "avec_reserves" | "refusee") {
        return Err(CoreError::Rule(format!("résultat de réception inconnu : {resultat}")));
    }
    if resultat == "avec_reserves" && vide(&r.reserves).is_none() {
        return Err(CoreError::Rule(
            "une réception avec réserves doit dire lesquelles".into(),
        ));
    }
    Ok(())
}

pub fn ajouter_reception(
    conn: &Connection,
    marche_id: &str,
    r: &NouvelleReception,
    par: Option<&str>,
) -> Result<Reception> {
    valider_reception(r)?;
    conn.query_row("SELECT id FROM marche WHERE id = ?1", params![marche_id], |x| {
        x.get::<_, String>(0)
    })
    .map_err(|_| CoreError::NotFound(format!("marché {marche_id}")))?;
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO marche_reception
            (id, marche_id, type_reception, date_reception, resultat, reserves,
             date_levee_reserves, garantie_mois, montant_retenue_garantie,
             observations, receptionne_par, cree_par, cree_le)
         VALUES (?1,?2,COALESCE(?3,'provisoire'),COALESCE(?4,?13),COALESCE(?5,'prononcee'),
                 ?6,?7,?8,?9,?10,?11,?12,?14)",
        params![
            id, marche_id, vide(&r.type_reception), vide(&r.date_reception),
            vide(&r.resultat), vide(&r.reserves), vide(&r.date_levee_reserves),
            r.garantie_mois, r.montant_retenue_garantie, vide(&r.observations),
            vide(&r.receptionne_par), par, aujourdhui(), now()
        ],
    )?;
    lire_reception(conn, &id)
}

pub fn modifier_reception(conn: &Connection, id: &str, r: &NouvelleReception) -> Result<Reception> {
    lire_reception(conn, id)?;
    valider_reception(r)?;
    conn.execute(
        "UPDATE marche_reception SET
            type_reception = COALESCE(?2, type_reception),
            date_reception = COALESCE(?3, date_reception),
            resultat = COALESCE(?4, resultat),
            reserves = ?5, date_levee_reserves = ?6, garantie_mois = ?7,
            montant_retenue_garantie = ?8, observations = ?9, receptionne_par = ?10
          WHERE id = ?1",
        params![
            id, vide(&r.type_reception), vide(&r.date_reception), vide(&r.resultat),
            vide(&r.reserves), vide(&r.date_levee_reserves), r.garantie_mois,
            r.montant_retenue_garantie, vide(&r.observations), vide(&r.receptionne_par)
        ],
    )?;
    lire_reception(conn, id)
}

/// Lever les réserves : le geste qui libère la retenue de garantie. Daté du jour
/// si aucune date n'est fournie.
pub fn lever_reserves(conn: &Connection, id: &str, date: Option<&str>) -> Result<Reception> {
    let r = lire_reception(conn, id)?;
    if r.resultat != "avec_reserves" {
        return Err(CoreError::Rule(
            "cette réception ne comporte pas de réserves à lever".into(),
        ));
    }
    let d = date.map(str::trim).filter(|d| !d.is_empty()).map(str::to_string)
        .unwrap_or_else(aujourdhui);
    conn.execute(
        "UPDATE marche_reception SET date_levee_reserves = ?2 WHERE id = ?1",
        params![id, d],
    )?;
    lire_reception(conn, id)
}

pub fn supprimer_reception(conn: &Connection, id: &str) -> Result<()> {
    lire_reception(conn, id)?;
    conn.execute("DELETE FROM marche_reception WHERE id = ?1", params![id])?;
    Ok(())
}

// ===========================================================================
// Le marché
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct Marche {
    pub id: String,
    pub numero: String,
    pub objet: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_libelle: Option<String>,
    pub montant_estime: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub montant_attribue: Option<f64>,
    pub monnaie: String,
    pub statut: String,
    pub date_lancement: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_cloture_prevue: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_cloture_effective: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributaire_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributaire_nom: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projet_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub projet_nom: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub responsable_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub responsable_nom: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lieu_execution: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observations: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motif_annulation: Option<String>,
    pub cree_le: String,

    // --- Calculés ---
    pub nb_etapes: i64,
    pub nb_etapes_terminees: i64,
    /// Part des étapes **obligatoires** terminées, en %.
    pub avancement: i64,
    /// Retard le plus important parmi les étapes en cours.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retard_jours: Option<i64>,
    pub nb_soumissionnaires: i64,
    /// Écart entre montant attribué et montant estimé.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ecart_montant: Option<f64>,

    // --- Avenants (seuls les approuvés comptent) ---
    pub nb_avenants: i64,
    pub montant_avenants: f64,
    pub delai_avenants_jours: i64,
    /// Montant réellement engagé : le contrat initial **plus** les avenants
    /// approuvés. Le montant d'origine, lui, ne bouge jamais.
    pub montant_courant: f64,
    /// Part des avenants dans le montant initial, en %.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avenants_pct: Option<f64>,
    /// Clôture prévue décalée des délais accordés par avenant. **Affichée, pas
    /// écrite** : aucune date n'est recalculée sans un geste explicite.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_cloture_revisee: Option<String>,

    // --- Réception ---
    pub nb_receptions: i64,
    /// Des réserves ont été émises et ne sont toujours pas levées.
    pub reserves_ouvertes: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub date_reception_definitive: Option<String>,

    // --- Compteurs servant aux alertes -------------------------------------
    // Agrégés en SQL pour que les alertes soient calculables SANS charger les
    // listes : c'est ce qui permet de les afficher aussi sur la liste.
    #[serde(skip_serializing)]
    pub nb_avenants_projet: i64,
    #[serde(skip_serializing)]
    pub nb_receptions_provisoires: i64,
    #[serde(skip_serializing)]
    pub derniere_etape_prevue: Option<String>,
    #[serde(skip_serializing)]
    pub nb_etapes_avant_lancement: i64,
    #[serde(skip_serializing)]
    pub nb_etapes_obligatoires_ouvertes: i64,
    /// Étapes datées AVANT une étape qui les précède : chronologiquement
    /// impossible. Signalé pour les dossiers saisis avant le contrôle.
    #[serde(skip_serializing)]
    pub nb_dates_incoherentes: i64,
    /// Incohérences détectées. **Informatif, jamais bloquant.**
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub alertes: Vec<String>,
    /// Remplis par [`lire`] seulement.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub etapes: Vec<Etape>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub soumissionnaires: Vec<Soumissionnaire>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub avenants: Vec<Avenant>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub receptions: Vec<Reception>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub incidents: Vec<Incident>,
    /// Numéro de tentative : 2 après un premier appel d'offres infructueux.
    pub tentative: i64,
    /// Motif du recours en cours, s'il y en a un : la procédure est gelée.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recours_en_cours: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouveauMarche {
    pub objet: String,
    #[serde(default)]
    pub type_id: Option<String>,
    #[serde(default)]
    pub montant_estime: f64,
    #[serde(default)]
    pub montant_attribue: Option<f64>,
    #[serde(default)]
    pub monnaie: Option<String>,
    #[serde(default)]
    pub date_lancement: Option<String>,
    #[serde(default)]
    pub date_cloture_prevue: Option<String>,
    #[serde(default)]
    pub attributaire_id: Option<String>,
    #[serde(default)]
    pub projet_id: Option<String>,
    #[serde(default)]
    pub responsable_id: Option<String>,
    #[serde(default)]
    pub lieu_execution: Option<String>,
    #[serde(default)]
    pub observations: Option<String>,
    /// Étapes fournies par l'écran de création. Absentes ou vides, on recopie la
    /// procédure du type telle quelle. Fournies, elles la **remplacent** : le
    /// formulaire montre les étapes et leurs dates, l'utilisateur les ajuste
    /// avant de créer plutôt que de les corriger une par une ensuite.
    #[serde(default)]
    pub etapes: Vec<EtapeSaisie>,
}

/// Une étape telle que l'écran de création la propose (et que l'utilisateur
/// peut modifier) avant l'enregistrement.
#[derive(Debug, Clone, Deserialize)]
pub struct EtapeSaisie {
    pub libelle: String,
    #[serde(default)]
    pub date_prevue: Option<String>,
    #[serde(default = "vrai")]
    pub obligatoire: bool,
    /// Étape d'origine, pour la traçabilité.
    #[serde(default)]
    pub etape_modele_id: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct FiltreMarches {
    #[serde(default)]
    pub statut: Option<String>,
    #[serde(default)]
    pub type_id: Option<String>,
    #[serde(default)]
    pub responsable_id: Option<String>,
    #[serde(default)]
    pub projet_id: Option<String>,
    #[serde(default)]
    pub du: Option<String>,
    #[serde(default)]
    pub au: Option<String>,
    /// Recherche libre sur le numéro et l'objet.
    #[serde(default)]
    pub texte: Option<String>,
    /// Ne garder que les marchés ayant au moins une étape en retard.
    #[serde(default)]
    pub en_retard: bool,
}

const MARCHE_COLS: &str = "SELECT m.id, m.numero, m.objet, m.type_id, t.libelle,
        m.montant_estime, m.montant_attribue, m.monnaie, m.statut, m.date_lancement,
        m.date_cloture_prevue, m.date_cloture_effective,
        m.attributaire_id, ti.nom, m.projet_id, p.nom, m.responsable_id, u.nom,
        m.lieu_execution, m.observations, m.motif_annulation, m.cree_le,
        (SELECT COUNT(*) FROM marche_etape e WHERE e.marche_id = m.id),
        (SELECT COUNT(*) FROM marche_etape e WHERE e.marche_id = m.id AND e.statut = 'termine'),
        (SELECT COUNT(*) FROM marche_etape e WHERE e.marche_id = m.id AND e.obligatoire = 1),
        (SELECT COUNT(*) FROM marche_etape e WHERE e.marche_id = m.id
                AND e.obligatoire = 1 AND e.statut = 'termine'),
        (SELECT COUNT(*) FROM marche_soumissionnaire s WHERE s.marche_id = m.id),
        (SELECT MIN(e.date_prevue) FROM marche_etape e
          WHERE e.marche_id = m.id AND e.statut = 'en_cours'),
        (SELECT COUNT(*) FROM marche_avenant a WHERE a.marche_id = m.id),
        (SELECT COALESCE(SUM(a.montant_variation), 0) FROM marche_avenant a
          WHERE a.marche_id = m.id AND a.statut = 'approuve'),
        (SELECT COALESCE(SUM(a.delai_jours), 0) FROM marche_avenant a
          WHERE a.marche_id = m.id AND a.statut = 'approuve'),
        (SELECT COUNT(*) FROM marche_reception r WHERE r.marche_id = m.id),
        (SELECT COUNT(*) FROM marche_reception r WHERE r.marche_id = m.id
                AND r.resultat = 'avec_reserves' AND r.date_levee_reserves IS NULL),
        (SELECT MAX(r.date_reception) FROM marche_reception r
          WHERE r.marche_id = m.id AND r.type_reception = 'definitive'
            AND r.resultat <> 'refusee'),
        -- Colonnes servant aux ALERTES. Elles sont agrégées ici, et non déduites
        -- des listes chargées, pour que les mêmes alertes s'affichent sur la
        -- LISTE des marchés et pas seulement dans le détail.
        (SELECT COUNT(*) FROM marche_avenant a
          WHERE a.marche_id = m.id AND a.statut = 'projet'),
        (SELECT COUNT(*) FROM marche_reception r WHERE r.marche_id = m.id
                AND r.type_reception = 'provisoire' AND r.resultat <> 'refusee'),
        (SELECT MAX(e.date_prevue) FROM marche_etape e WHERE e.marche_id = m.id),
        -- Une étape datée AVANT le lancement du marché : incohérence fréquente
        -- quand on reprend un dossier commencé sur papier, et qui fait afficher
        -- un retard énorme sur un marché tout juste lancé.
        (SELECT COUNT(*) FROM marche_etape e WHERE e.marche_id = m.id
                AND e.date_prevue IS NOT NULL AND e.date_prevue < m.date_lancement),
        (SELECT COUNT(*) FROM marche_etape e WHERE e.marche_id = m.id
                AND e.obligatoire = 1 AND e.statut <> 'termine'),
        -- Étapes dont la date de réalisation est ANTÉRIEURE à celle d'une étape
        -- qui les précède : chronologiquement impossible. Le contrôle empêche
        -- désormais d'en créer, mais les dossiers saisis avant lui en portent.
        (SELECT COUNT(*) FROM marche_etape e
          WHERE e.marche_id = m.id AND e.statut = 'termine' AND e.date_effective IS NOT NULL
            AND EXISTS (SELECT 1 FROM marche_etape p
                         WHERE p.marche_id = m.id AND p.ordre < e.ordre
                           AND p.statut = 'termine' AND p.date_effective IS NOT NULL
                           AND p.date_effective > e.date_effective)),
        m.tentative,
        (SELECT i.motif FROM marche_incident i
          WHERE i.marche_id = m.id AND i.type_incident = 'recours' AND i.statut = 'ouvert'
          ORDER BY i.date_incident DESC LIMIT 1)
   FROM marche m
   LEFT JOIN marche_type t ON t.id = m.type_id
   LEFT JOIN tiers ti ON ti.id = m.attributaire_id
   LEFT JOIN projet p ON p.id = m.projet_id
   LEFT JOIN utilisateur u ON u.id = m.responsable_id";

fn ligne_marche(r: &Row, today: &str) -> rusqlite::Result<Marche> {
    let nb_oblig: i64 = r.get(24)?;
    let nb_oblig_ok: i64 = r.get(25)?;
    let montant_estime: f64 = r.get(5)?;
    let montant_attribue: Option<f64> = r.get(6)?;
    let plus_ancienne_en_cours: Option<String> = r.get(27)?;
    let retard = plus_ancienne_en_cours
        .filter(|d| d.as_str() < today)
        .map(|d| jours_entre(&d, today));
    let montant_avenants: f64 = r.get(29)?;
    let delai_avenants_jours: i64 = r.get(30)?;
    let date_cloture_prevue: Option<String> = r.get(10)?;
    // Le montant de référence des avenants est celui qui engage : l'attribué
    // s'il existe, l'estimation sinon.
    let base = montant_attribue.unwrap_or(montant_estime);
    Ok(Marche {
        id: r.get(0)?,
        numero: r.get(1)?,
        objet: r.get(2)?,
        type_id: r.get(3)?,
        type_libelle: r.get(4)?,
        montant_estime,
        montant_attribue,
        monnaie: r.get(7)?,
        statut: r.get(8)?,
        date_lancement: r.get(9)?,
        date_cloture_revisee: match (&date_cloture_prevue, delai_avenants_jours) {
            (Some(d), j) if j != 0 => Some(ajouter_jours(d, j)),
            _ => None,
        },
        date_cloture_prevue,
        date_cloture_effective: r.get(11)?,
        attributaire_id: r.get(12)?,
        attributaire_nom: r.get(13)?,
        projet_id: r.get(14)?,
        projet_nom: r.get(15)?,
        responsable_id: r.get(16)?,
        responsable_nom: r.get(17)?,
        lieu_execution: r.get(18)?,
        observations: r.get(19)?,
        motif_annulation: r.get(20)?,
        cree_le: r.get(21)?,
        nb_etapes: r.get(22)?,
        nb_etapes_terminees: r.get(23)?,
        avancement: if nb_oblig > 0 { nb_oblig_ok * 100 / nb_oblig } else { 0 },
        retard_jours: retard,
        nb_soumissionnaires: r.get(26)?,
        ecart_montant: montant_attribue.map(|m| m - montant_estime),
        nb_avenants: r.get(28)?,
        montant_avenants,
        delai_avenants_jours,
        montant_courant: base + montant_avenants,
        avenants_pct: if base.abs() > 0.005 {
            Some((montant_avenants / base * 1000.0).round() / 10.0)
        } else {
            None
        },
        nb_receptions: r.get(31)?,
        reserves_ouvertes: r.get::<_, i64>(32)? > 0,
        date_reception_definitive: r.get(33)?,
        nb_avenants_projet: r.get(34)?,
        nb_receptions_provisoires: r.get(35)?,
        derniere_etape_prevue: r.get(36)?,
        nb_etapes_avant_lancement: r.get(37)?,
        nb_etapes_obligatoires_ouvertes: r.get(38)?,
        nb_dates_incoherentes: r.get(39)?,
        tentative: r.get(40)?,
        recours_en_cours: r.get(41)?,
        alertes: Vec::new(),
        etapes: Vec::new(),
        soumissionnaires: Vec::new(),
        avenants: Vec::new(),
        receptions: Vec::new(),
        incidents: Vec::new(),
    })
}

/// Incohérences signalées à l'écran. **Aucune n'empêche d'enregistrer.**
///
/// ⚠️ Cette fonction ne s'appuie QUE sur des champs scalaires du marché, jamais
/// sur `m.etapes` / `m.avenants` / `m.receptions`. C'est délibéré : les mêmes
/// alertes doivent apparaître sur la **liste** des marchés, où l'on ne charge
/// pas le détail de chaque dossier. Les compteurs nécessaires sont agrégés en
/// SQL dans `MARCHE_COLS`.
fn alertes(m: &Marche) -> Vec<String> {
    let mut a = Vec::new();
    if let Some(r) = m.retard_jours {
        a.push(format!("Une étape est en retard de {r} jour(s)."));
    }
    // Une étape datée avant le lancement : le marché affiche alors un retard
    // énorme alors qu'il vient d'être lancé. On nomme la cause plutôt que de
    // laisser l'utilisateur douter du chiffre.
    if m.nb_etapes_avant_lancement > 0 {
        a.push(format!(
            "{} étape(s) sont prévues AVANT la date de lancement du {} : le retard affiché vient de là.",
            m.nb_etapes_avant_lancement, m.date_lancement
        ));
    }
    if let (Some(attr), true) = (m.montant_attribue, m.montant_estime > 0.0) {
        if attr > m.montant_estime {
            let ecart = attr - m.montant_estime;
            let pct = (ecart / m.montant_estime * 100.0).round();
            a.push(format!(
                "Le montant attribué dépasse l'estimation de {ecart:.0} ({pct} %)."
            ));
        }
    }
    if let (Some(prevue), Some(derniere)) =
        (m.date_cloture_prevue.as_deref(), m.derniere_etape_prevue.as_deref())
    {
        if derniere > prevue {
            a.push(format!(
                "La dernière étape est prévue le {derniere}, après la clôture annoncée le {prevue}."
            ));
        }
    }
    if m.statut == "realise" && m.nb_etapes_obligatoires_ouvertes > 0 {
        a.push(format!(
            "Le marché est déclaré réalisé alors que {} étape(s) obligatoire(s) ne sont pas terminées.",
            m.nb_etapes_obligatoires_ouvertes
        ));
    }
    if m.attributaire_id.is_none() && m.montant_attribue.is_some() {
        a.push("Un montant est attribué mais aucun attributaire n'est enregistré.".into());
    }
    // --- Avenants ---
    if let Some(pct) = m.avenants_pct {
        if pct > SEUIL_ALERTE_AVENANTS_PCT {
            a.push(format!(
                "Les avenants approuvés représentent {pct} % du montant initial (repère usuel : {SEUIL_ALERTE_AVENANTS_PCT} %)."
            ));
        }
    }
    if m.nb_avenants_projet > 0 {
        a.push(format!(
            "{} avenant(s) en attente d'approbation : leur montant n'est pas encore compté.",
            m.nb_avenants_projet
        ));
    }
    if m.delai_avenants_jours != 0 {
        if let Some(rev) = m.date_cloture_revisee.as_deref() {
            a.push(format!(
                "Les avenants accordent {} jour(s) : la clôture serait reportée au {rev}.",
                m.delai_avenants_jours
            ));
        }
    }
    // --- Réception ---
    if m.reserves_ouvertes {
        a.push("Des réserves de réception ne sont pas levées : la retenue de garantie reste due.".into());
    }
    let definitive = m.date_reception_definitive.is_some();
    if definitive && m.nb_receptions_provisoires == 0 {
        a.push("Une réception définitive est enregistrée sans réception provisoire préalable.".into());
    }
    if definitive && m.reserves_ouvertes {
        a.push("La réception définitive est prononcée alors que des réserves restent ouvertes.".into());
    }
    if m.statut == "realise" && m.nb_receptions == 0 {
        a.push("Le marché est déclaré réalisé sans aucune réception enregistrée.".into());
    }
    if definitive && m.statut == "en_cours" {
        a.push("La réception définitive est prononcée : le marché peut être déclaré réalisé.".into());
    }
    // --- Procédure ---
    if m.nb_dates_incoherentes > 0 {
        a.push(format!(
            "{} étape(s) sont datées AVANT une étape qui les précède : c'est chronologiquement impossible. Corrigez la date de réalisation dans le déroulé.",
            m.nb_dates_incoherentes
        ));
    }
    // Un recours gèle la procédure : c'est un arrêt SUBI. Le dire évite qu'on
    // le prenne pour un retard de l'administration.
    if let Some(r) = m.recours_en_cours.as_deref() {
        a.push(format!(
            "Procédure gelée par un recours en cours : {r}. Les étapes n'avancent pas tant qu'il n'est pas tranché."
        ));
    }
    if m.tentative > 1 {
        a.push(format!(
            "Procédure relancée : {}ᵉ tentative après un appel d'offres infructueux.",
            m.tentative
        ));
    }
    a
}

pub fn lister(conn: &Connection, f: &FiltreMarches) -> Result<Vec<Marche>> {
    let today = aujourdhui();
    let texte = f.texte.as_deref().map(|t| format!("%{}%", t.trim().to_lowercase()));
    let sql = format!(
        "{MARCHE_COLS}
          WHERE (?1 IS NULL OR m.statut = ?1)
            AND (?2 IS NULL OR m.type_id = ?2)
            AND (?3 IS NULL OR m.responsable_id = ?3)
            AND (?4 IS NULL OR m.projet_id = ?4)
            AND (?5 IS NULL OR m.date_lancement >= ?5)
            AND (?6 IS NULL OR m.date_lancement <= ?6)
            AND (?7 IS NULL OR lower(m.numero) LIKE ?7 OR lower(m.objet) LIKE ?7)
          ORDER BY m.date_lancement DESC, m.numero DESC"
    );
    let mut st = conn.prepare(&sql)?;
    let mut v = st
        .query_map(
            params![f.statut, f.type_id, f.responsable_id, f.projet_id, f.du, f.au, texte],
            |r| ligne_marche(r, &today),
        )?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    if f.en_retard {
        v.retain(|m| m.retard_jours.is_some());
    }
    // Les alertes sont posées ici AUSSI : l'utilisateur doit voir ce qui cloche
    // depuis la liste, sans avoir à ouvrir chaque dossier pour le découvrir.
    for m in v.iter_mut() {
        m.alertes = alertes(m);
    }
    Ok(v)
}

// ===========================================================================
// Tableau de suivi par phase (vue « Kanban »)
//
// Répond à une question que la liste ne sait pas poser : **où les marchés
// se bloquent-ils ?**
//
// Le goulot ne se mesure PAS au nombre de cartes : une colonne chargée où tout
// avance vite n'est pas un problème. Il se mesure au **temps passé**, comparé à
// ce que la procédure elle-même avait prévu. Ce point de comparaison n'est donc
// pas un seuil inventé : il vient des dates prévues du marché.
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct CarteMarche {
    pub id: String,
    pub numero: String,
    pub objet: String,
    pub statut: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub type_libelle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub attributaire_nom: Option<String>,
    pub montant_courant: f64,
    pub monnaie: String,
    pub avancement: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retard_jours: Option<i64>,
    /// L'étape en cours, nommée : c'est elle qu'on va traiter.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub etape_courante: Option<String>,
    /// Depuis combien de jours ce marché est dans cette phase.
    pub jours_dans_phase: i64,
    /// Ce que la procédure prévoyait pour cette phase.
    pub jours_prevus_phase: i64,
    pub reserves_ouvertes: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recours_en_cours: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub alertes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ColonnePhase {
    pub code: String,
    pub libelle: String,
    pub nb: i64,
    pub montant_total: f64,
    /// Moyennes sur les marchés **présents** dans la colonne.
    pub jours_prevus_moy: i64,
    pub jours_reels_moy: i64,
    /// Le réel dépasse le prévu : c'est là que ça coince.
    pub goulot: bool,
    pub marches: Vec<CarteMarche>,
}

/// Date d'entrée d'un marché dans sa phase courante : la dernière date
/// effective d'une étape appartenant à une phase ANTÉRIEURE. À défaut (rien
/// n'est encore franchi), le lancement du marché.
fn entree_dans_phase(m: &Marche, phase: &str) -> String {
    let r = rang_phase(phase);
    m.etapes
        .iter()
        .filter(|e| e.phase.as_deref().map(|p| rang_phase(p) < r).unwrap_or(false))
        .filter_map(|e| e.date_effective.clone())
        .max()
        .unwrap_or_else(|| m.date_lancement.clone())
}

/// Ce que la procédure prévoyait pour cette phase : de la fin prévue de la
/// phase précédente à la fin prévue de celle-ci.
fn duree_prevue_phase(m: &Marche, phase: &str) -> i64 {
    let r = rang_phase(phase);
    let fin = m
        .etapes
        .iter()
        .filter(|e| e.phase.as_deref() == Some(phase))
        .filter_map(|e| e.date_prevue.as_deref())
        .max();
    let debut = m
        .etapes
        .iter()
        .filter(|e| e.phase.as_deref().map(|p| rang_phase(p) < r).unwrap_or(false))
        .filter_map(|e| e.date_prevue.as_deref())
        .max()
        .unwrap_or(m.date_lancement.as_str());
    match fin {
        Some(f) if f > debut => jours_entre(debut, f),
        _ => 0,
    }
}

/// Le tableau de suivi complet. Les marchés **annulés et réalisés** en sont
/// exclus : on regarde ce qui est en cours, pas ce qui est classé.
pub fn tableau_phases(conn: &Connection, f: &FiltreMarches) -> Result<Vec<ColonnePhase>> {
    let today = aujourdhui();
    let liste = lister(conn, f)?;

    let mut colonnes: Vec<ColonnePhase> = PHASES
        .iter()
        .map(|(c, l)| ColonnePhase {
            code: (*c).to_string(),
            libelle: (*l).to_string(),
            nb: 0,
            montant_total: 0.0,
            jours_prevus_moy: 0,
            jours_reels_moy: 0,
            goulot: false,
            marches: Vec::new(),
        })
        .collect();

    for ligne in liste {
        // Les dossiers CLASSÉS sortent du tableau : un marché réalisé ou annulé
        // n'attend plus rien, et le garder gonflerait les moyennes (un dossier
        // clos depuis 200 jours ferait passer sa colonne pour un goulot).
        if ligne.statut == "annule" || ligne.statut == "realise" {
            continue;
        }
        // On relit le marché : le tableau a besoin des étapes pour situer la
        // phase, et `lister` ne les charge pas (elle sert à afficher une liste).
        let m = lire(conn, &ligne.id)?;
        // La phase du moment : celle de l'étape courante ; à défaut, la dernière
        // phase atteinte — un marché dont tout est franchi est en exécution.
        let phase = m
            .etapes
            .iter()
            .find(|e| e.est_courante)
            .and_then(|e| e.phase.clone())
            .or_else(|| m.etapes.iter().filter_map(|e| e.phase.clone()).next_back())
            .unwrap_or_else(|| "preparation".to_string());

        let Some(col) = colonnes.iter_mut().find(|c| c.code == phase) else { continue };
        let debut = entree_dans_phase(&m, &phase);
        let carte = CarteMarche {
            jours_dans_phase: jours_entre(&debut, &today).max(0),
            jours_prevus_phase: duree_prevue_phase(&m, &phase),
            etape_courante: m.etapes.iter().find(|e| e.est_courante).map(|e| e.libelle.clone()),
            id: m.id.clone(),
            numero: m.numero.clone(),
            objet: m.objet.clone(),
            statut: m.statut.clone(),
            type_libelle: m.type_libelle.clone(),
            attributaire_nom: m.attributaire_nom.clone(),
            montant_courant: m.montant_courant,
            monnaie: m.monnaie.clone(),
            avancement: m.avancement,
            retard_jours: m.retard_jours,
            reserves_ouvertes: m.reserves_ouvertes,
            recours_en_cours: m.recours_en_cours.clone(),
            alertes: m.alertes.clone(),
        };
        col.nb += 1;
        col.montant_total += m.montant_courant;
        col.marches.push(carte);
    }

    for c in colonnes.iter_mut() {
        if c.nb > 0 {
            let n = c.nb;
            c.jours_reels_moy = c.marches.iter().map(|x| x.jours_dans_phase).sum::<i64>() / n;
            c.jours_prevus_moy = c.marches.iter().map(|x| x.jours_prevus_phase).sum::<i64>() / n;
            // Goulot : on y reste plus longtemps que ce que la procédure
            // prévoyait. Si rien n'était prévu, on ne crie pas au loup.
            c.goulot = c.jours_prevus_moy > 0 && c.jours_reels_moy > c.jours_prevus_moy;
            // Le plus long en tête : c'est le dossier à traiter en premier.
            c.marches.sort_by_key(|x| std::cmp::Reverse(x.jours_dans_phase));
        }
    }
    Ok(colonnes)
}

pub fn lire(conn: &Connection, id: &str) -> Result<Marche> {
    let today = aujourdhui();
    let mut st = conn.prepare(&format!("{MARCHE_COLS} WHERE m.id = ?1"))?;
    let mut m = st
        .query_row(params![id], |r| ligne_marche(r, &today))
        .map_err(|_| CoreError::NotFound(format!("marché {id}")))?;
    m.etapes = charger_etapes(conn, id)?;
    m.soumissionnaires = charger_soumissionnaires(conn, id, m.montant_estime)?;
    m.avenants = charger_avenants(conn, id)?;
    m.receptions = charger_receptions(conn, id)?;
    m.incidents = charger_incidents(conn, id)?;
    m.alertes = alertes(&m);
    Ok(m)
}

/// Numérotation : même mécanique que les factures et les ordres de fabrication.
fn numero_suivant(conn: &Connection, date: &str) -> Result<String> {
    let exercice: i64 = date.get(0..4).and_then(|a| a.parse().ok()).unwrap_or(1970);
    conn.execute(
        "INSERT INTO sequence_numero (type_document, exercice, dernier)
         VALUES ('marche', ?1, 1)
         ON CONFLICT(type_document, exercice) DO UPDATE SET dernier = dernier + 1",
        params![exercice],
    )?;
    let n: i64 = conn.query_row(
        "SELECT dernier FROM sequence_numero WHERE type_document = 'marche' AND exercice = ?1",
        params![exercice],
        |r| r.get(0),
    )?;
    let prefixe: String = conn
        .query_row(
            "SELECT prefixe FROM config_prefixe_document WHERE type_document = 'marche'",
            [],
            |r| r.get(0),
        )
        .unwrap_or_else(|_| "MA".to_string());
    Ok(format!("{prefixe}-{exercice}-{n:04}"))
}

/// Crée un marché et **instancie la procédure de son type** : les étapes sont
/// recopiées, leurs dates prévues calculées par cumul des durées depuis la date
/// de lancement. L'utilisateur peut ensuite tout modifier — le modèle amorce,
/// il ne contraint pas.
pub fn creer(conn: &Connection, n: &NouveauMarche, par: Option<&str>) -> Result<Marche> {
    if n.objet.trim().is_empty() {
        return Err(CoreError::Rule("l'objet du marché est obligatoire".into()));
    }
    let date = vide(&n.date_lancement).map(|s| s.to_string()).unwrap_or_else(aujourdhui);
    let numero = numero_suivant(conn, &date)?;
    let id = Uuid::new_v4().to_string();

    conn.execute(
        "INSERT INTO marche
            (id, numero, objet, type_id, montant_estime, montant_attribue, monnaie,
             statut, date_lancement, date_cloture_prevue, attributaire_id, projet_id,
             responsable_id, lieu_execution, observations, cree_par, cree_le)
         VALUES (?1,?2,?3,?4,?5,?6,COALESCE(?7,'FCFA'),'en_cours',?8,?9,?10,?11,?12,?13,?14,?15,?16)",
        params![
            id, numero, n.objet.trim(), vide(&n.type_id), n.montant_estime, n.montant_attribue,
            vide(&n.monnaie), date, vide(&n.date_cloture_prevue), vide(&n.attributaire_id),
            vide(&n.projet_id), vide(&n.responsable_id), vide(&n.lieu_execution),
            vide(&n.observations), par, now()
        ],
    )?;

    // Les étapes viennent soit de l'écran (l'utilisateur a ajusté les dates),
    // soit de la procédure du type. Dans les deux cas le libellé est RECOPIÉ :
    // corriger la procédure plus tard ne doit pas réécrire ce marché.
    let mut derniere_date: Option<String> = None;
    if !n.etapes.is_empty() {
        for (i, e) in n.etapes.iter().enumerate() {
            if e.libelle.trim().is_empty() {
                continue;
            }
            conn.execute(
                // La phase vient du modèle d'origine quand il est connu ;
                // sinon elle restera NULL et sera héritée de l'étape
                // précédente à l'affichage (voir `poser_enchainement`).
                "INSERT INTO marche_etape
                    (id, marche_id, etape_modele_id, libelle, ordre, date_prevue,
                     statut, obligatoire, phase, cree_le)
                 VALUES (?1,?2,?3,?4,?5,?6,'en_attente',?7,
                         (SELECT phase FROM marche_etape_modele WHERE id = ?3),?8)",
                params![
                    Uuid::new_v4().to_string(), id, vide(&e.etape_modele_id),
                    e.libelle.trim(), i as i64 + 1, vide(&e.date_prevue),
                    e.obligatoire as i64, now()
                ],
            )?;
            if let Some(d) = vide(&e.date_prevue) {
                derniere_date = Some(d.to_string());
            }
        }
    } else if let Some(type_id) = vide(&n.type_id) {
        let modeles = etapes_modele(conn, type_id)?;
        let mut curseur = date.clone();
        for m in modeles.iter().filter(|m| m.actif) {
            curseur = ajouter_jours(&curseur, m.duree_prevue_jours.max(0));
            conn.execute(
                // La PHASE est recopiée comme le libellé : l'instance doit
                // pouvoir vivre sa vie sans dépendre du modèle.
                "INSERT INTO marche_etape
                    (id, marche_id, etape_modele_id, libelle, description, ordre,
                     date_prevue, statut, obligatoire, phase, cree_le)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,'en_attente',?8,?9,?10)",
                params![
                    Uuid::new_v4().to_string(), id, m.id, m.libelle, m.description,
                    m.ordre, curseur, m.obligatoire as i64, m.phase, now()
                ],
            )?;
        }
        if !modeles.is_empty() {
            derniere_date = Some(curseur);
        }
    }
    // La clôture prévue, si elle n'est pas donnée, découle de la dernière étape.
    if vide(&n.date_cloture_prevue).is_none() {
        if let Some(d) = derniere_date {
            conn.execute(
                "UPDATE marche SET date_cloture_prevue = ?2 WHERE id = ?1",
                params![id, d],
            )?;
        }
    }
    lire(conn, &id)
}

pub fn modifier(conn: &Connection, id: &str, n: &NouveauMarche) -> Result<Marche> {
    lire(conn, id)?;
    if n.objet.trim().is_empty() {
        return Err(CoreError::Rule("l'objet du marché est obligatoire".into()));
    }
    conn.execute(
        "UPDATE marche SET objet = ?2, type_id = ?3, montant_estime = ?4,
                montant_attribue = ?5, monnaie = COALESCE(?6, monnaie),
                date_lancement = COALESCE(?7, date_lancement),
                date_cloture_prevue = ?8, attributaire_id = ?9, projet_id = ?10,
                responsable_id = ?11, lieu_execution = ?12, observations = ?13
          WHERE id = ?1",
        params![
            id, n.objet.trim(), vide(&n.type_id), n.montant_estime, n.montant_attribue,
            vide(&n.monnaie), vide(&n.date_lancement), vide(&n.date_cloture_prevue),
            vide(&n.attributaire_id), vide(&n.projet_id), vide(&n.responsable_id),
            vide(&n.lieu_execution), vide(&n.observations)
        ],
    )?;
    lire(conn, id)
}

/// Changement de statut. Passer à `realise` horodate la clôture effective.
/// Une étape obligatoire non terminée est **signalée**, jamais bloquante.
pub fn changer_statut(conn: &Connection, id: &str, statut: &str) -> Result<Marche> {
    const STATUTS: &[&str] = &["en_cours", "realise", "annule", "suspendu"];
    if !STATUTS.contains(&statut) {
        return Err(CoreError::Rule(format!("statut de marché inconnu : {statut}")));
    }
    if statut == "annule" {
        return Err(CoreError::Rule(
            "l'annulation d'un marché exige un motif : employez `annuler`".into(),
        ));
    }
    lire(conn, id)?;
    if statut == "realise" {
        conn.execute(
            "UPDATE marche SET statut = 'realise',
                    date_cloture_effective = COALESCE(date_cloture_effective, ?2)
              WHERE id = ?1",
            params![id, aujourdhui()],
        )?;
    } else {
        conn.execute("UPDATE marche SET statut = ?2 WHERE id = ?1", params![id, statut])?;
    }
    lire(conn, id)
}

/// Annulation : **motif obligatoire**, auteur et date tracés. Même exigence que
/// l'annulation d'une facture encaissée (migration 0019).
pub fn annuler(conn: &Connection, id: &str, motif: &str, par: Option<&str>) -> Result<Marche> {
    if motif.trim().is_empty() {
        return Err(CoreError::Rule("le motif d'annulation est obligatoire".into()));
    }
    lire(conn, id)?;
    conn.execute(
        "UPDATE marche SET statut = 'annule', motif_annulation = ?2, annule_par = ?3, annule_le = ?4
          WHERE id = ?1",
        params![id, motif.trim(), par, now()],
    )?;
    lire(conn, id)
}

/// Suppression : réservée aux marchés **sans aucune étape franchie**. Dès qu'une
/// étape est terminée, le marché est de l'histoire — on l'annule avec un motif.
pub fn supprimer(conn: &Connection, id: &str) -> Result<()> {
    lire(conn, id)?;
    let franchies: i64 = conn.query_row(
        "SELECT COUNT(*) FROM marche_etape WHERE marche_id = ?1 AND statut = 'termine'",
        params![id],
        |r| r.get(0),
    )?;
    if franchies > 0 {
        return Err(CoreError::Rule(
            "ce marché a des étapes déjà franchies : annulez-le avec un motif plutôt que de le supprimer".into(),
        ));
    }
    // Un avenant approuvé ou une réception sont des actes : ils font du marché
    // de l'historique, au même titre qu'une étape franchie.
    let actes: i64 = conn.query_row(
        "SELECT (SELECT COUNT(*) FROM marche_avenant WHERE marche_id = ?1 AND statut = 'approuve')
              + (SELECT COUNT(*) FROM marche_reception WHERE marche_id = ?1)",
        params![id],
        |r| r.get(0),
    )?;
    if actes > 0 {
        return Err(CoreError::Rule(
            "ce marché porte des avenants approuvés ou des réceptions : annulez-le avec un motif plutôt que de le supprimer".into(),
        ));
    }
    // Détachement des pièces jointes avant purge : un document ne doit jamais
    // se retrouver orphelin d'une clé étrangère.
    conn.execute(
        "UPDATE document_joint SET marche_etape_id = NULL WHERE marche_id = ?1",
        params![id],
    )?;
    conn.execute("UPDATE document_joint SET marche_id = NULL WHERE marche_id = ?1", params![id])?;
    conn.execute("DELETE FROM marche_incident WHERE marche_id = ?1", params![id])?;
    conn.execute("DELETE FROM marche_avenant WHERE marche_id = ?1", params![id])?;
    conn.execute("DELETE FROM marche_reception WHERE marche_id = ?1", params![id])?;
    conn.execute("DELETE FROM marche_soumissionnaire WHERE marche_id = ?1", params![id])?;
    conn.execute("DELETE FROM marche_etape WHERE marche_id = ?1", params![id])?;
    conn.execute("DELETE FROM marche WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn changer_statut_lot(conn: &Connection, ids: &[String], statut: &str) -> Result<usize> {
    let mut n = 0;
    for id in ids {
        if changer_statut(conn, id, statut).is_ok() {
            n += 1;
        }
    }
    Ok(n)
}

/// Résultat d'une suppression groupée. On rend compte des DEUX cas : ce qui a
/// été supprimé, et ce qui a été **conservé parce que le marché a une histoire**
/// (étape franchie, avenant approuvé, réception). Un simple compteur laisserait
/// l'utilisateur croire à un échec silencieux.
#[derive(Debug, Clone, Serialize)]
pub struct ResultatSuppressionLot {
    pub supprimes: usize,
    pub conserves: usize,
    /// Numéros des marchés conservés, pour pouvoir le dire à l'écran.
    pub numeros_conserves: Vec<String>,
}

pub fn supprimer_lot(conn: &Connection, ids: &[String]) -> Result<ResultatSuppressionLot> {
    let mut r = ResultatSuppressionLot {
        supprimes: 0,
        conserves: 0,
        numeros_conserves: Vec::new(),
    };
    for id in ids {
        // On lit le numéro AVANT : après suppression il n'existe plus.
        let numero: Option<String> = conn
            .query_row("SELECT numero FROM marche WHERE id = ?1", params![id], |x| x.get(0))
            .ok();
        match supprimer(conn, id) {
            Ok(()) => r.supprimes += 1,
            Err(_) => {
                r.conserves += 1;
                if let Some(n) = numero {
                    r.numeros_conserves.push(n);
                }
            }
        }
    }
    Ok(r)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn marche_travaux(conn: &Connection) -> Marche {
        creer(conn, &NouveauMarche {
            objet: "Construction d'une salle de classe".into(),
            type_id: Some("mt-travaux".into()),
            montant_estime: 25_000_000.0,
            montant_attribue: None, monnaie: None,
            date_lancement: Some("2026-01-05".into()),
            date_cloture_prevue: None, attributaire_id: None, projet_id: None,
            responsable_id: None, lieu_execution: Some("Matam".into()), observations: None,
            etapes: Vec::new(),
        }, Some("u1")).unwrap()
    }

    #[test]
    fn le_type_instancie_sa_procedure_avec_des_dates_cumulees() {
        let conn = db::open_in_memory().unwrap();
        let m = marche_travaux(&conn);

        assert!(m.numero.starts_with("MA-2026-"), "numéro : {}", m.numero);
        assert_eq!(m.nb_etapes, 8, "les 8 étapes de la procédure Travaux");
        assert_eq!(m.avancement, 0);

        // Dates par cumul : dossier 10 j → 15/01, publication 3 j → 18/01.
        assert_eq!(m.etapes[0].date_prevue.as_deref(), Some("2026-01-15"));
        assert_eq!(m.etapes[1].date_prevue.as_deref(), Some("2026-01-18"));
        // La clôture prévue découle de la dernière étape :
        // 10+3+21+1+7+7+10+5 = 64 jours après le 5 janvier.
        assert_eq!(m.date_cloture_prevue.as_deref(), Some("2026-03-10"));
    }

    #[test]
    fn terminer_une_etape_horodate_et_fait_avancer() {
        let conn = db::open_in_memory().unwrap();
        let m = marche_travaux(&conn);
        let e = changer_statut_etape(&conn, &m.etapes[0].id, "termine", Some("u1")).unwrap();
        assert_eq!(e.statut, "termine");
        assert!(e.date_effective.is_some(), "la date effective est posée toute seule");
        assert_eq!(e.valide_par.as_deref(), Some("u1"));
        assert!(e.valide_le.is_some());

        let m = lire(&conn, &m.id).unwrap();
        assert_eq!(m.nb_etapes_terminees, 1);
        assert_eq!(m.avancement, 12, "1 étape sur 8");
    }

    #[test]
    fn la_replanification_propose_avant_dappliquer() {
        let conn = db::open_in_memory().unwrap();
        let m = marche_travaux(&conn);
        // La première étape se termine avec 20 jours de retard.
        modifier_etape(&conn, &m.etapes[0].id, &MajEtape {
            date_effective: Some("2026-02-04".into()), statut: Some("termine".into()),
            libelle: None, date_prevue: None, observations: None, obligatoire: None,
            motif_derogation: None,
        }).unwrap();

        let plan = plan_replanification(&conn, &m.etapes[0].id).unwrap();
        assert_eq!(plan.len(), 7, "les 7 étapes suivantes");
        assert_eq!(plan[0].date_proposee, "2026-02-07", "20 jours de décalage");
        assert_eq!(plan[0].decalage_jours, 20);

        // ⚠️ L'aperçu n'écrit RIEN tant qu'on n'applique pas.
        let avant = lire(&conn, &m.id).unwrap();
        assert_eq!(avant.etapes[1].date_prevue.as_deref(), Some("2026-01-18"));

        let n = replanifier(&conn, &m.etapes[0].id).unwrap();
        assert_eq!(n, 7);
        let apres = lire(&conn, &m.id).unwrap();
        assert_eq!(apres.etapes[1].date_prevue.as_deref(), Some("2026-02-07"));
    }

    #[test]
    fn un_avenant_ne_compte_quune_fois_approuve_et_ne_reecrit_pas_le_marche() {
        let conn = db::open_in_memory().unwrap();
        let m = marche_travaux(&conn); // estimé 25 M, clôture prévue 2026-03-10

        let a = ajouter_avenant(&conn, &m.id, &NouvelAvenant {
            objet: "Travaux supplémentaires de terrassement".into(),
            montant_variation: 5_000_000.0, delai_jours: 15,
            date_avenant: Some("2026-02-01".into()), motif: None,
        }, Some("u1")).unwrap();
        assert_eq!(a.numero, 1, "la numérotation est par marché");
        assert_eq!(a.statut, "projet");

        // À l'état de projet : rien n'est engagé.
        let m2 = lire(&conn, &m.id).unwrap();
        assert_eq!(m2.montant_avenants, 0.0);
        assert_eq!(m2.montant_courant, 25_000_000.0);
        assert!(m2.date_cloture_revisee.is_none());
        assert!(m2.alertes.iter().any(|x| x.contains("attente d'approbation")), "{:?}", m2.alertes);

        // Approbation : le montant courant bouge, le montant d'origine JAMAIS.
        let a = statut_avenant(&conn, &a.id, "approuve", Some("u1")).unwrap();
        assert_eq!(a.approuve_par.as_deref(), Some("u1"));
        assert!(a.approuve_le.is_some());
        assert!(!a.modifiable, "un avenant approuvé est figé");

        let m3 = lire(&conn, &m.id).unwrap();
        assert_eq!(m3.montant_estime, 25_000_000.0, "le contrat initial reste intact");
        assert_eq!(m3.montant_avenants, 5_000_000.0);
        assert_eq!(m3.montant_courant, 30_000_000.0);
        assert_eq!(m3.avenants_pct, Some(20.0));
        // La clôture est PROPOSÉE, pas écrite : 10/03 + 15 j.
        assert_eq!(m3.date_cloture_revisee.as_deref(), Some("2026-03-25"));
        assert_eq!(m3.date_cloture_prevue.as_deref(), Some("2026-03-10"),
                   "aucune date n'est recalculée sans geste explicite");

        // Approuvé = ni modifiable, ni supprimable, ni rétractable.
        assert!(modifier_avenant(&conn, &a.id, &NouvelAvenant {
            objet: "Autre".into(), montant_variation: 0.0, delai_jours: 0,
            date_avenant: None, motif: None,
        }).is_err());
        assert!(supprimer_avenant(&conn, &a.id).is_err());
        assert!(statut_avenant(&conn, &a.id, "rejete", None).is_err());

        // Le seuil de 30 % est signalé, jamais refusé.
        let a2 = ajouter_avenant(&conn, &m.id, &NouvelAvenant {
            objet: "Reprise de la toiture".into(), montant_variation: 4_000_000.0,
            delai_jours: 0, date_avenant: None, motif: None,
        }, None).unwrap();
        assert_eq!(a2.numero, 2);
        statut_avenant(&conn, &a2.id, "approuve", None).unwrap();
        let m4 = lire(&conn, &m.id).unwrap();
        assert_eq!(m4.avenants_pct, Some(36.0));
        assert!(m4.alertes.iter().any(|x| x.contains("36 %")), "{:?}", m4.alertes);

        // Un marché porteur d'un avenant approuvé ne se supprime plus.
        assert!(supprimer(&conn, &m.id).is_err());
    }

    #[test]
    fn reception_avec_reserves_puis_levee_puis_definitive() {
        let conn = db::open_in_memory().unwrap();
        let m = marche_travaux(&conn);

        // Des réserves sans en dire le contenu : refusé, le PV serait invérifiable.
        assert!(ajouter_reception(&conn, &m.id, &NouvelleReception {
            type_reception: Some("provisoire".into()), date_reception: Some("2026-04-01".into()),
            resultat: Some("avec_reserves".into()), reserves: None, date_levee_reserves: None,
            garantie_mois: None, montant_retenue_garantie: None, observations: None,
            receptionne_par: None,
        }, None).is_err());

        let r = ajouter_reception(&conn, &m.id, &NouvelleReception {
            type_reception: Some("provisoire".into()), date_reception: Some("2026-04-01".into()),
            resultat: Some("avec_reserves".into()),
            reserves: Some("Peinture inachevée sur la façade nord".into()),
            date_levee_reserves: None, garantie_mois: Some(12),
            montant_retenue_garantie: Some(1_250_000.0), observations: None,
            receptionne_par: Some("Commission de réception".into()),
        }, Some("u1")).unwrap();
        assert!(r.reserves_ouvertes);
        assert_eq!(r.fin_garantie.as_deref(), Some("2027-03-27"), "12 mois de garantie");

        let m2 = lire(&conn, &m.id).unwrap();
        assert!(m2.reserves_ouvertes);
        assert!(m2.alertes.iter().any(|x| x.contains("retenue de garantie")), "{:?}", m2.alertes);

        // Une définitive prononcée trop tôt : acceptée, mais doublement signalée.
        let d = ajouter_reception(&conn, &m.id, &NouvelleReception {
            type_reception: Some("definitive".into()), date_reception: Some("2026-05-01".into()),
            resultat: None, reserves: None, date_levee_reserves: None, garantie_mois: None,
            montant_retenue_garantie: None, observations: None, receptionne_par: None,
        }, None).unwrap();
        assert_eq!(d.resultat, "prononcee", "résultat par défaut");
        let m3 = lire(&conn, &m.id).unwrap();
        assert!(m3.alertes.iter().any(|x| x.contains("réserves restent ouvertes")), "{:?}", m3.alertes);
        assert!(m3.alertes.iter().any(|x| x.contains("peut être déclaré réalisé")), "{:?}", m3.alertes);
        assert_eq!(m3.date_reception_definitive.as_deref(), Some("2026-05-01"));

        // La levée des réserves libère la garantie.
        let r = lever_reserves(&conn, &r.id, Some("2026-04-20")).unwrap();
        assert!(!r.reserves_ouvertes);
        assert_eq!(r.date_levee_reserves.as_deref(), Some("2026-04-20"));
        assert!(lever_reserves(&conn, &d.id, None).is_err(), "rien à lever ici");

        let m4 = lire(&conn, &m.id).unwrap();
        assert!(!m4.reserves_ouvertes);
        assert_eq!(m4.nb_receptions, 2);
        assert!(!m4.alertes.iter().any(|x| x.contains("retenue de garantie")), "{:?}", m4.alertes);

        // Un marché réceptionné ne se supprime pas ; on l'annule avec un motif.
        assert!(supprimer(&conn, &m.id).is_err());
    }

    /// Les alertes doivent être visibles depuis la LISTE, sans ouvrir chaque
    /// dossier — sinon l'utilisateur découvre les problèmes un par un.
    /// Vérifie aussi l'alerte « étape prévue avant le lancement », rencontrée
    /// sur les vraies données : elle explique un retard qui paraît aberrant.
    /// **LE cas signalé par l'utilisateur** : « quelqu'un peut annuler
    /// l'ouverture des plis et continuer les autres étapes. Ça ne se passe pas
    /// comme ça. » Une procédure de passation est une chaîne d'actes.
    #[test]
    fn on_ne_franchit_pas_une_etape_avant_celle_qui_la_fonde() {
        let conn = db::open_in_memory().unwrap();
        let m = marche_travaux(&conn);
        // Travaux : 1 dossier, 2 publication, 3 réception des offres,
        // 4 ouverture des plis, 5 évaluation, 6 attribution…
        let e = |mm: &Marche, i: usize| mm.etapes[i].id.clone();

        // Au départ : seule la première est ouverte, tout le reste est verrouillé.
        assert!(m.etapes[0].est_courante, "la 1re étape est celle du moment");
        assert!(!m.etapes[0].verrouillee);
        assert!(m.etapes[4].verrouillee, "l'évaluation est verrouillée");
        assert!(m.etapes[4].raison_verrou.as_deref().unwrap().contains("Préparation"),
                "{:?}", m.etapes[4].raison_verrou);

        // On ne peut pas évaluer des offres qu'on n'a pas ouvertes.
        let err = changer_statut_etape(&conn, &e(&m, 4), "termine", Some("u1")).unwrap_err();
        assert!(format!("{err}").contains("pas terminée"), "{err}");

        // On déroule proprement les 4 premières.
        for i in 0..4 {
            let m2 = lire(&conn, &m.id).unwrap();
            changer_statut_etape(&conn, &e(&m2, i), "termine", Some("u1")).unwrap();
        }
        let m2 = lire(&conn, &m.id).unwrap();
        assert!(!m2.etapes[4].verrouillee, "l'évaluation est maintenant ouverte");
        assert!(m2.etapes[4].est_courante);
        changer_statut_etape(&conn, &e(&m2, 4), "termine", Some("u1")).unwrap();
        changer_statut_etape(&conn, &lire(&conn, &m.id).unwrap().etapes[5].id, "termine", Some("u1")).unwrap();

        // --- Le geste litigieux : annuler l'ouverture des plis (étape 4) ---
        let m3 = lire(&conn, &m.id).unwrap();
        assert_eq!(m3.etapes[5].statut, "termine", "l'attribution était prononcée");
        let effet = changer_statut_etape_avec(&conn, &e(&m3, 3), "annule", Some("u1"), None).unwrap();

        // Tout ce qui découlait de l'ouverture des plis est remis en cause.
        assert_eq!(effet.etapes_rouvertes.len(), 2, "{:?}", effet.etapes_rouvertes);
        let apres = lire(&conn, &m.id).unwrap();
        assert_eq!(apres.etapes[4].statut, "en_attente", "l'évaluation est rouverte");
        assert_eq!(apres.etapes[5].statut, "en_attente", "l'attribution aussi");
        assert!(apres.etapes[4].date_effective.is_none(), "la validation est effacée");
        assert!(apres.etapes[4].valide_par.is_none());
        // ⚠️ Mais la trace, elle, ne disparaît PAS.
        let obs = apres.etapes[4].observations.clone().unwrap_or_default();
        assert!(obs.contains("Rouverte"), "trace attendue, obtenu : {obs:?}");
        assert!(obs.contains("Ouverture des plis"), "{obs:?}");
        assert!(obs.contains("validée par u1"), "l'ancien validateur est consigné : {obs:?}");
    }

    /// La porte de sortie : reprendre un dossier commencé sur papier. Sans elle
    /// le verrou rendrait le logiciel inutilisable sur les dossiers en cours.
    #[test]
    fn la_derogation_leve_le_verrou_mais_laisse_une_trace() {
        let conn = db::open_in_memory().unwrap();
        let m = marche_travaux(&conn);

        // Sans motif : refusé.
        assert!(changer_statut_etape_avec(&conn, &m.etapes[5].id, "termine", Some("u1"), None).is_err());
        // Motif vide : refusé aussi — « passer outre » n'est pas un clic anodin.
        assert!(changer_statut_etape_avec(&conn, &m.etapes[5].id, "termine", Some("u1"), Some("   ")).is_err());

        let effet = changer_statut_etape_avec(
            &conn, &m.etapes[5].id, "termine", Some("djigui"),
            Some("Dossier repris en cours : attribution déjà prononcée sur support papier"),
        ).unwrap();
        assert_eq!(effet.etape.statut, "termine");
        assert!(effet.etape.derogation, "la dérogation est marquée");
        assert!(effet.etape.motif_derogation.unwrap().contains("support papier"));
        assert_eq!(effet.etape.derogation_par.as_deref(), Some("djigui"));
    }

    /// Une seule étape « en cours » : c'est l'étape du moment, il ne peut pas y
    /// en avoir deux.
    #[test]
    fn une_seule_etape_en_cours_a_la_fois() {
        let conn = db::open_in_memory().unwrap();
        let m = marche_travaux(&conn);
        changer_statut_etape(&conn, &m.etapes[0].id, "en_cours", None).unwrap();
        changer_statut_etape(&conn, &m.etapes[0].id, "termine", None).unwrap();
        let m2 = lire(&conn, &m.id).unwrap();
        changer_statut_etape(&conn, &m2.etapes[1].id, "en_cours", None).unwrap();

        let apres = lire(&conn, &m.id).unwrap();
        assert_eq!(apres.etapes.iter().filter(|e| e.statut == "en_cours").count(), 1);
        assert_eq!(apres.etapes[1].statut, "en_cours");
    }

    /// Appel d'offres infructueux : la procédure repart, les offres sont
    /// écartées (jamais effacées : elles prouvent que la consultation a eu lieu)
    /// et l'attribution tombe.
    #[test]
    fn un_appel_doffres_infructueux_relance_la_procedure() {
        let conn = db::open_in_memory().unwrap();
        let m = marche_travaux(&conn);
        for i in 0..6 {
            let mm = lire(&conn, &m.id).unwrap();
            changer_statut_etape(&conn, &mm.etapes[i].id, "termine", Some("u1")).unwrap();
        }
        let s = ajouter_soumissionnaire(&conn, &m.id, &NouveauSoumissionnaire {
            nom: "Entreprise X".into(), montant_offre: Some(30_000_000.0),
            tiers_id: None, ninea: None, telephone: None, montant_offre_ttc: None,
            delai_jours: None, note_technique: None, note_financiere: None,
            statut: None, motif: None, observations: None, date_depot: None,
        }, None).unwrap();
        attribuer(&conn, &s.id).unwrap();
        assert!(lire(&conn, &m.id).unwrap().montant_attribue.is_some());

        let inc = declarer_incident(&conn, &m.id, &NouvelIncident {
            type_incident: "infructueux".into(),
            motif: "Une seule offre reçue, non conforme au cahier des charges".into(),
            etape_id: None, date_incident: None, auteur_recours: None,
        }, Some("u1")).unwrap();
        assert_eq!(inc.tentative, 1, "l'incident garde le n° de la tentative ratée");

        let apres = lire(&conn, &m.id).unwrap();
        assert_eq!(apres.tentative, 2, "le marché passe à la tentative suivante");
        assert!(apres.montant_attribue.is_none(), "l'attribution tombe avec la procédure");
        assert!(apres.attributaire_id.is_none());
        // La publication reste franchie ; tout ce qui suit repart.
        assert_eq!(apres.etapes[1].statut, "termine", "la publication a bien eu lieu");
        assert_eq!(apres.etapes[3].statut, "en_attente", "l'ouverture des plis repart");
        assert_eq!(apres.etapes[5].statut, "en_attente", "l'attribution aussi");
        // L'offre est écartée, pas effacée.
        assert_eq!(apres.soumissionnaires.len(), 1);
        assert_eq!(apres.soumissionnaires[0].statut, "ecarte");
        assert!(apres.alertes.iter().any(|a| a.contains("2ᵉ tentative")), "{:?}", apres.alertes);
    }

    /// Un recours gèle la procédure : c'est un arrêt subi, pas un oubli.
    #[test]
    fn un_recours_gele_la_procedure_jusqua_la_decision() {
        let conn = db::open_in_memory().unwrap();
        let m = marche_travaux(&conn);
        for i in 0..6 {
            let mm = lire(&conn, &m.id).unwrap();
            changer_statut_etape(&conn, &mm.etapes[i].id, "termine", Some("u1")).unwrap();
        }
        let inc = declarer_incident(&conn, &m.id, &NouvelIncident {
            type_incident: "recours".into(),
            motif: "Contestation des critères d'évaluation".into(),
            auteur_recours: Some("Entreprise Ndiaye & Frères".into()),
            etape_id: None, date_incident: None,
        }, Some("u1")).unwrap();

        let gele = lire(&conn, &m.id).unwrap();
        assert!(gele.recours_en_cours.is_some());
        assert!(gele.alertes.iter().any(|a| a.contains("gelée par un recours")), "{:?}", gele.alertes);

        // Impossible d'avancer tant que ce n'est pas tranché.
        let err = changer_statut_etape(&conn, &gele.etapes[6].id, "termine", Some("u1")).unwrap_err();
        assert!(format!("{err}").contains("recours"), "{err}");

        // Décision rendue : la procédure repart.
        assert!(clore_incident(&conn, &inc.id, "  ").is_err(), "une décision vide ne clôt rien");
        clore_incident(&conn, &inc.id, "Recours rejeté par la commission de règlement des différends").unwrap();
        let repris = lire(&conn, &m.id).unwrap();
        assert!(repris.recours_en_cours.is_none());
        assert!(changer_statut_etape(&conn, &repris.etapes[6].id, "termine", Some("u1")).is_ok());
    }

    /// **LE cas de la capture `controle_sur_date.jpg`** : « Publication de
    /// l'avis » faite le 04/11/2025 alors que « Préparation du dossier » l'a été
    /// le 28/07/2026. Un acte ne peut pas être daté avant celui qui le fonde.
    #[test]
    fn une_etape_ne_peut_pas_etre_datee_avant_celle_qui_la_fonde() {
        let conn = db::open_in_memory().unwrap();
        let m = marche_travaux(&conn);

        // Le dossier a été préparé le 28/07/2026.
        changer_statut_etape_saisie(&conn, &m.etapes[0].id, "termine", Some("u1"),
            &SaisieEtape { date_effective: Some("2026-07-28".into()), ..Default::default() }).unwrap();

        // Publier l'avis 9 mois AVANT : impossible.
        let err = changer_statut_etape_saisie(&conn, &m.etapes[1].id, "termine", Some("u1"),
            &SaisieEtape { date_effective: Some("2025-11-04".into()), ..Default::default() })
            .unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("2025-11-04"), "{msg}");
        assert!(msg.contains("Préparation"), "l'étape bloquante est nommée : {msg}");
        assert!(msg.contains("2026-07-28"), "sa date est rappelée : {msg}");

        // Le même jour passe (une procédure peut enchaîner deux actes le jour même).
        assert!(changer_statut_etape_saisie(&conn, &m.etapes[1].id, "termine", Some("u1"),
            &SaisieEtape { date_effective: Some("2026-07-28".into()), ..Default::default() }).is_ok());

        // La porte de sortie reste la dérogation motivée, comme pour l'ordre.
        let m2 = lire(&conn, &m.id).unwrap();
        let ok = changer_statut_etape_saisie(&conn, &m2.etapes[2].id, "termine", Some("djigui"),
            &SaisieEtape {
                date_effective: Some("2025-01-01".into()),
                motif_derogation: Some("Dossier repris : dates d'origine du support papier".into()),
                ..Default::default()
            });
        assert!(ok.is_ok(), "la dérogation lève aussi le contrôle de date");
        assert!(ok.unwrap().etape.derogation);
    }

    /// Corriger une date ne doit pas non plus la faire passer APRÈS des actes
    /// qui l'ont suivie : le contrôle joue dans les deux sens.
    #[test]
    fn corriger_une_date_reste_possible_dans_les_deux_sens() {
        let conn = db::open_in_memory().unwrap();
        let m = marche_travaux(&conn);
        for (i, d) in ["2026-02-01", "2026-02-10", "2026-02-20"].iter().enumerate() {
            let mm = lire(&conn, &m.id).unwrap();
            changer_statut_etape_saisie(&conn, &mm.etapes[i].id, "termine", Some("u1"),
                &SaisieEtape { date_effective: Some((*d).into()), ..Default::default() }).unwrap();
        }
        let m2 = lire(&conn, &m.id).unwrap();
        let maj = |d: &str| MajEtape {
            date_effective: Some(d.into()),
            libelle: None, date_prevue: None, statut: None,
            observations: None, obligatoire: None, motif_derogation: None,
        };
        // Trop tôt : avant l'étape 1 (01/02).
        let e = modifier_etape(&conn, &m2.etapes[1].id, &maj("2026-01-15")).unwrap_err();
        assert!(format!("{e}").contains("précédente"), "{e}");
        // Trop tard : après l'étape 3 (20/02).
        let e = modifier_etape(&conn, &m2.etapes[1].id, &maj("2026-03-01")).unwrap_err();
        assert!(format!("{e}").contains("suivante"), "{e}");
        // Entre les deux : accepté.
        assert!(modifier_etape(&conn, &m2.etapes[1].id, &maj("2026-02-14")).is_ok());
    }

    /// Les dossiers saisis AVANT ce contrôle portent déjà des dates
    /// impossibles : il faut les signaler, sinon personne ne les corrigera.
    #[test]
    fn les_dates_impossibles_deja_en_base_sont_signalees() {
        let conn = db::open_in_memory().unwrap();
        let m = marche_travaux(&conn);
        // On écrit directement en base pour reproduire l'état d'un dossier
        // saisi avant l'existence du contrôle.
        conn.execute(
            "UPDATE marche_etape SET statut = 'termine', date_effective = '2026-07-28'
              WHERE id = ?1", params![m.etapes[0].id]).unwrap();
        conn.execute(
            "UPDATE marche_etape SET statut = 'termine', date_effective = '2025-11-04'
              WHERE id = ?1", params![m.etapes[1].id]).unwrap();

        let lu = lire(&conn, &m.id).unwrap();
        assert!(lu.alertes.iter().any(|a| a.contains("chronologiquement")), "{:?}", lu.alertes);
        // Et l'alerte remonte jusqu'à la LISTE : on doit repérer le dossier sans
        // avoir à l'ouvrir.
        let liste = lister(&conn, &FiltreMarches::default()).unwrap();
        assert!(liste[0].alertes.iter().any(|a| a.contains("chronologiquement")));
    }

    /// L'écart réalisé / prévu : `+` retard, `−` avance, et surtout le **retard
    /// qui court** sur une étape échue mais pas encore faite — sans lui, un
    /// dossier qui traîne aujourd'hui n'apparaîtrait nulle part.
    #[test]
    fn lecart_dit_le_retard_lavance_et_ce_qui_traine() {
        let conn = db::open_in_memory().unwrap();
        let m = marche_travaux(&conn);   // étape 1 prévue le 2026-01-15

        // ⚠️ Les dates doivent rester CHRONOLOGIQUES entre elles : le contrôle
        // de date le vérifie désormais, et c'est lui qui a révélé que la
        // première version de ce test datait l'étape 2 avant l'étape 1.
        // Étape 1 : prévue le 15/01, faite le 13/01 → 2 jours d'AVANCE.
        changer_statut_etape_saisie(&conn, &m.etapes[0].id, "termine", Some("u1"),
            &SaisieEtape { date_effective: Some("2026-01-13".into()), ..Default::default() }).unwrap();
        // Étape 2 : prévue le 18/01, faite le 26/01 → 8 jours de RETARD.
        changer_statut_etape_saisie(&conn, &m.etapes[1].id, "termine", Some("u1"),
            &SaisieEtape { date_effective: Some("2026-01-26".into()), ..Default::default() }).unwrap();

        let m2 = lire(&conn, &m.id).unwrap();
        assert_eq!(m2.etapes[0].ecart_jours, Some(-2), "avance = négatif");
        assert!(!m2.etapes[0].ecart_en_cours, "c'est un écart CONSTATÉ");
        assert_eq!(m2.etapes[1].ecart_jours, Some(8), "retard = positif");

        // Étape 3 : prévue le 2026-02-08, pas faite → le retard court jusqu'à
        // aujourd'hui. C'est le dossier qui traîne en ce moment même.
        let e3 = &m2.etapes[2];
        assert_eq!(e3.date_prevue.as_deref(), Some("2026-02-08"));
        assert_eq!(e3.ecart_jours, Some(jours_entre("2026-02-08", &aujourdhui())));
        assert!(e3.ecart_en_cours, "retard EN COURS, à distinguer d'un constat");

        // Une étape dont l'échéance n'est pas passée n'a rien à dire.
        let futur = m2.etapes.iter().find(|e| {
            e.date_prevue.as_deref().map(|d| d > aujourdhui().as_str()).unwrap_or(false)
        });
        if let Some(f) = futur {
            assert!(f.ecart_jours.is_none(), "pas d'écart avant l'échéance");
        }

        // Une étape ANNULÉE ne traîne pas : elle a été écartée sciemment.
        changer_statut_etape(&conn, &m2.etapes[2].id, "annule", None).unwrap();
        let m3 = lire(&conn, &m.id).unwrap();
        assert!(m3.etapes[2].ecart_jours.is_none(), "une étape annulée n'est pas en retard");
    }

    /// Le tableau de suivi : chaque marché dans la colonne de sa phase, et le
    /// goulot mesuré au TEMPS PASSÉ comparé à ce que la procédure prévoyait —
    /// pas au nombre de cartes.
    #[test]
    fn le_tableau_de_phases_situe_les_marches_et_designe_le_goulot() {
        let conn = db::open_in_memory().unwrap();

        // Trois marchés à des stades différents.
        let a = marche_travaux(&conn);                       // rien de franchi
        let b = marche_travaux(&conn);
        for i in 0..2 {                                      // dossier + publication
            let mm = lire(&conn, &b.id).unwrap();
            changer_statut_etape(&conn, &mm.etapes[i].id, "termine", Some("u1")).unwrap();
        }
        // C est allé jusqu'à l'ouverture des plis, mais il y a des MOIS : ce sont
        // les dates effectives qui font le temps passé, pas le moment de la saisie.
        let c = marche_travaux(&conn);
        for (i, d) in ["2026-01-10", "2026-01-20", "2026-02-05", "2026-02-15"].iter().enumerate() {
            let mm = lire(&conn, &c.id).unwrap();
            changer_statut_etape_saisie(&conn, &mm.etapes[i].id, "termine", Some("u1"),
                &SaisieEtape { date_effective: Some((*d).into()), ..Default::default() }).unwrap();
        }

        let cols = tableau_phases(&conn, &FiltreMarches::default()).unwrap();
        assert_eq!(cols.len(), PHASES.len(), "une colonne par phase, toujours");
        let col = |code: &str| cols.iter().find(|x| x.code == code).unwrap();

        // A n'a rien franchi : il est en préparation.
        assert_eq!(col("preparation").nb, 1);
        assert!(col("preparation").marches.iter().any(|x| x.id == a.id));
        // B a fini dossier + publication : il en est à la réception des offres.
        assert_eq!(col("consultation").nb, 1);
        assert!(col("consultation").marches.iter().any(|x| x.id == b.id));
        // C a ouvert les plis : il en est à l'évaluation.
        assert_eq!(col("evaluation").nb, 1);
        assert_eq!(col("evaluation").marches[0].etape_courante.as_deref(),
                   Some("Évaluation des offres"));
        // Personne n'est encore à l'attribution.
        assert_eq!(col("attribution").nb, 0);

        // La carte porte de quoi décider : montant, étape, ancienneté.
        let carte = &col("evaluation").marches[0];
        assert_eq!(carte.montant_courant, 25_000_000.0);
        // Procédure Travaux : ouverture des plis 1 j + évaluation 7 j = 8 j prévus.
        assert_eq!(carte.jours_prevus_phase, 8, "durée tirée des dates prévues");
        // ⚠️ On entre dans une phase quand la PRÉCÉDENTE s'achève (05/02), pas
        // à la première étape franchie dedans : le temps d'attente avant de
        // commencer fait partie du temps passé dans la phase. C'est justement
        // ce qu'on veut voir dans un goulot.
        let attendu = jours_entre("2026-02-05", &aujourdhui());
        assert_eq!(carte.jours_dans_phase, attendu, "temps réel depuis l'entrée en phase");
        assert!(carte.jours_dans_phase > 8, "il dépasse largement le prévu");

        // Le goulot se déduit de la comparaison prévu/réel, pas du nombre.
        assert!(col("evaluation").goulot, "on y reste plus longtemps que prévu");
        assert!(!col("attribution").goulot, "une colonne vide n'est jamais un goulot");

        // Un dossier CLASSÉ sort du tableau : on suit ce qui est en cours.
        // Le garder gonflerait les moyennes et ferait passer sa colonne pour un
        // goulot alors que plus personne n'y attend quoi que ce soit.
        annuler(&conn, &a.id, "Crédits redéployés", None).unwrap();
        let cols = tableau_phases(&conn, &FiltreMarches::default()).unwrap();
        assert_eq!(cols.iter().find(|x| x.code == "preparation").unwrap().nb, 0,
                   "un marché annulé disparaît du suivi");

        changer_statut(&conn, &b.id, "realise").unwrap();
        let cols = tableau_phases(&conn, &FiltreMarches::default()).unwrap();
        assert_eq!(cols.iter().find(|x| x.code == "consultation").unwrap().nb, 0,
                   "un marché réalisé aussi");
    }

    /// Une étape ajoutée à la main, sans phase, ne doit pas trouer le tableau :
    /// elle appartient à la phase en cours.
    #[test]
    fn une_etape_sans_phase_herite_de_la_precedente() {
        let conn = db::open_in_memory().unwrap();
        let m = marche_travaux(&conn);
        conn.execute(
            "INSERT INTO marche_etape (id, marche_id, libelle, ordre, statut, obligatoire, cree_le)
             VALUES ('ajout', ?1, 'Visite de site', 25, 'en_attente', 1, datetime('now'))",
            params![m.id],
        ).unwrap();
        let m2 = lire(&conn, &m.id).unwrap();
        let ajoutee = m2.etapes.iter().find(|e| e.id == "ajout").unwrap();
        assert!(ajoutee.phase.is_some(), "aucune étape ne reste sans phase");
        // Placée après la dernière étape (contractualisation), elle en hérite.
        assert_eq!(ajoutee.phase.as_deref(), Some("contractualisation"));
    }

    #[test]
    fn les_alertes_sont_visibles_depuis_la_liste() {
        let conn = db::open_in_memory().unwrap();
        let m = marche_travaux(&conn);   // lancé le 2026-01-05

        // Un avenant non approuvé : son montant ne compte pas encore, et ça se dit.
        ajouter_avenant(&conn, &m.id, &NouvelAvenant {
            objet: "Extension".into(), montant_variation: 2_000_000.0, delai_jours: 0,
            date_avenant: None, motif: None,
        }, None).unwrap();

        // Une étape datée AVANT le lancement du marché.
        modifier_etape(&conn, &m.etapes[0].id, &MajEtape {
            date_prevue: Some("2025-11-02".into()),
            libelle: None, date_effective: None, statut: None,
            observations: None, obligatoire: None, motif_derogation: None,
        }).unwrap();

        let liste = lister(&conn, &FiltreMarches::default()).unwrap();
        let l = liste.iter().find(|x| x.id == m.id).unwrap();

        assert!(!l.alertes.is_empty(), "la liste doit porter les alertes");
        assert!(l.alertes.iter().any(|a| a.contains("AVANT la date de lancement")),
                "{:?}", l.alertes);
        assert!(l.alertes.iter().any(|a| a.contains("attente d'approbation")),
                "{:?}", l.alertes);

        // Les mêmes alertes qu'en détail : une seule et même règle.
        let d = lire(&conn, &m.id).unwrap();
        assert_eq!(l.alertes, d.alertes,
                   "liste et détail ne doivent jamais se contredire");
    }

    /// La suppression groupée rend compte des DEUX cas : un compteur seul
    /// laisserait croire à un échec silencieux sur les marchés protégés.
    #[test]
    fn la_suppression_groupee_dit_ce_quelle_a_conserve() {
        let conn = db::open_in_memory().unwrap();

        let vierge = creer(&conn, &NouveauMarche {
            objet: "Marché sans suite".into(), type_id: None, montant_estime: 0.0,
            montant_attribue: None, monnaie: None, date_lancement: None,
            date_cloture_prevue: None, attributaire_id: None, projet_id: None,
            responsable_id: None, lieu_execution: None, observations: None,
            etapes: Vec::new(),
        }, None).unwrap();

        // Celui-ci a une histoire : une étape franchie.
        let engage = marche_travaux(&conn);
        changer_statut_etape(&conn, &engage.etapes[0].id, "termine", None).unwrap();

        let r = supprimer_lot(&conn, &[vierge.id.clone(), engage.id.clone()]).unwrap();
        assert_eq!(r.supprimes, 1);
        assert_eq!(r.conserves, 1);
        assert_eq!(r.numeros_conserves, vec![engage.numero.clone()],
                   "on doit pouvoir DIRE lequel a été conservé");

        // Le marché engagé est toujours là, intact.
        assert!(lire(&conn, &engage.id).is_ok());
        assert!(lire(&conn, &vierge.id).is_err());
    }

    #[test]
    fn depouillement_et_attribution() {
        let conn = db::open_in_memory().unwrap();
        let m = marche_travaux(&conn);
        conn.execute(
            "INSERT INTO tiers (id, code, type_role, nom, solde, actif, cree_le, nature)
             VALUES ('f1','F1','fournisseur','BTP Sénégal',0,1,'2026-01-01','entreprise')", [],
        ).unwrap();

        let a = ajouter_soumissionnaire(&conn, &m.id, &NouveauSoumissionnaire {
            tiers_id: Some("f1".into()), nom: "BTP Sénégal".into(),
            montant_offre: Some(23_000_000.0), delai_jours: Some(120),
            ninea: None, telephone: None, montant_offre_ttc: None,
            note_technique: Some(85.0), note_financiere: Some(90.0),
            statut: None, motif: None, observations: None, date_depot: None,
        }, None).unwrap();
        // Écart avec l'estimation : −8 %.
        assert_eq!(a.ecart_estime_pct, Some(-8.0));

        // Un soumissionnaire sans fiche tiers est parfaitement admis : recevoir
        // une offre ne doit pas obliger à créer un tiers.
        let b = ajouter_soumissionnaire(&conn, &m.id, &NouveauSoumissionnaire {
            tiers_id: None, nom: "Entreprise de passage".into(),
            montant_offre: Some(28_000_000.0), delai_jours: Some(90),
            ninea: None, telephone: None, montant_offre_ttc: None,
            note_technique: None, note_financiere: None,
            statut: None, motif: None, observations: None, date_depot: None,
        }, None).unwrap();

        let m = attribuer(&conn, &a.id).unwrap();
        assert_eq!(m.attributaire_id.as_deref(), Some("f1"));
        assert_eq!(m.montant_attribue, Some(23_000_000.0));
        assert_eq!(m.ecart_montant, Some(-2_000_000.0));
        assert_eq!(m.nb_soumissionnaires, 2);
        let retenu = m.soumissionnaires.iter().find(|s| s.id == a.id).unwrap();
        assert_eq!(retenu.statut, "retenu");
        let _ = b;
    }

    #[test]
    fn les_alertes_signalent_sans_bloquer() {
        let conn = db::open_in_memory().unwrap();
        let m = marche_travaux(&conn);
        // Un montant attribué supérieur à l'estimation : accepté, mais signalé.
        let m = modifier(&conn, &m.id, &NouveauMarche {
            objet: m.objet.clone(), type_id: m.type_id.clone(),
            montant_estime: 25_000_000.0, montant_attribue: Some(30_000_000.0),
            monnaie: None, date_lancement: None, date_cloture_prevue: None,
            attributaire_id: None, projet_id: None, responsable_id: None,
            lieu_execution: None, observations: None, etapes: Vec::new(),
        }).unwrap();
        assert!(m.alertes.iter().any(|a| a.contains("dépasse l'estimation")), "{:?}", m.alertes);
        assert!(m.alertes.iter().any(|a| a.contains("aucun attributaire")), "{:?}", m.alertes);

        // Déclarer « réalisé » avec des étapes obligatoires ouvertes : passe,
        // mais l'écran le dit.
        let m = changer_statut(&conn, &m.id, "realise").unwrap();
        assert_eq!(m.statut, "realise");
        assert!(m.date_cloture_effective.is_some());
        assert!(m.alertes.iter().any(|a| a.contains("obligatoire")), "{:?}", m.alertes);
    }

    #[test]
    fn annulation_exige_un_motif_et_supprimer_respecte_lhistorique() {
        let conn = db::open_in_memory().unwrap();
        let m = marche_travaux(&conn);
        assert!(changer_statut(&conn, &m.id, "annule").is_err(), "il faut passer par annuler");
        assert!(annuler(&conn, &m.id, "  ", None).is_err(), "motif vide refusé");

        // Un marché sans étape franchie se supprime.
        let vierge = creer(&conn, &NouveauMarche {
            objet: "Essai".into(), type_id: None, montant_estime: 0.0,
            montant_attribue: None, monnaie: None, date_lancement: None,
            date_cloture_prevue: None, attributaire_id: None, projet_id: None,
            responsable_id: None, lieu_execution: None, observations: None,
            etapes: Vec::new(),
        }, None).unwrap();
        assert!(supprimer(&conn, &vierge.id).is_ok());

        // Mais dès qu'une étape est franchie, il devient de l'historique.
        changer_statut_etape(&conn, &m.etapes[0].id, "termine", None).unwrap();
        assert!(supprimer(&conn, &m.id).is_err());
        let m = annuler(&conn, &m.id, "financement retiré", Some("u1")).unwrap();
        assert_eq!(m.statut, "annule");
        assert_eq!(m.motif_annulation.as_deref(), Some("financement retiré"));
    }

    #[test]
    fn corriger_une_procedure_ne_reecrit_pas_les_marches_lances() {
        let conn = db::open_in_memory().unwrap();
        let m = marche_travaux(&conn);
        let libelle_origine = m.etapes[0].libelle.clone();

        // Le type est réécrit de fond en comble.
        modifier_type(&conn, "mt-travaux", &NouveauType {
            code: "TRAVAUX".into(), libelle: "Travaux".into(), description: None, actif: true,
            etapes: vec![NouvelleEtapeModele {
                libelle: "Procédure entièrement revue".into(), description: None,
                duree_prevue_jours: 5, obligatoire: true,
            }],
        }).unwrap();

        // Le marché déjà lancé garde ses étapes et leurs libellés.
        let apres = lire(&conn, &m.id).unwrap();
        assert_eq!(apres.nb_etapes, 8);
        assert_eq!(apres.etapes[0].libelle, libelle_origine);
    }
}
