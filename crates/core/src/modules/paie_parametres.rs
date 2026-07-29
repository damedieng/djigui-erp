//! Paramètres légaux de la paie (migration 0044).
//!
//! # La règle du module
//!
//! **Aucun taux, plafond, tranche ou montant légal n'est écrit dans le code.**
//! Ce module lit et écrit les tables `ref_*` ; le moteur de calcul (à venir) les
//! consommera sans jamais connaître un chiffre. Demande de l'utilisateur :
//! « il faut prévoir des interfaces qui permettent de paramétrer cela, car la
//! législation peut changer ».
//!
//! # Versionnement par période — et pourquoi on ne modifie jamais un taux
//!
//! Chaque ligne porte `date_debut` / `date_fin`. Un bulletin de janvier
//! réimprimé en juin doit retrouver **les taux de janvier**. Si l'on écrasait
//! une valeur, tous les bulletins passés se recalculeraient faux — et un
//! bulletin faux se paie en redressement.
//!
//! ⚠️ D'où [`nouvelle_periode`] : elle **ferme** la période en cours la veille
//! de la nouvelle et en ouvre une autre. Il n'existe volontairement pas de
//! fonction « modifier un taux ». La seule correction possible est
//! [`corriger_periode_courante`], réservée à une valeur qui n'a **jamais
//! servi** — sinon on rouvrirait la porte qu'on vient de fermer.
//!
//! # `a_verifier` n'est pas cosmétique
//!
//! Les valeurs installées viennent de sources publiques qui **se contredisent**
//! (plafond IPRES 360 000 ou 432 000 ? abattement plafonné à 900 000 ou
//! 1 800 000 ? TRIMF mensuelle ou annuelle ?). Tant que le drapeau vaut `true`,
//! l'écran affiche un avertissement : l'utilisateur ne peut pas deviner que le
//! logiciel a repris un chiffre non certifié.

use crate::error::{CoreError, Result};
use crate::now;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CotisationSociale {
    #[serde(default)]
    pub id: String,
    pub organisme: String,
    pub libelle: String,
    pub taux_salarial: f64,
    pub taux_patronal: f64,
    pub plafond_mensuel: Option<f64>,
    pub reserve_cadre: bool,
    pub date_debut: String,
    pub date_fin: Option<String>,
    #[serde(default)]
    pub a_verifier: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrancheIrpp {
    #[serde(default)]
    pub id: String,
    /// ⚠️ Bornes **annuelles** : le moteur annualise le net imposable avant de
    /// les appliquer. Les traiter comme mensuelles diviserait l'impôt par 12.
    pub borne_inf: f64,
    pub borne_sup: Option<f64>,
    pub taux: f64,
    pub date_debut: String,
    pub date_fin: Option<String>,
    #[serde(default)]
    pub a_verifier: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReductionFamille {
    #[serde(default)]
    pub id: String,
    pub nb_parts: f64,
    pub taux_reduction: f64,
    /// Plancher et plafond **annuels** de la réduction.
    pub reduction_min: f64,
    pub reduction_max: f64,
    pub date_debut: String,
    pub date_fin: Option<String>,
    #[serde(default)]
    pub a_verifier: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrancheTrimf {
    #[serde(default)]
    pub id: String,
    pub borne_inf: f64,
    pub borne_sup: Option<f64>,
    pub montant: f64,
    /// `mensuel` ou `annuel`. ⚠️ Point de divergence connu entre sources.
    pub periodicite: String,
    pub date_debut: String,
    pub date_fin: Option<String>,
    #[serde(default)]
    pub a_verifier: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimeReglementaire {
    #[serde(default)]
    pub id: String,
    pub code: String,
    pub libelle: String,
    /// `exoneration` (prime exonérée jusqu'au plafond) ou `avantage`
    /// (avantage en nature évalué puis ajouté au brut).
    pub usage_prime: String,
    pub plafond_exoneration: Option<f64>,
    /// `forfait` (francs) ou `pourcentage` (% du brut).
    pub mode_evaluation: String,
    pub valeur: f64,
    pub date_debut: String,
    pub date_fin: Option<String>,
    #[serde(default)]
    pub a_verifier: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbattementFraisPro {
    #[serde(default)]
    pub id: String,
    pub taux: f64,
    pub plafond_annuel: Option<f64>,
    /// ⚠️ Point de doctrine **non tranché** : selon la lecture du CGI,
    /// l'abattement de 30 % et la déduction des cotisations IPRES peuvent ne pas
    /// se cumuler. Interrupteur, pas décision prise à la place de l'utilisateur.
    pub cumul_avec_ipres: bool,
    pub date_debut: String,
    pub date_fin: Option<String>,
    #[serde(default)]
    pub a_verifier: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParametresEmployeur {
    pub secteur_risque_at: Option<String>,
    pub taux_at_retenu: f64,
    pub numero_ipres: Option<String>,
    pub numero_css: Option<String>,
    pub ipm_nom: Option<String>,
    pub numero_ipm: Option<String>,
    pub jours_ouvrables_mois: i64,
    pub majoration_hs1: f64,
    pub majoration_hs2: f64,
    pub majoration_nuit: f64,
    pub majoration_ferie: f64,
    pub taux_cfce: f64,
}

/// Tout ce que l'écran affiche, en un seul aller-retour.
#[derive(Debug, Clone, Serialize)]
pub struct JeuParametres {
    pub cotisations: Vec<CotisationSociale>,
    pub bareme_irpp: Vec<TrancheIrpp>,
    pub reductions_famille: Vec<ReductionFamille>,
    pub trimf: Vec<TrancheTrimf>,
    pub primes: Vec<PrimeReglementaire>,
    pub abattement: Option<AbattementFraisPro>,
    pub employeur: ParametresEmployeur,
    /// Contrôles de cohérence, **non bloquants**.
    pub alertes: Vec<String>,
    /// Nombre de valeurs encore marquées « à vérifier ».
    pub nb_a_verifier: i64,
}

// ---------------------------------------------------------------------------
// Lecture
// ---------------------------------------------------------------------------

/// Clause de sélection d'une période : celle qui couvre `a_la_date`.
///
/// ⚠️ `a_la_date` est la **date du bulletin**, pas la date du jour. C'est toute
/// la raison d'être du versionnement : recalculer janvier en juin doit rendre
/// les taux de janvier.
const PERIODE: &str = "date_debut <= ?1 AND (date_fin IS NULL OR date_fin >= ?1)";

pub fn cotisations(conn: &Connection, a_la_date: &str) -> Result<Vec<CotisationSociale>> {
    let mut st = conn.prepare(&format!(
        "SELECT id, organisme, libelle, taux_salarial, taux_patronal, plafond_mensuel,
                reserve_cadre, date_debut, date_fin, a_verifier, note
           FROM ref_parametres_sociaux WHERE {PERIODE} ORDER BY organisme"
    ))?;
    let v = st
        .query_map(params![a_la_date], |r| {
            Ok(CotisationSociale {
                id: r.get(0)?,
                organisme: r.get(1)?,
                libelle: r.get(2)?,
                taux_salarial: r.get(3)?,
                taux_patronal: r.get(4)?,
                plafond_mensuel: r.get(5)?,
                reserve_cadre: r.get::<_, i64>(6)? != 0,
                date_debut: r.get(7)?,
                date_fin: r.get(8)?,
                a_verifier: r.get::<_, i64>(9)? != 0,
                note: r.get(10)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn bareme_irpp(conn: &Connection, a_la_date: &str) -> Result<Vec<TrancheIrpp>> {
    let mut st = conn.prepare(&format!(
        "SELECT id, borne_inf, borne_sup, taux, date_debut, date_fin, a_verifier
           FROM ref_bareme_irpp WHERE {PERIODE} ORDER BY borne_inf"
    ))?;
    let v = st
        .query_map(params![a_la_date], |r| {
            Ok(TrancheIrpp {
                id: r.get(0)?,
                borne_inf: r.get(1)?,
                borne_sup: r.get(2)?,
                taux: r.get(3)?,
                date_debut: r.get(4)?,
                date_fin: r.get(5)?,
                a_verifier: r.get::<_, i64>(6)? != 0,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn reductions_famille(conn: &Connection, a_la_date: &str) -> Result<Vec<ReductionFamille>> {
    let mut st = conn.prepare(&format!(
        "SELECT id, nb_parts, taux_reduction, reduction_min, reduction_max,
                date_debut, date_fin, a_verifier
           FROM ref_reductions_famille WHERE {PERIODE} ORDER BY nb_parts"
    ))?;
    let v = st
        .query_map(params![a_la_date], |r| {
            Ok(ReductionFamille {
                id: r.get(0)?,
                nb_parts: r.get(1)?,
                taux_reduction: r.get(2)?,
                reduction_min: r.get(3)?,
                reduction_max: r.get(4)?,
                date_debut: r.get(5)?,
                date_fin: r.get(6)?,
                a_verifier: r.get::<_, i64>(7)? != 0,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn trimf(conn: &Connection, a_la_date: &str) -> Result<Vec<TrancheTrimf>> {
    let mut st = conn.prepare(&format!(
        "SELECT id, borne_inf, borne_sup, montant, periodicite, date_debut, date_fin, a_verifier
           FROM ref_trimf WHERE {PERIODE} ORDER BY borne_inf"
    ))?;
    let v = st
        .query_map(params![a_la_date], |r| {
            Ok(TrancheTrimf {
                id: r.get(0)?,
                borne_inf: r.get(1)?,
                borne_sup: r.get(2)?,
                montant: r.get(3)?,
                periodicite: r.get(4)?,
                date_debut: r.get(5)?,
                date_fin: r.get(6)?,
                a_verifier: r.get::<_, i64>(7)? != 0,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn primes(conn: &Connection, a_la_date: &str) -> Result<Vec<PrimeReglementaire>> {
    let mut st = conn.prepare(&format!(
        "SELECT id, code, libelle, usage_prime, plafond_exoneration, mode_evaluation,
                valeur, date_debut, date_fin, a_verifier
           FROM ref_primes_reglementaires WHERE {PERIODE} ORDER BY usage_prime, libelle"
    ))?;
    let v = st
        .query_map(params![a_la_date], |r| {
            Ok(PrimeReglementaire {
                id: r.get(0)?,
                code: r.get(1)?,
                libelle: r.get(2)?,
                usage_prime: r.get(3)?,
                plafond_exoneration: r.get(4)?,
                mode_evaluation: r.get(5)?,
                valeur: r.get(6)?,
                date_debut: r.get(7)?,
                date_fin: r.get(8)?,
                a_verifier: r.get::<_, i64>(9)? != 0,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn abattement(conn: &Connection, a_la_date: &str) -> Result<Option<AbattementFraisPro>> {
    let mut st = conn.prepare(&format!(
        "SELECT id, taux, plafond_annuel, cumul_avec_ipres, date_debut, date_fin, a_verifier
           FROM ref_abattement_frais_pro WHERE {PERIODE} LIMIT 1"
    ))?;
    let mut it = st.query_map(params![a_la_date], |r| {
        Ok(AbattementFraisPro {
            id: r.get(0)?,
            taux: r.get(1)?,
            plafond_annuel: r.get(2)?,
            cumul_avec_ipres: r.get::<_, i64>(3)? != 0,
            date_debut: r.get(4)?,
            date_fin: r.get(5)?,
            a_verifier: r.get::<_, i64>(6)? != 0,
        })
    })?;
    Ok(it.next().transpose()?)
}

pub fn employeur(conn: &Connection) -> Result<ParametresEmployeur> {
    let p = conn.query_row(
        "SELECT secteur_risque_at, taux_at_retenu, numero_ipres, numero_css, ipm_nom,
                numero_ipm, jours_ouvrables_mois, majoration_hs1, majoration_hs2,
                majoration_nuit, majoration_ferie, taux_cfce
           FROM parametres_entreprise_paie WHERE singleton = 1",
        [],
        |r| {
            Ok(ParametresEmployeur {
                secteur_risque_at: r.get(0)?,
                taux_at_retenu: r.get(1)?,
                numero_ipres: r.get(2)?,
                numero_css: r.get(3)?,
                ipm_nom: r.get(4)?,
                numero_ipm: r.get(5)?,
                jours_ouvrables_mois: r.get(6)?,
                majoration_hs1: r.get(7)?,
                majoration_hs2: r.get(8)?,
                majoration_nuit: r.get(9)?,
                majoration_ferie: r.get(10)?,
                taux_cfce: r.get(11)?,
            })
        },
    )?;
    Ok(p)
}

/// Tout le jeu de paramètres applicable à une date, avec ses alertes.
pub fn jeu_complet(conn: &Connection, a_la_date: &str) -> Result<JeuParametres> {
    let cotisations = cotisations(conn, a_la_date)?;
    let bareme_irpp = bareme_irpp(conn, a_la_date)?;
    let reductions_famille = reductions_famille(conn, a_la_date)?;
    let trimf = trimf(conn, a_la_date)?;
    let primes = primes(conn, a_la_date)?;
    let abattement = abattement(conn, a_la_date)?;
    let employeur = employeur(conn)?;

    let nb_a_verifier = cotisations.iter().filter(|x| x.a_verifier).count()
        + bareme_irpp.iter().filter(|x| x.a_verifier).count()
        + reductions_famille.iter().filter(|x| x.a_verifier).count()
        + trimf.iter().filter(|x| x.a_verifier).count()
        + primes.iter().filter(|x| x.a_verifier).count()
        + abattement.as_ref().map(|a| a.a_verifier as usize).unwrap_or(0);

    let alertes = alertes(
        &cotisations,
        &bareme_irpp,
        &reductions_famille,
        &trimf,
        &abattement,
        &employeur,
    );

    Ok(JeuParametres {
        cotisations,
        bareme_irpp,
        reductions_famille,
        trimf,
        primes,
        abattement,
        employeur,
        alertes,
        nb_a_verifier: nb_a_verifier as i64,
    })
}

// ---------------------------------------------------------------------------
// Alertes de cohérence — **non bloquantes**
// ---------------------------------------------------------------------------
//
// On signale, on n'interdit pas : c'est le standard des modules Djigui. Un
// paramétrage transitoire (une tranche en cours de saisie) doit rester
// possible ; mais un barème troué produirait un impôt faux en silence, et
// personne ne s'en apercevrait avant le contrôle.

fn alertes(
    cotisations: &[CotisationSociale],
    irpp: &[TrancheIrpp],
    famille: &[ReductionFamille],
    trimf: &[TrancheTrimf],
    abattement: &Option<AbattementFraisPro>,
    employeur: &ParametresEmployeur,
) -> Vec<String> {
    let mut a = Vec::new();

    if irpp.is_empty() {
        a.push("Aucune tranche d'impôt n'est définie pour cette période : aucun bulletin ne \
                pourra être calculé.".into());
    } else {
        // Un trou ou un chevauchement fausse l'impôt sans que rien ne le dise.
        for paire in irpp.windows(2) {
            let (bas, haut) = (&paire[0], &paire[1]);
            match bas.borne_sup {
                None => a.push(format!(
                    "La tranche d'impôt à {} % n'a pas de plafond alors qu'une tranche la suit : \
                     tout le revenu au-dessus de {} lui sera imputé.",
                    bas.taux, bas.borne_inf
                )),
                Some(sup) if (haut.borne_inf - sup - 1.0).abs() > 1.0 => a.push(format!(
                    "Trou ou chevauchement dans le barème d'impôt entre {sup} et {} : \
                     les revenus de cette zone seraient mal imposés.",
                    haut.borne_inf
                )),
                _ => {}
            }
        }
        if irpp.last().and_then(|t| t.borne_sup).is_some() {
            a.push("La dernière tranche d'impôt a un plafond : les revenus au-delà ne seraient \
                    pas imposés du tout. La dernière tranche doit être sans limite.".into());
        }
        if irpp.first().map(|t| t.borne_inf > 0.0).unwrap_or(false) {
            a.push("Le barème d'impôt ne commence pas à 0 : les premiers francs de revenu ne \
                    seraient pas couverts.".into());
        }
    }

    for o in ["ipres_rg", "css_pf", "ipm"] {
        if !cotisations.iter().any(|c| c.organisme == o) {
            a.push(format!(
                "Aucun taux « {o} » n'est défini pour cette période : cette cotisation sera \
                 absente de tous les bulletins."
            ));
        }
    }
    // Le RCC ne se calcule que sur la fraction AU-DESSUS du plafond général :
    // si son plafond est plus bas, la fraction est vide et la cotisation nulle.
    let plafond = |code: &str| {
        cotisations
            .iter()
            .find(|c| c.organisme == code)
            .and_then(|c| c.plafond_mensuel)
    };
    if let (Some(rg), Some(rcc)) = (plafond("ipres_rg"), plafond("ipres_rcc")) {
        if rcc <= rg {
            a.push(format!(
                "Le plafond de la retraite complémentaire des cadres ({rcc}) n'est pas supérieur \
                 à celui du régime général ({rg}) : aucun cadre ne cotiserait au complémentaire."
            ));
        }
    }

    if employeur.taux_at_retenu <= 0.0 {
        a.push("Le taux « accident du travail » de votre entreprise n'est pas renseigné. Il \
                dépend de votre secteur de risque (1 % à 5 %) et figure sur votre notification \
                CSS — voir l'onglet « Mon entreprise ».".into());
    }
    if employeur.jours_ouvrables_mois <= 0 {
        a.push("Le nombre de jours ouvrables du mois doit être supérieur à zéro : sans lui, \
                aucune absence ne peut être proratisée.".into());
    }

    if famille.is_empty() {
        a.push("Aucune réduction pour charge de famille n'est définie : les salariés mariés ou \
                avec enfants paieraient le même impôt qu'un célibataire.".into());
    } else {
        for f in famille.iter().filter(|f| f.reduction_max > 0.0 && f.reduction_min > f.reduction_max) {
            a.push(format!(
                "Réduction famille à {} part(s) : le plancher ({}) dépasse le plafond ({}).",
                f.nb_parts, f.reduction_min, f.reduction_max
            ));
        }
    }

    if trimf.is_empty() {
        a.push("Aucun barème TRIMF n'est défini pour cette période.".into());
    }
    if abattement.is_none() {
        a.push("Aucun abattement pour frais professionnels n'est défini : le revenu imposable \
                serait calculé sans abattement.".into());
    }

    a
}

// ---------------------------------------------------------------------------
// Écriture — toujours par NOUVELLE PÉRIODE
// ---------------------------------------------------------------------------

/// Ce que l'écran envoie pour ouvrir une nouvelle période.
#[derive(Debug, Clone, Deserialize)]
pub struct NouvellePeriode {
    /// `cotisations` | `bareme_irpp` | `reductions_famille` | `trimf` |
    /// `primes` | `abattement`.
    pub table: String,
    /// Date de prise d'effet des nouvelles valeurs.
    pub date_debut: String,
    /// Les lignes complètes du nouveau jeu. Elles **remplacent** l'ancien à
    /// partir de `date_debut` ; l'ancien reste consultable pour les bulletins
    /// déjà émis.
    pub lignes: serde_json::Value,
}

/// Ouvre une nouvelle période et **ferme la précédente la veille**.
///
/// ⚠️ C'est le seul geste d'écriture normal du module. Il n'existe pas de
/// « modifier un taux » : écraser une valeur ferait recalculer faux tous les
/// bulletins passés.
pub fn nouvelle_periode(conn: &Connection, n: &NouvellePeriode) -> Result<usize> {
    let table = nom_table(&n.table)?;
    let veille = veille_de(&n.date_debut)?;

    if n.date_debut.len() != 10 {
        return Err(CoreError::Rule(
            "La date de prise d'effet doit être une date complète (AAAA-MM-JJ).".into(),
        ));
    }
    let lignes = n.lignes.as_array().ok_or_else(|| {
        CoreError::Rule("Aucune valeur fournie pour la nouvelle période.".into())
    })?;
    if lignes.is_empty() {
        return Err(CoreError::Rule(
            "Une nouvelle période sans aucune valeur laisserait la paie sans règle : \
             ajoutez au moins une ligne."
                .into(),
        ));
    }

    // On ferme les périodes ouvertes qui débutent AVANT la nouvelle. Celles qui
    // commencent le même jour ou après sont remplacées : elles n'ont pas encore
    // servi à un bulletin puisque la nouvelle prend leur place.
    conn.execute(
        &format!(
            "UPDATE {table} SET date_fin = ?1 WHERE date_fin IS NULL AND date_debut < ?2"
        ),
        params![veille, n.date_debut],
    )?;
    conn.execute(
        &format!("DELETE FROM {table} WHERE date_debut >= ?1"),
        params![n.date_debut],
    )?;

    let mut posees = 0;
    for ligne in lignes {
        inserer(conn, &n.table, &n.date_debut, ligne)?;
        posees += 1;
    }
    Ok(posees)
}

/// Corrige les valeurs de la période **en cours** au lieu d'en ouvrir une.
///
/// ⚠️ Réservé au cas où la période n'a **jamais servi** à un bulletin : c'est
/// exactement la situation des valeurs installées d'origine, qui sont
/// indicatives et que l'utilisateur doit pouvoir corriger sans se retrouver
/// avec deux périodes dont la première n'a jamais rien produit.
/// L'appelant (couche API) vérifie qu'aucun bulletin n'existe.
pub fn corriger_periode_courante(
    conn: &Connection,
    table: &str,
    date_debut: &str,
    lignes: &serde_json::Value,
) -> Result<usize> {
    let nom = nom_table(table)?;
    let lignes = lignes
        .as_array()
        .ok_or_else(|| CoreError::Rule("Aucune valeur fournie.".into()))?;
    conn.execute(
        &format!("DELETE FROM {nom} WHERE date_debut = ?1"),
        params![date_debut],
    )?;
    for l in lignes {
        inserer(conn, table, date_debut, l)?;
    }
    Ok(lignes.len())
}

/// Retire le drapeau « à vérifier » : l'utilisateur confirme avoir confronté
/// les valeurs au texte en vigueur. Geste explicite, jamais automatique.
pub fn marquer_verifie(conn: &Connection, table: &str, date_debut: &str) -> Result<usize> {
    let nom = nom_table(table)?;
    let n = conn.execute(
        &format!("UPDATE {nom} SET a_verifier = 0 WHERE date_debut = ?1"),
        params![date_debut],
    )?;
    Ok(n)
}

#[derive(Debug, Clone, Deserialize)]
pub struct MajEmployeur {
    pub secteur_risque_at: Option<String>,
    pub taux_at_retenu: f64,
    pub numero_ipres: Option<String>,
    pub numero_css: Option<String>,
    pub ipm_nom: Option<String>,
    pub numero_ipm: Option<String>,
    pub jours_ouvrables_mois: i64,
    pub majoration_hs1: f64,
    pub majoration_hs2: f64,
    pub majoration_nuit: f64,
    pub majoration_ferie: f64,
    pub taux_cfce: f64,
}

pub fn enregistrer_employeur(conn: &Connection, m: &MajEmployeur) -> Result<ParametresEmployeur> {
    if m.jours_ouvrables_mois <= 0 || m.jours_ouvrables_mois > 31 {
        return Err(CoreError::Rule(
            "Le nombre de jours ouvrables du mois doit être compris entre 1 et 31.".into(),
        ));
    }
    if m.taux_at_retenu < 0.0 || m.taux_at_retenu > 100.0 {
        return Err(CoreError::Rule(
            "Le taux d'accident du travail doit être un pourcentage entre 0 et 100.".into(),
        ));
    }
    conn.execute(
        "UPDATE parametres_entreprise_paie
            SET secteur_risque_at = ?1, taux_at_retenu = ?2, numero_ipres = ?3,
                numero_css = ?4, ipm_nom = ?5, numero_ipm = ?6, jours_ouvrables_mois = ?7,
                majoration_hs1 = ?8, majoration_hs2 = ?9, majoration_nuit = ?10,
                majoration_ferie = ?11, taux_cfce = ?12, maj_le = ?13
          WHERE singleton = 1",
        params![
            m.secteur_risque_at, m.taux_at_retenu, m.numero_ipres, m.numero_css,
            m.ipm_nom, m.numero_ipm, m.jours_ouvrables_mois, m.majoration_hs1,
            m.majoration_hs2, m.majoration_nuit, m.majoration_ferie, m.taux_cfce, now()
        ],
    )?;
    employeur(conn)
}

// ---------------------------------------------------------------------------
// Outils internes
// ---------------------------------------------------------------------------

/// Traduit un nom logique en nom de table réel.
///
/// ⚠️ Cette liste blanche n'est pas une formalité : le nom vient de la requête
/// HTTP et sert à construire du SQL. Le concaténer directement ouvrirait une
/// injection.
fn nom_table(logique: &str) -> Result<&'static str> {
    Ok(match logique {
        "cotisations" => "ref_parametres_sociaux",
        "bareme_irpp" => "ref_bareme_irpp",
        "reductions_famille" => "ref_reductions_famille",
        "trimf" => "ref_trimf",
        "primes" => "ref_primes_reglementaires",
        "abattement" => "ref_abattement_frais_pro",
        autre => {
            return Err(CoreError::Rule(format!(
                "Jeu de paramètres inconnu : {autre}"
            )))
        }
    })
}

fn veille_de(date: &str) -> Result<String> {
    let d = chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map_err(|_| CoreError::Rule(format!("Date invalide : {date}")))?;
    Ok((d - chrono::Duration::days(1)).format("%Y-%m-%d").to_string())
}

fn nombre(v: &serde_json::Value, cle: &str) -> f64 {
    v.get(cle).and_then(|x| x.as_f64()).unwrap_or(0.0)
}
fn nombre_opt(v: &serde_json::Value, cle: &str) -> Option<f64> {
    v.get(cle).and_then(|x| x.as_f64())
}
fn texte(v: &serde_json::Value, cle: &str) -> String {
    v.get(cle).and_then(|x| x.as_str()).unwrap_or("").to_string()
}
fn texte_opt(v: &serde_json::Value, cle: &str) -> Option<String> {
    v.get(cle)
        .and_then(|x| x.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
}
fn booleen(v: &serde_json::Value, cle: &str) -> i64 {
    v.get(cle).and_then(|x| x.as_bool()).unwrap_or(false) as i64
}

fn inserer(
    conn: &Connection,
    table: &str,
    date_debut: &str,
    l: &serde_json::Value,
) -> Result<()> {
    let id = Uuid::new_v4().to_string();
    // Une valeur saisie par l'utilisateur est réputée VÉRIFIÉE : c'est lui qui
    // vient de la confronter au texte. Seules les valeurs installées d'origine
    // portent le drapeau.
    match table {
        "cotisations" => {
            let org = texte(l, "organisme");
            if !["ipres_rg", "ipres_rcc", "css_pf", "css_at", "ipm"].contains(&org.as_str()) {
                return Err(CoreError::Rule(format!("Organisme inconnu : {org}")));
            }
            conn.execute(
                "INSERT INTO ref_parametres_sociaux
                   (id, organisme, libelle, taux_salarial, taux_patronal, plafond_mensuel,
                    reserve_cadre, date_debut, a_verifier, note)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,0,?9)",
                params![
                    id, org, texte(l, "libelle"), nombre(l, "taux_salarial"),
                    nombre(l, "taux_patronal"), nombre_opt(l, "plafond_mensuel"),
                    booleen(l, "reserve_cadre"), date_debut, texte_opt(l, "note")
                ],
            )?;
        }
        "bareme_irpp" => {
            conn.execute(
                "INSERT INTO ref_bareme_irpp
                   (id, borne_inf, borne_sup, taux, date_debut, a_verifier)
                 VALUES (?1,?2,?3,?4,?5,0)",
                params![id, nombre(l, "borne_inf"), nombre_opt(l, "borne_sup"),
                        nombre(l, "taux"), date_debut],
            )?;
        }
        "reductions_famille" => {
            conn.execute(
                "INSERT INTO ref_reductions_famille
                   (id, nb_parts, taux_reduction, reduction_min, reduction_max, date_debut, a_verifier)
                 VALUES (?1,?2,?3,?4,?5,?6,0)",
                params![id, nombre(l, "nb_parts"), nombre(l, "taux_reduction"),
                        nombre(l, "reduction_min"), nombre(l, "reduction_max"), date_debut],
            )?;
        }
        "trimf" => {
            let p = texte(l, "periodicite");
            let p = if p == "mensuel" { "mensuel" } else { "annuel" };
            conn.execute(
                "INSERT INTO ref_trimf
                   (id, borne_inf, borne_sup, montant, periodicite, date_debut, a_verifier)
                 VALUES (?1,?2,?3,?4,?5,?6,0)",
                params![id, nombre(l, "borne_inf"), nombre_opt(l, "borne_sup"),
                        nombre(l, "montant"), p, date_debut],
            )?;
        }
        "primes" => {
            let usage = texte(l, "usage_prime");
            let usage = if usage == "avantage" { "avantage" } else { "exoneration" };
            let mode = texte(l, "mode_evaluation");
            let mode = if mode == "pourcentage" { "pourcentage" } else { "forfait" };
            conn.execute(
                "INSERT INTO ref_primes_reglementaires
                   (id, code, libelle, usage_prime, plafond_exoneration, mode_evaluation,
                    valeur, date_debut, a_verifier)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,0)",
                params![id, texte(l, "code"), texte(l, "libelle"), usage,
                        nombre_opt(l, "plafond_exoneration"), mode, nombre(l, "valeur"), date_debut],
            )?;
        }
        "abattement" => {
            conn.execute(
                "INSERT INTO ref_abattement_frais_pro
                   (id, taux, plafond_annuel, cumul_avec_ipres, date_debut, a_verifier)
                 VALUES (?1,?2,?3,?4,?5,0)",
                params![id, nombre(l, "taux"), nombre_opt(l, "plafond_annuel"),
                        booleen(l, "cumul_avec_ipres"), date_debut],
            )?;
        }
        autre => return Err(CoreError::Rule(format!("Jeu de paramètres inconnu : {autre}"))),
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use serde_json::json;

    #[test]
    fn les_valeurs_installees_sont_toutes_marquees_a_verifier() {
        let conn = db::open_in_memory().unwrap();
        let j = jeu_complet(&conn, "2026-07-29").unwrap();
        // ⚠️ Ce n'est pas cosmétique : les sources publiques se contredisent, et
        // l'utilisateur ne peut pas deviner qu'un chiffre n'est pas certifié.
        assert!(j.nb_a_verifier > 0, "l'écran doit pouvoir avertir");
        assert!(j.bareme_irpp.iter().all(|t| t.a_verifier));
        assert!(j.cotisations.iter().all(|c| c.a_verifier));
    }

    #[test]
    fn le_jeu_installe_est_coherent() {
        let conn = db::open_in_memory().unwrap();
        let j = jeu_complet(&conn, "2026-07-29").unwrap();
        assert_eq!(j.bareme_irpp.len(), 6);
        assert_eq!(j.reductions_famille.len(), 9);
        assert_eq!(j.cotisations.len(), 5);
        assert!(j.abattement.is_some());
        // Seule alerte attendue : le taux « accident du travail », qui dépend
        // du secteur de risque de CHAQUE entreprise et ne peut pas être seedé.
        assert_eq!(j.alertes.len(), 1, "alertes inattendues : {:?}", j.alertes);
        assert!(j.alertes[0].contains("accident du travail"));
    }

    /// ⚠️⚠️ LE test du module : un bulletin passé doit retrouver SES taux.
    #[test]
    fn une_nouvelle_periode_ne_reecrit_pas_le_passe() {
        let conn = db::open_in_memory().unwrap();
        // Barème d'origine : 0 % jusqu'à 630 000.
        let avant = bareme_irpp(&conn, "2026-03-15").unwrap();
        assert_eq!(avant[0].taux, 0.0);
        assert_eq!(avant.len(), 6);

        // Loi de finances : nouveau barème au 1er juillet.
        nouvelle_periode(&conn, &NouvellePeriode {
            table: "bareme_irpp".into(),
            date_debut: "2026-07-01".into(),
            lignes: json!([
                { "borne_inf": 0, "borne_sup": 800000, "taux": 0 },
                { "borne_inf": 800001, "borne_sup": null, "taux": 25 },
            ]),
        }).unwrap();

        // Mars doit rendre l'ANCIEN barème, inchangé.
        let mars = bareme_irpp(&conn, "2026-03-15").unwrap();
        assert_eq!(mars.len(), 6, "le barème de mars ne doit pas avoir bougé");
        assert_eq!(mars[1].taux, 20.0);
        assert_eq!(mars[0].date_fin.as_deref(), Some("2026-06-30"),
                   "l'ancienne période se ferme la VEILLE de la nouvelle");

        // Juillet rend le nouveau.
        let juillet = bareme_irpp(&conn, "2026-07-15").unwrap();
        assert_eq!(juillet.len(), 2);
        assert_eq!(juillet[1].taux, 25.0);

        // Le 30 juin appartient encore à l'ancien : pas de trou d'un jour.
        assert_eq!(bareme_irpp(&conn, "2026-06-30").unwrap().len(), 6);
    }

    /// Une valeur SAISIE par l'utilisateur est réputée vérifiée : c'est lui qui
    /// vient de la confronter au texte en vigueur.
    #[test]
    fn une_valeur_saisie_n_est_plus_a_verifier() {
        let conn = db::open_in_memory().unwrap();
        nouvelle_periode(&conn, &NouvellePeriode {
            table: "abattement".into(),
            date_debut: "2026-08-01".into(),
            lignes: json!([{ "taux": 30, "plafond_annuel": 1800000, "cumul_avec_ipres": false }]),
        }).unwrap();
        let a = abattement(&conn, "2026-08-15").unwrap().unwrap();
        assert!(!a.a_verifier);
        assert_eq!(a.plafond_annuel, Some(1_800_000.0));
        // L'interrupteur de doctrine est bien respecté.
        assert!(!a.cumul_avec_ipres);
    }

    #[test]
    fn un_bareme_troue_est_signale_sans_bloquer() {
        let conn = db::open_in_memory().unwrap();
        // Trou volontaire entre 500 000 et 900 000.
        nouvelle_periode(&conn, &NouvellePeriode {
            table: "bareme_irpp".into(),
            date_debut: "2026-08-01".into(),
            lignes: json!([
                { "borne_inf": 0, "borne_sup": 500000, "taux": 0 },
                { "borne_inf": 900000, "borne_sup": null, "taux": 25 },
            ]),
        }).unwrap();
        let j = jeu_complet(&conn, "2026-08-15").unwrap();
        assert!(j.alertes.iter().any(|a| a.contains("Trou ou chevauchement")),
                "le trou doit être signalé : {:?}", j.alertes);
    }

    #[test]
    fn une_derniere_tranche_plafonnee_est_signalee() {
        let conn = db::open_in_memory().unwrap();
        nouvelle_periode(&conn, &NouvellePeriode {
            table: "bareme_irpp".into(),
            date_debut: "2026-08-01".into(),
            lignes: json!([{ "borne_inf": 0, "borne_sup": 500000, "taux": 10 }]),
        }).unwrap();
        let j = jeu_complet(&conn, "2026-08-15").unwrap();
        assert!(j.alertes.iter().any(|a| a.contains("ne seraient pas imposés")));
    }

    /// Le complémentaire cadres ne porte que sur la fraction AU-DESSUS du
    /// plafond général : un plafond RCC plus bas rendrait la cotisation nulle.
    #[test]
    fn un_plafond_cadre_incoherent_est_signale() {
        let conn = db::open_in_memory().unwrap();
        nouvelle_periode(&conn, &NouvellePeriode {
            table: "cotisations".into(),
            date_debut: "2026-08-01".into(),
            lignes: json!([
                { "organisme": "ipres_rg", "libelle": "RG", "taux_salarial": 5.6,
                  "taux_patronal": 8.4, "plafond_mensuel": 432000 },
                { "organisme": "ipres_rcc", "libelle": "RCC", "taux_salarial": 2.4,
                  "taux_patronal": 3.6, "plafond_mensuel": 300000, "reserve_cadre": true },
                { "organisme": "css_pf", "libelle": "PF", "taux_salarial": 0, "taux_patronal": 7 },
                { "organisme": "ipm", "libelle": "IPM", "taux_salarial": 3, "taux_patronal": 3 },
            ]),
        }).unwrap();
        let j = jeu_complet(&conn, "2026-08-15").unwrap();
        assert!(j.alertes.iter().any(|a| a.contains("aucun cadre ne cotiserait")),
                "{:?}", j.alertes);
    }

    /// Une nouvelle période vide laisserait la paie sans aucune règle.
    #[test]
    fn une_periode_sans_valeur_est_refusee() {
        let conn = db::open_in_memory().unwrap();
        let e = nouvelle_periode(&conn, &NouvellePeriode {
            table: "bareme_irpp".into(),
            date_debut: "2026-08-01".into(),
            lignes: json!([]),
        }).unwrap_err();
        assert!(e.to_string().contains("au moins une ligne"));
    }

    /// ⚠️ Le nom de table vient d'une requête HTTP et sert à construire du SQL :
    /// la liste blanche est la barrière contre l'injection.
    #[test]
    fn un_nom_de_table_invente_est_refuse() {
        let conn = db::open_in_memory().unwrap();
        let e = nouvelle_periode(&conn, &NouvellePeriode {
            table: "tiers WHERE 1=1; DROP TABLE tiers; --".into(),
            date_debut: "2026-08-01".into(),
            lignes: json!([{ "taux": 1 }]),
        }).unwrap_err();
        assert!(e.to_string().contains("inconnu"));
        // La table visée est toujours là.
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM tiers", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0, "la table tiers doit exister (et être vide), pas avoir disparu");
    }

    #[test]
    fn le_taux_accident_du_travail_est_borne() {
        let conn = db::open_in_memory().unwrap();
        let base = MajEmployeur {
            secteur_risque_at: Some("Commerce".into()), taux_at_retenu: 3.0,
            numero_ipres: None, numero_css: None, ipm_nom: None, numero_ipm: None,
            jours_ouvrables_mois: 26, majoration_hs1: 15.0, majoration_hs2: 40.0,
            majoration_nuit: 60.0, majoration_ferie: 100.0, taux_cfce: 3.0,
        };
        let p = enregistrer_employeur(&conn, &base).unwrap();
        assert_eq!(p.taux_at_retenu, 3.0);
        // Renseigner le taux fait disparaître l'alerte correspondante.
        let j = jeu_complet(&conn, "2026-07-29").unwrap();
        assert!(!j.alertes.iter().any(|a| a.contains("accident du travail")));

        let mauvais = MajEmployeur { jours_ouvrables_mois: 0, ..base };
        assert!(enregistrer_employeur(&conn, &mauvais).is_err());
    }

    #[test]
    fn marquer_verifie_retire_l_avertissement() {
        let conn = db::open_in_memory().unwrap();
        assert!(jeu_complet(&conn, "2026-07-29").unwrap().nb_a_verifier > 0);
        for t in ["cotisations", "bareme_irpp", "reductions_famille", "trimf",
                  "primes", "abattement"] {
            marquer_verifie(&conn, t, "2026-01-01").unwrap();
        }
        assert_eq!(jeu_complet(&conn, "2026-07-29").unwrap().nb_a_verifier, 0);
    }
}
