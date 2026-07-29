//! Sélecteur de dossier **de Windows**, ouvert par le serveur.
//!
//! # Pourquoi ici, et pas dans la coquille Tauri
//!
//! Une première version passait par une commande Tauri appelée depuis la page.
//! Elle échouait : la fenêtre Djigui charge une **URL distante**
//! (`http://localhost:1704`), et Tauri 2 refuse tout l'IPC pour une URL distante
//! tant que la capacité ne l'autorise pas nommément. Le message d'erreur était
//! par-dessus le marché illisible (« undefined »), Tauri rejetant avec une
//! chaîne et non un objet `Error`.
//!
//! Le serveur, lui, tourne **sur la machine dont on veut les dossiers** — c'est
//! la définition même du poste serveur. Ouvrir la boîte de dialogue depuis ici
//! supprime toute la couche IPC, et se vérifie avec un simple appel HTTP.
//!
//! ⚠️ Limite assumée : le jour du mode client, la boîte s'ouvrira sur le poste
//! serveur et non devant l'utilisateur. C'est pourquoi l'écran conserve son
//! explorateur servi par le serveur, et pourquoi cette route est réservée à
//! l'administrateur.

/// Ouvre la boîte « Choisir un dossier » et renvoie le chemin retenu.
///
/// `Ok(None)` = l'utilisateur a annulé. Ce n'est pas une erreur, et cela ne doit
/// rien afficher d'alarmant.
#[cfg(windows)]
pub fn choisir() -> anyhow::Result<Option<String>> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{
        FileOpenDialog, IFileOpenDialog, FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, SIGDN_FILESYSPATH,
    };
    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;

    unsafe {
        // La boîte de dialogue exige un appartement COM cloisonné (STA). Le
        // thread qui nous appelle n'en a pas : on l'initialise ici.
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let issue = (|| -> windows::core::Result<Option<String>> {
            let boite: IFileOpenDialog =
                CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?;

            // FOS_FORCEFILESYSTEM en plus de FOS_PICKFOLDERS : sans lui,
            // l'utilisateur peut choisir un emplacement virtuel (« Ce PC », une
            // bibliothèque) qui n'a AUCUN chemin sur le disque — on récupérerait
            // un dossier dans lequel il est impossible d'écrire.
            let options = boite.GetOptions()? | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM;
            boite.SetOptions(options)?;

            let titre: Vec<u16> = "Choisir le dossier de sauvegarde Djigui"
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            let _ = boite.SetTitle(PCWSTR(titre.as_ptr()));

            // On rattache la boîte à la fenêtre au premier plan (celle de
            // Djigui). Sans propriétaire, elle peut s'ouvrir DERRIÈRE la fenêtre
            // principale : l'utilisateur croit que rien ne s'est passé et
            // reclique, pendant qu'une boîte invisible attend sa réponse.
            let proprietaire = GetForegroundWindow();

            // Show renvoie une erreur quand l'utilisateur annule : ce n'est pas
            // un incident, on le traduit en « aucun choix ».
            if boite.Show(proprietaire).is_err() {
                return Ok(None);
            }
            let element = boite.GetResult()?;
            let chemin = element.GetDisplayName(SIGDN_FILESYSPATH)?;
            let texte = chemin.to_string().ok();
            CoTaskMemFree(Some(chemin.0 as *const _));
            Ok(texte)
        })();

        CoUninitialize();
        issue.map_err(|e| anyhow::anyhow!("sélecteur de dossier Windows : {e}"))
    }
}

#[cfg(not(windows))]
pub fn choisir() -> anyhow::Result<Option<String>> {
    anyhow::bail!("Le sélecteur de dossier natif n'existe que sous Windows.")
}
