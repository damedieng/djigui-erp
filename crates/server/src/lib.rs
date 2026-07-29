//! Serveur Djigui exposé comme bibliothèque, pour être démarré aussi bien par le
//! binaire `djigui-server` que par la coquille desktop Tauri (§2.1/§2.2).

pub mod api;
pub mod dossier_natif;
pub mod export;
pub mod export_projet;
pub mod export_marches;
pub mod impression;
pub mod state;

use axum::http::header::{HeaderValue, CACHE_CONTROL};
use state::AppState;
use std::net::SocketAddr;
use std::sync::atomic::AtomicI64;
use tower_http::{cors::CorsLayer, services::ServeDir, trace::TraceLayer};

/// Nombre de tickets de caisse « en attente » (non encaissés), publié par l'UI.
/// Lu par la coquille desktop pour confirmer avant la fermeture de la fenêtre.
pub static TICKETS_EN_ATTENTE: AtomicI64 = AtomicI64::new(0);

/// Configuration de démarrage du serveur.
pub struct Config {
    pub db_path: String,
    pub frontend_dir: String,
    pub port: u16,
}

impl Config {
    /// Lit la configuration depuis l'environnement, avec valeurs par défaut.
    pub fn from_env() -> Self {
        Self {
            db_path: std::env::var("DJIGUI_DB").unwrap_or_else(|_| "djigui.db".into()),
            frontend_dir: std::env::var("DJIGUI_FRONTEND").unwrap_or_else(|_| "frontend".into()),
            port: std::env::var("DJIGUI_PORT").ok().and_then(|p| p.parse().ok()).unwrap_or(1704),
        }
    }
}

/// Ouvre la base, construit le routeur et sert jusqu'à l'arrêt du processus.
pub async fn serve(cfg: Config) -> anyhow::Result<()> {
    let state = AppState::ouvrir(&cfg.db_path)?;
    tracing::info!(db = %cfg.db_path, "base ouverte et migrée");

    let app = api::router(state)
        .fallback_service(ServeDir::new(&cfg.frontend_dir))
        // Empêche le WebView2 (Tauri) de servir une version cachée de l'UI :
        // les fichiers rechargés sont toujours les plus récents.
        .layer(axum::middleware::map_response(|mut res: axum::response::Response| async move {
            res.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("no-store"));
            res
        }))
        .layer(CorsLayer::permissive()) // réseau local ; à restreindre plus tard
        .layer(TraceLayer::new_for_http());

    let addr = SocketAddr::from(([0, 0, 0, 0], cfg.port));
    tracing::info!("serveur Djigui à l'écoute sur http://{addr}  (frontend : {})", cfg.frontend_dir);
    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

/// Sauvegarde déclenchée **à la fermeture de l'application** (mig 0042).
///
/// Appelée par la coquille desktop juste avant que la fenêtre ne se ferme.
/// Elle ouvre sa **propre connexion** au lieu de passer par l'`AppState` du
/// serveur : à cet instant le serveur tourne encore dans son thread, et
/// emprunter son `Mutex` depuis l'extérieur nous ferait dépendre de son état
/// d'avancement. En mode WAL, une seconde connexion lit sans gêner.
///
/// Renvoie `Ok(None)` quand il n'y a simplement rien à faire (sauvegarde
/// désactivée, poste secondaire, protection par mot de passe) — ce n'est pas
/// une erreur, et il ne faut surtout pas alerter l'utilisateur pour ça au
/// moment où il ferme son logiciel.
pub fn sauvegarder_a_la_fermeture(
    db_path: &str,
) -> anyhow::Result<Option<djigui_core::modules::sauvegarde::ResultatSauvegarde>> {
    use djigui_core::modules::sauvegarde;

    let conn = djigui_core::db::open(db_path)?;
    let p = sauvegarde::lire_parametres(&conn)?;
    if !p.activee || !p.a_la_fermeture || !p.cette_machine_est_serveur {
        return Ok(None);
    }
    // ⚠️ En mode mot de passe, personne ne peut le saisir : la fenêtre est en
    // train de disparaître. On ne bloque pas la fermeture pour autant — on
    // laisse une trace, et l'écran de sauvegarde signalera le manque.
    if p.mode_cle == "motdepasse" {
        tracing::warn!(
            "sauvegarde de fermeture ignorée : protection par mot de passe, \
             qui ne peut pas être saisi à la fermeture"
        );
        return Ok(None);
    }

    let base = std::path::Path::new(db_path);
    let racine = base.parent().unwrap_or_else(|| std::path::Path::new("."));
    let resultat = sauvegarde::executer(
        &conn,
        &racine.join("documents"),
        &racine.join("travail"),
        "fermeture",
        None,
    )?;
    Ok(Some(resultat))
}
