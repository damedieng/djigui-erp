//! Coquille desktop Djigui (spec §2.2 : shell Tauri, backend Rust dans le
//! processus serveur).
//!
//! Au lancement en **mode serveur** : on démarre le serveur axum en tâche de
//! fond (il détient la base et sert l'UI), on attend qu'il réponde, puis Tauri
//! ouvre une fenêtre native pointant sur `http://localhost:<port>`. L'origine
//! étant localhost, l'UI dialogue avec l'API exactement comme prévu.
//!
//! Le **mode client** (pointer vers l'IP d'un autre poste, sans démarrer de
//! serveur local) viendra brancher la même fenêtre sur une autre URL.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::net::TcpStream;
use std::time::{Duration, Instant};

fn main() {
    // Désactive le cache HTTP du WebView2 : l'UI est servie localement et évolue
    // souvent ; sans ça, Windows resert d'anciennes pages/CSS/JS en cache (décalage
    // trompeur entre le serveur et la fenêtre). L'appli recharge toujours du frais.
    std::env::set_var("WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS", "--disable-http-cache");

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,djigui_server=info,djigui_core=info".into()),
        )
        .init();

    let cfg = djigui_server::Config::from_env();
    let port = cfg.port;

    // Démarre le serveur dans son propre runtime Tokio, sur un thread dédié.
    std::thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().expect("runtime tokio");
        if let Err(e) = rt.block_on(djigui_server::serve(cfg)) {
            tracing::error!("serveur arrêté : {e:#}");
        }
    });

    // Attend que le port réponde avant d'ouvrir la fenêtre (évite un écran blanc).
    attendre_serveur(port, Duration::from_secs(10));

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![quitter_application])
        .on_window_event(|_window, event| {
            // Fermeture (X, Alt+F4) : la décision est prise ICI, nativement.
            //
            // ⚠️ Ne JAMAIS déléguer la fermeture à l'interface web. La version
            // précédente bloquait la fermeture puis émettait « demande-fermeture »
            // en comptant sur une confirmation côté page. Or `emit` réussit même
            // quand PERSONNE n'écoute : dès que le listener JS manquait (permission
            // Tauri absente, erreur de script, page en cours de chargement), la
            // fenêtre refusait de se fermer et il fallait tuer le processus.
            //
            // Règle : sans ticket en attente, on ferme immédiatement ; sinon on
            // demande confirmation par une boîte native, qui ne dépend de rien.
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if !fermeture_confirmee() {
                    api.prevent_close();
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("erreur au lancement de Tauri");
}

/// Ferme immédiatement l'application. Reste disponible pour un futur bouton
/// « Quitter » dans l'interface, mais la fermeture par le X ne dépend plus
/// d'elle : elle est décidée nativement dans `on_window_event`.
#[tauri::command]
fn quitter_application() {
    std::process::exit(0);
}

/// Retourne `true` si la fenêtre peut se fermer : soit aucun ticket en attente,
/// soit l'utilisateur confirme la perte via une boîte de dialogue native.
#[cfg(windows)]
fn fermeture_confirmee() -> bool {
    use std::sync::atomic::Ordering;
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDYES, MB_ICONWARNING, MB_YESNO,
    };

    let n = djigui_server::TICKETS_EN_ATTENTE.load(Ordering::Relaxed);
    if n <= 0 {
        return true;
    }
    let vers_utf16 = |s: String| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let msg = vers_utf16(format!(
        "Il reste {n} ticket(s) en attente (non encaissés).\n\n\
         Fermer Djigui quand même ? Ces tickets seront perdus.",
    ));
    let titre = vers_utf16("Djigui — Tickets en attente".into());
    let res = unsafe {
        MessageBoxW(None, PCWSTR(msg.as_ptr()), PCWSTR(titre.as_ptr()), MB_YESNO | MB_ICONWARNING)
    };
    res == IDYES
}

#[cfg(not(windows))]
fn fermeture_confirmee() -> bool {
    true
}

/// Boucle courte jusqu'à ce que le serveur accepte une connexion TCP locale.
fn attendre_serveur(port: u16, delai_max: Duration) {
    let debut = Instant::now();
    let adresse = format!("127.0.0.1:{port}");
    while debut.elapsed() < delai_max {
        if TcpStream::connect(&adresse).is_ok() {
            tracing::info!("serveur prêt sur {adresse}");
            return;
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    tracing::warn!("serveur non joignable après {delai_max:?}, ouverture quand même");
}
