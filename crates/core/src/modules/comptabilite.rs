//! Comptabilité — l'écran réservé au comptable (migration 0034).
//!
//! # Le procédé, en trois phrases
//!
//! Djigui enregistre des **faits de gestion** (ventes, encaissements) et ne
//! décide de rien en comptabilité. Le **comptable** crée ses comptes, écrit ses
//! **règles** de rattachement, et celles-ci s'appliquent d'un coup à **tout
//! l'historique déjà en base** comme aux opérations futures. Ce qu'aucune règle
//! ne couvre tombe dans la corbeille **« À ranger »**, qu'il traite à la main.
//!
//! Décision utilisateur (2026-07-27), textuelle : **« s'il y a des ambiguïtés
//! c'est le comptable qui tranche, il connaît mieux »**. Ce module propose,
//! il n'impose jamais.
//!
//! # Ce que le moteur sait, et ce que la règle apporte
//!
//! Le moteur connaît le **schéma** de chaque opération — qui va au débit, qui va
//! au crédit, et avec quels montants. Djigui possède déjà tous les chiffres. La
//! règle du comptable ne fait qu'une chose : **nommer les comptes**.
//!
//! ```text
//! VENTE (facture validée)            ENCAISSEMENT
//!   [tiers]    D  TTC                  [tresorerie] D  montant
//!   [produit]  C  HT                   [tiers]      C  montant
//!   [taxe]     C  TVA
//! ```
//!
//! # Les deux invariants
//!
//! 1. **Σ débit = Σ crédit** sur chaque écriture. C'est le seul endroit de
//!    Djigui où l'on refuse d'écrire : une écriture déséquilibrée est une faute,
//!    pas une souplesse. Garanti par construction et vérifié avant insertion.
//! 2. **On ne modifie ni ne supprime jamais une écriture** : on la contre-passe.
//!    Même réflexe que les paiements (migration 0019) et le journal de stock.
//!
//! Compte introuvable → **471 compte d'attente + alerte**, jamais un refus : la
//! comptabilité n'empêche jamais de vendre (`plan_comptable.md` §0).

use crate::domain::{DomaineComptable, RoleCompte};
use crate::error::{CoreError, Result};
use crate::now;
use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Compte d'attente : le seul compte que Djigui impose, pour ne jamais perdre
/// une opération faute de savoir où la ranger.
pub const COMPTE_ATTENTE: &str = "471";

/// Tolérance de comparaison des montants. Le franc CFA n'a pas de centimes,
/// mais les taxes en pourcentage en produisent : on arrondit au centime.
const EPSILON: f64 = 0.005;

fn vide(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|x| !x.is_empty())
}

fn arrondir(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// Exercice = année de la date « AAAA-MM-JJ ». Sans date exploitable on retombe
/// sur l'année courante plutôt que d'échouer.
fn exercice_de(date: &str) -> i64 {
    date.get(0..4).and_then(|a| a.parse().ok()).unwrap_or_else(|| {
        now().get(0..4).and_then(|a| a.parse().ok()).unwrap_or(0)
    })
}

// ===========================================================================
// Les comptes — créés par le comptable
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct Compte {
    pub numero: String,
    pub libelle: String,
    pub classe: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sens_normal: Option<String>,
    pub lettrable: bool,
    pub actif: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Nombre de lignes d'écriture pointant sur ce compte : un compte utilisé
    /// ne se supprime pas (mais se désactive).
    pub nb_lignes: i64,
    pub solde: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouveauCompte {
    pub numero: String,
    pub libelle: String,
    /// Absente = déduite du premier chiffre du numéro.
    #[serde(default)]
    pub classe: Option<i64>,
    #[serde(default)]
    pub sens_normal: Option<String>,
    #[serde(default)]
    pub lettrable: bool,
    #[serde(default = "vrai")]
    pub actif: bool,
    #[serde(default)]
    pub note: Option<String>,
}

fn vrai() -> bool {
    true
}

const COMPTE_COLS: &str = "SELECT c.numero, c.libelle, c.classe, c.sens_normal,
        c.lettrable, c.actif, c.note,
        (SELECT COUNT(*) FROM ecriture_ligne l WHERE l.compte_numero = c.numero),
        (SELECT COALESCE(SUM(l.debit - l.credit), 0)
           FROM ecriture_ligne l WHERE l.compte_numero = c.numero)
   FROM compte c";

fn ligne_compte(r: &Row) -> rusqlite::Result<Compte> {
    Ok(Compte {
        numero: r.get(0)?,
        libelle: r.get(1)?,
        classe: r.get(2)?,
        sens_normal: r.get(3)?,
        lettrable: r.get::<_, i64>(4)? != 0,
        actif: r.get::<_, i64>(5)? != 0,
        note: r.get(6)?,
        nb_lignes: r.get(7)?,
        solde: arrondir(r.get(8)?),
    })
}

pub fn lister_comptes(conn: &Connection, actifs_seulement: bool) -> Result<Vec<Compte>> {
    let sql = format!(
        "{COMPTE_COLS} {} ORDER BY c.numero",
        if actifs_seulement { "WHERE c.actif = 1" } else { "" }
    );
    let mut st = conn.prepare(&sql)?;
    let v = st.query_map([], ligne_compte)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn lire_compte(conn: &Connection, numero: &str) -> Result<Compte> {
    let mut st = conn.prepare(&format!("{COMPTE_COLS} WHERE c.numero = ?1"))?;
    st.query_row(params![numero], ligne_compte)
        .map_err(|_| CoreError::NotFound(format!("compte {numero}")))
}

/// Classe OHADA déduite du premier chiffre. Le comptable peut la corriger : on
/// ne lui interdit pas un plan qui sort de la norme.
fn classe_de(numero: &str) -> Option<i64> {
    numero.chars().next()?.to_digit(10).map(|d| d as i64)
}

pub fn creer_compte(conn: &Connection, c: &NouveauCompte, par: Option<&str>) -> Result<Compte> {
    let numero = c.numero.trim();
    if numero.is_empty() {
        return Err(CoreError::Rule("le numéro de compte est obligatoire".into()));
    }
    if c.libelle.trim().is_empty() {
        return Err(CoreError::Rule("le libellé du compte est obligatoire".into()));
    }
    let existe: i64 =
        conn.query_row("SELECT COUNT(*) FROM compte WHERE numero = ?1", params![numero], |r| r.get(0))?;
    if existe > 0 {
        return Err(CoreError::Rule(format!("le compte {numero} existe déjà")));
    }
    conn.execute(
        "INSERT INTO compte (numero, libelle, classe, sens_normal, lettrable, actif, note,
                             cree_par, cree_le)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            numero,
            c.libelle.trim(),
            c.classe.or_else(|| classe_de(numero)),
            vide(&c.sens_normal),
            c.lettrable as i64,
            c.actif as i64,
            vide(&c.note),
            par,
            now()
        ],
    )?;
    lire_compte(conn, numero)
}

/// Le numéro est la clé : le modifier reviendrait à casser les écritures déjà
/// passées. On modifie donc tout sauf lui.
pub fn modifier_compte(conn: &Connection, numero: &str, c: &NouveauCompte) -> Result<Compte> {
    lire_compte(conn, numero)?;
    if c.libelle.trim().is_empty() {
        return Err(CoreError::Rule("le libellé du compte est obligatoire".into()));
    }
    conn.execute(
        "UPDATE compte SET libelle = ?2, classe = ?3, sens_normal = ?4,
                           lettrable = ?5, actif = ?6, note = ?7
         WHERE numero = ?1",
        params![
            numero,
            c.libelle.trim(),
            c.classe.or_else(|| classe_de(numero)),
            vide(&c.sens_normal),
            c.lettrable as i64,
            c.actif as i64,
            vide(&c.note)
        ],
    )?;
    lire_compte(conn, numero)
}

/// Suppression refusée dès qu'une écriture ou une règle s'appuie sur le compte :
/// l'historique comptable ne doit jamais se retrouver orphelin. Le comptable
/// **désactive** un compte dont il ne veut plus.
pub fn supprimer_compte(conn: &Connection, numero: &str) -> Result<()> {
    if numero == COMPTE_ATTENTE {
        return Err(CoreError::Rule(
            "le compte d'attente 471 est nécessaire au fonctionnement : il ne peut pas être supprimé".into(),
        ));
    }
    lire_compte(conn, numero)?;
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ecriture_ligne WHERE compte_numero = ?1",
        params![numero],
        |r| r.get(0),
    )?;
    if n > 0 {
        return Err(CoreError::Rule(format!(
            "le compte {numero} porte {n} ligne(s) d'écriture : désactivez-le plutôt que de le supprimer"
        )));
    }
    let r: i64 = conn.query_row(
        "SELECT COUNT(*) FROM regle_comptable WHERE compte_numero = ?1",
        params![numero],
        |r| r.get(0),
    )?;
    if r > 0 {
        return Err(CoreError::Rule(format!(
            "le compte {numero} est employé par {r} règle(s) : retirez-les d'abord"
        )));
    }
    conn.execute("DELETE FROM compte WHERE numero = ?1", params![numero])?;
    Ok(())
}

/// Plan comptable OHADA de base — **proposé, jamais imposé**. Le comptable
/// l'installe d'un clic s'il veut partir d'une base, ou crée ses propres comptes.
/// Les comptes déjà présents ne sont pas touchés (on ne réécrit pas son travail).
const PLAN_OHADA: &[(&str, &str, &str, bool)] = &[
    // (numéro, libellé, sens normal, lettrable)
    ("101", "Capital social", "credit", false),
    ("131", "Résultat net de l'exercice", "credit", false),
    ("161", "Emprunts", "credit", false),
    ("241", "Matériel et outillage", "debit", false),
    ("244", "Matériel et mobilier de bureau", "debit", false),
    ("245", "Matériel de transport", "debit", false),
    ("311", "Marchandises", "debit", false),
    ("321", "Matières premières", "debit", false),
    ("361", "Produits finis", "debit", false),
    ("401", "Fournisseurs", "credit", true),
    ("411", "Clients", "debit", true),
    ("4431", "TVA facturée (collectée)", "credit", false),
    ("4451", "TVA récupérable (déductible)", "debit", false),
    ("447", "État, impôts retenus à la source", "credit", false),
    ("521", "Banques", "debit", false),
    ("531", "Établissements de monnaie électronique", "debit", false),
    ("571", "Caisse", "debit", false),
    ("585", "Virements de fonds internes", "debit", false),
    ("601", "Achats de marchandises", "debit", false),
    ("602", "Achats de matières premières", "debit", false),
    ("6031", "Variation des stocks de marchandises", "debit", false),
    ("605", "Autres achats (eau, électricité, fournitures)", "debit", false),
    ("622", "Locations", "debit", false),
    ("627", "Frais bancaires", "debit", false),
    ("641", "Impôts et taxes", "debit", false),
    ("661", "Rémunérations du personnel", "debit", false),
    ("701", "Ventes de marchandises", "credit", false),
    ("702", "Ventes de produits finis", "credit", false),
    ("706", "Services vendus", "credit", false),
    ("736", "Variation des stocks de produits finis", "credit", false),
    ("781", "Transferts de charges", "credit", false),
];

/// Installe le plan de base. Renvoie le nombre de comptes réellement ajoutés.
pub fn installer_plan_ohada(conn: &Connection, par: Option<&str>) -> Result<usize> {
    let mut ajoutes = 0;
    for (numero, libelle, sens, lettrable) in PLAN_OHADA {
        let existe: i64 = conn.query_row(
            "SELECT COUNT(*) FROM compte WHERE numero = ?1",
            params![numero],
            |r| r.get(0),
        )?;
        if existe > 0 {
            continue;
        }
        conn.execute(
            "INSERT INTO compte (numero, libelle, classe, sens_normal, lettrable, actif, cree_par, cree_le)
             VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6, ?7)",
            params![numero, libelle, classe_de(numero), sens, *lettrable as i64, par, now()],
        )?;
        ajoutes += 1;
    }
    Ok(ajoutes)
}

// ===========================================================================
// Les règles — « pour ce rôle, quand ces critères, prends ce compte »
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct Regle {
    pub id: String,
    pub nom: String,
    pub role: String,
    pub compte_numero: String,
    /// Libellé du compte, pour afficher « 701 — Ventes de marchandises ».
    pub compte_libelle: String,
    #[serde(flatten)]
    pub criteres: Criteres,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub journal_code: Option<String>,
    pub ordre: i64,
    pub actif: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
    /// Nombre de critères renseignés : c'est la **spécificité**. La règle la
    /// plus spécifique gagne, ce qui permet d'écrire un défaut large puis des
    /// exceptions étroites sans se soucier de l'ordre.
    pub specificite: i64,
    pub cree_le: String,
}

/// Critères d'une règle. Tous facultatifs : `None` = « peu importe ».
/// La même structure sert de **filtre de recherche multicritère** dans la
/// corbeille « À ranger » — c'est ce qui permet de transformer une sélection
/// en règle permanente d'un seul geste.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Criteres {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub domaine: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub categorie_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub article_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nature_comptable: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tiers_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nature_tiers: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub caisse_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub moyen_paiement_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub famille_paiement: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub depot_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taux_taxe: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub montant_min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub montant_max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub libelle_contient: Option<String>,
}

impl Criteres {
    fn nb_renseignes(&self) -> i64 {
        let mut n = 0;
        for present in [
            self.domaine.is_some(),
            self.categorie_id.is_some(),
            self.article_id.is_some(),
            self.nature_comptable.is_some(),
            self.tiers_id.is_some(),
            self.nature_tiers.is_some(),
            self.caisse_id.is_some(),
            self.moyen_paiement_id.is_some(),
            self.famille_paiement.is_some(),
            self.depot_id.is_some(),
            self.taux_taxe.is_some(),
            self.montant_min.is_some(),
            self.montant_max.is_some(),
            self.libelle_contient.is_some(),
        ] {
            if present {
                n += 1;
            }
        }
        n
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouvelleRegle {
    pub nom: String,
    pub role: String,
    pub compte_numero: String,
    #[serde(flatten)]
    pub criteres: Criteres,
    #[serde(default)]
    pub journal_code: Option<String>,
    #[serde(default)]
    pub ordre: i64,
    #[serde(default = "vrai")]
    pub actif: bool,
    #[serde(default)]
    pub note: Option<String>,
}

const REGLE_COLS: &str = "SELECT r.id, r.nom, r.role, r.compte_numero, c.libelle,
        r.domaine, r.categorie_id, r.article_id, r.nature_comptable, r.tiers_id,
        r.nature_tiers, r.caisse_id, r.moyen_paiement_id, r.famille_paiement,
        r.depot_id, r.taux_taxe, r.montant_min, r.montant_max, r.libelle_contient,
        r.journal_code, r.ordre, r.actif, r.note, r.cree_le
   FROM regle_comptable r JOIN compte c ON c.numero = r.compte_numero";

fn ligne_regle(r: &Row) -> rusqlite::Result<Regle> {
    let criteres = Criteres {
        domaine: r.get(5)?,
        categorie_id: r.get(6)?,
        article_id: r.get(7)?,
        nature_comptable: r.get(8)?,
        tiers_id: r.get(9)?,
        nature_tiers: r.get(10)?,
        caisse_id: r.get(11)?,
        moyen_paiement_id: r.get(12)?,
        famille_paiement: r.get(13)?,
        depot_id: r.get(14)?,
        taux_taxe: r.get(15)?,
        montant_min: r.get(16)?,
        montant_max: r.get(17)?,
        libelle_contient: r.get(18)?,
    };
    Ok(Regle {
        id: r.get(0)?,
        nom: r.get(1)?,
        role: r.get(2)?,
        compte_numero: r.get(3)?,
        compte_libelle: r.get(4)?,
        specificite: criteres.nb_renseignes(),
        criteres,
        journal_code: r.get(19)?,
        ordre: r.get(20)?,
        actif: r.get::<_, i64>(21)? != 0,
        note: r.get(22)?,
        cree_le: r.get(23)?,
    })
}

pub fn lister_regles(conn: &Connection) -> Result<Vec<Regle>> {
    let mut st = conn.prepare(&format!("{REGLE_COLS} ORDER BY r.role, r.ordre, r.cree_le"))?;
    let v = st.query_map([], ligne_regle)?.collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn lire_regle(conn: &Connection, id: &str) -> Result<Regle> {
    let mut st = conn.prepare(&format!("{REGLE_COLS} WHERE r.id = ?1"))?;
    st.query_row(params![id], ligne_regle)
        .map_err(|_| CoreError::NotFound(format!("règle {id}")))
}

fn valider_regle(conn: &Connection, r: &NouvelleRegle) -> Result<()> {
    if r.nom.trim().is_empty() {
        return Err(CoreError::Rule("donnez un nom à la règle".into()));
    }
    if RoleCompte::parse(&r.role).is_none() {
        return Err(CoreError::Rule(format!("rôle inconnu : {}", r.role)));
    }
    if let Some(d) = vide(&r.criteres.domaine) {
        if DomaineComptable::parse(d).is_none() {
            return Err(CoreError::Rule(format!("domaine inconnu : {d}")));
        }
    }
    lire_compte(conn, r.compte_numero.trim())?;
    Ok(())
}

pub fn creer_regle(conn: &Connection, r: &NouvelleRegle, par: Option<&str>) -> Result<Regle> {
    valider_regle(conn, r)?;
    let id = Uuid::new_v4().to_string();
    let c = &r.criteres;
    conn.execute(
        "INSERT INTO regle_comptable
            (id, nom, role, compte_numero, domaine, categorie_id, article_id,
             nature_comptable, tiers_id, nature_tiers, caisse_id, moyen_paiement_id,
             famille_paiement, depot_id, taux_taxe, montant_min, montant_max,
             libelle_contient, journal_code, ordre, actif, note, cree_par, cree_le)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15,
                 ?16, ?17, ?18, ?19, ?20, ?21, ?22, ?23, ?24)",
        params![
            id, r.nom.trim(), r.role, r.compte_numero.trim(),
            vide(&c.domaine), vide(&c.categorie_id), vide(&c.article_id),
            vide(&c.nature_comptable), vide(&c.tiers_id), vide(&c.nature_tiers),
            vide(&c.caisse_id), vide(&c.moyen_paiement_id), vide(&c.famille_paiement),
            vide(&c.depot_id), c.taux_taxe, c.montant_min, c.montant_max,
            vide(&c.libelle_contient), vide(&r.journal_code), r.ordre,
            r.actif as i64, vide(&r.note), par, now()
        ],
    )?;
    lire_regle(conn, &id)
}

pub fn modifier_regle(conn: &Connection, id: &str, r: &NouvelleRegle) -> Result<Regle> {
    lire_regle(conn, id)?;
    valider_regle(conn, r)?;
    let c = &r.criteres;
    conn.execute(
        "UPDATE regle_comptable SET
            nom = ?2, role = ?3, compte_numero = ?4, domaine = ?5, categorie_id = ?6,
            article_id = ?7, nature_comptable = ?8, tiers_id = ?9, nature_tiers = ?10,
            caisse_id = ?11, moyen_paiement_id = ?12, famille_paiement = ?13,
            depot_id = ?14, taux_taxe = ?15, montant_min = ?16, montant_max = ?17,
            libelle_contient = ?18, journal_code = ?19, ordre = ?20, actif = ?21, note = ?22
         WHERE id = ?1",
        params![
            id, r.nom.trim(), r.role, r.compte_numero.trim(),
            vide(&c.domaine), vide(&c.categorie_id), vide(&c.article_id),
            vide(&c.nature_comptable), vide(&c.tiers_id), vide(&c.nature_tiers),
            vide(&c.caisse_id), vide(&c.moyen_paiement_id), vide(&c.famille_paiement),
            vide(&c.depot_id), c.taux_taxe, c.montant_min, c.montant_max,
            vide(&c.libelle_contient), vide(&r.journal_code), r.ordre,
            r.actif as i64, vide(&r.note)
        ],
    )?;
    lire_regle(conn, id)
}

/// Une règle se supprime librement : elle ne porte aucun historique. Les
/// écritures déjà produites restent — elles sont l'historique, justement.
pub fn supprimer_regle(conn: &Connection, id: &str) -> Result<()> {
    lire_regle(conn, id)?;
    conn.execute("DELETE FROM regle_comptable WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn supprimer_regles(conn: &Connection, ids: &[String]) -> Result<usize> {
    let mut n = 0;
    for id in ids {
        if supprimer_regle(conn, id).is_ok() {
            n += 1;
        }
    }
    Ok(n)
}

// ===========================================================================
// Le moteur — résolution d'un compte pour un rôle et un contexte
// ===========================================================================

/// Ce que le moteur sait de l'opération au moment de chercher un compte.
/// Chaque champ est confronté au critère homonyme de la règle.
#[derive(Debug, Clone, Default)]
pub struct Contexte {
    pub domaine: Option<String>,
    pub categorie_id: Option<String>,
    pub article_id: Option<String>,
    pub nature_comptable: Option<String>,
    pub tiers_id: Option<String>,
    pub nature_tiers: Option<String>,
    pub caisse_id: Option<String>,
    pub moyen_paiement_id: Option<String>,
    pub famille_paiement: Option<String>,
    pub depot_id: Option<String>,
    pub taux_taxe: Option<f64>,
    pub montant: Option<f64>,
    pub libelle: Option<String>,
}

fn egal(critere: &Option<String>, valeur: &Option<String>) -> bool {
    match critere.as_deref() {
        None => true,                       // critère absent = « peu importe »
        Some(c) => valeur.as_deref() == Some(c),
    }
}

/// Une règle correspond si **tous** ses critères renseignés correspondent.
fn regle_correspond(r: &Regle, ctx: &Contexte) -> bool {
    let c = &r.criteres;
    if !egal(&c.domaine, &ctx.domaine)
        || !egal(&c.categorie_id, &ctx.categorie_id)
        || !egal(&c.article_id, &ctx.article_id)
        || !egal(&c.nature_comptable, &ctx.nature_comptable)
        || !egal(&c.tiers_id, &ctx.tiers_id)
        || !egal(&c.nature_tiers, &ctx.nature_tiers)
        || !egal(&c.caisse_id, &ctx.caisse_id)
        || !egal(&c.moyen_paiement_id, &ctx.moyen_paiement_id)
        || !egal(&c.famille_paiement, &ctx.famille_paiement)
        || !egal(&c.depot_id, &ctx.depot_id)
    {
        return false;
    }
    if let Some(t) = c.taux_taxe {
        match ctx.taux_taxe {
            Some(v) if (v - t).abs() < EPSILON => {}
            _ => return false,
        }
    }
    // Un critère de montant sur une opération sans montant ne correspond pas :
    // mieux vaut tomber dans la corbeille que ranger au hasard.
    if let Some(min) = c.montant_min {
        match ctx.montant {
            Some(v) if v >= min - EPSILON => {}
            _ => return false,
        }
    }
    if let Some(max) = c.montant_max {
        match ctx.montant {
            Some(v) if v <= max + EPSILON => {}
            _ => return false,
        }
    }
    if let Some(txt) = c.libelle_contient.as_deref() {
        let t = txt.trim().to_lowercase();
        if !t.is_empty() {
            match ctx.libelle.as_deref() {
                Some(l) if l.to_lowercase().contains(&t) => {}
                _ => return false,
            }
        }
    }
    true
}

/// Le jeu de règles, chargé une fois par lot de rattachement (et non par
/// opération : sur plusieurs milliers de pièces, la différence est nette).
pub struct Moteur {
    regles: Vec<Regle>,
}

impl Moteur {
    pub fn charger(conn: &Connection) -> Result<Self> {
        let regles = lister_regles(conn)?.into_iter().filter(|r| r.actif).collect();
        Ok(Self { regles })
    }

    /// Cherche le compte du rôle demandé. La règle **la plus spécifique** gagne
    /// (le plus grand nombre de critères), `ordre` départageant les ex æquo :
    /// le comptable peut ainsi écrire un défaut large puis des exceptions
    /// étroites, sans avoir à réfléchir à leur ordre.
    pub fn resoudre(&self, role: RoleCompte, ctx: &Contexte) -> Option<&Regle> {
        self.regles
            .iter()
            .filter(|r| r.role == role.as_str() && regle_correspond(r, ctx))
            .max_by_key(|r| (r.specificite, -r.ordre))
    }

    /// Compte du rôle, ou compte d'attente 471 accompagné d'une alerte.
    /// **Jamais d'erreur** : on ne perd pas une opération faute de paramétrage.
    fn compte(&self, role: RoleCompte, ctx: &Contexte, alertes: &mut Vec<String>) -> String {
        match self.resoudre(role, ctx) {
            Some(r) => r.compte_numero.clone(),
            None => {
                let a = format!(
                    "aucune règle pour le rôle « {} » : rangé en compte d'attente {COMPTE_ATTENTE}",
                    role.as_str()
                );
                if !alertes.contains(&a) {
                    alertes.push(a);
                }
                COMPTE_ATTENTE.to_string()
            }
        }
    }
}

// ===========================================================================
// Les écritures
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct LigneEcriture {
    pub id: String,
    pub compte_numero: String,
    pub compte_libelle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub libelle: Option<String>,
    pub debit: f64,
    pub credit: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiers_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiers_nom: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lettrage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Ecriture {
    pub id: String,
    pub journal_code: String,
    pub date: String,
    pub libelle: String,
    pub exercice: i64,
    pub origine_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub origine_id: Option<String>,
    /// Faux si une ligne pointe encore sur le compte d'attente.
    pub complete: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contrepasse_de: Option<String>,
    pub total_debit: f64,
    pub total_credit: f64,
    pub cree_le: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub lignes: Vec<LigneEcriture>,
}

/// Ligne en préparation, avant enregistrement.
#[derive(Debug, Clone)]
struct Projet {
    compte: String,
    libelle: Option<String>,
    debit: f64,
    credit: f64,
    tiers_id: Option<String>,
    role: RoleCompte,
}

impl Projet {
    fn debit(compte: String, montant: f64, role: RoleCompte) -> Self {
        Self { compte, libelle: None, debit: arrondir(montant), credit: 0.0, tiers_id: None, role }
    }
    fn credit(compte: String, montant: f64, role: RoleCompte) -> Self {
        Self { compte, libelle: None, debit: 0.0, credit: arrondir(montant), tiers_id: None, role }
    }
    fn avec_libelle(mut self, l: impl Into<String>) -> Self {
        self.libelle = Some(l.into());
        self
    }
    fn avec_tiers(mut self, t: Option<String>) -> Self {
        self.tiers_id = t;
        self
    }
}

/// Enregistre une écriture après avoir **vérifié son équilibre**. C'est le seul
/// refus d'écriture de tout Djigui, et il est délibéré : une écriture
/// déséquilibrée fausserait la balance sans que personne ne s'en aperçoive.
#[allow(clippy::too_many_arguments)]
fn enregistrer(
    conn: &Connection,
    journal: &str,
    date: &str,
    libelle: &str,
    origine_type: &str,
    origine_id: Option<&str>,
    lignes: Vec<Projet>,
    par: Option<&str>,
) -> Result<String> {
    let debit: f64 = lignes.iter().map(|l| l.debit).sum();
    let credit: f64 = lignes.iter().map(|l| l.credit).sum();
    if (debit - credit).abs() > EPSILON {
        return Err(CoreError::Rule(format!(
            "écriture déséquilibrée ({} au débit, {} au crédit) : {libelle}",
            arrondir(debit),
            arrondir(credit)
        )));
    }
    if lignes.is_empty() {
        return Err(CoreError::Rule(format!("écriture sans ligne : {libelle}")));
    }
    let complete = lignes.iter().all(|l| l.compte != COMPTE_ATTENTE);
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO ecriture (id, journal_code, date, libelle, exercice, origine_type,
                               origine_id, complete, cree_par, cree_le)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            id, journal, date, libelle, exercice_de(date), origine_type, origine_id,
            complete as i64, par, now()
        ],
    )?;
    for (i, l) in lignes.iter().enumerate() {
        conn.execute(
            "INSERT INTO ecriture_ligne
                (id, ecriture_id, compte_numero, libelle, debit, credit, tiers_id, role, ordre)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                Uuid::new_v4().to_string(), id, l.compte, l.libelle, l.debit, l.credit,
                l.tiers_id, l.role.as_str(), i as i64
            ],
        )?;
    }
    Ok(id)
}

const ECR_COLS: &str = "SELECT e.id, e.journal_code, e.date, e.libelle, e.exercice,
        e.origine_type, e.origine_id, e.complete, e.contrepasse_de,
        (SELECT COALESCE(SUM(l.debit), 0)  FROM ecriture_ligne l WHERE l.ecriture_id = e.id),
        (SELECT COALESCE(SUM(l.credit), 0) FROM ecriture_ligne l WHERE l.ecriture_id = e.id),
        e.cree_le
   FROM ecriture e";

fn ligne_ecriture(r: &Row) -> rusqlite::Result<Ecriture> {
    Ok(Ecriture {
        id: r.get(0)?,
        journal_code: r.get(1)?,
        date: r.get(2)?,
        libelle: r.get(3)?,
        exercice: r.get(4)?,
        origine_type: r.get(5)?,
        origine_id: r.get(6)?,
        complete: r.get::<_, i64>(7)? != 0,
        contrepasse_de: r.get(8)?,
        total_debit: arrondir(r.get(9)?),
        total_credit: arrondir(r.get(10)?),
        cree_le: r.get(11)?,
        lignes: Vec::new(),
    })
}

fn charger_lignes(conn: &Connection, ecriture_id: &str) -> Result<Vec<LigneEcriture>> {
    let mut st = conn.prepare(
        "SELECT l.id, l.compte_numero, c.libelle, l.libelle, l.debit, l.credit,
                l.tiers_id, t.nom, l.lettrage, l.role
           FROM ecriture_ligne l
           JOIN compte c ON c.numero = l.compte_numero
           LEFT JOIN tiers t ON t.id = l.tiers_id
          WHERE l.ecriture_id = ?1
          ORDER BY l.ordre",
    )?;
    let v = st
        .query_map(params![ecriture_id], |r| {
            Ok(LigneEcriture {
                id: r.get(0)?,
                compte_numero: r.get(1)?,
                compte_libelle: r.get(2)?,
                libelle: r.get(3)?,
                debit: arrondir(r.get(4)?),
                credit: arrondir(r.get(5)?),
                tiers_id: r.get(6)?,
                tiers_nom: r.get(7)?,
                lettrage: r.get(8)?,
                role: r.get(9)?,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

pub fn lire_ecriture(conn: &Connection, id: &str) -> Result<Ecriture> {
    let mut st = conn.prepare(&format!("{ECR_COLS} WHERE e.id = ?1"))?;
    let mut e = st
        .query_row(params![id], ligne_ecriture)
        .map_err(|_| CoreError::NotFound(format!("écriture {id}")))?;
    e.lignes = charger_lignes(conn, id)?;
    Ok(e)
}

/// Journal comptable : les écritures d'une période, dans l'ordre du temps.
pub fn lister_ecritures(
    conn: &Connection,
    du: Option<&str>,
    au: Option<&str>,
    journal: Option<&str>,
    incompletes_seulement: bool,
) -> Result<Vec<Ecriture>> {
    let sql = format!(
        "{ECR_COLS}
          WHERE (?1 IS NULL OR e.date >= ?1)
            AND (?2 IS NULL OR e.date <= ?2)
            AND (?3 IS NULL OR e.journal_code = ?3)
            AND (?4 = 0 OR e.complete = 0)
          ORDER BY e.date, e.cree_le"
    );
    let mut st = conn.prepare(&sql)?;
    let v = st
        .query_map(params![du, au, journal, incompletes_seulement as i64], ligne_ecriture)?
        .collect::<rusqlite::Result<Vec<_>>>()?;
    Ok(v)
}

/// **Contre-passation** : on n'efface jamais, on écrit l'inverse. La nouvelle
/// écriture porte la date du jour (ou celle fournie) : corriger une erreur ne
/// doit pas modifier le passé.
pub fn contrepasser(
    conn: &Connection,
    ecriture_id: &str,
    date: Option<&str>,
    motif: Option<&str>,
    par: Option<&str>,
) -> Result<Ecriture> {
    let src = lire_ecriture(conn, ecriture_id)?;
    let deja: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ecriture WHERE contrepasse_de = ?1",
        params![ecriture_id],
        |r| r.get(0),
    )?;
    if deja > 0 {
        return Err(CoreError::Rule("cette écriture est déjà contre-passée".into()));
    }
    let d = date.unwrap_or(&src.date).to_string();
    let d = if d.len() >= 10 { d[..10].to_string() } else { now()[..10].to_string() };
    let libelle = match motif {
        Some(m) if !m.trim().is_empty() => format!("Contre-passation — {} ({})", src.libelle, m.trim()),
        _ => format!("Contre-passation — {}", src.libelle),
    };
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO ecriture (id, journal_code, date, libelle, exercice, origine_type,
                               origine_id, complete, contrepasse_de, cree_par, cree_le)
         VALUES (?1, ?2, ?3, ?4, ?5, 'contrepassation', ?6, ?7, ?8, ?9, ?10)",
        params![
            id, src.journal_code, d, libelle, exercice_de(&d), src.origine_id,
            src.complete as i64, ecriture_id, par, now()
        ],
    )?;
    for (i, l) in src.lignes.iter().enumerate() {
        // Débit et crédit échangés : c'est toute la contre-passation.
        conn.execute(
            "INSERT INTO ecriture_ligne
                (id, ecriture_id, compte_numero, libelle, debit, credit, tiers_id, role, ordre)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                Uuid::new_v4().to_string(), id, l.compte_numero, l.libelle,
                l.credit, l.debit, l.tiers_id, l.role, i as i64
            ],
        )?;
    }
    lire_ecriture(conn, &id)
}

// ===========================================================================
// La corbeille « À ranger » — l'historique pas encore rattaché
// ===========================================================================

/// Une opération de gestion vue par le comptable : une facture, un encaissement.
/// C'est la ligne qu'il coche dans la corbeille.
#[derive(Debug, Clone, Serialize)]
pub struct Operation {
    /// `document` ou `paiement` — avec `id`, c'est la clé d'origine de l'écriture.
    pub origine_type: String,
    pub id: String,
    pub domaine: String,
    pub date: String,
    pub libelle: String,
    pub montant: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiers_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiers_nom: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caisse_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caisse_nom: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moyen_paiement_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub moyen_nom: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub famille_paiement: Option<String>,
    /// Vrai si une écriture existe déjà pour cette opération.
    pub rattachee: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ecriture_id: Option<String>,
    /// Vrai si l'écriture existante touche encore le compte d'attente.
    pub incomplete: bool,
}

/// Filtre de la corbeille — **la recherche multicritère**. C'est elle qui rend
/// le travail rapide (demande explicite de l'utilisateur) : le comptable isole
/// ce qu'il veut, coche tout, affecte en un geste, et peut transformer sa
/// sélection en règle permanente.
#[derive(Debug, Clone, Deserialize)]
pub struct FiltreOperations {
    #[serde(default)]
    pub du: Option<String>,
    #[serde(default)]
    pub au: Option<String>,
    #[serde(default)]
    pub domaine: Option<String>,
    #[serde(default)]
    pub tiers_id: Option<String>,
    #[serde(default)]
    pub caisse_id: Option<String>,
    #[serde(default)]
    pub moyen_paiement_id: Option<String>,
    #[serde(default)]
    pub famille_paiement: Option<String>,
    #[serde(default)]
    pub depot_id: Option<String>,
    /// Filtres portant sur le contenu des lignes du document.
    #[serde(default)]
    pub categorie_id: Option<String>,
    #[serde(default)]
    pub article_id: Option<String>,
    #[serde(default)]
    pub nature_comptable: Option<String>,
    #[serde(default)]
    pub montant_min: Option<f64>,
    #[serde(default)]
    pub montant_max: Option<f64>,
    /// Recherche libre : numéro de pièce, nom du tiers.
    #[serde(default)]
    pub texte: Option<String>,
    /// Ne montrer que ce qui reste à ranger (défaut : vrai).
    #[serde(default = "vrai")]
    pub a_ranger_seulement: bool,
}

/// ⚠️ `Default` est écrit à la main **exprès**. Le `#[serde(default = "vrai")]`
/// ci-dessus ne vaut qu'à la désérialisation : un `Default` dérivé aurait donné
/// `a_ranger_seulement = false`, et les appels internes en `..Default::default()`
/// se seraient comportés autrement que ceux venant de l'API. Les deux chemins
/// doivent dire la même chose.
impl Default for FiltreOperations {
    fn default() -> Self {
        Self {
            du: None,
            au: None,
            domaine: None,
            tiers_id: None,
            caisse_id: None,
            moyen_paiement_id: None,
            famille_paiement: None,
            depot_id: None,
            categorie_id: None,
            article_id: None,
            nature_comptable: None,
            montant_min: None,
            montant_max: None,
            texte: None,
            a_ranger_seulement: true,
        }
    }
}

/// Documents concernés : uniquement les pièces **validées** qui constatent une
/// créance ou une dette. Les brouillons ne sont pas des faits comptables, et
/// une commande transformée ne doit surtout pas être comptée en plus de la
/// facture qu'elle est devenue (double comptage).
const TYPES_COMPTABLES: &str = "('facture','avoir')";

pub fn lister_operations(conn: &Connection, f: &FiltreOperations) -> Result<Vec<Operation>> {
    let mut ops = Vec::new();
    let texte = f.texte.as_deref().map(|t| format!("%{}%", t.trim().to_lowercase()));

    // --- Documents (ventes et achats) --------------------------------------
    let veut_docs = match f.domaine.as_deref() {
        None => true,
        Some("vente") | Some("achat") => true,
        _ => false,
    };
    // Un filtre portant sur un moyen de paiement ou une caisse ne concerne que
    // les règlements : inutile de parcourir les documents.
    let veut_docs = veut_docs
        && f.caisse_id.is_none()
        && f.moyen_paiement_id.is_none()
        && f.famille_paiement.is_none();

    if veut_docs {
        let sql = format!(
            "SELECT d.id, d.sens, d.date, d.numero, d.type_document, d.total_ttc,
                    d.tiers_id, t.nom,
                    e.id, COALESCE(e.complete, 1)
               FROM document d
               LEFT JOIN tiers t ON t.id = d.tiers_id
               LEFT JOIN ecriture e ON e.origine_type = 'document' AND e.origine_id = d.id
              WHERE d.statut = 'valide'
                AND d.type_document IN {TYPES_COMPTABLES}
                AND (?1 IS NULL OR d.date >= ?1)
                AND (?2 IS NULL OR d.date <= ?2)
                AND (?3 IS NULL OR d.sens = ?3)
                AND (?4 IS NULL OR d.tiers_id = ?4)
                AND (?5 IS NULL OR d.depot_id = ?5)
                AND (?6 IS NULL OR d.total_ttc >= ?6)
                AND (?7 IS NULL OR d.total_ttc <= ?7)
                AND (?8 IS NULL OR lower(d.numero) LIKE ?8 OR lower(COALESCE(t.nom,'')) LIKE ?8)
                AND (?9 IS NULL OR EXISTS (
                        SELECT 1 FROM document_ligne dl JOIN article a ON a.id = dl.article_id
                         WHERE dl.document_id = d.id AND a.categorie_id = ?9))
                AND (?10 IS NULL OR EXISTS (
                        SELECT 1 FROM document_ligne dl
                         WHERE dl.document_id = d.id AND dl.article_id = ?10))
                AND (?11 IS NULL OR EXISTS (
                        SELECT 1 FROM document_ligne dl JOIN article a ON a.id = dl.article_id
                         WHERE dl.document_id = d.id AND a.nature_comptable = ?11))
                AND (?12 = 0 OR e.id IS NULL OR e.complete = 0)
              ORDER BY d.date, d.numero"
        );
        let mut st = conn.prepare(&sql)?;
        let rows = st.query_map(
            params![
                f.du, f.au, f.domaine, f.tiers_id, f.depot_id, f.montant_min, f.montant_max,
                texte, f.categorie_id, f.article_id, f.nature_comptable,
                f.a_ranger_seulement as i64
            ],
            |r| {
                let sens: String = r.get(1)?;
                let numero: String = r.get(3)?;
                let type_doc: String = r.get(4)?;
                let tiers_nom: Option<String> = r.get(7)?;
                let ecriture_id: Option<String> = r.get(8)?;
                Ok(Operation {
                    origine_type: "document".into(),
                    id: r.get(0)?,
                    domaine: sens.clone(),
                    date: r.get(2)?,
                    libelle: match &tiers_nom {
                        Some(n) => format!("{type_doc} {numero} — {n}"),
                        None => format!("{type_doc} {numero}"),
                    },
                    montant: arrondir(r.get(5)?),
                    tiers_id: r.get(6)?,
                    tiers_nom,
                    caisse_id: None,
                    caisse_nom: None,
                    moyen_paiement_id: None,
                    moyen_nom: None,
                    famille_paiement: None,
                    incomplete: ecriture_id.is_some() && r.get::<_, i64>(9)? == 0,
                    rattachee: ecriture_id.is_some(),
                    ecriture_id,
                })
            },
        )?;
        for o in rows {
            ops.push(o?);
        }
    }

    // --- Paiements (encaissements et décaissements) ------------------------
    let veut_paie = match f.domaine.as_deref() {
        None => true,
        Some("encaissement") | Some("decaissement") => true,
        _ => false,
    };
    // Idem : un filtre sur le contenu des articles ne concerne pas un règlement.
    let veut_paie = veut_paie
        && f.categorie_id.is_none()
        && f.article_id.is_none()
        && f.nature_comptable.is_none()
        && f.depot_id.is_none();

    if veut_paie {
        let sql = "SELECT p.id, p.sens, p.date, p.montant, p.tiers_id, t.nom,
                          p.caisse_id, ca.nom, p.moyen_paiement_id, mp.nom,
                          COALESCE(mp.famille, p.mode), d.numero,
                          e.id, COALESCE(e.complete, 1)
                     FROM paiement p
                     LEFT JOIN tiers t ON t.id = p.tiers_id
                     LEFT JOIN caisse ca ON ca.id = p.caisse_id
                     LEFT JOIN moyen_paiement mp ON mp.id = p.moyen_paiement_id
                     LEFT JOIN document d ON d.id = p.document_id
                     LEFT JOIN ecriture e ON e.origine_type = 'paiement' AND e.origine_id = p.id
                    WHERE (?1 IS NULL OR p.date >= ?1)
                      AND (?2 IS NULL OR p.date <= ?2)
                      AND (?3 IS NULL OR p.sens = ?3)
                      AND (?4 IS NULL OR p.tiers_id = ?4)
                      AND (?5 IS NULL OR p.caisse_id = ?5)
                      AND (?6 IS NULL OR p.moyen_paiement_id = ?6)
                      AND (?7 IS NULL OR COALESCE(mp.famille, p.mode) = ?7)
                      AND (?8 IS NULL OR p.montant >= ?8)
                      AND (?9 IS NULL OR p.montant <= ?9)
                      AND (?10 IS NULL OR lower(COALESCE(d.numero,'')) LIKE ?10
                                       OR lower(COALESCE(t.nom,'')) LIKE ?10)
                      AND (?11 = 0 OR e.id IS NULL OR e.complete = 0)
                    ORDER BY p.date";
        let mut st = conn.prepare(sql)?;
        let rows = st.query_map(
            params![
                f.du, f.au, f.domaine, f.tiers_id, f.caisse_id, f.moyen_paiement_id,
                f.famille_paiement, f.montant_min, f.montant_max, texte,
                f.a_ranger_seulement as i64
            ],
            |r| {
                let sens: String = r.get(1)?;
                let tiers_nom: Option<String> = r.get(5)?;
                let moyen: Option<String> = r.get(9)?;
                let num_doc: Option<String> = r.get(11)?;
                let ecriture_id: Option<String> = r.get(12)?;
                let quoi = if sens == "encaissement" { "Encaissement" } else { "Décaissement" };
                let mut libelle = quoi.to_string();
                if let Some(n) = &num_doc {
                    libelle.push_str(&format!(" {n}"));
                }
                if let Some(n) = &tiers_nom {
                    libelle.push_str(&format!(" — {n}"));
                }
                if let Some(m) = &moyen {
                    libelle.push_str(&format!(" ({m})"));
                }
                Ok(Operation {
                    origine_type: "paiement".into(),
                    id: r.get(0)?,
                    domaine: sens,
                    date: r.get(2)?,
                    libelle,
                    montant: arrondir(r.get(3)?),
                    tiers_id: r.get(4)?,
                    tiers_nom,
                    caisse_id: r.get(6)?,
                    caisse_nom: r.get(7)?,
                    moyen_paiement_id: r.get(8)?,
                    moyen_nom: moyen,
                    famille_paiement: r.get(10)?,
                    incomplete: ecriture_id.is_some() && r.get::<_, i64>(13)? == 0,
                    rattachee: ecriture_id.is_some(),
                    ecriture_id,
                })
            },
        )?;
        for o in rows {
            ops.push(o?);
        }
    }

    ops.sort_by(|a, b| a.date.cmp(&b.date));
    Ok(ops)
}

// ===========================================================================
// Le rattachement — application des règles à l'historique
// ===========================================================================

#[derive(Debug, Clone, Default, Serialize)]
pub struct Rapport {
    /// Écritures créées.
    pub creees: usize,
    /// Opérations ignorées parce qu'elles étaient déjà rattachées.
    pub deja_rattachees: usize,
    /// Écritures créées mais touchant le compte d'attente : à compléter.
    pub incompletes: usize,
    /// Alertes de paramétrage, sans doublon. **Informatif, jamais bloquant.**
    pub alertes: Vec<String>,
}

impl Rapport {
    fn ajouter_alertes(&mut self, a: Vec<String>, contexte: &str) {
        for x in a {
            let msg = format!("{contexte} : {x}");
            if !self.alertes.contains(&msg) {
                self.alertes.push(msg);
            }
        }
    }
}

fn journal_pour(domaine: DomaineComptable, famille: Option<&str>) -> &'static str {
    match domaine {
        DomaineComptable::Vente => "VT",
        DomaineComptable::Achat => "AC",
        DomaineComptable::Stock => "ST",
        // Espèces → journal de caisse ; tout le reste transite par un compte
        // bancaire ou de monnaie électronique → journal de banque.
        DomaineComptable::Encaissement | DomaineComptable::Decaissement => match famille {
            Some("espece") | None => "CA",
            _ => "BQ",
        },
    }
}

/// Rattache un document (facture ou avoir, vente ou achat).
///
/// ```text
/// VENTE                         ACHAT
///   [tiers]   D  TTC              [charge] D  HT
///   [produit] C  HT               [taxe]   D  TVA
///   [taxe]    C  TVA              [tiers]  C  TTC
/// ```
/// Un **avoir** est l'inverse exact de la facture de même sens.
fn rattacher_document(
    conn: &Connection,
    moteur: &Moteur,
    document_id: &str,
    par: Option<&str>,
) -> Result<(String, Vec<String>)> {
    // Les totaux HT et TVA de l'entête ne servent qu'au contrôle : les lignes
    // font foi, et l'écart éventuel est porté au compte d'attente (voir plus bas).
    let (numero, type_doc, sens, date, tiers_id, _total_ht, _total_tva, total_ttc, depot_id): (
        String, String, String, String, Option<String>, f64, f64, f64, Option<String>,
    ) = conn.query_row(
        "SELECT numero, type_document, sens, date, tiers_id, total_ht, total_tva,
                total_ttc, depot_id
           FROM document WHERE id = ?1",
        params![document_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?,
                r.get(6)?, r.get(7)?, r.get(8)?)),
    ).map_err(|_| CoreError::NotFound(format!("document {document_id}")))?;

    let vente = sens == "vente";
    let domaine = if vente { DomaineComptable::Vente } else { DomaineComptable::Achat };
    // L'avoir renverse tout : ce qui était au débit passe au crédit.
    let signe = if type_doc == "avoir" { -1.0 } else { 1.0 };

    let (nature_tiers, tiers_nom): (Option<String>, Option<String>) = match &tiers_id {
        Some(t) => conn.query_row(
            "SELECT nature, nom FROM tiers WHERE id = ?1",
            params![t],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap_or((None, None)),
        None => (None, None),
    };

    let mut alertes = Vec::new();
    let mut lignes: Vec<Projet> = Vec::new();

    let ctx_base = Contexte {
        domaine: Some(domaine.as_str().to_string()),
        tiers_id: tiers_id.clone(),
        nature_tiers: nature_tiers.clone(),
        depot_id: depot_id.clone(),
        montant: Some(total_ttc),
        libelle: Some(numero.clone()),
        ..Default::default()
    };

    // --- Le tiers : la contrepartie, pour le TTC --------------------------
    let compte_tiers = moteur.compte(RoleCompte::Tiers, &ctx_base, &mut alertes);
    let montant_tiers = total_ttc * signe;
    lignes.push(
        if vente {
            Projet::debit(compte_tiers.clone(), montant_tiers.abs(), RoleCompte::Tiers)
        } else {
            Projet::credit(compte_tiers.clone(), montant_tiers.abs(), RoleCompte::Tiers)
        }
        .avec_libelle(tiers_nom.clone().unwrap_or_else(|| "Client de passage".into()))
        .avec_tiers(tiers_id.clone()),
    );
    // Un avoir inverse la ligne du tiers.
    if signe < 0.0 {
        let l = lignes.pop().unwrap();
        lignes.push(Projet { debit: l.credit, credit: l.debit, ..l });
    }

    // --- Les produits (ou charges) : ligne par ligne, groupées par compte --
    let mut st = conn.prepare(
        "SELECT dl.total_ligne_ht, dl.designation, a.id, a.categorie_id, a.nature_comptable
           FROM document_ligne dl
           LEFT JOIN article a ON a.id = dl.article_id
          WHERE dl.document_id = ?1
          ORDER BY dl.id",
    )?;
    let details = st
        .query_map(params![document_id], |r| {
            Ok((
                r.get::<_, f64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, Option<String>>(3)?,
                r.get::<_, Option<String>>(4)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let role_resultat = if vente { RoleCompte::Produit } else { RoleCompte::Charge };
    let mut par_compte: Vec<(String, f64)> = Vec::new();
    let mut somme_ht = 0.0;
    for (ht, _designation, article_id, categorie_id, nature) in &details {
        let ctx = Contexte {
            article_id: article_id.clone(),
            categorie_id: categorie_id.clone(),
            nature_comptable: nature.clone(),
            montant: Some(*ht),
            ..ctx_base.clone()
        };
        let compte = moteur.compte(role_resultat, &ctx, &mut alertes);
        somme_ht += *ht;
        match par_compte.iter_mut().find(|(c, _)| *c == compte) {
            Some((_, m)) => *m += *ht,
            None => par_compte.push((compte, *ht)),
        }
    }
    for (compte, montant) in par_compte {
        let m = montant * signe;
        lignes.push(if (vente && m >= 0.0) || (!vente && m < 0.0) {
            Projet::credit(compte, m.abs(), role_resultat)
        } else {
            Projet::debit(compte, m.abs(), role_resultat)
        });
    }

    // --- Les taxes : groupées par compte, d'après le détail figé -----------
    let mut st = conn.prepare(
        "SELECT lt.nom, lt.taux, SUM(lt.montant)
           FROM document_ligne_taxe lt
           JOIN document_ligne dl ON dl.id = lt.ligne_id
          WHERE dl.document_id = ?1
          GROUP BY lt.nom, lt.taux",
    )?;
    let taxes = st
        .query_map(params![document_id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, f64>(1)?, r.get::<_, f64>(2)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut somme_taxes = 0.0;
    let mut taxes_par_compte: Vec<(String, f64)> = Vec::new();
    for (nom, taux, montant) in &taxes {
        if montant.abs() < EPSILON {
            continue; // taxe à 0 % (client exonéré) : rien à écrire
        }
        let ctx = Contexte {
            taux_taxe: Some(*taux),
            libelle: Some(nom.clone()),
            montant: Some(*montant),
            ..ctx_base.clone()
        };
        let compte = moteur.compte(RoleCompte::Taxe, &ctx, &mut alertes);
        somme_taxes += *montant;
        match taxes_par_compte.iter_mut().find(|(c, _)| *c == compte) {
            Some((_, m)) => *m += *montant,
            None => taxes_par_compte.push((compte, *montant)),
        }
    }
    for (compte, montant) in taxes_par_compte {
        let m = montant * signe;
        lignes.push(if (vente && m >= 0.0) || (!vente && m < 0.0) {
            Projet::credit(compte, m.abs(), RoleCompte::Taxe)
        } else {
            Projet::debit(compte, m.abs(), RoleCompte::Taxe)
        });
    }

    // --- L'écart d'arrondi ------------------------------------------------
    // Les totaux du document font foi. S'ils ne retombent pas exactement sur la
    // somme des lignes (arrondis de TVA), on porte la différence au compte
    // d'attente PLUTÔT que de la dissimuler : le comptable la verra et tranchera.
    let debit: f64 = lignes.iter().map(|l| l.debit).sum();
    let credit: f64 = lignes.iter().map(|l| l.credit).sum();
    let ecart = arrondir(debit - credit);
    if ecart.abs() > EPSILON {
        alertes.push(format!(
            "écart d'arrondi de {ecart} porté au compte d'attente {COMPTE_ATTENTE} \
             (HT {}, taxes {}, TTC {total_ttc})",
            arrondir(somme_ht),
            arrondir(somme_taxes)
        ));
        lignes.push(
            if ecart > 0.0 {
                Projet::credit(COMPTE_ATTENTE.into(), ecart.abs(), role_resultat)
            } else {
                Projet::debit(COMPTE_ATTENTE.into(), ecart.abs(), role_resultat)
            }
            .avec_libelle("Écart d'arrondi"),
        );
    }

    let ctx_journal = ctx_base.clone();
    let journal = moteur
        .resoudre(RoleCompte::Tiers, &ctx_journal)
        .and_then(|r| r.journal_code.clone())
        .unwrap_or_else(|| journal_pour(domaine, None).to_string());

    let libelle = match &tiers_nom {
        Some(n) => format!("{type_doc} {numero} — {n}"),
        None => format!("{type_doc} {numero}"),
    };
    let id = enregistrer(conn, &journal, &date, &libelle, "document", Some(document_id), lignes, par)?;
    Ok((id, alertes))
}

/// Rattache un règlement.
///
/// ```text
/// ENCAISSEMENT                  DÉCAISSEMENT
///   [tresorerie] D  montant       [tiers]      D  montant
///   [tiers]      C  montant       [tresorerie] C  montant
/// ```
fn rattacher_paiement(
    conn: &Connection,
    moteur: &Moteur,
    paiement_id: &str,
    par: Option<&str>,
) -> Result<(String, Vec<String>)> {
    let (sens, date, montant, tiers_id, caisse_id, moyen_id, famille, num_doc): (
        String, String, f64, Option<String>, Option<String>, Option<String>,
        Option<String>, Option<String>,
    ) = conn.query_row(
        "SELECT p.sens, p.date, p.montant, p.tiers_id, p.caisse_id, p.moyen_paiement_id,
                COALESCE(mp.famille, p.mode), d.numero
           FROM paiement p
           LEFT JOIN moyen_paiement mp ON mp.id = p.moyen_paiement_id
           LEFT JOIN document d ON d.id = p.document_id
          WHERE p.id = ?1",
        params![paiement_id],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?,
                r.get(6)?, r.get(7)?)),
    ).map_err(|_| CoreError::NotFound(format!("paiement {paiement_id}")))?;

    let encaissement = sens == "encaissement";
    let domaine = if encaissement {
        DomaineComptable::Encaissement
    } else {
        DomaineComptable::Decaissement
    };

    let (nature_tiers, tiers_nom): (Option<String>, Option<String>) = match &tiers_id {
        Some(t) => conn.query_row(
            "SELECT nature, nom FROM tiers WHERE id = ?1",
            params![t],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ).unwrap_or((None, None)),
        None => (None, None),
    };

    let date_courte = if date.len() >= 10 { date[..10].to_string() } else { date.clone() };
    let mut libelle = if encaissement { "Encaissement".to_string() } else { "Décaissement".to_string() };
    if let Some(n) = &num_doc {
        libelle.push_str(&format!(" {n}"));
    }
    if let Some(n) = &tiers_nom {
        libelle.push_str(&format!(" — {n}"));
    }

    let ctx = Contexte {
        domaine: Some(domaine.as_str().to_string()),
        tiers_id: tiers_id.clone(),
        nature_tiers,
        caisse_id: caisse_id.clone(),
        moyen_paiement_id: moyen_id.clone(),
        famille_paiement: famille.clone(),
        montant: Some(montant),
        libelle: Some(libelle.clone()),
        ..Default::default()
    };

    let mut alertes = Vec::new();
    let compte_tresorerie = moteur.compte(RoleCompte::Tresorerie, &ctx, &mut alertes);
    let compte_tiers = moteur.compte(RoleCompte::Tiers, &ctx, &mut alertes);

    let lignes = if encaissement {
        vec![
            Projet::debit(compte_tresorerie, montant, RoleCompte::Tresorerie),
            Projet::credit(compte_tiers, montant, RoleCompte::Tiers)
                .avec_tiers(tiers_id.clone()),
        ]
    } else {
        vec![
            Projet::debit(compte_tiers, montant, RoleCompte::Tiers).avec_tiers(tiers_id.clone()),
            Projet::credit(compte_tresorerie, montant, RoleCompte::Tresorerie),
        ]
    };

    let journal = moteur
        .resoudre(RoleCompte::Tresorerie, &ctx)
        .and_then(|r| r.journal_code.clone())
        .unwrap_or_else(|| journal_pour(domaine, famille.as_deref()).to_string());

    let id = enregistrer(
        conn, &journal, &date_courte, &libelle, "paiement", Some(paiement_id), lignes, par,
    )?;
    Ok((id, alertes))
}

/// Une opération à rattacher, désignée par son origine.
#[derive(Debug, Clone, Deserialize)]
pub struct RefOperation {
    pub origine_type: String,
    pub id: String,
}

/// Applique les règles aux opérations désignées. C'est le geste central de
/// l'écran : le comptable coche, il clique, tout se range.
///
/// Une opération déjà rattachée est **ignorée** (jamais rattachée deux fois) —
/// l'index unique sur `(origine_type, origine_id)` est la ceinture, ce test la
/// bretelle, et le comptable voit le compte dans le rapport.
pub fn rattacher(
    conn: &Connection,
    ops: &[RefOperation],
    par: Option<&str>,
) -> Result<Rapport> {
    let moteur = Moteur::charger(conn)?;
    let mut rapport = Rapport::default();

    for op in ops {
        let deja: i64 = conn.query_row(
            "SELECT COUNT(*) FROM ecriture
              WHERE origine_type = ?1 AND origine_id = ?2 AND origine_type <> 'contrepassation'",
            params![op.origine_type, op.id],
            |r| r.get(0),
        )?;
        if deja > 0 {
            rapport.deja_rattachees += 1;
            continue;
        }
        let resultat = match op.origine_type.as_str() {
            "document" => rattacher_document(conn, &moteur, &op.id, par),
            "paiement" => rattacher_paiement(conn, &moteur, &op.id, par),
            autre => Err(CoreError::Rule(format!("origine inconnue : {autre}"))),
        };
        match resultat {
            Ok((id, alertes)) => {
                rapport.creees += 1;
                let e = lire_ecriture(conn, &id)?;
                if !e.complete {
                    rapport.incompletes += 1;
                }
                rapport.ajouter_alertes(alertes, &e.libelle);
            }
            // Une opération qui échoue ne doit pas faire échouer le lot : on la
            // signale et on continue. Le comptable tranchera.
            Err(e) => rapport.alertes.push(format!("opération {} non rattachée : {e}", op.id)),
        }
    }
    Ok(rapport)
}

/// Rattache **tout** ce que le filtre désigne — le geste « ranger tout
/// l'historique » une fois les règles écrites.
pub fn rattacher_selon(
    conn: &Connection,
    f: &FiltreOperations,
    par: Option<&str>,
) -> Result<Rapport> {
    let mut filtre = f.clone();
    filtre.a_ranger_seulement = true;
    let ops: Vec<RefOperation> = lister_operations(conn, &filtre)?
        .into_iter()
        .filter(|o| !o.rattachee)
        .map(|o| RefOperation { origine_type: o.origine_type, id: o.id })
        .collect();
    rattacher(conn, &ops, par)
}

/// **Rejouer** une écriture après avoir corrigé ou ajouté une règle.
///
/// Le cas est fréquent et il faut le rendre facile : le comptable range son
/// historique, découvre dans la corbeille qu'il a oublié une règle (les achats,
/// typiquement), l'écrit, et veut que Djigui recommence.
///
/// ⚠️ **Uniquement sur une écriture incomplète** (qui touche encore le compte
/// d'attente). Ce n'est pas une entorse à la règle « on n'efface jamais » : une
/// écriture en 471 n'est pas un enregistrement comptable, c'est un brouillon que
/// le comptable n'a pas fini de ranger. Dès qu'elle est complète, elle devient
/// intouchable et seule la contre-passation s'applique.
pub fn rejouer(conn: &Connection, ecriture_id: &str, par: Option<&str>) -> Result<Rapport> {
    let e = lire_ecriture(conn, ecriture_id)?;
    if e.complete {
        return Err(CoreError::Rule(
            "cette écriture est complète : elle ne se rejoue pas, elle se contre-passe".into(),
        ));
    }
    let contrepassee: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ecriture WHERE contrepasse_de = ?1",
        params![ecriture_id],
        |r| r.get(0),
    )?;
    if contrepassee > 0 {
        return Err(CoreError::Rule("cette écriture est déjà contre-passée".into()));
    }
    let (origine_type, origine_id) = match (&e.origine_type, &e.origine_id) {
        (t, Some(id)) if t != "manuel" => (t.clone(), id.clone()),
        _ => {
            return Err(CoreError::Rule(
                "cette écriture n'a pas de pièce d'origine : complétez-la à la main".into(),
            ))
        }
    };
    conn.execute("DELETE FROM ecriture_ligne WHERE ecriture_id = ?1", params![ecriture_id])?;
    conn.execute("DELETE FROM ecriture WHERE id = ?1", params![ecriture_id])?;
    rattacher(conn, &[RefOperation { origine_type, id: origine_id }], par)
}

/// Rejoue **toutes** les écritures restées en compte d'attente — le geste qui
/// suit l'ajout d'une règle manquante.
pub fn rejouer_incompletes(conn: &Connection, par: Option<&str>) -> Result<Rapport> {
    let incompletes = lister_ecritures(conn, None, None, None, true)?;
    let mut total = Rapport::default();
    for e in incompletes {
        if e.origine_id.is_none() || e.origine_type == "manuel" {
            continue;
        }
        match rejouer(conn, &e.id, par) {
            Ok(r) => {
                total.creees += r.creees;
                total.incompletes += r.incompletes;
                total.deja_rattachees += r.deja_rattachees;
                total.ajouter_alertes(r.alertes, "rejeu");
            }
            Err(err) => total.alertes.push(format!("écriture {} non rejouée : {err}", e.libelle)),
        }
    }
    Ok(total)
}

/// Affectation **manuelle** d'un compte à une ligne d'écriture — la sortie de
/// secours quand aucune règle ne convient. C'est le comptable qui tranche.
pub fn affecter_ligne(conn: &Connection, ligne_id: &str, compte_numero: &str) -> Result<()> {
    lire_compte(conn, compte_numero)?;
    let ecriture_id: String = conn
        .query_row(
            "SELECT ecriture_id FROM ecriture_ligne WHERE id = ?1",
            params![ligne_id],
            |r| r.get(0),
        )
        .map_err(|_| CoreError::NotFound(format!("ligne d'écriture {ligne_id}")))?;
    conn.execute(
        "UPDATE ecriture_ligne SET compte_numero = ?2 WHERE id = ?1",
        params![ligne_id, compte_numero],
    )?;
    // L'écriture redevient complète dès qu'aucune ligne ne traîne en 471.
    conn.execute(
        "UPDATE ecriture SET complete = NOT EXISTS (
             SELECT 1 FROM ecriture_ligne l
              WHERE l.ecriture_id = ?1 AND l.compte_numero = ?2)
         WHERE id = ?1",
        params![ecriture_id, COMPTE_ATTENTE],
    )?;
    Ok(())
}

// ===========================================================================
// Restitutions — grand livre et balance
// ===========================================================================

#[derive(Debug, Clone, Serialize)]
pub struct MouvementCompte {
    pub ecriture_id: String,
    pub ligne_id: String,
    pub date: String,
    pub journal_code: String,
    pub libelle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub libelle_ligne: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tiers_nom: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lettrage: Option<String>,
    pub debit: f64,
    pub credit: f64,
    /// Solde après cette ligne — c'est ce qui fait d'une liste un grand livre.
    pub solde: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct GrandLivre {
    pub compte: Compte,
    /// Solde à la veille de la période (report à nouveau).
    pub solde_initial: f64,
    pub mouvements: Vec<MouvementCompte>,
    pub total_debit: f64,
    pub total_credit: f64,
    pub solde_final: f64,
}

/// Grand livre d'un compte : ses mouvements sur la période, avec un solde qui
/// court. C'est ce que l'historique de Djigui ne sait pas faire aujourd'hui —
/// il est chronologique et cloisonné par objet, jamais trié par compte.
pub fn grand_livre(
    conn: &Connection,
    compte_numero: &str,
    du: Option<&str>,
    au: Option<&str>,
) -> Result<GrandLivre> {
    let compte = lire_compte(conn, compte_numero)?;

    let solde_initial: f64 = match du {
        Some(d) => conn.query_row(
            "SELECT COALESCE(SUM(l.debit - l.credit), 0)
               FROM ecriture_ligne l JOIN ecriture e ON e.id = l.ecriture_id
              WHERE l.compte_numero = ?1 AND e.date < ?2",
            params![compte_numero, d],
            |r| r.get(0),
        )?,
        None => 0.0,
    };

    let mut st = conn.prepare(
        "SELECT e.id, l.id, e.date, e.journal_code, e.libelle, l.libelle, t.nom,
                l.lettrage, l.debit, l.credit
           FROM ecriture_ligne l
           JOIN ecriture e ON e.id = l.ecriture_id
           LEFT JOIN tiers t ON t.id = l.tiers_id
          WHERE l.compte_numero = ?1
            AND (?2 IS NULL OR e.date >= ?2)
            AND (?3 IS NULL OR e.date <= ?3)
          ORDER BY e.date, e.cree_le, l.ordre",
    )?;
    let brut = st
        .query_map(params![compte_numero, du, au], |r| {
            Ok((
                r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
                r.get::<_, String>(3)?, r.get::<_, String>(4)?, r.get::<_, Option<String>>(5)?,
                r.get::<_, Option<String>>(6)?, r.get::<_, Option<String>>(7)?,
                r.get::<_, f64>(8)?, r.get::<_, f64>(9)?,
            ))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mut solde = solde_initial;
    let (mut total_debit, mut total_credit) = (0.0, 0.0);
    let mut mouvements = Vec::with_capacity(brut.len());
    for (ecriture_id, ligne_id, date, journal_code, libelle, libelle_ligne, tiers_nom, lettrage, debit, credit) in brut {
        solde += debit - credit;
        total_debit += debit;
        total_credit += credit;
        mouvements.push(MouvementCompte {
            ecriture_id, ligne_id, date, journal_code, libelle, libelle_ligne, tiers_nom,
            lettrage,
            debit: arrondir(debit),
            credit: arrondir(credit),
            solde: arrondir(solde),
        });
    }

    Ok(GrandLivre {
        compte,
        solde_initial: arrondir(solde_initial),
        mouvements,
        total_debit: arrondir(total_debit),
        total_credit: arrondir(total_credit),
        solde_final: arrondir(solde),
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct LigneBalance {
    pub numero: String,
    pub libelle: String,
    pub classe: Option<i64>,
    pub debit: f64,
    pub credit: f64,
    pub solde: f64,
    /// Vrai si le solde part dans le sens contraire de l'habitude du compte
    /// (un compte client créditeur, par exemple). **Signalement, pas erreur.**
    pub solde_anormal: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Balance {
    pub lignes: Vec<LigneBalance>,
    pub total_debit: f64,
    pub total_credit: f64,
    /// La balance doit **toujours** tomber juste. Si elle ne s'équilibre pas,
    /// c'est un vrai défaut — pas une souplesse — et l'écran l'affiche en rouge.
    pub equilibree: bool,
    /// Nombre d'écritures touchant encore le compte d'attente.
    pub nb_incompletes: i64,
}

pub fn balance(conn: &Connection, du: Option<&str>, au: Option<&str>) -> Result<Balance> {
    let mut st = conn.prepare(
        "SELECT c.numero, c.libelle, c.classe, c.sens_normal,
                COALESCE(SUM(l.debit), 0), COALESCE(SUM(l.credit), 0)
           FROM compte c
           JOIN ecriture_ligne l ON l.compte_numero = c.numero
           JOIN ecriture e ON e.id = l.ecriture_id
          WHERE (?1 IS NULL OR e.date >= ?1)
            AND (?2 IS NULL OR e.date <= ?2)
          GROUP BY c.numero, c.libelle, c.classe, c.sens_normal
          HAVING SUM(l.debit) <> 0 OR SUM(l.credit) <> 0
          ORDER BY c.numero",
    )?;
    let lignes = st
        .query_map(params![du, au], |r| {
            let sens: Option<String> = r.get(3)?;
            let debit: f64 = r.get(4)?;
            let credit: f64 = r.get(5)?;
            let solde = debit - credit;
            let anormal = match sens.as_deref() {
                Some("debit") => solde < -EPSILON,
                Some("credit") => solde > EPSILON,
                _ => false,
            };
            Ok(LigneBalance {
                numero: r.get(0)?,
                libelle: r.get(1)?,
                classe: r.get(2)?,
                debit: arrondir(debit),
                credit: arrondir(credit),
                solde: arrondir(solde),
                solde_anormal: anormal,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let total_debit: f64 = lignes.iter().map(|l| l.debit).sum();
    let total_credit: f64 = lignes.iter().map(|l| l.credit).sum();
    let nb_incompletes: i64 = conn.query_row(
        "SELECT COUNT(*) FROM ecriture WHERE complete = 0
          AND (?1 IS NULL OR date >= ?1) AND (?2 IS NULL OR date <= ?2)",
        params![du, au],
        |r| r.get(0),
    )?;

    Ok(Balance {
        lignes,
        total_debit: arrondir(total_debit),
        total_credit: arrondir(total_credit),
        equilibree: (total_debit - total_credit).abs() < EPSILON,
        nb_incompletes,
    })
}

// ===========================================================================
// Lettrage — rapprocher une facture de son règlement
// ===========================================================================

/// Pose le même code de lettrage sur plusieurs lignes. Le comptable choisit ce
/// qu'il rapproche : Djigui vérifie seulement que l'ensemble est cohérent
/// (même compte) et **signale** si les montants ne se compensent pas.
pub fn lettrer(conn: &Connection, lignes: &[String], code: Option<&str>) -> Result<String> {
    if lignes.len() < 2 {
        return Err(CoreError::Rule(
            "sélectionnez au moins deux lignes à rapprocher".into(),
        ));
    }
    // Seule vérification : toutes les lignes appartiennent au même compte. On ne
    // contrôle PAS que les montants se compensent — un lettrage partiel (un
    // acompte, un règlement en plusieurs fois) est parfaitement légitime.
    let mut comptes: Vec<String> = Vec::new();
    for id in lignes {
        let compte: String = conn
            .query_row(
                "SELECT compte_numero FROM ecriture_ligne WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .map_err(|_| CoreError::NotFound(format!("ligne d'écriture {id}")))?;
        if !comptes.contains(&compte) {
            comptes.push(compte);
        }
    }
    if comptes.len() > 1 {
        return Err(CoreError::Rule(
            "on ne rapproche que des lignes d'un même compte".into(),
        ));
    }
    let code = match code {
        Some(c) if !c.trim().is_empty() => c.trim().to_string(),
        _ => prochain_code_lettrage(conn, &comptes[0])?,
    };
    for id in lignes {
        conn.execute(
            "UPDATE ecriture_ligne SET lettrage = ?2 WHERE id = ?1",
            params![id, code],
        )?;
    }
    Ok(code)
}

pub fn delettrer(conn: &Connection, lignes: &[String]) -> Result<usize> {
    let mut n = 0;
    for id in lignes {
        n += conn.execute(
            "UPDATE ecriture_ligne SET lettrage = NULL WHERE id = ?1",
            params![id],
        )?;
    }
    Ok(n)
}

/// Codes A, B, … Z, AA, AB… par compte, comme dans les logiciels comptables.
fn prochain_code_lettrage(conn: &Connection, compte: &str) -> Result<String> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(DISTINCT lettrage) FROM ecriture_ligne
          WHERE compte_numero = ?1 AND lettrage IS NOT NULL",
        params![compte],
        |r| r.get(0),
    )?;
    let mut i = n;
    let mut code = String::new();
    loop {
        code.insert(0, (b'A' + (i % 26) as u8) as char);
        i = i / 26 - 1;
        if i < 0 {
            break;
        }
    }
    Ok(code)
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    /// Jeu d'essai minimal : un client, un article, une facture validée de
    /// 1 000 HT + 180 de TVA, et son encaissement en espèces.
    fn base_avec_vente() -> Connection {
        let conn = db::open_in_memory().unwrap();
        conn.execute_batch(
            "INSERT INTO tiers (id, code, type_role, nom, solde, actif, cree_le, nature)
                VALUES ('T1', 'CL001', 'client', 'Awa Ndiaye', 0, 1, '2026-07-01', 'particulier');
             INSERT INTO article (id, code, type, designation, prix_vente, taux_tva,
                                  gere_stock, actif, nature_comptable)
                VALUES ('A1', 'ART1', 'bien', 'Riz 1kg', 1000, 18, 1, 1, 'marchandise');
             INSERT INTO document (id, numero, type_document, sens, tiers_id, date, statut,
                                   total_ht, total_tva, total_ttc, cree_le)
                VALUES ('D1', 'FA-2026-0001', 'facture', 'vente', 'T1', '2026-07-10', 'valide',
                        1000, 180, 1180, '2026-07-10');
             INSERT INTO document_ligne (id, document_id, article_id, designation, quantite,
                                         prix_unitaire, taux_tva, remise, total_ligne_ht)
                VALUES ('DL1', 'D1', 'A1', 'Riz 1kg', 1, 1000, 18, 0, 1000);
             INSERT INTO document_ligne_taxe (id, ligne_id, nom, type, taux, montant)
                VALUES ('LT1', 'DL1', 'TVA 18 %', 'pourcentage', 18, 180);
             INSERT INTO caisse (id, nom, solde) VALUES ('C1', 'Caisse principale', 0);
             INSERT INTO paiement (id, tiers_id, caisse_id, document_id, sens, montant, mode, date)
                VALUES ('P1', 'T1', 'C1', 'D1', 'encaissement', 1180, 'espece', '2026-07-10');",
        )
        .unwrap();
        conn
    }

    /// Les règles que le comptable poserait : quatre lignes, et tout se range.
    fn poser_regles(conn: &Connection) {
        installer_plan_ohada(conn, None).unwrap();
        for (nom, role, compte, domaine) in [
            ("Ventes", "produit", "701", Some("vente")),
            ("Clients", "tiers", "411", None),
            ("TVA collectée", "taxe", "4431", Some("vente")),
            ("Caisse", "tresorerie", "571", None),
        ] {
            creer_regle(
                conn,
                &NouvelleRegle {
                    nom: nom.into(),
                    role: role.into(),
                    compte_numero: compte.into(),
                    criteres: Criteres {
                        domaine: domaine.map(|d| d.to_string()),
                        ..Default::default()
                    },
                    journal_code: None,
                    ordre: 0,
                    actif: true,
                    note: None,
                },
                None,
            )
            .unwrap();
        }
    }

    #[test]
    fn vente_et_encaissement_produisent_des_ecritures_equilibrees() {
        let conn = base_avec_vente();
        poser_regles(&conn);

        let rapport = rattacher_selon(&conn, &FiltreOperations::default(), None).unwrap();
        assert_eq!(rapport.creees, 2, "la facture et son encaissement");
        assert_eq!(rapport.incompletes, 0, "aucun compte d'attente");
        assert!(rapport.alertes.is_empty(), "{:?}", rapport.alertes);

        // La facture : 411 au débit 1180 ; 701 au crédit 1000 ; 4431 au crédit 180.
        let ecritures = lister_ecritures(&conn, None, None, None, false).unwrap();
        let facture = ecritures.iter().find(|e| e.journal_code == "VT").unwrap();
        assert_eq!(facture.total_debit, 1180.0);
        assert_eq!(facture.total_credit, 1180.0);

        let detail = lire_ecriture(&conn, &facture.id).unwrap();
        let cherche = |c: &str| detail.lignes.iter().find(|l| l.compte_numero == c).unwrap();
        assert_eq!(cherche("411").debit, 1180.0);
        assert_eq!(cherche("701").credit, 1000.0);
        assert_eq!(cherche("4431").credit, 180.0);

        // L'encaissement va bien au journal de caisse (espèces).
        let encaissement = ecritures.iter().find(|e| e.journal_code == "CA").unwrap();
        let detail = lire_ecriture(&conn, &encaissement.id).unwrap();
        assert_eq!(cherche_compte(&detail, "571").debit, 1180.0);
        assert_eq!(cherche_compte(&detail, "411").credit, 1180.0);

        // Et la balance tombe juste — c'est tout l'intérêt de la partie double.
        let b = balance(&conn, None, None).unwrap();
        assert!(b.equilibree, "{} vs {}", b.total_debit, b.total_credit);
        // Le compte client est soldé : facturé puis encaissé.
        let client = b.lignes.iter().find(|l| l.numero == "411").unwrap();
        assert_eq!(client.solde, 0.0);
    }

    fn cherche_compte<'a>(e: &'a Ecriture, c: &str) -> &'a LigneEcriture {
        e.lignes.iter().find(|l| l.compte_numero == c).unwrap()
    }

    #[test]
    fn sans_regle_tout_part_en_compte_dattente_mais_rien_nest_perdu() {
        let conn = base_avec_vente();
        // Aucune règle : le comptable n'a rien paramétré.
        let rapport = rattacher_selon(&conn, &FiltreOperations::default(), None).unwrap();
        assert_eq!(rapport.creees, 2, "on écrit quand même : rien ne se perd");
        assert_eq!(rapport.incompletes, 2);
        assert!(!rapport.alertes.is_empty(), "et on le signale");

        // Les écritures restent équilibrées, même entièrement en 471.
        let b = balance(&conn, None, None).unwrap();
        assert!(b.equilibree);
        assert_eq!(b.nb_incompletes, 2);
    }

    #[test]
    fn la_regle_la_plus_specifique_gagne() {
        let conn = base_avec_vente();
        installer_plan_ohada(&conn, None).unwrap();
        conn.execute(
            "INSERT INTO categorie (id, nom) VALUES ('CAT1', 'Alimentaire')",
            [],
        )
        .unwrap();
        conn.execute("UPDATE article SET categorie_id = 'CAT1' WHERE id = 'A1'", [])
            .unwrap();

        // Défaut large…
        creer_regle(&conn, &NouvelleRegle {
            nom: "Ventes (défaut)".into(), role: "produit".into(), compte_numero: "701".into(),
            criteres: Criteres { domaine: Some("vente".into()), ..Default::default() },
            journal_code: None, ordre: 0, actif: true, note: None,
        }, None).unwrap();
        // …puis une exception étroite, écrite APRÈS et avec un ordre plus grand :
        // elle doit tout de même l'emporter, sinon le comptable devrait réfléchir
        // à l'ordre de ses règles.
        creer_regle(&conn, &NouvelleRegle {
            nom: "Alimentaire".into(), role: "produit".into(), compte_numero: "702".into(),
            criteres: Criteres {
                domaine: Some("vente".into()),
                categorie_id: Some("CAT1".into()),
                ..Default::default()
            },
            journal_code: None, ordre: 99, actif: true, note: None,
        }, None).unwrap();

        let moteur = Moteur::charger(&conn).unwrap();
        let ctx = Contexte {
            domaine: Some("vente".into()),
            categorie_id: Some("CAT1".into()),
            ..Default::default()
        };
        assert_eq!(moteur.resoudre(RoleCompte::Produit, &ctx).unwrap().compte_numero, "702");

        // Un article d'une autre catégorie retombe sur le défaut.
        let autre = Contexte {
            domaine: Some("vente".into()),
            categorie_id: Some("CAT2".into()),
            ..Default::default()
        };
        assert_eq!(moteur.resoudre(RoleCompte::Produit, &autre).unwrap().compte_numero, "701");
    }

    #[test]
    fn une_operation_ne_se_rattache_jamais_deux_fois() {
        let conn = base_avec_vente();
        poser_regles(&conn);
        let r1 = rattacher_selon(&conn, &FiltreOperations::default(), None).unwrap();
        assert_eq!(r1.creees, 2);

        // Relancer ne doit RIEN créer : le double comptage est le pire risque.
        let ops = vec![
            RefOperation { origine_type: "document".into(), id: "D1".into() },
            RefOperation { origine_type: "paiement".into(), id: "P1".into() },
        ];
        let r2 = rattacher(&conn, &ops, None).unwrap();
        assert_eq!(r2.creees, 0);
        assert_eq!(r2.deja_rattachees, 2);

        let b = balance(&conn, None, None).unwrap();
        assert_eq!(b.total_debit, 2360.0, "1180 facturé + 1180 encaissé, pas le double");
    }

    #[test]
    fn contrepassation_annule_sans_effacer() {
        let conn = base_avec_vente();
        poser_regles(&conn);
        rattacher_selon(&conn, &FiltreOperations::default(), None).unwrap();
        let ecritures = lister_ecritures(&conn, None, None, Some("VT"), false).unwrap();
        let facture = &ecritures[0];

        let inverse = contrepasser(&conn, &facture.id, None, Some("erreur de saisie"), None).unwrap();
        assert_eq!(inverse.total_debit, facture.total_credit);
        assert_eq!(inverse.contrepasse_de.as_deref(), Some(facture.id.as_str()));

        // L'écriture d'origine est toujours là : on n'efface jamais.
        assert!(lire_ecriture(&conn, &facture.id).is_ok());
        // Et le compte de vente est soldé.
        let gl = grand_livre(&conn, "701", None, None).unwrap();
        assert_eq!(gl.solde_final, 0.0);
        assert_eq!(gl.mouvements.len(), 2);

        // Deux fois, non.
        assert!(contrepasser(&conn, &facture.id, None, None, None).is_err());
    }

    #[test]
    fn corbeille_a_ranger_et_recherche_multicritere() {
        let conn = base_avec_vente();
        // Tout est à ranger au départ.
        let tout = lister_operations(&conn, &FiltreOperations::default()).unwrap();
        assert_eq!(tout.len(), 2);
        assert!(tout.iter().all(|o| !o.rattachee));

        // Filtre par domaine.
        let ventes = lister_operations(&conn, &FiltreOperations {
            domaine: Some("vente".into()), ..Default::default()
        }).unwrap();
        assert_eq!(ventes.len(), 1);
        assert_eq!(ventes[0].origine_type, "document");

        // Filtre par montant et par texte libre (numéro de pièce).
        let par_texte = lister_operations(&conn, &FiltreOperations {
            texte: Some("FA-2026".into()), ..Default::default()
        }).unwrap();
        assert_eq!(par_texte.len(), 2, "la facture et son encaissement rattaché au document");

        let trop_cher = lister_operations(&conn, &FiltreOperations {
            montant_min: Some(5000.0), ..Default::default()
        }).unwrap();
        assert!(trop_cher.is_empty());

        // Une fois rangé, la corbeille se vide.
        poser_regles(&conn);
        rattacher_selon(&conn, &FiltreOperations::default(), None).unwrap();
        let reste = lister_operations(&conn, &FiltreOperations::default()).unwrap();
        assert!(reste.is_empty());
    }

    #[test]
    fn grand_livre_donne_un_solde_qui_court() {
        let conn = base_avec_vente();
        poser_regles(&conn);
        rattacher_selon(&conn, &FiltreOperations::default(), None).unwrap();

        let gl = grand_livre(&conn, "411", None, None).unwrap();
        assert_eq!(gl.mouvements.len(), 2);
        assert_eq!(gl.mouvements[0].solde, 1180.0, "facturé");
        assert_eq!(gl.mouvements[1].solde, 0.0, "puis encaissé");
        assert_eq!(gl.solde_final, 0.0);

        // Contrôle croisé gratuit : le solde du compte client doit retomber sur
        // le solde que le module paiement tient de son côté.
        let solde_tiers: f64 = conn
            .query_row("SELECT solde FROM tiers WHERE id = 'T1'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(gl.solde_final, solde_tiers);
    }

    #[test]
    fn lettrage_rapproche_facture_et_reglement() {
        let conn = base_avec_vente();
        poser_regles(&conn);
        rattacher_selon(&conn, &FiltreOperations::default(), None).unwrap();

        let gl = grand_livre(&conn, "411", None, None).unwrap();
        let ids: Vec<String> = gl.mouvements.iter().map(|m| m.ligne_id.clone()).collect();
        let code = lettrer(&conn, &ids, None).unwrap();
        assert_eq!(code, "A");

        let gl = grand_livre(&conn, "411", None, None).unwrap();
        assert!(gl.mouvements.iter().all(|m| m.lettrage.as_deref() == Some("A")));

        // On ne rapproche pas des lignes de comptes différents.
        let vente = grand_livre(&conn, "701", None, None).unwrap();
        let melange = vec![ids[0].clone(), vente.mouvements[0].ligne_id.clone()];
        assert!(lettrer(&conn, &melange, None).is_err());

        delettrer(&conn, &ids).unwrap();
        let gl = grand_livre(&conn, "411", None, None).unwrap();
        assert!(gl.mouvements.iter().all(|m| m.lettrage.is_none()));
    }

    #[test]
    fn un_compte_utilise_ne_se_supprime_pas() {
        let conn = base_avec_vente();
        poser_regles(&conn);
        rattacher_selon(&conn, &FiltreOperations::default(), None).unwrap();
        // Employé par une écriture.
        assert!(supprimer_compte(&conn, "701").is_err());
        // Et le compte d'attente est intouchable.
        assert!(supprimer_compte(&conn, COMPTE_ATTENTE).is_err());
        // Un compte libre, en revanche, se supprime.
        creer_compte(&conn, &NouveauCompte {
            numero: "999".into(), libelle: "Essai".into(), classe: None,
            sens_normal: None, lettrable: false, actif: true, note: None,
        }, None).unwrap();
        assert!(supprimer_compte(&conn, "999").is_ok());
    }

    #[test]
    fn ajouter_la_regle_manquante_puis_rejouer() {
        let conn = base_avec_vente();
        installer_plan_ohada(&conn, None).unwrap();
        // Le comptable oublie la règle des produits : la vente part en 471.
        for (nom, role, compte) in [
            ("Clients", "tiers", "411"),
            ("TVA", "taxe", "4431"),
            ("Caisse", "tresorerie", "571"),
        ] {
            creer_regle(&conn, &NouvelleRegle {
                nom: nom.into(), role: role.into(), compte_numero: compte.into(),
                criteres: Criteres::default(), journal_code: None, ordre: 0,
                actif: true, note: None,
            }, None).unwrap();
        }
        let r = rattacher_selon(&conn, &FiltreOperations::default(), None).unwrap();
        assert_eq!(r.incompletes, 1, "la facture, faute de compte de produit");

        // Il écrit la règle manquante…
        creer_regle(&conn, &NouvelleRegle {
            nom: "Ventes".into(), role: "produit".into(), compte_numero: "701".into(),
            criteres: Criteres { domaine: Some("vente".into()), ..Default::default() },
            journal_code: None, ordre: 0, actif: true, note: None,
        }, None).unwrap();

        // …et rejoue : plus rien en compte d'attente.
        let r = rejouer_incompletes(&conn, None).unwrap();
        assert_eq!(r.creees, 1);
        assert_eq!(r.incompletes, 0);
        let gl = grand_livre(&conn, "701", None, None).unwrap();
        assert_eq!(gl.solde_final, -1000.0, "1000 au crédit des ventes");
        assert_eq!(grand_livre(&conn, COMPTE_ATTENTE, None, None).unwrap().mouvements.len(), 0);

        // Toujours pas de double comptage après un rejeu.
        let b = balance(&conn, None, None).unwrap();
        assert!(b.equilibree);
        assert_eq!(b.lignes.iter().find(|l| l.numero == "411").unwrap().debit, 1180.0);

        // Et une écriture complète, elle, ne se rejoue plus : on la contre-passe.
        let e = &lister_ecritures(&conn, None, None, Some("VT"), false).unwrap()[0];
        assert!(rejouer(&conn, &e.id, None).is_err());
    }

    #[test]
    fn affectation_manuelle_complete_lecriture() {
        let conn = base_avec_vente();
        installer_plan_ohada(&conn, None).unwrap();
        // Aucune règle : tout tombe en 471.
        rattacher_selon(&conn, &FiltreOperations::default(), None).unwrap();
        let incompletes = lister_ecritures(&conn, None, None, None, true).unwrap();
        assert_eq!(incompletes.len(), 2);

        // Le comptable range à la main, ligne par ligne : c'est lui qui tranche.
        let e = lire_ecriture(&conn, &incompletes[0].id).unwrap();
        for (i, l) in e.lignes.iter().enumerate() {
            let compte = if l.debit > 0.0 { "411" } else if i == 1 { "701" } else { "4431" };
            affecter_ligne(&conn, &l.id, compte).unwrap();
        }
        let e = lire_ecriture(&conn, &incompletes[0].id).unwrap();
        assert!(e.complete, "l'écriture est complète dès que 471 est vidé");
    }
}
