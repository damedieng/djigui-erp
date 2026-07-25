//! Notifications quotidiennes — « ce qui demande votre attention aujourd'hui ».
//!
//! **Rien n'est stocké** : tout est recalculé à la demande depuis les données
//! existantes. Une table d'alertes se désynchroniserait de la réalité (une
//! facture réglée, une tâche terminée, et l'alerte resterait), alors qu'un
//! calcul à la volée est toujours juste. Le seul état conservé est « lu »,
//! posé sur une **clé stable** (migration 0030).
//!
//! Toutes les sources dérivent de ce qui existe déjà : projets, activités,
//! jalons, livrables, liens de précédence, agenda, stock, abonnements, caisse.

use crate::error::Result;
use crate::now;
use rusqlite::{params, Connection};
use serde::Serialize;

/// Importance d'une notification. Sert au tri et à la couleur.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Gravite {
    /// Information : à savoir, rien d'urgent.
    Info,
    /// Attention : échéance proche, à surveiller.
    Attention,
    /// Urgent : échéance dépassée.
    Urgent,
}

impl Gravite {
    fn rang(self) -> u8 {
        match self {
            Gravite::Urgent => 0,
            Gravite::Attention => 1,
            Gravite::Info => 2,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Notification {
    /// Clé **stable** entre deux calculs : c'est elle qui porte l'état « lu ».
    /// Elle change si le fond change (ex. le nombre de jours de retard), pour
    /// qu'une aggravation réapparaisse au lieu de rester masquée.
    pub cle: String,
    pub categorie: String,
    pub gravite: Gravite,
    pub titre: String,
    pub detail: String,
    /// Page à ouvrir au clic.
    pub lien: String,
    pub lu: bool,
}

/// Toutes les notifications du jour, les plus urgentes d'abord.
pub fn lister(conn: &Connection) -> Result<Vec<Notification>> {
    let aujourdhui = now()[..10].to_string();
    let j = aujourdhui.as_str();
    let mut n = Vec::new();

    projets_en_retard(conn, j, &mut n)?;
    activites_en_retard(conn, j, &mut n)?;
    jalons(conn, j, &mut n)?;
    livrables(conn, j, &mut n)?;
    liens_incoherents(conn, &mut n)?;
    rendez_vous_du_jour(conn, j, &mut n)?;
    stock_bas(conn, &mut n)?;
    abonnements_dus(conn, j, &mut n)?;
    caisses_ouvertes(conn, j, &mut n)?;

    // Marquage « lu » (table 0030).
    let mut lues = std::collections::HashSet::new();
    let mut st = conn.prepare("SELECT cle FROM notification_lue")?;
    for r in st.query_map([], |r| r.get::<_, String>(0))? {
        lues.insert(r?);
    }
    for x in n.iter_mut() {
        x.lu = lues.contains(&x.cle);
    }

    n.sort_by_key(|x| (x.gravite.rang(), x.categorie.clone()));
    Ok(n)
}

/// Marque des notifications comme lues. Les clés inconnues sont ignorées :
/// elles peuvent venir d'un calcul précédent.
pub fn marquer_lues(conn: &Connection, cles: &[String]) -> Result<usize> {
    let mut n = 0;
    for c in cles {
        n += conn.execute(
            "INSERT OR IGNORE INTO notification_lue (cle, lu_le) VALUES (?1, ?2)",
            params![c, now()],
        )?;
    }
    Ok(n)
}

/// Réaffiche tout : on vide l'historique des lectures.
pub fn tout_reafficher(conn: &Connection) -> Result<usize> {
    Ok(conn.execute("DELETE FROM notification_lue", [])?)
}

fn pousser(
    n: &mut Vec<Notification>,
    cle: String,
    categorie: &str,
    gravite: Gravite,
    titre: String,
    detail: String,
    lien: String,
) {
    n.push(Notification { cle, categorie: categorie.into(), gravite, titre, detail, lien, lu: false });
}

fn pluriel(n: i64) -> &'static str {
    if n > 1 { "s" } else { "" }
}

// ---------------------------------------------------------------------------
// Sources
// ---------------------------------------------------------------------------

/// Projet en cours dont la fin prévue est passée.
fn projets_en_retard(conn: &Connection, j: &str, n: &mut Vec<Notification>) -> Result<()> {
    let mut st = conn.prepare(
        "SELECT id, nom, date_fin_prevue, CAST(julianday(?1) - julianday(date_fin_prevue) AS INTEGER)
         FROM projet
         WHERE statut IN ('planifie','en_cours')
           AND date_fin_prevue IS NOT NULL AND date_fin_prevue < ?1",
    )?;
    let mut rows = st.query(params![j])?;
    while let Some(r) = rows.next()? {
        let (id, nom, fin, jours): (String, String, String, i64) =
            (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?);
        pousser(
            n,
            format!("projet-retard:{id}:{jours}"),
            "Projets",
            Gravite::Urgent,
            format!("Projet en retard : {nom}"),
            format!("La fin était prévue le {} — {jours} jour{} de retard.", fr(&fin), pluriel(jours)),
            format!("projet-detail.html?id={id}"),
        );
    }
    Ok(())
}

/// Activités non terminées dont la fin prévue est dépassée, regroupées par
/// projet : une notification par projet, pas une par tâche (sinon la cloche
/// devient illisible).
fn activites_en_retard(conn: &Connection, j: &str, n: &mut Vec<Notification>) -> Result<()> {
    let mut st = conn.prepare(
        "SELECT p.id, p.nom, COUNT(*), MIN(t.date_fin_prevue)
         FROM tache t JOIN projet p ON p.id = t.projet_id
         WHERE t.statut <> 'terminee'
           AND t.date_fin_prevue IS NOT NULL AND t.date_fin_prevue < ?1
           AND p.statut IN ('planifie','en_cours')
           AND NOT EXISTS (SELECT 1 FROM tache c WHERE c.tache_parente_id = t.id)
         GROUP BY p.id, p.nom",
    )?;
    let mut rows = st.query(params![j])?;
    while let Some(r) = rows.next()? {
        let (id, nom, nb, plus_vieille): (String, String, i64, String) =
            (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?);
        pousser(
            n,
            format!("taches-retard:{id}:{nb}"),
            "Projets",
            Gravite::Urgent,
            format!("{nb} activité{} en retard — {nom}", pluriel(nb)),
            format!("La plus ancienne devait finir le {}.", fr(&plus_vieille)),
            format!("projet-detail.html?id={id}"),
        );
    }
    Ok(())
}

/// Jalons dépassés, et jalons à atteindre dans les 7 jours.
fn jalons(conn: &Connection, j: &str, n: &mut Vec<Notification>) -> Result<()> {
    let mut st = conn.prepare(
        "SELECT ja.id, ja.nom, ja.date_prevue, p.id, p.nom,
                CAST(julianday(ja.date_prevue) - julianday(?1) AS INTEGER)
         FROM jalon ja JOIN projet p ON p.id = ja.projet_id
         WHERE ja.statut = 'a_venir' AND ja.date_reelle IS NULL
           AND p.statut IN ('planifie','en_cours')
           AND julianday(ja.date_prevue) - julianday(?1) <= 7",
    )?;
    let mut rows = st.query(params![j])?;
    while let Some(r) = rows.next()? {
        let (jid, jnom, date, pid, pnom, dans): (String, String, String, String, String, i64) =
            (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?);
        let (grav, titre, detail) = if dans < 0 {
            (
                Gravite::Urgent,
                format!("Jalon dépassé : {jnom}"),
                format!("Attendu le {} ({} jour{} de retard) — projet {pnom}.", fr(&date), -dans, pluriel(-dans)),
            )
        } else if dans == 0 {
            (Gravite::Urgent, format!("Jalon aujourd'hui : {jnom}"), format!("Projet {pnom}."))
        } else {
            (
                Gravite::Attention,
                format!("Jalon dans {dans} jour{} : {jnom}", pluriel(dans)),
                format!("Prévu le {} — projet {pnom}.", fr(&date)),
            )
        };
        pousser(n, format!("jalon:{jid}:{dans}"), "Projets", grav, titre, detail,
                format!("projet-detail.html?id={pid}"));
    }
    Ok(())
}

/// Livrables attendus et non remis.
fn livrables(conn: &Connection, j: &str, n: &mut Vec<Notification>) -> Result<()> {
    let mut st = conn.prepare(
        "SELECT p.id, p.nom, COUNT(*), MIN(l.date_attendue)
         FROM livrable l JOIN projet p ON p.id = l.projet_id
         WHERE l.statut NOT IN ('livre','accepte') AND l.date_livraison IS NULL
           AND l.date_attendue IS NOT NULL AND l.date_attendue < ?1
           AND p.statut IN ('planifie','en_cours')
         GROUP BY p.id, p.nom",
    )?;
    let mut rows = st.query(params![j])?;
    while let Some(r) = rows.next()? {
        let (id, nom, nb, plus_vieux): (String, String, i64, String) =
            (r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?);
        pousser(
            n,
            format!("livrables:{id}:{nb}"),
            "Projets",
            Gravite::Attention,
            format!("{nb} livrable{} en attente — {nom}", pluriel(nb)),
            format!("Le plus ancien était attendu le {}.", fr(&plus_vieux)),
            format!("projet-detail.html?id={id}"),
        );
    }
    Ok(())
}

/// Liens de précédence non respectés (le successeur démarre trop tôt).
fn liens_incoherents(conn: &Connection, n: &mut Vec<Notification>) -> Result<()> {
    let mut st = conn.prepare(
        "SELECT p.id, p.nom, COUNT(*)
         FROM dependance d
         JOIN tache s ON s.id = d.tache_id
         JOIN tache pr ON pr.id = d.predecesseur_id
         JOIN projet p ON p.id = s.projet_id
         WHERE s.date_debut_prevue IS NOT NULL AND pr.date_fin_prevue IS NOT NULL
           AND julianday(s.date_debut_prevue) < julianday(pr.date_fin_prevue) + 1 + d.decalage
           AND p.statut IN ('planifie','en_cours')
         GROUP BY p.id, p.nom",
    )?;
    let mut rows = st.query([])?;
    while let Some(r) = rows.next()? {
        let (id, nom, nb): (String, String, i64) = (r.get(0)?, r.get(1)?, r.get(2)?);
        pousser(
            n,
            format!("liens:{id}:{nb}"),
            "Projets",
            Gravite::Attention,
            format!("{nb} lien{} de précédence non respecté{} — {nom}", pluriel(nb), pluriel(nb)),
            "Des activités commencent avant la fin de ce qui les précède. Le bouton « Harmoniser les dates » corrige.".into(),
            format!("projet-detail.html?id={id}"),
        );
    }
    Ok(())
}

/// Rendez-vous du jour (agenda).
fn rendez_vous_du_jour(conn: &Connection, j: &str, n: &mut Vec<Notification>) -> Result<()> {
    let mut st = conn.prepare(
        "SELECT COUNT(*), MIN(debut) FROM rendez_vous
         WHERE substr(debut, 1, 10) = ?1 AND statut IN ('planifie','confirme')",
    )?;
    let (nb, premier): (i64, Option<String>) =
        st.query_row(params![j], |r| Ok((r.get(0)?, r.get(1)?)))?;
    if nb > 0 {
        let heure = premier
            .as_deref()
            .and_then(|d| d.get(11..16))
            .map(|h| format!(" — le premier à {h}"))
            .unwrap_or_default();
        pousser(
            n,
            format!("rdv:{j}:{nb}"),
            "Agenda",
            Gravite::Attention,
            format!("{nb} rendez-vous aujourd'hui", ),
            format!("À honorer{heure}."),
            "agenda.html".into(),
        );
    }
    Ok(())
}

/// Articles sous leur seuil d'alerte. Le stock est dérivé du journal (§3.3).
fn stock_bas(conn: &Connection, n: &mut Vec<Notification>) -> Result<()> {
    // Le seuil est propre à chaque article : on compare ligne par ligne
    // plutôt qu'en SQL, c'est plus clair et le volume reste petit.
    let mut st = conn.prepare(
        "SELECT a.designation,
                COALESCE((SELECT SUM(CASE WHEN m.sens='entree' THEN m.quantite ELSE -m.quantite END)
                          FROM mouvement_stock m WHERE m.article_id = a.id), 0) AS stock,
                a.stock_alerte
         FROM article a
         WHERE a.actif = 1 AND a.gere_stock = 1
           AND a.stock_alerte IS NOT NULL AND a.stock_alerte > 0",
    )?;
    let mut rows = st.query([])?;
    let (mut nb, mut exemple) = (0i64, String::new());
    while let Some(r) = rows.next()? {
        let (nom, stock, seuil): (String, f64, f64) = (r.get(0)?, r.get(1)?, r.get(2)?);
        if stock <= seuil {
            if nb == 0 {
                exemple = nom;
            }
            nb += 1;
        }
    }
    if nb > 0 {
        pousser(
            n,
            format!("stock:{nb}"),
            "Stock",
            Gravite::Attention,
            format!("{nb} article{} sous le seuil d'alerte", pluriel(nb)),
            format!("Par exemple : {exemple}. Pensez à réapprovisionner."),
            "articles.html".into(),
        );
    }
    Ok(())
}

/// Abonnements dont une échéance est due (facture à générer).
fn abonnements_dus(conn: &Connection, j: &str, n: &mut Vec<Notification>) -> Result<()> {
    let mut st = conn.prepare(
        "SELECT COUNT(*) FROM abonnement
         WHERE actif = 1 AND prochaine_echeance IS NOT NULL AND prochaine_echeance <= ?1",
    )?;
    let nb: i64 = st.query_row(params![j], |r| r.get(0))?;
    if nb > 0 {
        pousser(
            n,
            format!("abo:{j}:{nb}"),
            "Facturation",
            Gravite::Attention,
            format!("{nb} abonnement{} à facturer", pluriel(nb)),
            "Une ou plusieurs échéances sont arrivées : générez les factures.".into(),
            "abonnements.html".into(),
        );
    }
    Ok(())
}

/// Sessions de caisse ouvertes depuis un jour antérieur : oubli de fermeture.
fn caisses_ouvertes(conn: &Connection, j: &str, n: &mut Vec<Notification>) -> Result<()> {
    let mut st = conn.prepare(
        "SELECT COUNT(*), MIN(substr(ouvert_le, 1, 10)) FROM session_caisse
         WHERE statut = 'ouverte' AND substr(ouvert_le, 1, 10) < ?1",
    )?;
    let (nb, depuis): (i64, Option<String>) =
        st.query_row(params![j], |r| Ok((r.get(0)?, r.get(1)?)))?;
    if nb > 0 {
        pousser(
            n,
            format!("caisse:{nb}"),
            "Caisse",
            Gravite::Urgent,
            format!("{nb} caisse{} non fermée{}", pluriel(nb), pluriel(nb)),
            format!("Ouverte depuis le {}. Fermez la session pour arrêter les comptes.",
                    depuis.as_deref().map(fr).unwrap_or_else(|| "—".into())),
            "caisse-etat.html".into(),
        );
    }
    Ok(())
}

fn fr(d: &str) -> String {
    match (d.get(0..4), d.get(5..7), d.get(8..10)) {
        (Some(a), Some(m), Some(x)) => format!("{x}/{m}/{a}"),
        _ => d.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use crate::modules::projet::{self, NouveauProjet, NouvelleTache};

    #[test]
    fn projet_et_activites_en_retard_sont_signales() {
        let conn = db::open_in_memory().unwrap();
        // Projet dont la fin est largement passée, avec une activité non finie.
        let p = projet::creer(&conn, &NouveauProjet {
            nom: "Chantier".into(), client_id: None, chef_de_projet_id: None,
            date_debut_prevue: Some("2020-01-01".into()),
            date_fin_prevue: Some("2020-06-30".into()),
            date_debut_reelle: None, date_fin_reelle: None, statut: None,
            budget_global: 0.0, note: None,
        }, Some("u1")).unwrap().id;
        projet::creer_tache(&conn, &NouvelleTache {
            projet_id: p.clone(), tache_parente_id: None, nom: "Gros oeuvre".into(),
            description: None, date_debut_prevue: Some("2020-02-01".into()),
            date_fin_prevue: Some("2020-03-01".into()), date_debut_reelle: None,
            date_fin_reelle: None, statut: None, avancement: None, budget: 0.0,
        }).unwrap();

        let n = lister(&conn).unwrap();
        assert!(n.iter().any(|x| x.titre.contains("Projet en retard")));
        assert!(n.iter().any(|x| x.titre.contains("activité") && x.titre.contains("retard")));
        assert!(n.iter().all(|x| !x.lu), "rien n'est lu au premier calcul");
        // Les urgentes remontent en tête.
        assert_eq!(n[0].gravite, Gravite::Urgent);
    }

    #[test]
    fn marquer_lu_puis_reafficher() {
        let conn = db::open_in_memory().unwrap();
        projet::creer(&conn, &NouveauProjet {
            nom: "Vieux projet".into(), client_id: None, chef_de_projet_id: None,
            date_debut_prevue: None, date_fin_prevue: Some("2020-01-01".into()),
            date_debut_reelle: None, date_fin_reelle: None, statut: None,
            budget_global: 0.0, note: None,
        }, Some("u1")).unwrap();

        let n = lister(&conn).unwrap();
        assert!(!n.is_empty());
        let cles: Vec<String> = n.iter().map(|x| x.cle.clone()).collect();
        marquer_lues(&conn, &cles).unwrap();

        let n2 = lister(&conn).unwrap();
        assert!(n2.iter().all(|x| x.lu), "tout doit être marqué lu");

        tout_reafficher(&conn).unwrap();
        let n3 = lister(&conn).unwrap();
        assert!(n3.iter().all(|x| !x.lu));
    }

    #[test]
    fn projet_cloture_ne_declenche_rien() {
        let conn = db::open_in_memory().unwrap();
        let p = projet::creer(&conn, &NouveauProjet {
            nom: "Terminé".into(), client_id: None, chef_de_projet_id: None,
            date_debut_prevue: None, date_fin_prevue: Some("2020-01-01".into()),
            date_debut_reelle: None, date_fin_reelle: None, statut: None,
            budget_global: 0.0, note: None,
        }, Some("u1")).unwrap().id;
        projet::changer_statut(&conn, &p, crate::domain::StatutProjet::Cloture).unwrap();
        let n = lister(&conn).unwrap();
        assert!(!n.iter().any(|x| x.titre.contains("Terminé")),
                "un projet clôturé ne doit plus alerter");
    }
}
