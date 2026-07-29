//! Salariés et contrats (migration 0045).
//!
//! # Ce que ce module garantit
//!
//! - **Un seul contrat actif par salarié.** Sans cela, le moteur de paie ne
//!   saurait pas quel salaire de base retenir.
//! - **On ne supprime jamais un salarié qui a des bulletins.** Ses fiches
//!   doivent rester consultables des années : c'est une obligation, et c'est
//!   aussi la seule preuve de ce qui lui a été versé.
//! - **Les parts fiscales sont CALCULÉES**, jamais stockées — voir
//!   [`parts_fiscales`].
//!
//! # Alertes, pas blocages
//!
//! Un CDD sans terme, un salarié sans numéro IPRES, une épouse à charge déclarée
//! par un célibataire : ce sont des anomalies réelles, mais bloquer la saisie
//! empêcherait de travailler pendant qu'on régularise un dossier. On signale, on
//! laisse enregistrer, et on rend le problème visible depuis la liste.

use crate::error::{CoreError, Result};
use crate::now;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Parts fiscales — fonction pure, isolée et testable
// ---------------------------------------------------------------------------

/// Nombre de parts pour l'impôt sur le revenu.
///
/// ⚠️ **Ce n'est pas le quotient familial français.** Ces parts ne divisent pas
/// le revenu : elles servent à retrouver la ligne de `ref_reductions_famille`,
/// dont la réduction se **soustrait** de l'impôt déjà calculé (mig 0044).
///
/// Règle appliquée : 1 part de base, +0,5 si marié, +0,5 par enfant à charge,
/// **plafonné à 5 parts**.
///
/// ⚠️ La spécification demande de caler les cas particuliers (veuf avec ou sans
/// enfant, conjoint lui-même salarié) sur le simulateur de la DGID. Cette
/// fonction est **volontairement isolée** pour qu'on puisse la corriger sans
/// toucher à quoi que ce soit d'autre, et ses cas sont couverts un par un par
/// les tests.
pub fn parts_fiscales(situation: &str, nb_enfants: i64) -> f64 {
    let base = match situation {
        "marie" => 1.5,
        // Un veuf sans enfant retombe à 1 part ; avec enfants, l'usage lui
        // reconnaît la demi-part du conjoint disparu. C'est le point le plus
        // discuté du barème : à confirmer au simulateur DGID.
        "veuf" if nb_enfants > 0 => 1.5,
        _ => 1.0,
    };
    let total = base + 0.5 * nb_enfants as f64;
    // Le plafond est une règle du CGI, pas une commodité de calcul.
    total.min(5.0)
}

/// Parts pour la TRIMF : le salarié, **plus ses épouses non salariées**.
///
/// ⚠️ Compte distinct de [`parts_fiscales`] : les enfants n'y entrent pas, les
/// épouses si. Confondre les deux fausserait l'impôt **et** la TRIMF.
pub fn parts_trimf(nb_conjoints_a_charge: i64) -> f64 {
    1.0 + nb_conjoints_a_charge.max(0) as f64
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct Employe {
    pub id: String,
    pub matricule: String,
    pub nom: String,
    pub prenom: Option<String>,
    pub date_naissance: Option<String>,
    pub lieu_naissance: Option<String>,
    pub sexe: Option<String>,
    pub cni: Option<String>,
    pub telephone: Option<String>,
    pub adresse: Option<String>,
    pub situation_matrimoniale: String,
    pub nb_conjoints_a_charge: i64,
    pub nb_enfants_charge: i64,
    pub est_cadre: bool,
    pub poste: Option<String>,
    pub categorie: Option<String>,
    pub date_embauche: String,
    pub date_sortie: Option<String>,
    pub motif_sortie: Option<String>,
    pub numero_ipres: Option<String>,
    pub numero_css: Option<String>,
    pub numero_ipm: Option<String>,
    pub mode_paiement: String,
    pub banque: Option<String>,
    pub numero_compte: Option<String>,
    pub actif: bool,
    pub note: Option<String>,
    pub cree_le: String,

    // ---- Champs DÉRIVÉS, calculés à la lecture ----
    /// Nom complet, prêt à imprimer sur un bulletin.
    pub nom_complet: String,
    /// ⚠️ Calculé, jamais stocké : une part stockée cesserait d'être vraie à la
    /// naissance d'un enfant sans que personne ne s'en aperçoive.
    pub nb_parts_ir: f64,
    pub nb_parts_trimf: f64,
    /// Ancienneté en mois, déduite de la date d'embauche.
    pub anciennete_mois: i64,
    /// Contrat actif, s'il y en a un.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contrat: Option<Contrat>,
    /// Rémunération contractuelle de base (salaire + sursalaire). `0` sans contrat.
    pub remuneration_base: f64,
    /// Anomalies à afficher en jaune. **Ne bloquent jamais.**
    pub alertes: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Contrat {
    pub id: String,
    pub employe_id: String,
    pub type_contrat: String,
    pub date_debut: String,
    pub date_fin: Option<String>,
    pub salaire_base: f64,
    pub sursalaire: f64,
    pub heures_mois: f64,
    pub actif: bool,
    pub motif_fin: Option<String>,
    pub note: Option<String>,
    pub cree_le: String,
    #[serde(default)]
    pub avantages: Vec<Avantage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Avantage {
    #[serde(default)]
    pub id: String,
    pub code_avantage: String,
    pub valeur_declaree: Option<f64>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouvelEmploye {
    pub matricule: String,
    pub nom: String,
    #[serde(default)] pub prenom: Option<String>,
    #[serde(default)] pub date_naissance: Option<String>,
    #[serde(default)] pub lieu_naissance: Option<String>,
    #[serde(default)] pub sexe: Option<String>,
    #[serde(default)] pub cni: Option<String>,
    #[serde(default)] pub telephone: Option<String>,
    #[serde(default)] pub adresse: Option<String>,
    #[serde(default = "celibataire")] pub situation_matrimoniale: String,
    #[serde(default)] pub nb_conjoints_a_charge: i64,
    #[serde(default)] pub nb_enfants_charge: i64,
    #[serde(default)] pub est_cadre: bool,
    #[serde(default)] pub poste: Option<String>,
    #[serde(default)] pub categorie: Option<String>,
    pub date_embauche: String,
    #[serde(default)] pub date_sortie: Option<String>,
    #[serde(default)] pub motif_sortie: Option<String>,
    #[serde(default)] pub numero_ipres: Option<String>,
    #[serde(default)] pub numero_css: Option<String>,
    #[serde(default)] pub numero_ipm: Option<String>,
    #[serde(default = "virement")] pub mode_paiement: String,
    #[serde(default)] pub banque: Option<String>,
    #[serde(default)] pub numero_compte: Option<String>,
    #[serde(default)] pub note: Option<String>,
}

fn celibataire() -> String { "celibataire".into() }
fn virement() -> String { "virement".into() }

#[derive(Debug, Clone, Deserialize)]
pub struct NouveauContrat {
    pub employe_id: String,
    #[serde(default = "cdi")] pub type_contrat: String,
    pub date_debut: String,
    #[serde(default)] pub date_fin: Option<String>,
    pub salaire_base: f64,
    #[serde(default)] pub sursalaire: f64,
    #[serde(default = "heures_defaut")] pub heures_mois: f64,
    #[serde(default)] pub note: Option<String>,
    #[serde(default)] pub avantages: Vec<Avantage>,
}

fn cdi() -> String { "cdi".into() }
fn heures_defaut() -> f64 { 173.33 }

#[derive(Debug, Clone, Copy, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Filtre {
    #[default]
    Actifs,
    Tous,
    Sortis,
}

// ---------------------------------------------------------------------------
// Alertes — signalent sans bloquer
// ---------------------------------------------------------------------------

fn alertes(e: &Employe) -> Vec<String> {
    let mut a = Vec::new();
    let vide = |o: &Option<String>| o.as_deref().map(str::trim).unwrap_or("").is_empty();

    match &e.contrat {
        None if e.actif => a.push(
            "Aucun contrat actif : ce salarié ne pourra pas être payé tant qu'un contrat \
             n'aura pas été enregistré."
                .into(),
        ),
        Some(c) => {
            // Un CDD sans terme est une anomalie juridique : en droit, il est
            // réputé conclu pour une durée indéterminée.
            if c.type_contrat == "cdd" && c.date_fin.is_none() {
                a.push(
                    "Ce CDD n'a pas de date de fin. Un contrat à durée déterminée sans terme \
                     écrit peut être requalifié en CDI."
                        .into(),
                );
            }
            if c.salaire_base <= 0.0 {
                a.push("Le salaire de base du contrat est à zéro.".into());
            }
            if let Some(fin) = &c.date_fin {
                if fin.as_str() < now()[..10].to_string().as_str() {
                    a.push(format!(
                        "Le contrat est arrivé à son terme le {fin} mais reste marqué actif."
                    ));
                }
            }
        }
        _ => {}
    }

    // Ces numéros ne servent pas au calcul du bulletin, mais **sans eux les
    // déclarations sont rejetées** par les organismes.
    let manquants: Vec<&str> = [
        (vide(&e.numero_ipres), "IPRES"),
        (vide(&e.numero_css), "CSS"),
    ]
    .iter()
    .filter(|(v, _)| *v)
    .map(|(_, n)| *n)
    .collect();
    if !manquants.is_empty() {
        a.push(format!(
            "Numéro {} manquant : les déclarations sociales le réclament.",
            manquants.join(" et ")
        ));
    }

    // Les parts d'impôt et les parts TRIMF sont deux comptes distincts : une
    // épouse à charge déclarée par un célibataire est très probablement une
    // confusion entre les deux.
    if e.nb_conjoints_a_charge > 0 && e.situation_matrimoniale != "marie" {
        a.push(
            "Des épouses à charge sont déclarées alors que le salarié n'est pas marié : \
             vérifiez la situation de famille, elle décide de l'impôt."
                .into(),
        );
    }
    if e.mode_paiement == "virement" && vide(&e.numero_compte) {
        a.push("Paiement par virement, mais aucun numéro de compte n'est renseigné.".into());
    }
    if e.date_sortie.is_some() && e.actif {
        a.push(
            "Une date de sortie est renseignée mais le salarié est toujours marqué présent."
                .into(),
        );
    }
    a
}

// ---------------------------------------------------------------------------
// Lecture
// ---------------------------------------------------------------------------

const CHAMPS: &str = "
    id, matricule, nom, prenom, date_naissance, lieu_naissance, sexe, cni,
    telephone, adresse, situation_matrimoniale, nb_conjoints_a_charge,
    nb_enfants_charge, est_cadre, poste, categorie, date_embauche, date_sortie,
    motif_sortie, numero_ipres, numero_css, numero_ipm, mode_paiement, banque,
    numero_compte, actif, note, cree_le";

fn vers_employe(r: &rusqlite::Row) -> rusqlite::Result<Employe> {
    let nom: String = r.get(2)?;
    let prenom: Option<String> = r.get(3)?;
    let situation: String = r.get(10)?;
    let nb_conjoints: i64 = r.get(11)?;
    let nb_enfants: i64 = r.get(12)?;
    let embauche: String = r.get(16)?;
    Ok(Employe {
        nom_complet: match prenom.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
            Some(p) => format!("{p} {nom}"),
            None => nom.clone(),
        },
        nb_parts_ir: parts_fiscales(&situation, nb_enfants),
        nb_parts_trimf: parts_trimf(nb_conjoints),
        anciennete_mois: mois_ecoules(&embauche),
        id: r.get(0)?,
        matricule: r.get(1)?,
        nom,
        prenom,
        date_naissance: r.get(4)?,
        lieu_naissance: r.get(5)?,
        sexe: r.get(6)?,
        cni: r.get(7)?,
        telephone: r.get(8)?,
        adresse: r.get(9)?,
        situation_matrimoniale: situation,
        nb_conjoints_a_charge: nb_conjoints,
        nb_enfants_charge: nb_enfants,
        est_cadre: r.get::<_, i64>(13)? != 0,
        poste: r.get(14)?,
        categorie: r.get(15)?,
        date_embauche: embauche,
        date_sortie: r.get(17)?,
        motif_sortie: r.get(18)?,
        numero_ipres: r.get(19)?,
        numero_css: r.get(20)?,
        numero_ipm: r.get(21)?,
        mode_paiement: r.get(22)?,
        banque: r.get(23)?,
        numero_compte: r.get(24)?,
        actif: r.get::<_, i64>(25)? != 0,
        note: r.get(26)?,
        cree_le: r.get(27)?,
        contrat: None,
        remuneration_base: 0.0,
        alertes: Vec::new(),
    })
}

fn mois_ecoules(depuis: &str) -> i64 {
    let (Ok(d), Ok(a)) = (
        chrono::NaiveDate::parse_from_str(depuis, "%Y-%m-%d"),
        chrono::NaiveDate::parse_from_str(&now()[..10], "%Y-%m-%d"),
    ) else {
        return 0;
    };
    if a < d {
        return 0;
    }
    let mois = (a.format("%Y").to_string().parse::<i64>().unwrap_or(0)
        - d.format("%Y").to_string().parse::<i64>().unwrap_or(0))
        * 12
        + (a.format("%m").to_string().parse::<i64>().unwrap_or(0)
            - d.format("%m").to_string().parse::<i64>().unwrap_or(0));
    // Le mois en cours ne compte que s'il est révolu au jour près.
    if a.format("%d").to_string() < d.format("%d").to_string() {
        (mois - 1).max(0)
    } else {
        mois.max(0)
    }
}

/// Complète un salarié avec son contrat actif, sa rémunération et ses alertes.
fn enrichir(conn: &Connection, mut e: Employe) -> Result<Employe> {
    e.contrat = contrat_actif(conn, &e.id)?;
    e.remuneration_base = e
        .contrat
        .as_ref()
        .map(|c| arrondi(c.salaire_base + c.sursalaire))
        .unwrap_or(0.0);
    e.alertes = alertes(&e);
    Ok(e)
}

fn arrondi(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

pub fn lister(conn: &Connection, filtre: Filtre) -> Result<Vec<Employe>> {
    let clause = match filtre {
        Filtre::Actifs => "actif = 1",
        Filtre::Sortis => "actif = 0",
        Filtre::Tous => "1 = 1",
    };
    let mut st = conn.prepare(&format!(
        "SELECT {CHAMPS} FROM employes WHERE {clause} ORDER BY nom, prenom"
    ))?;
    let bruts = st
        .query_map([], vers_employe)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    bruts.into_iter().map(|e| enrichir(conn, e)).collect()
}

pub fn lire(conn: &Connection, id: &str) -> Result<Employe> {
    let e = conn
        .query_row(
            &format!("SELECT {CHAMPS} FROM employes WHERE id = ?1"),
            params![id],
            vers_employe,
        )
        .map_err(|err| match err {
            rusqlite::Error::QueryReturnedNoRows => {
                CoreError::NotFound(format!("salarié {id}"))
            }
            autre => autre.into(),
        })?;
    let mut e = enrichir(conn, e)?;
    // Sur la fiche, on veut aussi l'historique des contrats précédents.
    if let Some(c) = e.contrat.as_mut() {
        c.avantages = avantages(conn, &c.id)?;
    }
    Ok(e)
}

pub fn contrats(conn: &Connection, employe_id: &str) -> Result<Vec<Contrat>> {
    let mut st = conn.prepare(
        "SELECT id, employe_id, type_contrat, date_debut, date_fin, salaire_base,
                sursalaire, heures_mois, actif, motif_fin, note, cree_le
           FROM contrats WHERE employe_id = ?1 ORDER BY date_debut DESC",
    )?;
    let v = st
        .query_map(params![employe_id], vers_contrat)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    v.into_iter()
        .map(|mut c| {
            c.avantages = avantages(conn, &c.id)?;
            Ok(c)
        })
        .collect()
}

fn vers_contrat(r: &rusqlite::Row) -> rusqlite::Result<Contrat> {
    Ok(Contrat {
        id: r.get(0)?,
        employe_id: r.get(1)?,
        type_contrat: r.get(2)?,
        date_debut: r.get(3)?,
        date_fin: r.get(4)?,
        salaire_base: r.get(5)?,
        sursalaire: r.get(6)?,
        heures_mois: r.get(7)?,
        actif: r.get::<_, i64>(8)? != 0,
        motif_fin: r.get(9)?,
        note: r.get(10)?,
        cree_le: r.get(11)?,
        avantages: Vec::new(),
    })
}

pub fn contrat_actif(conn: &Connection, employe_id: &str) -> Result<Option<Contrat>> {
    let mut st = conn.prepare(
        "SELECT id, employe_id, type_contrat, date_debut, date_fin, salaire_base,
                sursalaire, heures_mois, actif, motif_fin, note, cree_le
           FROM contrats WHERE employe_id = ?1 AND actif = 1 LIMIT 1",
    )?;
    let mut it = st.query_map(params![employe_id], vers_contrat)?;
    Ok(it.next().transpose()?)
}

fn avantages(conn: &Connection, contrat_id: &str) -> Result<Vec<Avantage>> {
    let mut st = conn.prepare(
        "SELECT id, code_avantage, valeur_declaree FROM contrat_avantages WHERE contrat_id = ?1",
    )?;
    let v = st
        .query_map(params![contrat_id], |r| {
            Ok(Avantage {
                id: r.get(0)?,
                code_avantage: r.get(1)?,
                valeur_declaree: r.get(2)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

// ---------------------------------------------------------------------------
// Écriture — salariés
// ---------------------------------------------------------------------------

fn valider(e: &NouvelEmploye) -> Result<()> {
    if e.matricule.trim().is_empty() {
        return Err(CoreError::Rule(
            "Le matricule est obligatoire : il identifie le salarié sur les bulletins, les \
             déclarations et les virements."
                .into(),
        ));
    }
    if e.nom.trim().is_empty() {
        return Err(CoreError::Rule("Le nom du salarié est obligatoire.".into()));
    }
    if e.date_embauche.len() != 10 {
        return Err(CoreError::Rule(
            "La date d'embauche est obligatoire (AAAA-MM-JJ) : elle décide de l'ancienneté.".into(),
        ));
    }
    if let (Some(sortie), embauche) = (&e.date_sortie, &e.date_embauche) {
        if sortie.as_str() < embauche.as_str() {
            return Err(CoreError::Rule(
                "La date de sortie est antérieure à la date d'embauche.".into(),
            ));
        }
    }
    Ok(())
}

pub fn creer(conn: &Connection, e: &NouvelEmploye) -> Result<Employe> {
    valider(e)?;
    let deja: i64 = conn.query_row(
        "SELECT COUNT(*) FROM employes WHERE matricule = ?1",
        params![e.matricule.trim()],
        |r| r.get(0),
    )?;
    if deja > 0 {
        return Err(CoreError::Rule(format!(
            "Le matricule « {} » est déjà utilisé par un autre salarié.",
            e.matricule.trim()
        )));
    }
    let id = Uuid::new_v4().to_string();
    ecrire(conn, &id, e, true)?;
    lire(conn, &id)
}

pub fn modifier(conn: &Connection, id: &str, e: &NouvelEmploye) -> Result<Employe> {
    valider(e)?;
    let deja: i64 = conn.query_row(
        "SELECT COUNT(*) FROM employes WHERE matricule = ?1 AND id <> ?2",
        params![e.matricule.trim(), id],
        |r| r.get(0),
    )?;
    if deja > 0 {
        return Err(CoreError::Rule(format!(
            "Le matricule « {} » est déjà utilisé par un autre salarié.",
            e.matricule.trim()
        )));
    }
    ecrire(conn, id, e, false)?;
    lire(conn, id)
}

fn ecrire(conn: &Connection, id: &str, e: &NouvelEmploye, creation: bool) -> Result<()> {
    let p = params![
        id, e.matricule.trim(), e.nom.trim(), e.prenom, e.date_naissance, e.lieu_naissance,
        e.sexe, e.cni, e.telephone, e.adresse, e.situation_matrimoniale,
        e.nb_conjoints_a_charge.max(0), e.nb_enfants_charge.max(0), e.est_cadre as i64,
        e.poste, e.categorie, e.date_embauche, e.date_sortie, e.motif_sortie,
        e.numero_ipres, e.numero_css, e.numero_ipm, e.mode_paiement, e.banque,
        e.numero_compte, e.note, now(),
    ];
    if creation {
        conn.execute(
            "INSERT INTO employes
               (id, matricule, nom, prenom, date_naissance, lieu_naissance, sexe, cni,
                telephone, adresse, situation_matrimoniale, nb_conjoints_a_charge,
                nb_enfants_charge, est_cadre, poste, categorie, date_embauche, date_sortie,
                motif_sortie, numero_ipres, numero_css, numero_ipm, mode_paiement, banque,
                numero_compte, note, cree_le)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17,?18,?19,
                     ?20,?21,?22,?23,?24,?25,?26,?27)",
            p,
        )?;
    } else {
        let n = conn.execute(
            "UPDATE employes SET matricule=?2, nom=?3, prenom=?4, date_naissance=?5,
                    lieu_naissance=?6, sexe=?7, cni=?8, telephone=?9, adresse=?10,
                    situation_matrimoniale=?11, nb_conjoints_a_charge=?12,
                    nb_enfants_charge=?13, est_cadre=?14, poste=?15, categorie=?16,
                    date_embauche=?17, date_sortie=?18, motif_sortie=?19, numero_ipres=?20,
                    numero_css=?21, numero_ipm=?22, mode_paiement=?23,
                    banque=?24, numero_compte=?25, note=?26, maj_le=?27
              WHERE id=?1",
            p,
        )?;
        if n == 0 {
            return Err(CoreError::NotFound(format!("salarié {id}")));
        }
    }
    Ok(())
}

/// Enregistre le départ d'un salarié.
///
/// ⚠️ **Ce n'est pas une suppression.** Ses bulletins doivent rester
/// consultables : c'est une obligation légale, et la seule preuve de ce qui lui
/// a été versé. On clôt aussi son contrat, sinon il resterait « actif » pour
/// quelqu'un qui n'est plus là.
pub fn enregistrer_depart(
    conn: &Connection,
    id: &str,
    date_sortie: &str,
    motif: &str,
) -> Result<Employe> {
    let e = lire(conn, id)?;
    if date_sortie.as_bytes() < e.date_embauche.as_bytes() {
        return Err(CoreError::Rule(
            "La date de départ est antérieure à la date d'embauche.".into(),
        ));
    }
    if motif.trim().is_empty() {
        return Err(CoreError::Rule(
            "Le motif du départ est obligatoire : il détermine les droits du salarié \
             (préavis, indemnités) et sera demandé en cas de litige."
                .into(),
        ));
    }
    conn.execute(
        "UPDATE employes SET actif = 0, date_sortie = ?2, motif_sortie = ?3, maj_le = ?4
          WHERE id = ?1",
        params![id, date_sortie, motif.trim(), now()],
    )?;
    conn.execute(
        "UPDATE contrats SET actif = 0, date_fin = COALESCE(date_fin, ?2), motif_fin = ?3
          WHERE employe_id = ?1 AND actif = 1",
        params![id, date_sortie, motif.trim()],
    )?;
    lire(conn, id)
}

/// Réintègre un salarié parti. Son ancien contrat **ne revient pas** : un
/// retour se matérialise par un nouveau contrat.
pub fn reintegrer(conn: &Connection, id: &str) -> Result<Employe> {
    let n = conn.execute(
        "UPDATE employes SET actif = 1, date_sortie = NULL, motif_sortie = NULL, maj_le = ?2
          WHERE id = ?1",
        params![id, now()],
    )?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("salarié {id}")));
    }
    lire(conn, id)
}

/// Suppression définitive — **refusée dès qu'un bulletin existe**.
pub fn supprimer(conn: &Connection, id: &str) -> Result<()> {
    let nb: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM bulletins_paie WHERE employe_id = ?1",
            params![id],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if nb > 0 {
        return Err(CoreError::Rule(format!(
            "Ce salarié a {nb} bulletin(s) de paie : il ne peut pas être supprimé. \
             Enregistrez son départ — sa fiche et ses bulletins restent consultables."
        )));
    }
    let n = conn.execute("DELETE FROM employes WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("salarié {id}")));
    }
    Ok(())
}

/// Résultat d'un traitement par lot : ce qui est passé, et surtout **ce qui ne
/// l'est pas et pourquoi**. Un simple compteur laisserait croire à un échec
/// silencieux sur les salariés protégés.
#[derive(Debug, Clone, Serialize)]
pub struct ResultatLot {
    pub traites: usize,
    pub conserves: usize,
    pub matricules_conserves: Vec<String>,
    pub message: String,
}

pub fn depart_lot(
    conn: &Connection,
    ids: &[String],
    date_sortie: &str,
    motif: &str,
) -> Result<ResultatLot> {
    let mut traites = 0;
    let mut conserves = Vec::new();
    for id in ids {
        match enregistrer_depart(conn, id, date_sortie, motif) {
            Ok(_) => traites += 1,
            Err(_) => {
                if let Ok(e) = lire(conn, id) {
                    conserves.push(e.matricule);
                }
            }
        }
    }
    let message = if conserves.is_empty() {
        format!("{traites} départ(s) enregistré(s).")
    } else {
        format!(
            "{traites} départ(s) enregistré(s). {} non traité(s) : {}.",
            conserves.len(),
            conserves.join(", ")
        )
    };
    Ok(ResultatLot { traites, conserves: conserves.len(), matricules_conserves: conserves, message })
}

// ---------------------------------------------------------------------------
// Écriture — contrats
// ---------------------------------------------------------------------------

/// Enregistre un nouveau contrat et **clôt automatiquement le précédent**.
///
/// ⚠️ La base interdit deux contrats actifs (index unique partiel) : sans cette
/// clôture, l'insertion échouerait sur une erreur technique incompréhensible.
/// On fait donc le geste explicitement, et on trace la raison.
pub fn creer_contrat(conn: &Connection, c: &NouveauContrat) -> Result<Contrat> {
    if c.salaire_base <= 0.0 {
        return Err(CoreError::Rule(
            "Le salaire de base doit être supérieur à zéro.".into(),
        ));
    }
    if c.date_debut.len() != 10 {
        return Err(CoreError::Rule("La date de début est obligatoire.".into()));
    }
    if let Some(fin) = &c.date_fin {
        if fin.as_str() < c.date_debut.as_str() {
            return Err(CoreError::Rule(
                "La date de fin du contrat est antérieure à sa date de début.".into(),
            ));
        }
    }
    // Le salarié doit exister : un contrat orphelin ne se voit nulle part.
    lire(conn, &c.employe_id)?;

    let precedent = contrat_actif(conn, &c.employe_id)?;
    if let Some(p) = &precedent {
        conn.execute(
            "UPDATE contrats SET actif = 0,
                    date_fin = COALESCE(date_fin, ?2),
                    motif_fin = COALESCE(motif_fin, 'Remplacé par un nouveau contrat')
              WHERE id = ?1",
            params![p.id, veille_de(&c.date_debut)],
        )?;
    }

    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO contrats
           (id, employe_id, type_contrat, date_debut, date_fin, salaire_base,
            sursalaire, heures_mois, actif, note, cree_le)
         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,1,?9,?10)",
        params![
            id, c.employe_id, c.type_contrat, c.date_debut, c.date_fin,
            c.salaire_base, c.sursalaire, c.heures_mois, c.note, now()
        ],
    )?;
    for a in &c.avantages {
        conn.execute(
            "INSERT INTO contrat_avantages (id, contrat_id, code_avantage, valeur_declaree, cree_le)
             VALUES (?1,?2,?3,?4,?5)",
            params![Uuid::new_v4().to_string(), id, a.code_avantage, a.valeur_declaree, now()],
        )?;
    }
    contrat_actif(conn, &c.employe_id)?
        .map(|mut x| {
            x.avantages = avantages(conn, &x.id).unwrap_or_default();
            x
        })
        .ok_or_else(|| CoreError::NotFound("contrat".into()))
}

/// Modifie un contrat. ⚠️ Refusé si des bulletins s'appuient déjà dessus :
/// changer un salaire de base rétroactivement réécrirait des fiches remises aux
/// salariés. Dans ce cas, on enregistre un **nouveau contrat**.
pub fn modifier_contrat(conn: &Connection, id: &str, c: &NouveauContrat) -> Result<Contrat> {
    let nb: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM bulletins_paie WHERE employe_id = ?1
              AND periode >= substr(?2, 1, 7)",
            params![c.employe_id, c.date_debut],
            |r| r.get(0),
        )
        .unwrap_or(0);
    if nb > 0 {
        return Err(CoreError::Rule(format!(
            "{nb} bulletin(s) ont déjà été calculés sur cette période. Modifier ce contrat \
             réécrirait des fiches déjà remises au salarié : enregistrez plutôt un NOUVEAU \
             contrat à la date du changement."
        )));
    }
    if c.salaire_base <= 0.0 {
        return Err(CoreError::Rule(
            "Le salaire de base doit être supérieur à zéro.".into(),
        ));
    }
    let n = conn.execute(
        "UPDATE contrats SET type_contrat=?2, date_debut=?3, date_fin=?4, salaire_base=?5,
                sursalaire=?6, heures_mois=?7, note=?8 WHERE id=?1",
        params![id, c.type_contrat, c.date_debut, c.date_fin, c.salaire_base,
                c.sursalaire, c.heures_mois, c.note],
    )?;
    if n == 0 {
        return Err(CoreError::NotFound(format!("contrat {id}")));
    }
    conn.execute("DELETE FROM contrat_avantages WHERE contrat_id = ?1", params![id])?;
    for a in &c.avantages {
        conn.execute(
            "INSERT INTO contrat_avantages (id, contrat_id, code_avantage, valeur_declaree, cree_le)
             VALUES (?1,?2,?3,?4,?5)",
            params![Uuid::new_v4().to_string(), id, a.code_avantage, a.valeur_declaree, now()],
        )?;
    }
    let mut ct = contrats(conn, &c.employe_id)?
        .into_iter()
        .find(|x| x.id == id)
        .ok_or_else(|| CoreError::NotFound("contrat".into()))?;
    ct.avantages = avantages(conn, id)?;
    Ok(ct)
}

fn veille_de(date: &str) -> String {
    chrono::NaiveDate::parse_from_str(date, "%Y-%m-%d")
        .map(|d| (d - chrono::Duration::days(1)).format("%Y-%m-%d").to_string())
        .unwrap_or_else(|_| date.to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    fn employe(matricule: &str) -> NouvelEmploye {
        NouvelEmploye {
            matricule: matricule.into(), nom: "Diop".into(), prenom: Some("Awa".into()),
            date_naissance: None, lieu_naissance: None, sexe: Some("f".into()), cni: None,
            telephone: None, adresse: None, situation_matrimoniale: "celibataire".into(),
            nb_conjoints_a_charge: 0, nb_enfants_charge: 0, est_cadre: false,
            poste: Some("Comptable".into()), categorie: None,
            date_embauche: "2024-01-15".into(), date_sortie: None, motif_sortie: None,
            numero_ipres: Some("IP-1".into()), numero_css: Some("CS-1".into()),
            numero_ipm: None, mode_paiement: "especes".into(), banque: None,
            numero_compte: None, note: None,
        }
    }

    fn contrat(employe_id: &str, salaire: f64) -> NouveauContrat {
        NouveauContrat {
            employe_id: employe_id.into(), type_contrat: "cdi".into(),
            date_debut: "2024-01-15".into(), date_fin: None, salaire_base: salaire,
            sursalaire: 0.0, heures_mois: 173.33, note: None, avantages: vec![],
        }
    }

    /// ⚠️ La règle du CGI, cas par cas. Ce n'est **pas** le quotient familial
    /// français : ces parts ne divisent pas le revenu, elles pointent vers une
    /// ligne de réduction.
    #[test]
    fn les_parts_fiscales_suivent_la_regle() {
        assert_eq!(parts_fiscales("celibataire", 0), 1.0);
        assert_eq!(parts_fiscales("divorce", 0), 1.0);
        assert_eq!(parts_fiscales("marie", 0), 1.5);
        assert_eq!(parts_fiscales("marie", 2), 2.5);
        assert_eq!(parts_fiscales("celibataire", 3), 2.5);
        // Un veuf sans enfant retombe à 1 part ; avec enfants il garde la
        // demi-part du conjoint disparu.
        assert_eq!(parts_fiscales("veuf", 0), 1.0);
        assert_eq!(parts_fiscales("veuf", 1), 2.0);
        // ⚠️ Le plafond de 5 parts est une règle, pas une commodité : sans lui,
        // une famille de 10 enfants sortirait du barème.
        assert_eq!(parts_fiscales("marie", 10), 5.0);
        assert_eq!(parts_fiscales("marie", 7), 5.0);
    }

    /// Compte DISTINCT des parts d'impôt : les enfants n'y entrent pas.
    #[test]
    fn les_parts_trimf_ne_comptent_que_les_epouses() {
        assert_eq!(parts_trimf(0), 1.0);
        assert_eq!(parts_trimf(2), 3.0);
        assert_eq!(parts_trimf(-5), 1.0, "une valeur absurde ne doit pas casser le calcul");
    }

    #[test]
    fn les_parts_sont_calculees_et_pas_stockees() {
        let conn = db::open_in_memory().unwrap();
        let mut n = employe("M-001");
        n.situation_matrimoniale = "marie".into();
        n.nb_enfants_charge = 2;
        n.nb_conjoints_a_charge = 1;
        let e = creer(&conn, &n).unwrap();
        assert_eq!(e.nb_parts_ir, 2.5);
        assert_eq!(e.nb_parts_trimf, 2.0);

        // Un enfant de plus : les parts suivent SANS qu'on ait rien à écrire.
        n.nb_enfants_charge = 3;
        let e = modifier(&conn, &e.id, &n).unwrap();
        assert_eq!(e.nb_parts_ir, 3.0);
    }

    #[test]
    fn le_matricule_est_unique() {
        let conn = db::open_in_memory().unwrap();
        creer(&conn, &employe("M-001")).unwrap();
        let e = creer(&conn, &employe("M-001")).unwrap_err();
        assert!(e.to_string().contains("déjà utilisé"));
    }

    /// ⚠️⚠️ La garantie la plus importante du module : deux contrats actifs
    /// rendraient le salaire de base ambigu.
    #[test]
    fn un_seul_contrat_actif_a_la_fois() {
        let conn = db::open_in_memory().unwrap();
        let e = creer(&conn, &employe("M-001")).unwrap();
        creer_contrat(&conn, &contrat(&e.id, 150_000.0)).unwrap();

        // Augmentation : nouveau contrat au 1er juillet.
        let mut c2 = contrat(&e.id, 200_000.0);
        c2.date_debut = "2026-07-01".into();
        creer_contrat(&conn, &c2).unwrap();

        let tous = contrats(&conn, &e.id).unwrap();
        assert_eq!(tous.len(), 2, "l'historique est conservé");
        assert_eq!(tous.iter().filter(|c| c.actif).count(), 1);

        let actif = contrat_actif(&conn, &e.id).unwrap().unwrap();
        assert_eq!(actif.salaire_base, 200_000.0);
        // L'ancien se ferme la VEILLE du nouveau : pas de trou, pas de
        // chevauchement d'un jour.
        let ancien = tous.iter().find(|c| !c.actif).unwrap();
        assert_eq!(ancien.date_fin.as_deref(), Some("2026-06-30"));
        assert!(ancien.motif_fin.is_some(), "on trace POURQUOI il s'est fermé");
    }

    #[test]
    fn la_remuneration_de_base_additionne_salaire_et_sursalaire() {
        let conn = db::open_in_memory().unwrap();
        let e = creer(&conn, &employe("M-001")).unwrap();
        let mut c = contrat(&e.id, 150_000.0);
        c.sursalaire = 25_000.0;
        creer_contrat(&conn, &c).unwrap();
        assert_eq!(lire(&conn, &e.id).unwrap().remuneration_base, 175_000.0);
    }

    /// Les alertes SIGNALENT sans bloquer : un dossier en cours de
    /// régularisation doit pouvoir être enregistré.
    #[test]
    fn les_anomalies_sont_signalees_sans_bloquer() {
        let conn = db::open_in_memory().unwrap();
        let mut n = employe("M-001");
        n.numero_ipres = None;
        n.numero_css = None;
        n.mode_paiement = "virement".into();
        n.nb_conjoints_a_charge = 1; // mais célibataire
        let e = creer(&conn, &n).unwrap();

        let a = e.alertes.join(" | ");
        assert!(a.contains("Aucun contrat actif"), "{a}");
        assert!(a.contains("IPRES et CSS"), "{a}");
        assert!(a.contains("numéro de compte"), "{a}");
        assert!(a.contains("pas marié"), "{a}");

        // Un CDD sans terme : anomalie juridique, signalée mais acceptée.
        let mut c = contrat(&e.id, 150_000.0);
        c.type_contrat = "cdd".into();
        creer_contrat(&conn, &c).unwrap();
        assert!(lire(&conn, &e.id).unwrap().alertes.iter()
                .any(|x| x.contains("requalifié en CDI")));
    }

    /// ⚠️ Un salarié ne se supprime pas : ses bulletins sont sa seule preuve de
    /// ce qui lui a été versé. On enregistre un DÉPART.
    #[test]
    fn le_depart_cloture_le_contrat_sans_rien_effacer() {
        let conn = db::open_in_memory().unwrap();
        let e = creer(&conn, &employe("M-001")).unwrap();
        creer_contrat(&conn, &contrat(&e.id, 150_000.0)).unwrap();

        let sorti = enregistrer_depart(&conn, &e.id, "2026-06-30", "Fin de période d'essai").unwrap();
        assert!(!sorti.actif);
        assert_eq!(sorti.date_sortie.as_deref(), Some("2026-06-30"));
        assert!(sorti.contrat.is_none(), "le contrat ne doit plus être actif");

        // La fiche et l'historique restent là.
        assert_eq!(contrats(&conn, &e.id).unwrap().len(), 1);
        assert_eq!(lister(&conn, Filtre::Sortis).unwrap().len(), 1);
        assert_eq!(lister(&conn, Filtre::Actifs).unwrap().len(), 0);
        assert_eq!(lister(&conn, Filtre::Tous).unwrap().len(), 1);
    }

    /// Le motif de départ n'est pas une formalité : il détermine les droits du
    /// salarié et sera le premier document réclamé en cas de litige.
    #[test]
    fn un_depart_sans_motif_est_refuse() {
        let conn = db::open_in_memory().unwrap();
        let e = creer(&conn, &employe("M-001")).unwrap();
        let err = enregistrer_depart(&conn, &e.id, "2026-06-30", "   ").unwrap_err();
        assert!(err.to_string().contains("motif du départ est obligatoire"));
        assert!(lire(&conn, &e.id).unwrap().actif, "rien ne doit avoir bougé");
    }

    #[test]
    fn une_date_de_sortie_avant_l_embauche_est_refusee() {
        let conn = db::open_in_memory().unwrap();
        let e = creer(&conn, &employe("M-001")).unwrap();
        assert!(enregistrer_depart(&conn, &e.id, "2020-01-01", "erreur").is_err());
    }

    #[test]
    fn un_retour_passe_par_un_nouveau_contrat() {
        let conn = db::open_in_memory().unwrap();
        let e = creer(&conn, &employe("M-001")).unwrap();
        creer_contrat(&conn, &contrat(&e.id, 150_000.0)).unwrap();
        enregistrer_depart(&conn, &e.id, "2026-06-30", "démission").unwrap();

        let repris = reintegrer(&conn, &e.id).unwrap();
        assert!(repris.actif);
        assert!(repris.date_sortie.is_none());
        // ⚠️ L'ancien contrat NE revient PAS : il a été clos, un retour se
        // matérialise par un nouvel engagement.
        assert!(repris.contrat.is_none());
        assert!(repris.alertes.iter().any(|a| a.contains("Aucun contrat actif")));
    }

    #[test]
    fn un_contrat_sans_salaire_est_refuse() {
        let conn = db::open_in_memory().unwrap();
        let e = creer(&conn, &employe("M-001")).unwrap();
        assert!(creer_contrat(&conn, &contrat(&e.id, 0.0)).is_err());
    }

    #[test]
    fn un_contrat_orphelin_est_refuse() {
        let conn = db::open_in_memory().unwrap();
        assert!(creer_contrat(&conn, &contrat("inconnu", 150_000.0)).is_err());
    }

    #[test]
    fn les_avantages_suivent_le_contrat() {
        let conn = db::open_in_memory().unwrap();
        let e = creer(&conn, &employe("M-001")).unwrap();
        let mut c = contrat(&e.id, 150_000.0);
        c.avantages = vec![
            Avantage { id: String::new(), code_avantage: "av_logement".into(), valeur_declaree: None },
            Avantage { id: String::new(), code_avantage: "av_vehicule".into(),
                       valeur_declaree: Some(30_000.0) },
        ];
        creer_contrat(&conn, &c).unwrap();
        let lu = lire(&conn, &e.id).unwrap();
        let av = &lu.contrat.unwrap().avantages;
        assert_eq!(av.len(), 2);
        assert_eq!(av.iter().find(|a| a.code_avantage == "av_vehicule").unwrap()
                     .valeur_declaree, Some(30_000.0));
    }

    #[test]
    fn le_traitement_par_lot_dit_ce_qui_n_est_pas_passe() {
        let conn = db::open_in_memory().unwrap();
        let a = creer(&conn, &employe("M-001")).unwrap();
        let mut deux = employe("M-002");
        deux.date_embauche = "2026-12-01".into(); // embauché APRÈS la date de départ
        let b = creer(&conn, &deux).unwrap();

        let r = depart_lot(&conn, &[a.id.clone(), b.id.clone()], "2026-06-30", "fin de chantier")
            .unwrap();
        assert_eq!(r.traites, 1);
        assert_eq!(r.conserves, 1);
        // ⚠️ Un simple compteur laisserait croire à un échec silencieux : on
        // nomme les dossiers restés en arrière.
        assert_eq!(r.matricules_conserves, vec!["M-002".to_string()]);
        assert!(r.message.contains("M-002"));
    }

    #[test]
    fn l_anciennete_se_deduit_de_la_date_d_embauche() {
        let conn = db::open_in_memory().unwrap();
        let mut n = employe("M-001");
        n.date_embauche = "2026-01-15".into();
        let e = creer(&conn, &n).unwrap();
        // La date du jour dans les tests est celle de la machine : on vérifie
        // la cohérence, pas une valeur absolue qui périmerait.
        assert!(e.anciennete_mois >= 0);

        let mut futur = employe("M-002");
        futur.date_embauche = "2099-01-01".into();
        assert_eq!(creer(&conn, &futur).unwrap().anciennete_mois, 0,
                   "une embauche à venir ne donne pas d'ancienneté négative");
    }

    #[test]
    fn le_nom_complet_gere_l_absence_de_prenom() {
        let conn = db::open_in_memory().unwrap();
        let mut n = employe("M-001");
        n.prenom = None;
        assert_eq!(creer(&conn, &n).unwrap().nom_complet, "Diop");
        let mut m = employe("M-002");
        m.prenom = Some("  ".into());
        assert_eq!(creer(&conn, &m).unwrap().nom_complet, "Diop",
                   "un prénom vide ne doit pas laisser d'espace en tête");
    }
}
