//! Impression native des tickets de caisse (§ caisse).
//!
//! But : imprimer le ticket **en arrière-plan**, sans boîte de dialogue, sur une
//! imprimante choisie une fois dans les Paramètres. Primaire : imprimante
//! **thermique ESC/POS** (envoi d'un flux brut au spouleur). Le crate `printers`
//! encapsule winspool (Windows) ; l'énumération liste les imprimantes du poste.
//!
//! Repli : si aucune imprimante n'est configurée ou en cas d'échec, l'UI garde
//! le bouton « Imprimer le dernier ticket » (dialogue navigateur).

/// Liste les noms des imprimantes installées sur le poste.
pub fn lister() -> Vec<String> {
    printers::get_printers().into_iter().map(|p| p.name).collect()
}

/// Envoie des octets au spouleur avec un datatype donné (RAW pour l'ESC/POS,
/// TEXT pour un rendu par le pilote de l'imprimante standard).
fn envoyer(nom: &str, data: &[u8], datatype: &str) -> Result<(), String> {
    let cible = printers::get_printers()
        .into_iter()
        .find(|p| p.name == nom)
        .ok_or_else(|| format!("imprimante introuvable : {nom}"))?;
    let options = printers::common::base::job::PrinterJobOptions {
        name: Some("Ticket Djigui"),
        // "document-format" pilote le datatype winspool (RAW par défaut).
        raw_properties: &[("document-format", datatype)],
        converter: printers::common::converters::Converter::None,
    };
    cible.print(data, options).map(|_| ()).map_err(|e| format!("{e:?}"))
}

/// Imprime un ticket selon le **mode** de l'imprimante :
/// - `thermique` : flux ESC/POS brut (datatype RAW), coupe automatique.
/// - `standard`  : rendu **par le pilote** de l'imprimante bureautique (jet
///   d'encre / laser), en silence, via l'utilitaire d'impression de Windows.
pub fn imprimer_ticket(nom: &str, texte: &str, mode: &str) -> Result<(), String> {
    if mode == "thermique" {
        envoyer(nom, &escpos_depuis_texte(texte), "RAW")
    } else {
        imprimer_via_pilote(nom, texte)
    }
}

/// Impression silencieuse sur une imprimante bureautique **via GDI** : on crée
/// un contexte d'appareil sur l'imprimante nommée et on dessine le ticket ligne
/// par ligne (police à chasse fixe, dimensionnée au DPI). Aucune fenêtre, aucun
/// dialogue, aucun fichier — le pilote reçoit une page prête à imprimer.
#[cfg(windows)]
fn imprimer_via_pilote(nom: &str, texte: &str) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::Graphics::Gdi::{
        CreateDCW, CreateFontW, DeleteDC, DeleteObject, GetDeviceCaps, SelectObject, TextOutW,
        LOGPIXELSX, LOGPIXELSY,
    };
    use windows::Win32::Storage::Xps::{DOCINFOW, EndDoc, EndPage, StartDocW, StartPage};

    // Constantes GDI (le prototype CreateFontW attend des u32 bruts).
    const DEFAULT_CHARSET: u32 = 1;
    const OUT_DEFAULT_PRECIS: u32 = 0;
    const CLIP_DEFAULT_PRECIS: u32 = 0;
    const DEFAULT_QUALITY: u32 = 0;
    const FIXED_PITCH_FF_MODERN: u32 = 1 | 48; // FIXED_PITCH | FF_MODERN

    // Chaînes UTF-16 terminées par NUL, requises par l'API Win32.
    let vers_utf16 = |s: &str| -> Vec<u16> { s.encode_utf16().chain(std::iter::once(0)).collect() };
    let device = vers_utf16(nom);
    let face = vers_utf16("Courier New");
    let doc_nom = vers_utf16("Ticket Djigui");

    unsafe {
        let hdc = CreateDCW(PCWSTR::null(), PCWSTR(device.as_ptr()), PCWSTR::null(), None);
        if hdc.is_invalid() {
            return Err(format!("impossible d'ouvrir l'imprimante « {nom} »"));
        }

        let dpi_x = GetDeviceCaps(hdc, LOGPIXELSX);
        let dpi_y = GetDeviceCaps(hdc, LOGPIXELSY);

        // Police à chasse fixe ~11 pt, mise à l'échelle du DPI de l'imprimante.
        let hfont = CreateFontW(
            -(dpi_y * 11 / 72), 0, 0, 0, 400, 0, 0, 0,
            DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS, DEFAULT_QUALITY,
            FIXED_PITCH_FF_MODERN, PCWSTR(face.as_ptr()),
        );
        let ancien = SelectObject(hdc, hfont);

        let di = DOCINFOW {
            cbSize: std::mem::size_of::<DOCINFOW>() as i32,
            lpszDocName: PCWSTR(doc_nom.as_ptr()),
            lpszOutput: PCWSTR::null(),
            lpszDatatype: PCWSTR::null(),
            fwType: 0,
        };

        let mut erreur: Option<String> = None;
        if StartDocW(hdc, &di) > 0 {
            if StartPage(hdc) > 0 {
                let marge_x = dpi_x / 2; // 0,5 pouce
                let mut y = dpi_y / 2;
                let interligne = dpi_y * 14 / 72;
                for ligne in texte.split('\n') {
                    let w: Vec<u16> = ligne.encode_utf16().collect();
                    let _ = TextOutW(hdc, marge_x, y, &w);
                    y += interligne;
                }
                let _ = EndPage(hdc);
            } else {
                erreur = Some("StartPage a échoué".into());
            }
            let _ = EndDoc(hdc);
        } else {
            erreur = Some("StartDoc a échoué".into());
        }

        SelectObject(hdc, ancien);
        let _ = DeleteObject(hfont);
        let _ = DeleteDC(hdc);
        match erreur {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

#[cfg(not(windows))]
fn imprimer_via_pilote(_nom: &str, _texte: &str) -> Result<(), String> {
    Err("impression pilote disponible sur Windows uniquement".into())
}

/// Construit un flux ESC/POS minimal depuis un texte déjà mis en forme
/// (une ligne source = une ligne imprimée) : init, texte, avance papier, coupe.
pub fn escpos_depuis_texte(texte: &str) -> Vec<u8> {
    let mut out = Vec::with_capacity(texte.len() + 16);
    out.extend_from_slice(&[0x1B, 0x40]); // ESC @ : réinitialise l'imprimante
    for ligne in texte.split('\n') {
        out.extend(encoder(ligne));
        out.push(0x0A); // saut de ligne
    }
    out.extend_from_slice(&[0x0A, 0x0A, 0x0A]); // avance avant coupe
    out.extend_from_slice(&[0x1D, 0x56, 0x42, 0x00]); // GS V B 0 : coupe partielle
    out
}

/// Encode une ligne en CP858 (jeu latin des imprimantes tickets, avec accents
/// français et €). Tout caractère non couvert dégrade en ASCII ou en « ? ».
fn encoder(ligne: &str) -> Vec<u8> {
    ligne.chars().map(cp858).collect()
}

fn cp858(c: char) -> u8 {
    match c {
        'é' => 0x82, 'è' => 0x8A, 'ê' => 0x88, 'ë' => 0x89,
        'à' => 0x85, 'â' => 0x83, 'ä' => 0x84,
        'ù' => 0x97, 'û' => 0x96, 'ü' => 0x81,
        'î' => 0x8C, 'ï' => 0x8B,
        'ô' => 0x93, 'ö' => 0x94,
        'ç' => 0x87, 'Ç' => 0x80,
        'É' => 0x90, 'È' => 0xD4, 'À' => 0xB7,
        '€' => 0xD5, '°' => 0xF8,
        c if (c as u32) <= 0x7F => c as u8,
        _ => b'?',
    }
}
