//! Activation des modules (migration 0040).
//!
//! ⚠️ **Ce n'est pas un filtre d'affichage.** À l'installation, la formule
//! vendue détermine les modules auxquels le client a droit : c'est une **donnée
//! de facturation**. On doit pouvoir dire, des mois plus tard, ce qui a été
//! souscrit, quand et par qui.
//!
//! # Deux niveaux, à ne jamais confondre
//!
//! - **`souscrit`** — décidé par l'installateur selon la formule vendue. Le
//!   client n'y touche pas.
//! - **`actif`** — décidé par le client, parmi ce qu'il a souscrit. C'est du
//!   confort : masquer un module qu'il n'utilise pas encore allège son menu et
//!   ne change rien à ce qu'il paie.
//!
//! # Ce que la désactivation ne fait pas
//!
//! Elle ne touche à **aucune donnée**. Masquer « Marchés » cache le menu ; les
//! marchés restent en base et réapparaissent intacts à la réactivation. C'est
//! la règle du projet : on détache, on masque, on n'efface jamais.

use crate::error::{CoreError, Result};
use crate::now;
use rusqlite::{params, Connection, Row};
use serde::{Deserialize, Serialize};

/// Formules vendues : un préréglage de modules, ajustable avant validation.
/// Elles vivent dans le code et non en base : ce sont des **offres
/// commerciales**, elles changent avec le catalogue de vente, pas avec les
/// données d'un client.
pub const FORMULES: &[(&str, &str, &str, &[&str])] = &[
    (
        "commerce",
        "Commerce",
        "Le nécessaire d'un commerçant : vendre, encaisser, facturer, suivre son stock.",
        &["caisse", "facturation", "magasins", "agenda", "rapports"],
    ),
    (
        "commerce_plus",
        "Commerce +",
        "Tout Commerce, plus la fabrication et la comptabilité OHADA.",
        &["caisse", "facturation", "magasins", "agenda", "rapports",
          "production", "comptabilite", "abonnements"],
    ),
    (
        "ong_projets",
        "ONG / Projets",
        "Piloter des projets et passer des marchés, sans tenir de caisse.",
        &["projets", "marches", "agenda", "rapports"],
    ),
    (
        "complete",
        "Complète",
        "Tous les modules de Djigui.",
        &["caisse", "facturation", "abonnements", "magasins", "production",
          "projets", "marches", "agenda", "rapports", "comptabilite"],
    ),
    (
        "sur_mesure",
        "Sur mesure",
        "Aucun module présélectionné : cochez ceux qui sont vendus.",
        &[],
    ),
];

#[derive(Debug, Clone, Serialize)]
pub struct Formule {
    pub code: String,
    pub libelle: String,
    pub description: String,
    pub modules: Vec<String>,
}

pub fn formules() -> Vec<Formule> {
    FORMULES
        .iter()
        .map(|(c, l, d, m)| Formule {
            code: (*c).to_string(),
            libelle: (*l).to_string(),
            description: (*d).to_string(),
            modules: m.iter().map(|x| (*x).to_string()).collect(),
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct Module {
    pub code: String,
    pub libelle: String,
    pub description: String,
    pub icone: String,
    pub famille: String,
    pub ordre: i64,
    pub socle: bool,
    pub souscrit: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub souscrit_le: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub souscrit_par: Option<String>,
    pub actif: bool,
    /// Modules dont celui-ci a besoin pour fonctionner.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub requiert: Vec<String>,
    /// **Visible dans le menu** : souscrit ET activé. C'est la seule chose que
    /// l'interface a besoin de savoir pour construire la barre latérale.
    pub visible: bool,
    /// Ce que le module contient déjà. Sert à prévenir avant de le masquer :
    /// « 8 marchés seront conservés mais deviendront invisibles ».
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub contenu: Vec<Compte>,
}

/// Un décompte parlant : « 8 marchés », « 6 soumissionnaires ».
#[derive(Debug, Clone, Serialize)]
pub struct Compte {
    pub libelle: String,
    pub nb: i64,
}

fn ligne(r: &Row) -> rusqlite::Result<Module> {
    let souscrit: i64 = r.get(7)?;
    let actif: i64 = r.get(10)?;
    let socle: i64 = r.get(6)?;
    let requiert: Option<String> = r.get(11)?;
    Ok(Module {
        code: r.get(0)?,
        libelle: r.get(1)?,
        description: r.get(2)?,
        icone: r.get(3)?,
        famille: r.get(4)?,
        ordre: r.get(5)?,
        socle: socle != 0,
        souscrit: souscrit != 0,
        souscrit_le: r.get(8)?,
        souscrit_par: r.get(9)?,
        actif: actif != 0,
        requiert: requiert
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|x| !x.is_empty())
            .map(str::to_string)
            .collect(),
        // Le socle est toujours visible : sans lui il n'y a plus d'application.
        visible: socle != 0 || (souscrit != 0 && actif != 0),
        contenu: Vec::new(),
    })
}

const COLS: &str = "SELECT code, libelle, description, icone, famille, ordre, socle,
        souscrit, souscrit_le, souscrit_par, actif, requiert FROM module";

pub fn lister(conn: &Connection) -> Result<Vec<Module>> {
    let mut st = conn.prepare(&format!("{COLS} ORDER BY ordre, libelle"))?;
    let mut v = st.query_map([], ligne)?.collect::<rusqlite::Result<Vec<_>>>()?;
    for m in v.iter_mut() {
        m.contenu = contenu(conn, &m.code)?;
    }
    Ok(v)
}

pub fn lire(conn: &Connection, code: &str) -> Result<Module> {
    let mut st = conn.prepare(&format!("{COLS} WHERE code = ?1"))?;
    let mut m = st
        .query_row(params![code], ligne)
        .map_err(|_| CoreError::NotFound(format!("module {code}")))?;
    m.contenu = contenu(conn, code)?;
    Ok(m)
}

/// Les codes des modules **visibles** : c'est tout ce dont le menu a besoin.
pub fn visibles(conn: &Connection) -> Result<Vec<String>> {
    Ok(lister(conn)?.into_iter().filter(|m| m.visible).map(|m| m.code).collect())
}

/// Ce que contient déjà un module. On compte pour **prévenir avant de masquer** :
/// sans ce message, l'utilisateur croit qu'il a effacé son travail.
fn contenu(conn: &Connection, code: &str) -> Result<Vec<Compte>> {
    let requetes: &[(&str, &str)] = match code {
        "caisse" => &[
            ("encaissements", "SELECT COUNT(*) FROM paiement"),
            ("sessions de caisse", "SELECT COUNT(*) FROM session_caisse"),
        ],
        "facturation" => &[("pièces (devis, factures…)", "SELECT COUNT(*) FROM document")],
        "abonnements" => &[("abonnements", "SELECT COUNT(*) FROM abonnement")],
        "magasins" => &[
            ("magasins", "SELECT COUNT(*) FROM depot"),
            ("inventaires", "SELECT COUNT(*) FROM inventaire"),
        ],
        "production" => &[
            ("recettes", "SELECT COUNT(*) FROM nomenclature"),
            ("ordres de fabrication", "SELECT COUNT(*) FROM ordre_production"),
        ],
        "projets" => &[
            ("projets", "SELECT COUNT(*) FROM projet"),
            ("activités", "SELECT COUNT(*) FROM tache"),
        ],
        "marches" => &[
            ("marchés", "SELECT COUNT(*) FROM marche"),
            ("soumissionnaires", "SELECT COUNT(*) FROM marche_soumissionnaire"),
            ("avenants", "SELECT COUNT(*) FROM marche_avenant"),
        ],
        "agenda" => &[("rendez-vous", "SELECT COUNT(*) FROM rendez_vous")],
        "comptabilite" => &[
            ("écritures", "SELECT COUNT(*) FROM ecriture"),
            ("comptes", "SELECT COUNT(*) FROM compte"),
        ],
        _ => &[],
    };
    let mut out = Vec::new();
    for (libelle, sql) in requetes {
        // Une table absente ne doit pas faire échouer l'écran : on ignore.
        if let Ok(n) = conn.query_row(sql, [], |r| r.get::<_, i64>(0)) {
            if n > 0 {
                out.push(Compte { libelle: (*libelle).to_string(), nb: n });
            }
        }
    }
    Ok(out)
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChoixFormule {
    pub formule: String,
    /// Les modules réellement vendus. La formule n'est qu'un préréglage :
    /// c'est cette liste qui fait foi, parce qu'on l'ajuste au cas par cas.
    #[serde(default)]
    pub modules: Vec<String>,
}

/// Ouvre les droits à l'installation. **Remplace** l'état de souscription :
/// c'est un acte d'installation, pas un ajout au coup par coup.
pub fn appliquer_formule(conn: &Connection, c: &ChoixFormule, par: Option<&str>) -> Result<Vec<Module>> {
    if !FORMULES.iter().any(|(code, ..)| *code == c.formule) {
        return Err(CoreError::Rule(format!("formule inconnue : {}", c.formule)));
    }
    // On ferme tout, sauf le socle, puis on ouvre ce qui est vendu. Ainsi
    // rejouer une formule donne toujours le même résultat.
    conn.execute(
        "UPDATE module SET souscrit = 0, souscrit_le = NULL, souscrit_par = NULL WHERE socle = 0",
        [],
    )?;
    for code in &c.modules {
        let n = conn.execute(
            "UPDATE module SET souscrit = 1, souscrit_le = ?2, souscrit_par = ?3 WHERE code = ?1",
            params![code, now(), par],
        )?;
        if n == 0 {
            return Err(CoreError::Rule(format!("module inconnu : {code}")));
        }
    }
    // Les dépendances suivent : vendre la caisse sans le socle n'a pas de sens.
    completer_dependances(conn)?;
    conn.execute(
        "INSERT INTO parametre_global (cle, valeur) VALUES ('formule_installee', ?1)
         ON CONFLICT(cle) DO UPDATE SET valeur = ?1",
        params![c.formule],
    )?;
    lister(conn)
}

/// Ouvre d'office ce dont les modules vendus ont besoin. Un module dont la
/// dépendance manquerait afficherait des écrans vides — autant l'éviter.
fn completer_dependances(conn: &Connection) -> Result<()> {
    // Deux passes suffisent : les dépendances de Djigui ne dépassent pas un
    // niveau (abonnements → facturation → socle).
    for _ in 0..2 {
        let manquantes: Vec<String> = lister(conn)?
            .into_iter()
            .filter(|m| m.souscrit)
            .flat_map(|m| m.requiert)
            .collect();
        for code in manquantes {
            conn.execute(
                "UPDATE module SET souscrit = 1, souscrit_le = COALESCE(souscrit_le, ?2)
                  WHERE code = ?1 AND souscrit = 0",
                params![code, now()],
            )?;
        }
    }
    Ok(())
}

/// Le client masque ou réaffiche un module **qu'il a souscrit**.
///
/// Trois refus, et ils sont tous structurels :
/// - le **socle** ne se masque pas : l'application n'existerait plus ;
/// - un module **non souscrit** ne s'active pas — c'est la règle de
///   facturation, et c'est tout l'intérêt de la distinction ;
/// - un module dont **un autre dépend** ne se masque pas tant que celui-là est
///   affiché (masquer la facturation laisserait les abonnements sans objet).
pub fn changer_actif(conn: &Connection, code: &str, actif: bool) -> Result<Module> {
    let m = lire(conn, code)?;
    if m.socle {
        return Err(CoreError::Rule(
            "ce module est la base de l'application : il ne peut pas être masqué".into(),
        ));
    }
    if actif && !m.souscrit {
        return Err(CoreError::Rule(format!(
            "le module « {} » n'est pas souscrit. Contactez Djigui pour l'ajouter à votre formule.",
            m.libelle
        )));
    }
    if !actif {
        let dependants: Vec<String> = lister(conn)?
            .into_iter()
            .filter(|x| x.visible && x.requiert.iter().any(|r| r == code))
            .map(|x| x.libelle)
            .collect();
        if !dependants.is_empty() {
            return Err(CoreError::Rule(format!(
                "« {} » est nécessaire à : {}. Masquez d'abord ce ou ces modules.",
                m.libelle,
                dependants.join(", ")
            )));
        }
    }
    conn.execute("UPDATE module SET actif = ?2 WHERE code = ?1", params![code, actif as i64])?;
    lire(conn, code)
}

/// La formule retenue à l'installation, pour mémoire.
pub fn formule_installee(conn: &Connection) -> String {
    conn.query_row(
        "SELECT valeur FROM parametre_global WHERE cle = 'formule_installee'",
        [],
        |r| r.get::<_, String>(0),
    )
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    /// **Tout est ouvert tant que la formule n'est pas posée** (migration 0041).
    ///
    /// C'est délibéré, et le sens de l'erreur compte : un module ouvert par
    /// excès se referme d'un clic, tandis qu'un module fermé par erreur fait
    /// croire à une perte de données. Surtout, une mise à jour ne doit **jamais**
    /// retirer un accès à un client qui travaillait déjà — sans quoi son menu
    /// s'effondrerait au premier démarrage après installation de la version.
    #[test]
    fn tout_est_ouvert_tant_que_la_formule_nest_pas_posee() {
        let conn = db::open_in_memory().unwrap();
        let mods = lister(&conn).unwrap();
        assert!(mods.len() >= 10, "le catalogue est présent");
        let socle = mods.iter().find(|m| m.code == "socle").unwrap();
        assert!(socle.souscrit && socle.visible, "sans le socle, rien ne marche");
        // ⚠️ La règle est « une mise à jour n'ENLÈVE jamais un accès », pas
        // « une mise à jour DONNE tout ». La migration 0041 a ouvert le
        // catalogue qui existait alors, pour qu'aucun client en service ne voie
        // son menu s'effondrer. Un module livré PLUS TARD (« Paie & RH »,
        // mig 0044) est une offre commerciale distincte : l'ouvrir d'office
        // reviendrait à l'offrir, et `souscrit` est une donnée de FACTURATION.
        // Il reste donc en vitrine jusqu'à ce que l'installateur pose la formule.
        const OUVERTS_PAR_0041: &[&str] = &[
            "socle", "caisse", "facturation", "abonnements", "magasins",
            "production", "projets", "marches", "agenda",
        ];
        for code in OUVERTS_PAR_0041 {
            let m = mods.iter().find(|m| &m.code == code)
                .unwrap_or_else(|| panic!("module {code} absent du catalogue"));
            assert!(m.souscrit && m.visible,
                    "une mise à jour n'enlève jamais un accès : {code} s'est refermé");
        }
        let paie = mods.iter().find(|m| m.code == "paie").unwrap();
        assert!(!paie.souscrit,
                "un module livré après coup se vend, il ne s'offre pas");
        // Mais la formule reste VIDE : l'écran affichera « non définie », ce qui
        // invite justement l'installateur à la poser. On ne prétend pas que le
        // client a souscrit quoi que ce soit.
        assert_eq!(formule_installee(&conn), "",
                   "aucune formule n'est réputée souscrite d'office");
    }

    #[test]
    fn la_formule_ouvre_les_droits_et_entraine_les_dependances() {
        let conn = db::open_in_memory().unwrap();
        // Cas ONG : projets et marchés, sans caisse ni facturation.
        appliquer_formule(&conn, &ChoixFormule {
            formule: "ong_projets".into(),
            modules: vec!["projets".into(), "marches".into(), "agenda".into()],
        }, Some("djigui")).unwrap();

        let v = visibles(&conn).unwrap();
        assert!(v.contains(&"projets".to_string()));
        assert!(v.contains(&"marches".to_string()));
        assert!(!v.contains(&"caisse".to_string()), "la caisse n'est pas vendue");
        assert!(v.contains(&"socle".to_string()), "le socle suit toujours");

        // La souscription est TRACÉE : c'est une donnée de facturation.
        let m = lire(&conn, "marches").unwrap();
        assert!(m.souscrit);
        assert_eq!(m.souscrit_par.as_deref(), Some("djigui"));
        assert!(m.souscrit_le.is_some());
        assert_eq!(formule_installee(&conn), "ong_projets");

        // Une formule vendue avec les abonnements entraîne la facturation,
        // sans quoi l'écran serait vide.
        appliquer_formule(&conn, &ChoixFormule {
            formule: "sur_mesure".into(),
            modules: vec!["abonnements".into()],
        }, None).unwrap();
        let v = visibles(&conn).unwrap();
        assert!(v.contains(&"facturation".to_string()), "dépendance entraînée : {v:?}");
        // Et rejouer une formule REMPLACE : les marchés ne sont plus vendus.
        assert!(!v.contains(&"marches".to_string()), "{v:?}");
    }

    #[test]
    fn le_client_masque_ce_quil_veut_parmi_ce_quil_a_souscrit() {
        let conn = db::open_in_memory().unwrap();
        appliquer_formule(&conn, &ChoixFormule {
            formule: "commerce".into(),
            modules: vec!["caisse".into(), "facturation".into(), "abonnements".into()],
        }, None).unwrap();

        // Masquer un module souscrit : permis, et ça ne change pas la souscription.
        let m = changer_actif(&conn, "caisse", false).unwrap();
        assert!(!m.actif && !m.visible);
        assert!(m.souscrit, "masquer n'annule PAS la souscription : il paie toujours");
        assert!(changer_actif(&conn, "caisse", true).is_ok());

        // Le socle ne se masque jamais.
        assert!(changer_actif(&conn, "socle", false).is_err());

        // Un module non souscrit ne s'active pas : c'est toute la règle.
        let err = changer_actif(&conn, "production", true).unwrap_err();
        assert!(format!("{err}").contains("pas souscrit"), "{err}");

        // On ne masque pas ce dont un autre module dépend.
        let err = changer_actif(&conn, "facturation", false).unwrap_err();
        assert!(format!("{err}").contains("Abonnements"), "{err}");
        // En masquant d'abord le dépendant, cela devient possible.
        changer_actif(&conn, "abonnements", false).unwrap();
        assert!(changer_actif(&conn, "facturation", false).is_ok());
    }

    #[test]
    fn masquer_un_module_ne_touche_a_aucune_donnee() {
        let conn = db::open_in_memory().unwrap();
        appliquer_formule(&conn, &ChoixFormule {
            formule: "sur_mesure".into(),
            modules: vec!["projets".into()],
        }, None).unwrap();
        crate::modules::projet::creer(&conn, &crate::modules::projet::NouveauProjet {
            nom: "Chantier".into(), client_id: None, chef_de_projet_id: None,
            date_debut_prevue: None, date_fin_prevue: None, date_debut_reelle: None,
            date_fin_reelle: None, statut: None, budget_global: 0.0, note: None,
        }, None).unwrap();

        // Le décompte sert à PRÉVENIR avant de masquer.
        let m = lire(&conn, "projets").unwrap();
        assert!(m.contenu.iter().any(|c| c.libelle == "projets" && c.nb == 1),
                "on doit pouvoir dire ce que le module contient : {:?}", m.contenu);

        changer_actif(&conn, "projets", false).unwrap();
        // ⚠️ La donnée est INTACTE : on masque, on n'efface pas.
        assert_eq!(crate::modules::projet::lister(&conn, None).unwrap().len(), 1);
        let m = lire(&conn, "projets").unwrap();
        assert!(!m.visible);
        assert_eq!(m.contenu.iter().find(|c| c.libelle == "projets").unwrap().nb, 1);
    }
}
