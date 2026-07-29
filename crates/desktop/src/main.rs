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
        .invoke_handler(tauri::generate_handler![quitter_application, choisir_dossier])
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
                    return;
                }
                // La fermeture est acquise : c'est le bon moment pour
                // sauvegarder (mig 0042). Plus personne ne saisit, la base est
                // au repos. On le fait AVANT de rendre la main, sinon le
                // processus disparaîtrait au milieu de l'écriture.
                sauvegarder_avant_de_fermer();
            }
        })
        .run(tauri::generate_context!())
        .expect("erreur au lancement de Tauri");
}

/// Ouvre le **vrai sélecteur de dossier de Windows** (demande de l'utilisateur :
/// « utilise l'explorateur Windows, c'est plus simple »).
///
/// Renvoie le chemin choisi, ou `None` si l'utilisateur annule — une annulation
/// n'est pas une erreur et ne doit rien afficher d'alarmant.
///
/// ⚠️ Il montre les dossiers de **la machine où tourne cette fenêtre**. C'est
/// exact aujourd'hui, où chaque installation est son propre serveur. Le jour du
/// mode client, cette fenêtre ne sera plus sur la machine qui détient les
/// données : c'est pourquoi l'écran garde son explorateur côté serveur en
/// second recours, et n'affiche ce bouton que dans la coquille desktop.
#[tauri::command]
fn choisir_dossier() -> Option<String> {
    #[cfg(windows)]
    {
        use windows::core::PCWSTR;
        use windows::Win32::System::Com::{
            CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_APARTMENTTHREADED,
        };
        use windows::Win32::UI::Shell::{
            FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS,
            SIGDN_FILESYSPATH,
        };

        unsafe {
            // La boîte de dialogue exige un appartement COM cloisonné (STA).
            // Le thread d'une commande Tauri n'en a pas : on l'initialise ici.
            let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
            let resultat = (|| -> windows::core::Result<Option<String>> {
                let boite: IFileOpenDialog =
                    CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;
                // FOS_FORCEFILESYSTEM en plus de FOS_PICKFOLDERS : sans lui,
                // l'utilisateur peut choisir un emplacement virtuel (« Ce PC »,
                // une bibliothèque) qui n'a AUCUN chemin sur le disque, et on
                // récupérerait un dossier dans lequel il est impossible d'écrire.
                let options = boite.GetOptions()? | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM;
                boite.SetOptions(options)?;
                let titre: Vec<u16> = "Choisir le dossier de sauvegarde Djigui"
                    .encode_utf16()
                    .chain(std::iter::once(0))
                    .collect();
                let _ = boite.SetTitle(PCWSTR(titre.as_ptr()));

                // Show renvoie une erreur quand l'utilisateur annule : ce n'est
                // pas un incident, on le traduit en « pas de choix ».
                if boite.Show(None).is_err() {
                    return Ok(None);
                }
                let element = boite.GetResult()?;
                let chemin = element.GetDisplayName(SIGDN_FILESYSPATH)?;
                let texte = chemin.to_string().ok();
                windows::Win32::System::Com::CoTaskMemFree(Some(chemin.0 as *const _));
                Ok(texte)
            })();
            CoUninitialize();
            match resultat {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!("sélecteur de dossier indisponible : {e}");
                    None
                }
            }
        }
    }
    #[cfg(not(windows))]
    {
        None
    }
}

/// Sauvegarde à la fermeture, si elle est configurée.
///
/// ⚠️ Un échec **ne bloque pas** la fermeture : empêcher quelqu'un de fermer
/// son logiciel parce qu'une clé USB est débranchée serait pire que le mal.
/// En revanche on le lui DIT — un échec silencieux installerait la fausse
/// tranquillité que tout ce module cherche justement à éviter.
fn sauvegarder_avant_de_fermer() {
    let cfg = djigui_server::Config::from_env();
    match djigui_server::sauvegarder_a_la_fermeture(&cfg.db_path) {
        Ok(None) => {}
        Ok(Some(r)) => {
            tracing::info!(statut = %r.statut, "sauvegarde de fermeture : {}", r.message);
            if r.statut != "succes" {
                avertir("Djigui — Sauvegarde incomplète", &r.message);
            }
        }
        Err(e) => {
            tracing::error!("sauvegarde de fermeture impossible : {e:#}");
            avertir(
                "Djigui — Sauvegarde impossible",
                &format!(
                    "La sauvegarde automatique n'a pas pu être effectuée :

{e}

                     Vos données restent en place, mais elles ne sont pas copiées.                      Ouvrez l'écran Sauvegarde à la prochaine ouverture."
                ),
            );
        }
    }
}

#[cfg(windows)]
fn avertir(titre: &str, message: &str) {
    use windows::core::PCWSTR;
    use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONWARNING, MB_OK};
    let w = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let (m, t) = (w(message), w(titre));
    unsafe {
        MessageBoxW(None, PCWSTR(m.as_ptr()), PCWSTR(t.as_ptr()), MB_OK | MB_ICONWARNING);
    }
}

#[cfg(not(windows))]
fn avertir(titre: &str, message: &str) {
    tracing::warn!("{titre} : {message}");
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
