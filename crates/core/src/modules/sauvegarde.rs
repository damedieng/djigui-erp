//! Sauvegarde automatique chiffrée (migration 0042).
//!
//! # Ce que fait ce module
//!
//! Il fabrique, à la demande ou à la fermeture de l'application, **une archive
//! chiffrée contenant la base ET le dossier des documents**, et la dépose dans
//! chacune des destinations que l'utilisateur a configurées (disque local,
//! dossier synchronisé par Google Drive Desktop, clé USB, partage réseau).
//!
//! # Trois précautions qui font la différence entre une sauvegarde et un fichier
//!
//! 1. **`VACUUM INTO`, jamais une copie du fichier.** La base tourne en mode
//!    WAL : à un instant donné, une partie des données n'est pas encore dans
//!    `djigui.db` mais dans `djigui.db-wal`. Copier le fichier avec l'explorateur
//!    Windows produit donc une base **amputée des dernières écritures**, ou
//!    carrément corrompue si une transaction est en cours. `VACUUM INTO` demande
//!    à SQLite lui-même d'écrire un instantané cohérent.
//!
//! 2. **Chiffrement authentifié (AES-256-GCM).** Le chiffrement empêche de lire ;
//!    l'authentification empêche de restaurer une archive abîmée ou trafiquée.
//!    Sans elle, un fichier à moitié copié sur une clé USB arrachée se
//!    déchiffrerait en bouillie et on écraserait la base saine avec.
//!
//! 3. **Relecture de contrôle après écriture.** On rouvre l'archive qu'on vient
//!    d'écrire et on la déchiffre. Une copie qu'on n'a pas su rouvrir n'est pas
//!    une copie — et on préfère l'apprendre le jour de la sauvegarde plutôt que
//!    le jour du sinistre.
//!
//! # D'où vient la clé — trois modes
//!
//! | Mode | Secret | Récupérable si tout est perdu ? |
//! |------|--------|----------------------------------|
//! | `licence` | la **clé de licence du client**, saisie à l'installation | **oui** — elle figure sur ses documents d'installation et chez SODEVITEL |
//! | `integree` | une constante du logiciel | oui, mais elle est la même pour tous |
//! | `motdepasse` | une phrase choisie par le client | **non** — perdue, les archives sont définitivement illisibles |
//!
//! **`licence` est le mode normal en exploitation**, et c'est le bon compromis :
//! le secret est **propre à chaque client** (il ne voyage donc pas dans
//! l'exécutable, contrairement à la clé intégrée) tout en restant
//! **récupérable** — un client qui perd sa machine ET sa base redemande sa
//! licence et rouvre ses sauvegardes.
//!
//! `integree` ne subsiste que pour la fenêtre où une installation toute neuve
//! n'a pas encore reçu sa licence : sans elle, cette machine ne pourrait pas se
//! sauvegarder du tout, et c'est justement le moment où l'on saisit le plus de
//! données de départ.
//!
//! ⚠️ **Chaque archive porte, en clair, le mode avec lequel elle a été écrite.**
//! Saisir la licence plus tard ne rend donc pas illisibles les copies faites
//! avant. Aucune archive existante n'est jamais réécrite : toucher à la seule
//! copie de secours serait le plus mauvais moment pour prendre un risque.

use crate::error::{CoreError, Result};
use crate::now;
use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use argon2::Argon2;
use rand::RngCore;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Format de fichier
// ---------------------------------------------------------------------------

/// Reconnaît un fichier Djigui avant même d'essayer de le déchiffrer : on peut
/// ainsi refuser poliment un fichier qui n'en est pas un, au lieu de renvoyer
/// une erreur de cryptographie incompréhensible.
const MAGIE: &[u8; 16] = b"DJIGUI-SAUVEGA\x01\x00";

/// Version du CONTENANT (pas de l'application). Elle nous permettra de lire
/// encore, dans cinq ans, une archive écrite aujourd'hui.
const VERSION_FORMAT: u8 = 1;

/// Clé intégrée : secret de repli, utilisé **uniquement** tant que la licence
/// n'a pas été saisie. Voir l'avertissement en tête de module — ce n'est PAS un
/// secret fort, puisqu'il voyage dans l'exécutable.
const SECRET_INTEGRE: &[u8] = b"DJIGUI/SODEVITEL/sauvegarde-v1/cle-integree";

/// Préfixe mêlé à la licence avant dérivation. Il évite qu'une licence puisse
/// servir de secret ailleurs (ou l'inverse) : le même texte ne produit pas la
/// même clé selon l'usage.
const PREFIXE_LICENCE: &[u8] = b"DJIGUI/sauvegarde-v1/licence/";

/// Clé du paramètre global qui porte la licence de l'installation.
pub const CLE_LICENCE: &str = "licence_installation";

/// Lit la licence saisie à l'installation. `None` tant qu'elle est vide.
pub fn licence(conn: &Connection) -> Option<String> {
    conn.query_row(
        "SELECT valeur FROM parametre_global WHERE cle = ?1",
        params![CLE_LICENCE],
        |r| r.get::<_, String>(0),
    )
    .ok()
    .map(|v| v.trim().to_string())
    .filter(|v| !v.is_empty())
}

/// Enregistre la licence remise au client et **bascule la sauvegarde en mode
/// `licence`** — sauf si le client a délibérément posé un mot de passe, auquel
/// cas on ne lui retire pas sa protection dans son dos.
pub fn definir_licence(conn: &Connection, valeur: &str) -> Result<ParametresSauvegarde> {
    let v = valeur.trim();
    if v.chars().count() < 8 {
        return Err(CoreError::Rule(
            "La clé de licence semble incomplète : recopiez-la telle qu'elle figure sur vos \
             documents d'installation."
                .into(),
        ));
    }
    crate::modules::parametres::ecrire_global(conn, CLE_LICENCE, v)?;
    let p = lire_parametres(conn)?;
    if p.mode_cle != "motdepasse" {
        conn.execute(
            "UPDATE parametres_sauvegarde SET mode_cle = 'licence', maj_le = ?1 WHERE singleton = 1",
            params![now()],
        )?;
    }
    lire_parametres(conn)
}

const NOM_BASE_DANS_ARCHIVE: &str = "base/djigui.db";
const PREFIXE_DOCUMENTS: &str = "documents/";

/// Entête **en clair** de l'archive. Il ne contient aucune donnée de gestion :
/// seulement de quoi savoir comment ouvrir le reste. C'est ce qui permet à
/// l'écran de restauration d'annoncer « cette sauvegarde du 12/03 demande un
/// mot de passe » **avant** de demander quoi que ce soit à l'utilisateur.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnteteArchive {
    pub mode_cle: String,
    /// Sel de dérivation, en hexadécimal.
    pub sel: String,
    pub nonce: String,
    pub cree_le: String,
    pub version_application: String,
    pub nb_documents: usize,
    /// Taille de l'archive interne avant chiffrement — sert au contrôle de
    /// vraisemblance et à l'affichage.
    pub taille_contenu: u64,
}

// ---------------------------------------------------------------------------
// Réglages
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParametresSauvegarde {
    pub activee: bool,
    pub cette_machine_est_serveur: bool,
    pub a_la_fermeture: bool,
    pub copies_a_conserver: i64,
    pub mode_cle: String,
    /// Jamais le mot de passe : seulement le fait qu'il en existe un.
    pub mot_de_passe_defini: bool,
    /// Une licence est-elle enregistrée ? L'écran doit pouvoir dire « votre
    /// sauvegarde n'est pas encore protégée par VOTRE clé » à une installation
    /// neuve, sinon personne ne pense à saisir la licence.
    pub licence_definie: bool,
    /// Les 4 derniers caractères de la licence, pour que l'utilisateur
    /// reconnaisse la sienne sans qu'on l'affiche en entier à l'écran.
    pub licence_fin: Option<String>,
    pub derniere_sauvegarde: Option<String>,
    pub dernier_statut: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Destination {
    pub id: String,
    pub libelle: String,
    pub chemin: String,
    pub actif: bool,
    pub ordre: i64,
    pub dernier_essai: Option<String>,
    pub dernier_statut: Option<String>,
    pub dernier_message: Option<String>,
    /// Calculé à la lecture : le dossier existe-t-il et peut-on y écrire ?
    /// Une clé USB débranchée doit se voir AVANT le jour du sinistre.
    #[serde(default)]
    pub accessible: bool,
    #[serde(default)]
    pub espace_libre_octets: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntreeJournal {
    pub id: String,
    pub horodatage: String,
    pub declencheur: String,
    pub nom_fichier: Option<String>,
    pub taille_octets: Option<i64>,
    pub statut: String,
    pub nb_destinations_ok: i64,
    pub nb_destinations_echec: i64,
    pub verifiee: bool,
    pub message: Option<String>,
}

/// Ce que l'écran affiche après une sauvegarde : le détail par destination,
/// pas un simple « c'est fait ». Une copie sur trois qui échoue doit se voir.
#[derive(Debug, Clone, Serialize)]
pub struct ResultatSauvegarde {
    pub statut: String,
    pub nom_fichier: String,
    pub taille_octets: u64,
    pub verifiee: bool,
    pub destinations: Vec<ResultatDestination>,
    pub anciennes_supprimees: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResultatDestination {
    pub libelle: String,
    pub chemin: String,
    pub reussi: bool,
    pub message: String,
}

// ---------------------------------------------------------------------------
// Lecture / écriture des réglages
// ---------------------------------------------------------------------------

pub fn lire_parametres(conn: &Connection) -> Result<ParametresSauvegarde> {
    let lic = licence(conn);
    let mut p = conn.query_row(
        "SELECT activee, cette_machine_est_serveur, a_la_fermeture, copies_a_conserver,
                mode_cle, empreinte_mot_de_passe, derniere_sauvegarde, dernier_statut
           FROM parametres_sauvegarde WHERE singleton = 1",
        [],
        |r| {
            Ok(ParametresSauvegarde {
                activee: r.get::<_, i64>(0)? != 0,
                cette_machine_est_serveur: r.get::<_, i64>(1)? != 0,
                a_la_fermeture: r.get::<_, i64>(2)? != 0,
                copies_a_conserver: r.get(3)?,
                mode_cle: r.get(4)?,
                mot_de_passe_defini: r.get::<_, Option<String>>(5)?.is_some(),
                licence_definie: false,
                licence_fin: None,
                derniere_sauvegarde: r.get(6)?,
                dernier_statut: r.get(7)?,
            })
        },
    )?;
    p.licence_definie = lic.is_some();
    p.licence_fin = lic.map(|l| {
        let n = l.chars().count();
        l.chars().skip(n.saturating_sub(4)).collect()
    });
    Ok(p)
}

#[derive(Debug, Deserialize)]
pub struct MajParametres {
    pub activee: bool,
    pub cette_machine_est_serveur: bool,
    pub a_la_fermeture: bool,
    pub copies_a_conserver: i64,
}

pub fn modifier_parametres(conn: &Connection, m: &MajParametres) -> Result<ParametresSauvegarde> {
    if m.copies_a_conserver < 1 {
        return Err(CoreError::Rule(
            "Il faut conserver au moins une copie : à 0, la sauvegarde s'effacerait elle-même."
                .into(),
        ));
    }
    conn.execute(
        "UPDATE parametres_sauvegarde
            SET activee = ?1, cette_machine_est_serveur = ?2, a_la_fermeture = ?3,
                copies_a_conserver = ?4, maj_le = ?5
          WHERE singleton = 1",
        params![
            m.activee as i64,
            m.cette_machine_est_serveur as i64,
            m.a_la_fermeture as i64,
            m.copies_a_conserver,
            now()
        ],
    )?;
    lire_parametres(conn)
}

/// Pose, remplace ou retire le mot de passe.
///
/// ⚠️ Changer de mot de passe **ne rend pas les anciennes archives illisibles**
/// et ne les convertit pas non plus : chacune garde le mode et le sel avec
/// lesquels elle a été écrite. C'est voulu — réécrire d'anciennes sauvegardes
/// serait à la fois long et dangereux (on toucherait à la seule copie de secours
/// pendant l'opération). L'écran doit le dire en toutes lettres.
pub fn definir_mot_de_passe(conn: &Connection, mot_de_passe: Option<&str>) -> Result<ParametresSauvegarde> {
    match mot_de_passe {
        Some(mdp) => {
            if mdp.chars().count() < 8 {
                return Err(CoreError::Rule(
                    "Le mot de passe de sauvegarde doit faire au moins 8 caractères.".into(),
                ));
            }
            let mut sel = [0u8; 16];
            rand::thread_rng().fill_bytes(&mut sel);
            let cle = deriver_cle(mdp.as_bytes(), &sel)?;
            // On enregistre l'empreinte de la CLÉ DÉRIVÉE, pas du mot de passe,
            // et elle ne sert qu'à dire « ce n'est pas le bon » avant de lancer
            // une restauration qui prendrait plusieurs minutes.
            let empreinte = empreinte_hex(&cle);
            conn.execute(
                "UPDATE parametres_sauvegarde
                    SET mode_cle = 'motdepasse', sel_mot_de_passe = ?1,
                        empreinte_mot_de_passe = ?2, maj_le = ?3
                  WHERE singleton = 1",
                params![hex(&sel), empreinte, now()],
            )?;
        }
        None => {
            // Retirer le mot de passe ne ramène pas à la clé intégrée si une
            // licence existe : on revient au mode normal, qui est meilleur.
            let mode = if licence(conn).is_some() { "licence" } else { "integree" };
            conn.execute(
                "UPDATE parametres_sauvegarde
                    SET mode_cle = ?1, sel_mot_de_passe = NULL,
                        empreinte_mot_de_passe = NULL, maj_le = ?2
                  WHERE singleton = 1",
                params![mode, now()],
            )?;
        }
    }
    lire_parametres(conn)
}

/// Vérifie un mot de passe saisi contre l'empreinte enregistrée.
pub fn verifier_mot_de_passe(conn: &Connection, mdp: &str) -> Result<bool> {
    let ligne: Option<(Option<String>, Option<String>)> = conn
        .query_row(
            "SELECT sel_mot_de_passe, empreinte_mot_de_passe
               FROM parametres_sauvegarde WHERE singleton = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    let Some((Some(sel_hex), Some(emp))) = ligne else {
        return Ok(false);
    };
    let sel = dehex(&sel_hex)?;
    let cle = deriver_cle(mdp.as_bytes(), &sel)?;
    Ok(empreinte_hex(&cle) == emp)
}

// ---------------------------------------------------------------------------
// Destinations
// ---------------------------------------------------------------------------

pub fn lister_destinations(conn: &Connection) -> Result<Vec<Destination>> {
    let mut st = conn.prepare(
        "SELECT id, libelle, chemin, actif, ordre, dernier_essai, dernier_statut, dernier_message
           FROM sauvegarde_destination ORDER BY ordre, libelle",
    )?;
    let v = st
        .query_map([], |r| {
            Ok(Destination {
                id: r.get(0)?,
                libelle: r.get(1)?,
                chemin: r.get(2)?,
                actif: r.get::<_, i64>(3)? != 0,
                ordre: r.get(4)?,
                dernier_essai: r.get(5)?,
                dernier_statut: r.get(6)?,
                dernier_message: r.get(7)?,
                accessible: false,
                espace_libre_octets: None,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    // L'état du dossier est constaté MAINTENANT, pas mémorisé : une clé USB se
    // débranche entre deux affichages.
    Ok(v.into_iter()
        .map(|mut d| {
            d.accessible = dossier_utilisable(Path::new(&d.chemin)).is_ok();
            d
        })
        .collect())
}

#[derive(Debug, Deserialize)]
pub struct NouvelleDestination {
    pub libelle: String,
    pub chemin: String,
    #[serde(default = "vrai")]
    pub actif: bool,
}

fn vrai() -> bool {
    true
}

pub fn ajouter_destination(conn: &Connection, d: &NouvelleDestination) -> Result<Destination> {
    let libelle = d.libelle.trim();
    let chemin = d.chemin.trim();
    if libelle.is_empty() {
        return Err(CoreError::Rule(
            "Donnez un nom à cette destination (« Clé USB bleue », « Dossier Drive ») : \
             c'est ce nom qui apparaîtra si la copie échoue."
                .into(),
        ));
    }
    if chemin.is_empty() {
        return Err(CoreError::Rule("Indiquez le dossier où écrire les copies.".into()));
    }
    // Le dossier est créé s'il manque : quelqu'un qui indique
    // « E:\Sauvegardes Djigui » veut ce dossier, pas un message d'erreur.
    // Si le support n'est pas branché, la création échoue de toute façon.
    let _ = std::fs::create_dir_all(chemin);
    // On refuse tout de suite un dossier inutilisable plutôt que de l'accepter
    // et de laisser l'utilisateur croire qu'il est sauvegardé.
    dossier_utilisable(Path::new(chemin))?;

    let deja: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sauvegarde_destination WHERE chemin = ?1",
        params![chemin],
        |r| r.get(0),
    )?;
    if deja > 0 {
        return Err(CoreError::Rule(
            "Ce dossier est déjà dans la liste. Deux copies au même endroit ne protègent pas deux fois."
                .into(),
        ));
    }

    let id = Uuid::new_v4().to_string();
    let ordre: i64 = conn.query_row(
        "SELECT COALESCE(MAX(ordre), 0) + 1 FROM sauvegarde_destination",
        [],
        |r| r.get(0),
    )?;
    conn.execute(
        "INSERT INTO sauvegarde_destination (id, libelle, chemin, actif, ordre, cree_le)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![id, libelle, chemin, d.actif as i64, ordre, now()],
    )?;
    lister_destinations(conn)?
        .into_iter()
        .find(|x| x.id == id)
        .ok_or_else(|| CoreError::NotFound("destination".into()))
}

pub fn modifier_destination(conn: &Connection, id: &str, d: &NouvelleDestination) -> Result<Destination> {
    let chemin = d.chemin.trim();
    dossier_utilisable(Path::new(chemin))?;
    let n = conn.execute(
        "UPDATE sauvegarde_destination SET libelle = ?1, chemin = ?2, actif = ?3 WHERE id = ?4",
        params![d.libelle.trim(), chemin, d.actif as i64, id],
    )?;
    if n == 0 {
        return Err(CoreError::NotFound("destination".into()));
    }
    lister_destinations(conn)?
        .into_iter()
        .find(|x| x.id == id)
        .ok_or_else(|| CoreError::NotFound("destination".into()))
}

/// Retire une destination de la liste. **Ne supprime aucun fichier déjà écrit** :
/// les copies présentes dans ce dossier restent, et c'est voulu — l'utilisateur
/// retire souvent une destination parce qu'elle n'est plus pratique, pas parce
/// qu'il veut détruire ses sauvegardes.
pub fn supprimer_destination(conn: &Connection, id: &str) -> Result<()> {
    let n = conn.execute("DELETE FROM sauvegarde_destination WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(CoreError::NotFound("destination".into()));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Choix du dossier : explorateur et suggestions
// ---------------------------------------------------------------------------
//
// ⚠️ Pourquoi côté SERVEUR et pas une boîte de dialogue Windows : les dossiers
// qui nous intéressent sont ceux de **la machine qui détient les données**.
// Une boîte native ouverte depuis l'interface montrerait les dossiers du poste
// où l'on regarde l'écran — qui sera un autre poste le jour du mode client.
// L'accès est réservé à l'administrateur (voir la couche API).

#[derive(Debug, Clone, Serialize)]
pub struct Dossier {
    pub nom: String,
    pub chemin: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Exploration {
    pub chemin: String,
    pub parent: Option<String>,
    pub dossiers: Vec<Dossier>,
    pub inscriptible: bool,
}

/// Liste les sous-dossiers. `chemin` vide = racines (lecteurs sous Windows).
pub fn parcourir(chemin: Option<&str>) -> Result<Exploration> {
    let brut = chemin.map(str::trim).filter(|s| !s.is_empty());

    let Some(c) = brut else {
        return Ok(Exploration {
            chemin: String::new(),
            parent: None,
            dossiers: racines(),
            inscriptible: false,
        });
    };

    let p = Path::new(c);
    if !p.is_dir() {
        return Err(CoreError::Rule(format!("« {c} » n'est pas un dossier accessible.")));
    }
    let mut dossiers: Vec<Dossier> = std::fs::read_dir(p)
        .map_err(|e| CoreError::Rule(format!("Dossier illisible : {e}")))?
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter(|e| {
            // Les dossiers cachés et système n'ont rien à faire dans une liste
            // destinée à quelqu'un qui cherche « ma clé USB ».
            !e.file_name().to_string_lossy().starts_with('.')
        })
        .map(|e| Dossier {
            nom: e.file_name().to_string_lossy().to_string(),
            chemin: e.path().to_string_lossy().to_string(),
        })
        .collect();
    dossiers.sort_by(|a, b| a.nom.to_lowercase().cmp(&b.nom.to_lowercase()));

    Ok(Exploration {
        chemin: c.to_string(),
        parent: p.parent().map(|x| x.to_string_lossy().to_string()),
        // Constaté en écrivant réellement : un dossier peut être visible et
        // refuser l'écriture (lecture seule, droits réseau).
        inscriptible: dossier_utilisable(p).is_ok(),
        dossiers,
    })
}

#[cfg(windows)]
fn racines() -> Vec<Dossier> {
    ('A'..='Z')
        .map(|l| format!("{l}:\\"))
        .filter(|c| Path::new(c).is_dir())
        .map(|c| Dossier { nom: c.clone(), chemin: c })
        .collect()
}

#[cfg(not(windows))]
fn racines() -> Vec<Dossier> {
    vec![Dossier { nom: "/".into(), chemin: "/".into() }]
}

/// Endroits probables, proposés en un clic.
///
/// Le but n'est pas d'être exhaustif : c'est d'éviter à quelqu'un qui ne sait
/// pas ce qu'est un « chemin » d'avoir à en taper un. On ne propose que ce qui
/// existe vraiment sur la machine.
#[derive(Debug, Clone, Serialize)]
pub struct Suggestion {
    pub libelle: String,
    pub chemin: String,
    pub explication: String,
    /// Cette copie part-elle hors de la machine ? C'est le seul critère qui
    /// compte vraiment : une copie sur le même disque ne protège pas d'une panne
    /// de ce disque.
    pub hors_machine: bool,
}

pub fn suggestions() -> Vec<Suggestion> {
    let mut v = Vec::new();
    let maison = std::env::var("USERPROFILE").or_else(|_| std::env::var("HOME")).ok();

    if let Some(m) = &maison {
        // Google Drive Desktop crée un dossier local synchronisé. Djigui y écrit
        // un fichier ; c'est Drive qui l'envoie dans le nuage. Aucun compte,
        // aucune autorisation Google à donner à Djigui.
        for candidat in ["Mon Drive", "My Drive", "Google Drive"] {
            let p = Path::new(m).join(candidat);
            if p.is_dir() {
                v.push(Suggestion {
                    libelle: format!("Google Drive ({candidat})"),
                    chemin: p.to_string_lossy().to_string(),
                    explication: "Copie envoyée automatiquement dans votre Drive par Google Drive \
                                  sur ordinateur. Djigui ne se connecte à aucun compte : il écrit \
                                  simplement dans ce dossier."
                        .into(),
                    hors_machine: true,
                });
            }
        }
        for candidat in ["Documents", "Desktop", "Bureau"] {
            let p = Path::new(m).join(candidat);
            if p.is_dir() {
                v.push(Suggestion {
                    libelle: format!("Mes {}", candidat.to_lowercase()),
                    chemin: p.join("Sauvegardes Djigui").to_string_lossy().to_string(),
                    explication: "Facile à retrouver, mais SUR CET ORDINATEUR : cette copie ne \
                                  protège pas d'une panne du disque ni d'un vol."
                        .into(),
                    hors_machine: false,
                });
                break;
            }
        }
    }

    // Supports amovibles : on ne cherche pas à deviner lequel est une clé USB,
    // on propose les lecteurs qui ne sont pas celui du système.
    #[cfg(windows)]
    {
        let systeme = std::env::var("SystemDrive").unwrap_or_else(|_| "C:".into());
        for d in racines() {
            if d.chemin.starts_with(&systeme) {
                continue;
            }
            v.push(Suggestion {
                libelle: format!("Lecteur {} (clé USB, disque externe…)", d.nom),
                chemin: format!("{}Sauvegardes Djigui", d.chemin),
                explication: "Une copie qui quitte l'ordinateur : c'est ce qui protège le mieux. \
                              Pensez à laisser le support branché au moment de fermer Djigui."
                    .into(),
                hors_machine: true,
            });
        }
    }
    v
}

/// Crée un dossier proposé s'il n'existe pas encore : quelqu'un qui clique sur
/// « Sauvegardes Djigui » veut le dossier, pas un message d'erreur.
pub fn creer_dossier(chemin: &str) -> Result<()> {
    std::fs::create_dir_all(chemin.trim())
        .map_err(|e| CoreError::Rule(format!("Impossible de créer « {chemin} » : {e}")))
}

// ---------------------------------------------------------------------------
// Journal
// ---------------------------------------------------------------------------

pub fn lister_journal(conn: &Connection, limite: i64) -> Result<Vec<EntreeJournal>> {
    let mut st = conn.prepare(
        "SELECT id, horodatage, declencheur, nom_fichier, taille_octets, statut,
                nb_destinations_ok, nb_destinations_echec, verifiee, message
           FROM sauvegarde_journal ORDER BY horodatage DESC LIMIT ?1",
    )?;
    let v = st
        .query_map(params![limite], |r| {
            Ok(EntreeJournal {
                id: r.get(0)?,
                horodatage: r.get(1)?,
                declencheur: r.get(2)?,
                nom_fichier: r.get(3)?,
                taille_octets: r.get(4)?,
                statut: r.get(5)?,
                nb_destinations_ok: r.get(6)?,
                nb_destinations_echec: r.get(7)?,
                verifiee: r.get::<_, i64>(8)? != 0,
                message: r.get(9)?,
            })
        })?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(v)
}

// ---------------------------------------------------------------------------
// Chiffrement
// ---------------------------------------------------------------------------

/// Dérive une clé de 32 octets. Argon2id est volontairement **lent** : il rend
/// l'essai systématique de mots de passe coûteux. Un simple SHA-256 se teste à
/// des milliards par seconde et ne protégerait rien.
fn deriver_cle(secret: &[u8], sel: &[u8]) -> Result<[u8; 32]> {
    let mut cle = [0u8; 32];
    Argon2::default()
        .hash_password_into(secret, sel, &mut cle)
        .map_err(|e| CoreError::Rule(format!("préparation de la clé impossible : {e}")))?;
    Ok(cle)
}

/// Fabrique la clé correspondant à un mode.
///
/// `secret` est ce que l'utilisateur a fourni : sa **clé de licence** en mode
/// `licence`, son **mot de passe** en mode `motdepasse`, rien en mode `integree`.
/// C'est un seul champ à l'écran, parce que du point de vue de l'utilisateur
/// c'est une seule question : « quel secret ouvre ce fichier ? ».
fn cle_du_mode(mode: &str, secret: Option<&str>, sel: &[u8]) -> Result<[u8; 32]> {
    match mode {
        "integree" => deriver_cle(SECRET_INTEGRE, sel),
        "licence" => {
            let l = secret.map(str::trim).filter(|s| !s.is_empty()).ok_or_else(|| {
                CoreError::Rule(
                    "Cette sauvegarde est protégée par la clé de licence de l'installation. \
                     Saisissez la licence remise lors de l'installation pour continuer."
                        .into(),
                )
            })?;
            let mut secret_complet = PREFIXE_LICENCE.to_vec();
            secret_complet.extend_from_slice(l.as_bytes());
            deriver_cle(&secret_complet, sel)
        }
        "motdepasse" => {
            let mdp = secret.filter(|s| !s.is_empty()).ok_or_else(|| {
                CoreError::Rule(
                    "Cette sauvegarde est protégée par un mot de passe. Saisissez-le pour continuer."
                        .into(),
                )
            })?;
            deriver_cle(mdp.as_bytes(), sel)
        }
        autre => Err(CoreError::Rule(format!(
            "Mode de protection inconnu dans cette sauvegarde : {autre}"
        ))),
    }
}

fn empreinte_hex(cle: &[u8; 32]) -> String {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(b"djigui-empreinte-verification");
    h.update(cle);
    hex(&h.finalize())
}

fn hex(v: &[u8]) -> String {
    v.iter().map(|o| format!("{o:02x}")).collect()
}

fn dehex(s: &str) -> Result<Vec<u8>> {
    if s.len() % 2 != 0 {
        return Err(CoreError::Rule("fichier de sauvegarde illisible (entête abîmé)".into()));
    }
    (0..s.len())
        .step_by(2)
        .map(|i| {
            u8::from_str_radix(&s[i..i + 2], 16)
                .map_err(|_| CoreError::Rule("fichier de sauvegarde illisible (entête abîmé)".into()))
        })
        .collect()
}

/// Assemble le fichier final : magie + version + entête clair + contenu chiffré.
///
/// L'entête clair est **inclus dans le calcul d'authentification** (« données
/// associées »). Sans cela, quelqu'un pourrait changer la date affichée ou le
/// mode de clé annoncé sans que le déchiffrement ne s'en aperçoive.
fn emballer(entete: &EnteteArchive, cle: &[u8; 32], nonce: &[u8; 12], contenu: &[u8]) -> Result<Vec<u8>> {
    let json = serde_json::to_vec(entete)
        .map_err(|e| CoreError::Rule(format!("entête de sauvegarde : {e}")))?;
    let chiffreur = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(cle));
    let chiffre = chiffreur
        .encrypt(Nonce::from_slice(nonce), Payload { msg: contenu, aad: &json })
        .map_err(|_| CoreError::Rule("le chiffrement de la sauvegarde a échoué".into()))?;

    let mut sortie = Vec::with_capacity(chiffre.len() + json.len() + 32);
    sortie.extend_from_slice(MAGIE);
    sortie.push(VERSION_FORMAT);
    sortie.extend_from_slice(&(json.len() as u32).to_le_bytes());
    sortie.extend_from_slice(&json);
    sortie.extend_from_slice(&chiffre);
    Ok(sortie)
}

/// Lit l'entête **sans déchiffrer** : c'est ce qui permet d'annoncer la date et
/// le mode de clé d'une archive avant de demander son mot de passe.
pub fn lire_entete(octets: &[u8]) -> Result<EnteteArchive> {
    if octets.len() < MAGIE.len() + 5 || &octets[..MAGIE.len()] != MAGIE {
        return Err(CoreError::Rule(
            "Ce fichier n'est pas une sauvegarde Djigui.".into(),
        ));
    }
    let version = octets[MAGIE.len()];
    if version > VERSION_FORMAT {
        return Err(CoreError::Rule(format!(
            "Cette sauvegarde a été écrite par une version plus récente de Djigui (format {version}). \
             Mettez l'application à jour avant de la restaurer."
        )));
    }
    let debut = MAGIE.len() + 1;
    let taille = u32::from_le_bytes([
        octets[debut],
        octets[debut + 1],
        octets[debut + 2],
        octets[debut + 3],
    ]) as usize;
    let fin = debut + 4 + taille;
    if octets.len() < fin {
        return Err(CoreError::Rule("Fichier de sauvegarde incomplet ou tronqué.".into()));
    }
    serde_json::from_slice(&octets[debut + 4..fin])
        .map_err(|_| CoreError::Rule("Entête de sauvegarde illisible.".into()))
}

fn deballer(octets: &[u8], mot_de_passe: Option<&str>) -> Result<Vec<u8>> {
    let entete = lire_entete(octets)?;
    let debut = MAGIE.len() + 1;
    let taille = u32::from_le_bytes([
        octets[debut],
        octets[debut + 1],
        octets[debut + 2],
        octets[debut + 3],
    ]) as usize;
    let json = &octets[debut + 4..debut + 4 + taille];
    let chiffre = &octets[debut + 4 + taille..];

    let sel = dehex(&entete.sel)?;
    let nonce = dehex(&entete.nonce)?;
    let cle = cle_du_mode(&entete.mode_cle, mot_de_passe, &sel)?;

    let chiffreur = Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&cle));
    chiffreur
        .decrypt(Nonce::from_slice(&nonce), Payload { msg: chiffre, aad: json })
        .map_err(|_| {
            CoreError::Rule(
                if entete.mode_cle == "motdepasse" {
                    "Mot de passe incorrect, ou fichier de sauvegarde abîmé."
                } else {
                    "Fichier de sauvegarde abîmé : il a été modifié ou copié incomplètement."
                }
                .into(),
            )
        })
}

// ---------------------------------------------------------------------------
// Fabrication de l'archive
// ---------------------------------------------------------------------------

/// Écrit un instantané cohérent de la base dans un fichier temporaire.
///
/// ⚠️ `VACUUM INTO` et **pas** une copie de fichier : voir l'explication en tête
/// de module (mode WAL).
fn instantane_base(conn: &Connection, vers: &Path) -> Result<()> {
    // VACUUM INTO refuse d'écraser un fichier existant.
    let _ = std::fs::remove_file(vers);
    let cible = vers.to_string_lossy().replace('\'', "''");
    conn.execute_batch(&format!("VACUUM INTO '{cible}'"))?;
    Ok(())
}

fn ajouter_dossier_au_zip<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    racine: &Path,
    dossier: &Path,
    nb: &mut usize,
) -> Result<()> {
    let opts: zip::write::FileOptions<'_, ()> =
        zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let entrees = match std::fs::read_dir(dossier) {
        Ok(e) => e,
        // Un dossier `documents/` absent n'est pas une erreur : une installation
        // neuve n'a encore aucune pièce jointe.
        Err(_) => return Ok(()),
    };
    for e in entrees.flatten() {
        let chemin = e.path();
        if chemin.is_dir() {
            ajouter_dossier_au_zip(zip, racine, &chemin, nb)?;
        } else {
            let relatif = chemin.strip_prefix(racine).unwrap_or(&chemin);
            let nom = format!("{PREFIXE_DOCUMENTS}{}", relatif.to_string_lossy().replace('\\', "/"));
            let contenu = std::fs::read(&chemin)
                .map_err(|e| CoreError::Rule(format!("lecture de {} : {e}", chemin.display())))?;
            zip.start_file(nom, opts)
                .map_err(|e| CoreError::Rule(format!("archive : {e}")))?;
            zip.write_all(&contenu)
                .map_err(|e| CoreError::Rule(format!("archive : {e}")))?;
            *nb += 1;
        }
    }
    Ok(())
}

/// Construit l'archive chiffrée complète en mémoire.
fn fabriquer_archive(
    conn: &Connection,
    dossier_documents: &Path,
    dossier_travail: &Path,
    mode_cle: &str,
    mot_de_passe: Option<&str>,
) -> Result<(Vec<u8>, usize)> {
    std::fs::create_dir_all(dossier_travail)
        .map_err(|e| CoreError::Rule(format!("dossier de travail inaccessible : {e}")))?;
    let temporaire = dossier_travail.join(format!("instantane-{}.db", Uuid::new_v4()));
    instantane_base(conn, &temporaire)?;

    let mut nb_documents = 0usize;
    let contenu = {
        let curseur = std::io::Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(curseur);
        let opts: zip::write::FileOptions<'_, ()> =
            zip::write::FileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        zip.start_file(NOM_BASE_DANS_ARCHIVE, opts)
            .map_err(|e| CoreError::Rule(format!("archive : {e}")))?;
        let mut f = std::fs::File::open(&temporaire)
            .map_err(|e| CoreError::Rule(format!("instantané illisible : {e}")))?;
        let mut tampon = Vec::new();
        f.read_to_end(&mut tampon)
            .map_err(|e| CoreError::Rule(format!("instantané illisible : {e}")))?;
        zip.write_all(&tampon)
            .map_err(|e| CoreError::Rule(format!("archive : {e}")))?;
        drop(f);

        ajouter_dossier_au_zip(&mut zip, dossier_documents, dossier_documents, &mut nb_documents)?;
        zip.finish()
            .map_err(|e| CoreError::Rule(format!("archive : {e}")))?
            .into_inner()
    };
    let _ = std::fs::remove_file(&temporaire);

    let mut sel = [0u8; 16];
    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut sel);
    rand::thread_rng().fill_bytes(&mut nonce);

    let cle = cle_du_mode(mode_cle, mot_de_passe, &sel)?;

    let entete = EnteteArchive {
        mode_cle: mode_cle.to_string(),
        sel: hex(&sel),
        nonce: hex(&nonce),
        cree_le: now(),
        version_application: env!("CARGO_PKG_VERSION").to_string(),
        nb_documents,
        taille_contenu: contenu.len() as u64,
    };
    Ok((emballer(&entete, &cle, &nonce, &contenu)?, nb_documents))
}

// ---------------------------------------------------------------------------
// Exécution
// ---------------------------------------------------------------------------

fn dossier_utilisable(p: &Path) -> Result<()> {
    if p.as_os_str().is_empty() {
        return Err(CoreError::Rule("Aucun dossier indiqué.".into()));
    }
    if !p.exists() {
        return Err(CoreError::Rule(format!(
            "Le dossier « {} » est introuvable. Si c'est une clé USB ou un disque externe, \
             vérifiez qu'il est bien branché.",
            p.display()
        )));
    }
    if !p.is_dir() {
        return Err(CoreError::Rule(format!(
            "« {} » n'est pas un dossier.",
            p.display()
        )));
    }
    // Le seul test qui vaille : écrire pour de vrai. Un dossier peut exister,
    // être visible, et refuser l'écriture (lecture seule, droits réseau).
    let temoin = p.join(format!(".djigui-essai-{}", Uuid::new_v4()));
    std::fs::write(&temoin, b"djigui").map_err(|e| {
        CoreError::Rule(format!(
            "Impossible d'écrire dans « {} » : {e}. Vérifiez les droits d'accès.",
            p.display()
        ))
    })?;
    let _ = std::fs::remove_file(&temoin);
    Ok(())
}

fn nom_fichier_sauvegarde() -> String {
    // Nom triable et lisible à l'œil : c'est ce nom que l'utilisateur cherchera
    // dans son explorateur le jour du sinistre.
    format!("djigui-{}.djigui", chrono::Local::now().format("%Y%m%d-%H%M%S"))
}

/// Supprime les copies les plus anciennes d'une destination, au-delà de `garder`.
fn faire_le_menage(dossier: &Path, garder: usize) -> usize {
    let Ok(entrees) = std::fs::read_dir(dossier) else {
        return 0;
    };
    let mut fichiers: Vec<PathBuf> = entrees
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.is_file()
                && p.extension().map(|x| x == "djigui").unwrap_or(false)
                && p.file_name()
                    .map(|n| n.to_string_lossy().starts_with("djigui-"))
                    .unwrap_or(false)
        })
        .collect();
    // Tri par NOM et non par date du système de fichiers : la date de
    // modification d'un fichier copié par Drive ou par une clé USB n'est pas
    // fiable, alors que le nom porte l'horodatage que NOUS avons écrit.
    fichiers.sort();
    if fichiers.len() <= garder {
        return 0;
    }
    let a_supprimer = fichiers.len() - garder;
    fichiers
        .iter()
        .take(a_supprimer)
        .filter(|p| std::fs::remove_file(p).is_ok())
        .count()
}

/// Lance une sauvegarde complète.
///
/// `declencheur` : `"fermeture"` ou `"manuelle"`.
pub fn executer(
    conn: &Connection,
    dossier_documents: &Path,
    dossier_travail: &Path,
    declencheur: &str,
    mot_de_passe: Option<&str>,
) -> Result<ResultatSauvegarde> {
    let p = lire_parametres(conn)?;

    // ⚠️ Verrou de rôle : un poste client n'a ni la base ni les documents ; le
    // laisser écrire produirait une archive vide dans le dossier partagé, qui
    // prendrait la place de la vraie dans la rotation.
    if !p.cette_machine_est_serveur {
        return Err(CoreError::Rule(
            "Cette machine n'est pas le serveur Djigui : la sauvegarde se fait depuis le poste \
             qui héberge les données."
                .into(),
        ));
    }
    if !p.activee && declencheur != "manuelle" {
        return Err(CoreError::Rule("La sauvegarde automatique est désactivée.".into()));
    }

    let destinations: Vec<Destination> =
        lister_destinations(conn)?.into_iter().filter(|d| d.actif).collect();
    if destinations.is_empty() {
        let msg = "Aucun dossier de sauvegarde n'est configuré. Indiquez au moins un endroit \
                   où écrire les copies — de préférence un support qui n'est pas dans cet ordinateur.";
        journaliser(conn, declencheur, None, None, "echec", 0, 0, false, msg)?;
        return Err(CoreError::Rule(msg.into()));
    }

    // Résolution du secret. En mode `licence`, l'appelant n'a rien à fournir :
    // la licence est déjà en base. C'est indispensable pour la sauvegarde
    // AUTOMATIQUE de la fermeture, qui se déclenche sans personne devant l'écran.
    let secret_possede;
    let secret: Option<&str> = if p.mode_cle == "licence" {
        secret_possede = licence(conn).ok_or_else(|| {
            CoreError::Rule(
                "La sauvegarde utilise la clé de licence, mais aucune licence n'est enregistrée. \
                 Saisissez-la dans les réglages de sauvegarde."
                    .into(),
            )
        })?;
        Some(secret_possede.as_str())
    } else {
        mot_de_passe
    };

    let (archive, nb_documents) =
        fabriquer_archive(conn, dossier_documents, dossier_travail, &p.mode_cle, secret)?;
    let nom = nom_fichier_sauvegarde();
    let taille = archive.len() as u64;

    let mut resultats = Vec::new();
    let mut menage = 0usize;
    let mut verifiee = false;

    for d in &destinations {
        let cible = Path::new(&d.chemin).join(&nom);
        let issue = ecrire_et_verifier(&cible, &archive, secret);
        match &issue {
            Ok(()) => {
                menage += faire_le_menage(Path::new(&d.chemin), p.copies_a_conserver as usize);
                verifiee = true;
                resultats.push(ResultatDestination {
                    libelle: d.libelle.clone(),
                    chemin: d.chemin.clone(),
                    reussi: true,
                    message: "Copie écrite et relue avec succès.".into(),
                });
            }
            Err(e) => resultats.push(ResultatDestination {
                libelle: d.libelle.clone(),
                chemin: d.chemin.clone(),
                reussi: false,
                message: e.to_string(),
            }),
        }
        let (statut, message) = match &issue {
            Ok(()) => ("succes", String::new()),
            Err(e) => ("echec", e.to_string()),
        };
        conn.execute(
            "UPDATE sauvegarde_destination
                SET dernier_essai = ?1, dernier_statut = ?2, dernier_message = ?3
              WHERE id = ?4",
            params![now(), statut, message, d.id],
        )?;
    }

    let ok = resultats.iter().filter(|r| r.reussi).count();
    let echec = resultats.len() - ok;
    let statut = if ok == 0 {
        "echec"
    } else if echec > 0 {
        "partiel"
    } else {
        "succes"
    };

    let message = match statut {
        "succes" => format!(
            "Sauvegarde réussie : {} copie(s), {} document(s) joints, {}.",
            ok,
            nb_documents,
            taille_lisible(taille)
        ),
        "partiel" => format!(
            "Sauvegarde écrite sur {ok} destination(s), mais {echec} a/ont échoué. \
             Vos données sont sauvegardées, mais moins bien protégées que prévu."
        ),
        _ => "Aucune copie n'a pu être écrite. Vos données ne sont PAS sauvegardées.".to_string(),
    };

    journaliser(
        conn,
        declencheur,
        Some(&nom),
        Some(taille as i64),
        statut,
        ok as i64,
        echec as i64,
        verifiee,
        &message,
    )?;
    conn.execute(
        "UPDATE parametres_sauvegarde SET derniere_sauvegarde = ?1, dernier_statut = ?2 WHERE singleton = 1",
        params![now(), statut],
    )?;

    Ok(ResultatSauvegarde {
        statut: statut.into(),
        nom_fichier: nom,
        taille_octets: taille,
        verifiee,
        destinations: resultats,
        anciennes_supprimees: menage,
        message,
    })
}

/// Écrit la copie **puis la relit et la déchiffre**.
///
/// L'écriture se fait sous un nom temporaire renommé à la fin : si le courant
/// saute au milieu, on laisse un `.encours` visiblement inachevé plutôt qu'un
/// `.djigui` d'apparence normale qui se révélerait vide le jour du sinistre.
fn ecrire_et_verifier(cible: &Path, archive: &[u8], mot_de_passe: Option<&str>) -> Result<()> {
    let provisoire = cible.with_extension("encours");
    std::fs::write(&provisoire, archive).map_err(|e| {
        CoreError::Rule(format!("écriture impossible : {e}"))
    })?;
    std::fs::rename(&provisoire, cible).map_err(|e| {
        let _ = std::fs::remove_file(&provisoire);
        CoreError::Rule(format!("finalisation impossible : {e}"))
    })?;

    let relu = std::fs::read(cible)
        .map_err(|e| CoreError::Rule(format!("copie écrite mais illisible : {e}")))?;
    let contenu = deballer(&relu, mot_de_passe)
        .map_err(|e| CoreError::Rule(format!("copie écrite mais invérifiable : {e}")))?;
    // Dernier contrôle : l'archive s'ouvre et contient bien la base.
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&contenu))
        .map_err(|_| CoreError::Rule("copie écrite mais son contenu est illisible".into()))?;
    zip.by_name(NOM_BASE_DANS_ARCHIVE)
        .map_err(|_| CoreError::Rule("copie écrite mais elle ne contient pas la base".into()))?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn journaliser(
    conn: &Connection,
    declencheur: &str,
    nom: Option<&str>,
    taille: Option<i64>,
    statut: &str,
    ok: i64,
    echec: i64,
    verifiee: bool,
    message: &str,
) -> Result<()> {
    conn.execute(
        "INSERT INTO sauvegarde_journal
           (id, horodatage, declencheur, nom_fichier, taille_octets, statut,
            nb_destinations_ok, nb_destinations_echec, verifiee, message)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
        params![
            Uuid::new_v4().to_string(),
            now(),
            declencheur,
            nom,
            taille,
            statut,
            ok,
            echec,
            verifiee as i64,
            message
        ],
    )?;
    Ok(())
}

fn taille_lisible(o: u64) -> String {
    const M: f64 = 1024.0 * 1024.0;
    if (o as f64) < M {
        format!("{:.0} Ko", o as f64 / 1024.0)
    } else {
        format!("{:.1} Mo", o as f64 / M)
    }
}

// ---------------------------------------------------------------------------
// Restauration
// ---------------------------------------------------------------------------

/// Ce qu'on peut dire d'une archive **avant** de la restaurer, et avant même de
/// connaître son mot de passe.
#[derive(Debug, Clone, Serialize)]
pub struct ApercuArchive {
    pub cree_le: String,
    pub mode_cle: String,
    /// ⚠️ « un secret est-il nécessaire ? », et **pas** « est-ce un mot de
    /// passe ? ». Le mode `licence` en exige un lui aussi ; l'avoir oublié
    /// aurait fait un écran de restauration qui ne demande rien et échoue.
    pub secret_requis: bool,
    /// Ce qu'il faut demander, en toutes lettres, pour que l'utilisateur sache
    /// quoi aller chercher : sa licence n'est pas dans sa tête, elle est sur
    /// ses papiers d'installation.
    pub secret_attendu: String,
    pub version_application: String,
    pub nb_documents: usize,
    pub taille_fichier: u64,
}

pub fn apercu(chemin: &Path) -> Result<ApercuArchive> {
    let octets = std::fs::read(chemin)
        .map_err(|e| CoreError::Rule(format!("fichier illisible : {e}")))?;
    let e = lire_entete(&octets)?;
    let (secret_requis, secret_attendu) = match e.mode_cle.as_str() {
        "licence" => (
            true,
            "La clé de licence remise lors de l'installation".to_string(),
        ),
        "motdepasse" => (true, "Le mot de passe de sauvegarde".to_string()),
        _ => (false, "Aucun — cette sauvegarde s'ouvre directement".to_string()),
    };
    Ok(ApercuArchive {
        cree_le: e.cree_le,
        secret_requis,
        secret_attendu,
        mode_cle: e.mode_cle,
        version_application: e.version_application,
        nb_documents: e.nb_documents,
        taille_fichier: octets.len() as u64,
    })
}

#[derive(Debug, Clone, Serialize)]
pub struct ResultatRestauration {
    pub base_restauree: bool,
    pub nb_documents: usize,
    pub sauvegarde_de_securite: Option<String>,
    pub message: String,
}

/// Restaure une archive **par-dessus l'installation courante**.
///
/// ⚠️⚠️ Opération destructrice : elle remplace la base et les documents actuels.
/// Deux garde-fous, dans cet ordre :
///
/// 1. **On déchiffre et on vérifie TOUT avant de toucher à quoi que ce soit.**
///    Découvrir un mot de passe erroné après avoir effacé la base courante
///    laisserait l'utilisateur sans rien.
/// 2. **On met la base courante de côté** (`.avant-restauration`) au lieu de
///    l'écraser. Restaurer la mauvaise sauvegarde est une erreur banale et
///    paniquée ; elle doit rester réversible.
///
/// L'appelant doit ensuite **redémarrer le serveur** : la connexion ouverte
/// pointe encore sur l'ancien fichier.
pub fn restaurer(
    chemin_archive: &Path,
    chemin_base: &Path,
    dossier_documents: &Path,
    mot_de_passe: Option<&str>,
) -> Result<ResultatRestauration> {
    let octets = std::fs::read(chemin_archive)
        .map_err(|e| CoreError::Rule(format!("fichier illisible : {e}")))?;
    let contenu = deballer(&octets, mot_de_passe)?;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&contenu))
        .map_err(|_| CoreError::Rule("Le contenu de cette sauvegarde est illisible.".into()))?;

    // --- Étape 1 : tout extraire en zone d'attente, rien n'est encore écrasé.
    let attente = chemin_base
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!("restauration-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&attente)
        .map_err(|e| CoreError::Rule(format!("dossier de restauration : {e}")))?;

    let mut nb_documents = 0usize;
    let mut base_trouvee = false;
    for i in 0..zip.len() {
        let mut f = zip
            .by_index(i)
            .map_err(|_| CoreError::Rule("Sauvegarde abîmée.".into()))?;
        let nom = f.name().to_string();
        let mut donnees = Vec::new();
        f.read_to_end(&mut donnees)
            .map_err(|_| CoreError::Rule(format!("Contenu illisible : {nom}")))?;

        let destination = if nom == NOM_BASE_DANS_ARCHIVE {
            base_trouvee = true;
            attente.join("djigui.db")
        } else if let Some(reste) = nom.strip_prefix(PREFIXE_DOCUMENTS) {
            // Un nom d'entrée d'archive est une donnée venue de l'extérieur :
            // « ../ » y écrirait n'importe où sur le disque.
            if reste.contains("..") {
                return Err(CoreError::Rule("Sauvegarde refusée : chemin de fichier suspect.".into()));
            }
            nb_documents += 1;
            attente.join("documents").join(reste)
        } else {
            continue;
        };
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| CoreError::Rule(format!("dossier de restauration : {e}")))?;
        }
        std::fs::write(&destination, &donnees)
            .map_err(|e| CoreError::Rule(format!("écriture de {nom} : {e}")))?;
    }

    if !base_trouvee {
        let _ = std::fs::remove_dir_all(&attente);
        return Err(CoreError::Rule(
            "Cette sauvegarde ne contient pas de base de données : elle ne peut pas être restaurée."
                .into(),
        ));
    }

    // Contrôle de dernière minute : la base extraite s'ouvre-t-elle vraiment ?
    // Restaurer un fichier corrompu par-dessus une base saine serait le pire
    // résultat possible.
    {
        let essai = Connection::open(attente.join("djigui.db"))
            .map_err(|e| CoreError::Rule(format!("La base sauvegardée ne s'ouvre pas : {e}")))?;
        let ok: String = essai
            .query_row("PRAGMA integrity_check", [], |r| r.get(0))
            .map_err(|e| CoreError::Rule(format!("La base sauvegardée est illisible : {e}")))?;
        if ok != "ok" {
            let _ = std::fs::remove_dir_all(&attente);
            return Err(CoreError::Rule(
                "La base contenue dans cette sauvegarde est endommagée. Restauration annulée : \
                 vos données actuelles n'ont pas été touchées."
                    .into(),
            ));
        }
    }

    // --- Étape 2 : mise de côté de l'existant, puis bascule.
    let mut sauvegarde_de_securite = None;
    if chemin_base.exists() {
        let horo = chrono::Local::now().format("%Y%m%d-%H%M%S");
        let mis_de_cote = chemin_base.with_extension(format!("avant-restauration-{horo}.db"));
        std::fs::rename(chemin_base, &mis_de_cote)
            .map_err(|e| CoreError::Rule(format!("impossible de mettre l'ancienne base de côté : {e}")))?;
        sauvegarde_de_securite = Some(mis_de_cote.to_string_lossy().to_string());
    }
    // Les journaux WAL de l'ancienne base n'ont plus de sens et corrompraient
    // la nouvelle s'ils étaient rejoués par-dessus.
    for suffixe in ["-wal", "-shm"] {
        let p = PathBuf::from(format!("{}{suffixe}", chemin_base.to_string_lossy()));
        let _ = std::fs::remove_file(p);
    }
    std::fs::rename(attente.join("djigui.db"), chemin_base)
        .map_err(|e| CoreError::Rule(format!("mise en place de la base : {e}")))?;

    if attente.join("documents").exists() {
        // Les documents existants sont conservés à côté : la sauvegarde peut
        // être plus ancienne que certaines pièces jointes, les effacer
        // détruirait des fichiers que l'archive ne contient pas.
        let _ = std::fs::create_dir_all(dossier_documents);
        copier_recursivement(&attente.join("documents"), dossier_documents)?;
    }
    let _ = std::fs::remove_dir_all(&attente);

    Ok(ResultatRestauration {
        base_restauree: true,
        nb_documents,
        sauvegarde_de_securite,
        message: format!(
            "Restauration terminée : base remise en place et {nb_documents} document(s) rétablis. \
             Redémarrez Djigui pour travailler sur les données restaurées."
        ),
    })
}

fn copier_recursivement(de: &Path, vers: &Path) -> Result<()> {
    for e in std::fs::read_dir(de)
        .map_err(|e| CoreError::Rule(format!("lecture du dossier restauré : {e}")))?
        .flatten()
    {
        let source = e.path();
        let cible = vers.join(e.file_name());
        if source.is_dir() {
            std::fs::create_dir_all(&cible)
                .map_err(|e| CoreError::Rule(format!("création de dossier : {e}")))?;
            copier_recursivement(&source, &cible)?;
        } else {
            std::fs::copy(&source, &cible)
                .map_err(|e| CoreError::Rule(format!("copie de fichier : {e}")))?;
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn base_test() -> (Connection, tempdir::Dossier) {
        let d = tempdir::Dossier::neuf();
        let conn = crate::db::open(d.chemin().join("djigui.db")).unwrap();
        (conn, d)
    }

    /// Pose une raison sociale reconnaissable dans la base.
    ///
    /// ⚠️ Passer par `parametres::enregistrer` et **pas** par un `UPDATE` direct :
    /// le singleton `parametres_entreprise` n'est créé qu'à la première lecture,
    /// donc un `UPDATE` sur une base neuve ne touche aucune ligne — en silence.
    /// Deux de ces tests passaient au vert en ne vérifiant rien à cause de ça.
    fn poser_nom(conn: &Connection, nom: &str) {
        let mut p = crate::modules::parametres::lire(conn).unwrap();
        p.raison_sociale = nom.into();
        crate::modules::parametres::enregistrer(conn, &p).unwrap();
        let relu: String = conn
            .query_row(
                "SELECT raison_sociale FROM parametres_entreprise WHERE singleton = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(relu, nom, "le nom doit être réellement en base avant de tester");
    }

    /// Petit dossier temporaire maison — le projet n'a pas de dépendance de
    /// test, et en ajouter une pour trois lignes n'en vaut pas la peine.
    mod tempdir {
        use std::path::PathBuf;
        pub struct Dossier(PathBuf);
        impl Dossier {
            pub fn neuf() -> Self {
                let p = std::env::temp_dir().join(format!("djigui-test-{}", uuid::Uuid::new_v4()));
                std::fs::create_dir_all(&p).unwrap();
                Self(p)
            }
            pub fn chemin(&self) -> &PathBuf {
                &self.0
            }
        }
        impl Drop for Dossier {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
    }

    #[test]
    fn une_sauvegarde_ecrite_se_relit_et_contient_la_base() {
        let (conn, d) = base_test();
        let docs = d.chemin().join("documents");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(docs.join("piece.txt"), b"une piece jointe").unwrap();

        let dest = d.chemin().join("copies");
        std::fs::create_dir_all(&dest).unwrap();
        super::ajouter_destination(
            &conn,
            &NouvelleDestination {
                libelle: "Dossier d'essai".into(),
                chemin: dest.to_string_lossy().to_string(),
                actif: true,
            },
        )
        .unwrap();

        let r = executer(&conn, &docs, d.chemin(), "manuelle", None).unwrap();
        assert_eq!(r.statut, "succes");
        assert!(r.verifiee, "la copie doit avoir été relue");
        assert!(dest.join(&r.nom_fichier).exists());
    }

    /// Le point qui compte vraiment : le fichier posé sur la clé USB ne doit
    /// PAS laisser lire son contenu à qui l'ouvre avec un éditeur.
    #[test]
    fn le_fichier_ecrit_ne_laisse_rien_lire_en_clair() {
        let (conn, d) = base_test();
        // On met en base une chaîne reconnaissable.
        poser_nom(&conn, "SECRET-COMMERCIAL-XYZ");
        let docs = d.chemin().join("documents");
        std::fs::create_dir_all(&docs).unwrap();
        let dest = d.chemin().join("copies");
        std::fs::create_dir_all(&dest).unwrap();
        ajouter_destination(
            &conn,
            &NouvelleDestination {
                libelle: "Essai".into(),
                chemin: dest.to_string_lossy().to_string(),
                actif: true,
            },
        )
        .unwrap();

        let r = executer(&conn, &docs, d.chemin(), "manuelle", None).unwrap();
        let brut = std::fs::read(dest.join(&r.nom_fichier)).unwrap();
        assert!(
            !contient(&brut, b"SECRET-COMMERCIAL-XYZ"),
            "le contenu de la base ne doit jamais apparaître en clair dans l'archive"
        );
        assert!(
            !contient(&brut, b"SQLite format 3"),
            "l'entête SQLite ne doit pas être reconnaissable : sinon on sait quoi attaquer"
        );
    }

    fn contient(foin: &[u8], aiguille: &[u8]) -> bool {
        foin.windows(aiguille.len()).any(|f| f == aiguille)
    }

    #[test]
    fn le_tour_complet_sauvegarde_puis_restauration_rend_les_donnees() {
        let (conn, d) = base_test();
        poser_nom(&conn, "AVANT-SINISTRE");
        let docs = d.chemin().join("documents");
        std::fs::create_dir_all(&docs).unwrap();
        std::fs::write(docs.join("facture.pdf"), b"contenu").unwrap();
        let dest = d.chemin().join("copies");
        std::fs::create_dir_all(&dest).unwrap();
        ajouter_destination(
            &conn,
            &NouvelleDestination {
                libelle: "Essai".into(),
                chemin: dest.to_string_lossy().to_string(),
                actif: true,
            },
        )
        .unwrap();
        let r = executer(&conn, &docs, d.chemin(), "manuelle", None).unwrap();

        // Le sinistre : on modifie les données et on efface une pièce jointe.
        poser_nom(&conn, "APRES-SINISTRE");
        std::fs::remove_file(docs.join("facture.pdf")).unwrap();
        drop(conn);

        let res = restaurer(&dest.join(&r.nom_fichier), &d.chemin().join("djigui.db"), &docs, None)
            .unwrap();
        assert!(res.base_restauree);
        assert!(
            res.sauvegarde_de_securite.is_some(),
            "l'ancienne base doit être mise de côté, jamais écrasée"
        );
        assert!(docs.join("facture.pdf").exists(), "les documents doivent revenir");

        let conn2 = crate::db::open(d.chemin().join("djigui.db")).unwrap();
        let nom: String = conn2
            .query_row(
                "SELECT raison_sociale FROM parametres_entreprise WHERE singleton = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(nom, "AVANT-SINISTRE");
    }

    #[test]
    fn un_mot_de_passe_errone_est_refuse_avant_toute_ecriture() {
        let (conn, d) = base_test();
        definir_mot_de_passe(&conn, Some("motdepasse-solide")).unwrap();
        assert!(verifier_mot_de_passe(&conn, "motdepasse-solide").unwrap());
        assert!(!verifier_mot_de_passe(&conn, "autre-chose-ici").unwrap());

        let docs = d.chemin().join("documents");
        std::fs::create_dir_all(&docs).unwrap();
        let dest = d.chemin().join("copies");
        std::fs::create_dir_all(&dest).unwrap();
        ajouter_destination(
            &conn,
            &NouvelleDestination {
                libelle: "Essai".into(),
                chemin: dest.to_string_lossy().to_string(),
                actif: true,
            },
        )
        .unwrap();
        let r = executer(&conn, &docs, d.chemin(), "manuelle", Some("motdepasse-solide")).unwrap();
        assert_eq!(r.statut, "succes");

        let archive = dest.join(&r.nom_fichier);
        // L'aperçu doit fonctionner SANS le mot de passe, sinon l'écran de
        // restauration ne peut rien annoncer à l'utilisateur.
        let a = apercu(&archive).unwrap();
        assert!(a.secret_requis);

        let base = d.chemin().join("djigui.db");
        let avant = std::fs::metadata(&base).unwrap().len();
        assert!(restaurer(&archive, &base, &docs, Some("mauvais-mot-de-passe")).is_err());
        assert!(restaurer(&archive, &base, &docs, None).is_err());
        assert_eq!(
            std::fs::metadata(&base).unwrap().len(),
            avant,
            "un échec de mot de passe ne doit PAS avoir touché la base en place"
        );
    }

    #[test]
    fn une_archive_trafiquee_est_refusee_et_ne_detruit_rien() {
        let (conn, d) = base_test();
        let docs = d.chemin().join("documents");
        std::fs::create_dir_all(&docs).unwrap();
        let dest = d.chemin().join("copies");
        std::fs::create_dir_all(&dest).unwrap();
        ajouter_destination(
            &conn,
            &NouvelleDestination {
                libelle: "Essai".into(),
                chemin: dest.to_string_lossy().to_string(),
                actif: true,
            },
        )
        .unwrap();
        let r = executer(&conn, &docs, d.chemin(), "manuelle", None).unwrap();

        let archive = dest.join(&r.nom_fichier);
        let mut octets = std::fs::read(&archive).unwrap();
        let dernier = octets.len() - 5;
        octets[dernier] ^= 0xFF; // un octet modifié suffit
        std::fs::write(&archive, &octets).unwrap();

        let base = d.chemin().join("djigui.db");
        let avant = std::fs::metadata(&base).unwrap().len();
        assert!(
            restaurer(&archive, &base, &docs, None).is_err(),
            "une archive modifiée doit être refusée, pas restaurée à moitié"
        );
        assert_eq!(std::fs::metadata(&base).unwrap().len(), avant);
    }

    #[test]
    fn la_rotation_garde_les_plus_recentes() {
        let d = tempdir::Dossier::neuf();
        for n in ["djigui-20260101-000000", "djigui-20260102-000000", "djigui-20260103-000000"] {
            std::fs::write(d.chemin().join(format!("{n}.djigui")), b"x").unwrap();
        }
        // Un fichier étranger au dossier ne doit jamais être emporté.
        std::fs::write(d.chemin().join("photo-vacances.jpg"), b"x").unwrap();

        assert_eq!(faire_le_menage(d.chemin(), 2), 1);
        assert!(!d.chemin().join("djigui-20260101-000000.djigui").exists());
        assert!(d.chemin().join("djigui-20260103-000000.djigui").exists());
        assert!(d.chemin().join("photo-vacances.jpg").exists());
    }

    #[test]
    fn un_poste_client_ne_sauvegarde_pas() {
        let (conn, d) = base_test();
        conn.execute(
            "UPDATE parametres_sauvegarde SET cette_machine_est_serveur = 0 WHERE singleton = 1",
            [],
        )
        .unwrap();
        let docs = d.chemin().join("documents");
        std::fs::create_dir_all(&docs).unwrap();
        let e = executer(&conn, &docs, d.chemin(), "manuelle", None).unwrap_err();
        assert!(e.to_string().contains("serveur"));
    }

    /// Prépare une base avec une destination d'essai déjà configurée.
    fn avec_destination(d: &tempdir::Dossier, conn: &Connection) -> (PathBuf, PathBuf) {
        let docs = d.chemin().join("documents");
        std::fs::create_dir_all(&docs).unwrap();
        let dest = d.chemin().join("copies");
        std::fs::create_dir_all(&dest).unwrap();
        ajouter_destination(
            conn,
            &NouvelleDestination {
                libelle: "Essai".into(),
                chemin: dest.to_string_lossy().to_string(),
                actif: true,
            },
        )
        .unwrap();
        (docs, dest)
    }

    /// Le mode normal en exploitation : la licence du client sert de secret, et
    /// la sauvegarde automatique n'a besoin de personne devant l'écran.
    #[test]
    fn la_licence_devient_la_cle_et_la_sauvegarde_tourne_toute_seule() {
        let (conn, d) = base_test();
        let p = definir_licence(&conn, "DJG-MATAM-2026-7Q4X").unwrap();
        assert_eq!(p.mode_cle, "licence");
        assert!(p.licence_definie);
        assert_eq!(p.licence_fin.as_deref(), Some("7Q4X"));

        poser_nom(&conn, "SECRET-COMMERCIAL-XYZ");
        let (docs, dest) = avec_destination(&d, &conn);

        // Aucun secret fourni par l'appelant : il vient de la base.
        let r = executer(&conn, &docs, d.chemin(), "fermeture", None).unwrap();
        assert_eq!(r.statut, "succes");

        let archive = dest.join(&r.nom_fichier);
        let brut = std::fs::read(&archive).unwrap();
        assert!(!contient(&brut, b"SECRET-COMMERCIAL-XYZ"));
        // ⚠️ La licence elle-même ne doit pas se retrouver écrite dans le
        // fichier : elle en est la clé, l'y joindre reviendrait à scotcher la
        // clé sur le coffre.
        assert!(!contient(&brut, b"DJG-MATAM-2026-7Q4X"));

        let a = apercu(&archive).unwrap();
        assert_eq!(a.mode_cle, "licence");
        assert!(a.secret_requis, "l'écran doit demander un secret");
        assert!(a.secret_attendu.contains("licence"), "et dire lequel");
    }

    /// Le cas de la réinstallation : machine neuve, base vide, l'utilisateur
    /// n'a QUE sa licence sur papier. Il doit pouvoir tout récupérer.
    #[test]
    fn une_installation_neuve_restaure_avec_la_seule_licence() {
        let (conn, d) = base_test();
        definir_licence(&conn, "DJG-MATAM-2026-7Q4X").unwrap();
        poser_nom(&conn, "AVANT-SINISTRE");
        let (docs, dest) = avec_destination(&d, &conn);
        let r = executer(&conn, &docs, d.chemin(), "fermeture", None).unwrap();
        drop(conn);

        // Machine neuve : autre dossier, aucune base, aucune licence en mémoire.
        let neuve = tempdir::Dossier::neuf();
        let base_neuve = neuve.chemin().join("djigui.db");
        let docs_neufs = neuve.chemin().join("documents");
        let archive = dest.join(&r.nom_fichier);

        assert!(
            restaurer(&archive, &base_neuve, &docs_neufs, None).is_err(),
            "sans la licence, la restauration doit être refusée"
        );
        assert!(restaurer(&archive, &base_neuve, &docs_neufs, Some("DJG-AUTRE-CLIENT")).is_err());

        restaurer(&archive, &base_neuve, &docs_neufs, Some("DJG-MATAM-2026-7Q4X")).unwrap();
        let conn2 = crate::db::open(&base_neuve).unwrap();
        let nom: String = conn2
            .query_row(
                "SELECT raison_sociale FROM parametres_entreprise WHERE singleton = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(nom, "AVANT-SINISTRE");
    }

    /// Saisir la licence après coup ne doit pas condamner les archives déjà
    /// écrites avec la clé intégrée : chacune garde son mode.
    #[test]
    fn les_archives_faites_avant_la_licence_restent_lisibles() {
        let (conn, d) = base_test();
        let (docs, dest) = avec_destination(&d, &conn);
        // Installation neuve, pas encore de licence : mode 'integree'.
        assert_eq!(lire_parametres(&conn).unwrap().mode_cle, "integree");
        let ancienne = executer(&conn, &docs, d.chemin(), "manuelle", None).unwrap();

        definir_licence(&conn, "DJG-MATAM-2026-7Q4X").unwrap();
        let nouvelle = executer(&conn, &docs, d.chemin(), "manuelle", None).unwrap();

        assert_eq!(apercu(&dest.join(&ancienne.nom_fichier)).unwrap().mode_cle, "integree");
        assert_eq!(apercu(&dest.join(&nouvelle.nom_fichier)).unwrap().mode_cle, "licence");
        // L'ancienne s'ouvre toujours sans rien saisir.
        let base = d.chemin().join("djigui.db");
        drop(conn);
        restaurer(&dest.join(&ancienne.nom_fichier), &base, &docs, None).unwrap();
    }

    /// Un mot de passe posé volontairement l'emporte sur la licence : on ne
    /// retire pas dans son dos la protection que le client a choisie.
    #[test]
    fn un_mot_de_passe_choisi_n_est_pas_ecrase_par_la_licence() {
        let (conn, _d) = base_test();
        definir_mot_de_passe(&conn, Some("phrase-bien-a-moi")).unwrap();
        let p = definir_licence(&conn, "DJG-MATAM-2026-7Q4X").unwrap();
        assert_eq!(p.mode_cle, "motdepasse");
        assert!(p.licence_definie, "la licence est enregistrée malgré tout");

        // En la retirant, on retombe sur le mode licence, pas sur la clé intégrée.
        let p2 = definir_mot_de_passe(&conn, None).unwrap();
        assert_eq!(p2.mode_cle, "licence");
    }

    #[test]
    fn un_fichier_quelconque_n_est_pas_pris_pour_une_sauvegarde() {
        let d = tempdir::Dossier::neuf();
        let f = d.chemin().join("document.pdf");
        std::fs::write(&f, b"%PDF-1.7 ceci n'est pas une sauvegarde").unwrap();
        let e = apercu(&f).unwrap_err();
        assert!(e.to_string().contains("pas une sauvegarde"));
    }
}
