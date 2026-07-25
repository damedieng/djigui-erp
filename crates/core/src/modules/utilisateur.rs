//! Utilisateurs & authentification (§ gestion des accès).
//!
//! Chaque utilisateur se connecte avec un `login` + mot de passe. Le mot de
//! passe n'est **jamais** stocké en clair : on garde un hachage salé
//! (`sha256$iterations$sel_hex$hash_hex`). Deux rôles : `admin` (tout, dont la
//! gestion des utilisateurs et des paramètres) et `caissier` (caisse/ventes).
//!
//! L'utilisateur par défaut « djigui / djigui » (admin) est créé à
//! l'installation par [`assurer_defaut`], appelée après les migrations.

use crate::domain::RoleUtilisateur;
use crate::error::{CoreError, Result};
use crate::now;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

/// Nombre d'itérations du hachage (étirement simple contre le brute-force).
const ITERATIONS: u32 = 50_000;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Utilisateur exposé (jamais le hachage du mot de passe).
#[derive(Debug, Clone, Serialize)]
pub struct Utilisateur {
    pub id: String,
    pub login: String,
    pub nom: String,
    pub role: RoleUtilisateur,
    pub actif: bool,
    pub cree_le: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct NouvelUtilisateur {
    pub login: String,
    pub mot_de_passe: String,
    pub nom: String,
    #[serde(default = "role_caissier")]
    pub role: RoleUtilisateur,
}
fn role_caissier() -> RoleUtilisateur { RoleUtilisateur::Caissier }

/// Modification : le mot de passe est optionnel (vide = inchangé).
#[derive(Debug, Clone, Deserialize)]
pub struct MajUtilisateur {
    pub nom: String,
    pub role: RoleUtilisateur,
    pub actif: bool,
    #[serde(default)]
    pub mot_de_passe: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Identifiants {
    pub login: String,
    pub mot_de_passe: String,
}

// ---------------------------------------------------------------------------
// Hachage (sha256 salé + itéré)
// ---------------------------------------------------------------------------

fn hacher(mot_de_passe: &str, sel_hex: &str, iterations: u32) -> String {
    // h0 = sha256(sel || mot_de_passe), puis on ré-hache `iterations` fois.
    let mut courant = {
        let mut h = Sha256::new();
        h.update(sel_hex.as_bytes());
        h.update(mot_de_passe.as_bytes());
        h.finalize().to_vec()
    };
    for _ in 1..iterations {
        let mut h = Sha256::new();
        h.update(&courant);
        courant = h.finalize().to_vec();
    }
    hex(&courant)
}

fn hex(octets: &[u8]) -> String {
    let mut s = String::with_capacity(octets.len() * 2);
    for b in octets {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Fabrique une empreinte stockable : `sha256$iterations$sel$hash`.
fn empreinte(mot_de_passe: &str) -> String {
    let sel = Uuid::new_v4().simple().to_string();
    let hash = hacher(mot_de_passe, &sel, ITERATIONS);
    format!("sha256${ITERATIONS}${sel}${hash}")
}

/// Vérifie un mot de passe contre une empreinte stockée.
fn verifier(mot_de_passe: &str, empreinte_stockee: &str) -> bool {
    let parts: Vec<&str> = empreinte_stockee.split('$').collect();
    if parts.len() != 4 || parts[0] != "sha256" {
        return false;
    }
    let iterations: u32 = match parts[1].parse() {
        Ok(n) => n,
        Err(_) => return false,
    };
    let attendu = parts[3];
    let calcule = hacher(mot_de_passe, parts[2], iterations);
    // comparaison simple ; les empreintes ne sont pas exposées
    calcule == attendu
}

// ---------------------------------------------------------------------------
// Installation : utilisateur par défaut
// ---------------------------------------------------------------------------

/// Crée l'utilisateur admin par défaut « djigui / djigui » si AUCUN utilisateur
/// n'existe encore. Idempotent : ne fait rien si la table contient déjà quelqu'un.
pub fn assurer_defaut(conn: &Connection) -> Result<()> {
    let n: i64 = conn.query_row("SELECT COUNT(*) FROM utilisateur", [], |r| r.get(0))?;
    if n > 0 {
        return Ok(());
    }
    inserer(conn, "djigui", "djigui", "Administrateur", RoleUtilisateur::Admin)?;
    tracing::info!("utilisateur par défaut créé : djigui / djigui (admin)");
    Ok(())
}

fn inserer(conn: &Connection, login: &str, mdp: &str, nom: &str, role: RoleUtilisateur) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO utilisateur (id, login, mot_de_passe_hash, nom, role, actif, cree_le)
         VALUES (?1,?2,?3,?4,?5,1,?6)",
        params![id, login, empreinte(mdp), nom, role.as_str(), now()],
    )?;
    Ok(id)
}

// ---------------------------------------------------------------------------
// CRUD
// ---------------------------------------------------------------------------

pub fn creer(conn: &Connection, u: &NouvelUtilisateur) -> Result<Utilisateur> {
    let login = u.login.trim();
    if login.is_empty() {
        return Err(CoreError::Rule("le login est obligatoire".into()));
    }
    if u.mot_de_passe.len() < 4 {
        return Err(CoreError::Rule("le mot de passe doit faire au moins 4 caractères".into()));
    }
    let existe: bool = conn
        .query_row("SELECT 1 FROM utilisateur WHERE login = ?1", params![login], |_| Ok(true))
        .unwrap_or(false);
    if existe {
        return Err(CoreError::Rule(format!("le login « {login} » est déjà utilisé")));
    }
    let id = inserer(conn, login, &u.mot_de_passe, u.nom.trim(), u.role)?;
    lire(conn, &id)
}

pub fn modifier(conn: &Connection, id: &str, m: &MajUtilisateur) -> Result<Utilisateur> {
    // vérifie l'existence
    lire(conn, id)?;
    conn.execute(
        "UPDATE utilisateur SET nom = ?2, role = ?3, actif = ?4 WHERE id = ?1",
        params![id, m.nom.trim(), m.role.as_str(), m.actif as i64],
    )?;
    if let Some(mdp) = m.mot_de_passe.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        if mdp.len() < 4 {
            return Err(CoreError::Rule("le mot de passe doit faire au moins 4 caractères".into()));
        }
        conn.execute(
            "UPDATE utilisateur SET mot_de_passe_hash = ?2 WHERE id = ?1",
            params![id, empreinte(mdp)],
        )?;
    }
    lire(conn, id)
}

/// Désactive un utilisateur (ne le supprime pas — traçabilité). Refuse de
/// désactiver le dernier admin actif.
pub fn desactiver(conn: &Connection, id: &str) -> Result<()> {
    let u = lire(conn, id)?;
    if u.role == RoleUtilisateur::Admin {
        let admins_actifs: i64 = conn.query_row(
            "SELECT COUNT(*) FROM utilisateur WHERE role = 'admin' AND actif = 1",
            [], |r| r.get(0))?;
        if admins_actifs <= 1 {
            return Err(CoreError::Rule("impossible de désactiver le dernier administrateur actif".into()));
        }
    }
    conn.execute("UPDATE utilisateur SET actif = 0 WHERE id = ?1", params![id])?;
    Ok(())
}

pub fn lire(conn: &Connection, id: &str) -> Result<Utilisateur> {
    conn.query_row(
        "SELECT id, login, nom, role, actif, cree_le FROM utilisateur WHERE id = ?1",
        params![id], ligne_vers_utilisateur,
    )
    .map_err(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => CoreError::NotFound(format!("utilisateur {id}")),
        autre => autre.into(),
    })
}

pub fn lister(conn: &Connection) -> Result<Vec<Utilisateur>> {
    let mut stmt = conn.prepare(
        "SELECT id, login, nom, role, actif, cree_le FROM utilisateur ORDER BY actif DESC, login",
    )?;
    let rows = stmt.query_map([], ligne_vers_utilisateur)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn ligne_vers_utilisateur(r: &rusqlite::Row) -> rusqlite::Result<Utilisateur> {
    let role: String = r.get(3)?;
    Ok(Utilisateur {
        id: r.get(0)?,
        login: r.get(1)?,
        nom: r.get(2)?,
        role: RoleUtilisateur::parse(&role).unwrap_or(RoleUtilisateur::Caissier),
        actif: r.get::<_, i64>(4)? != 0,
        cree_le: r.get(5)?,
    })
}

// ---------------------------------------------------------------------------
// Authentification
// ---------------------------------------------------------------------------

/// Vérifie login + mot de passe. Renvoie l'utilisateur si OK et actif.
pub fn authentifier(conn: &Connection, ids: &Identifiants) -> Result<Utilisateur> {
    let login = ids.login.trim();
    let ligne: Option<(String, String, i64)> = conn
        .query_row(
            "SELECT id, mot_de_passe_hash, actif FROM utilisateur WHERE login = ?1",
            params![login],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .ok();
    match ligne {
        Some((id, empreinte_stockee, actif)) if verifier(&ids.mot_de_passe, &empreinte_stockee) => {
            if actif == 0 {
                return Err(CoreError::Unauthorized("ce compte est désactivé".into()));
            }
            lire(conn, &id)
        }
        _ => Err(CoreError::Unauthorized("login ou mot de passe incorrect".into())),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;

    #[test]
    fn user_defaut_djigui_existe_et_se_connecte() {
        let conn = db::open_in_memory().unwrap();
        let u = authentifier(&conn, &Identifiants {
            login: "djigui".into(), mot_de_passe: "djigui".into(),
        }).unwrap();
        assert_eq!(u.login, "djigui");
        assert_eq!(u.role, RoleUtilisateur::Admin);
        // mauvais mot de passe refusé
        assert!(authentifier(&conn, &Identifiants {
            login: "djigui".into(), mot_de_passe: "mauvais".into(),
        }).is_err());
    }

    #[test]
    fn creation_et_login_nouvel_utilisateur() {
        let conn = db::open_in_memory().unwrap();
        creer(&conn, &NouvelUtilisateur {
            login: "caisse1".into(), mot_de_passe: "1234".into(),
            nom: "Caissier 1".into(), role: RoleUtilisateur::Caissier,
        }).unwrap();
        let u = authentifier(&conn, &Identifiants {
            login: "caisse1".into(), mot_de_passe: "1234".into(),
        }).unwrap();
        assert_eq!(u.role, RoleUtilisateur::Caissier);
        // login en double refusé
        assert!(creer(&conn, &NouvelUtilisateur {
            login: "caisse1".into(), mot_de_passe: "5678".into(),
            nom: "Doublon".into(), role: RoleUtilisateur::Caissier,
        }).is_err());
    }

    #[test]
    fn empreinte_ne_contient_pas_le_mot_de_passe_en_clair() {
        let e = empreinte("secret123");
        assert!(!e.contains("secret123"));
        assert!(verifier("secret123", &e));
        assert!(!verifier("secret124", &e));
    }

    #[test]
    fn dernier_admin_non_desactivable() {
        let conn = db::open_in_memory().unwrap();
        let admin = lister(&conn).unwrap().into_iter()
            .find(|u| u.role == RoleUtilisateur::Admin).unwrap();
        assert!(desactiver(&conn, &admin.id).is_err());
    }
}
