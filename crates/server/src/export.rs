//! Export Excel (.xlsx) multi-feuilles — hors-ligne (crate `rust_xlsxwriter`).
//!
//! **Bonnes pratiques grands volumes** :
//! - **mémoire constante** : les feuilles volumineuses sont écrites en mode
//!   `constant_memory` (chaque ligne est déversée dans un fichier temporaire,
//!   la RAM reste plate quel que soit le nombre de lignes) ;
//! - **streaming SQLite** : on itère un curseur (`query_map`) sans charger de
//!   grand tableau intermédiaire ;
//! - **écriture directe sur disque** (`save`), sans buffer mémoire complet ;
//! - **bornage par période** : l'appelant fournit `[du, au]` pour limiter le
//!   volume (la vraie protection). Excel plafonne à 1 048 576 lignes/feuille ;
//!   au-delà on renvoie une erreur invitant à réduire la période.
//!
//! Feuilles : **Ventes** (une ligne par facture), **Détail ventes** (une ligne
//! par article vendu), **Mouvements** (journal de caisse), **Sessions** et
//! **Bénéfices** (par mois × caisse).

use djigui_core::error::Result as CoreResult;
use djigui_core::modules::{rapport, rendez_vous, session_caisse};
use djigui_core::CoreError;
use rusqlite::{params, Connection};
use rust_xlsxwriter::{Format, FormatBorder, Workbook, Worksheet};
use std::collections::HashMap;
use std::path::Path;

/// Limite dure d'Excel (lignes par feuille). On garde une marge pour l'en-tête.
const MAX_LIGNES: u32 = 1_048_575;

const MODES: &[(&str, &str)] = &[
    ("espece", "Espèces"), ("mobile_money", "Mobile Money"),
    ("virement", "Virement"), ("cheque", "Chèque"),
];
fn mode_label(m: &str) -> String {
    MODES.iter().find(|(k, _)| *k == m).map(|(_, v)| v.to_string()).unwrap_or_else(|| m.to_string())
}
fn statut_label(s: &str) -> &'static str {
    match s {
        "valide" => "Validée", "annule" => "Annulée",
        "transforme" => "Transformée", "accepte" => "Acceptée", _ => "—",
    }
}

fn noms(conn: &Connection, table: &str) -> CoreResult<HashMap<String, String>> {
    let mut m = HashMap::new();
    let mut s = conn.prepare(&format!("SELECT id, nom FROM {table}"))?;
    let it = s.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
    for x in it { let (a, b) = x?; m.insert(a, b); }
    Ok(m)
}

fn trop_de_lignes() -> CoreError {
    CoreError::Rule(
        "trop de lignes pour un seul export (limite Excel) : réduisez la période".into(),
    )
}

/// Construit le classeur borné à `[du, au]` (dates « AAAA-MM-JJ », incluses ;
/// `None` = sans borne) et l'écrit dans `chemin`.
/// Écrit le classeur et renvoie le **nombre de ventes** incluses (0 = période
/// sans vente : le fichier ne contient que les en-têtes).
pub fn ecrire_classeur(
    conn: &Connection,
    chemin: &Path,
    du: Option<&str>,
    au: Option<&str>,
) -> CoreResult<(std::path::PathBuf, usize)> {
    let nb_ventes: usize;
    let tiers_nom = noms(conn, "tiers")?;
    let caisse_nom = noms(conn, "caisse")?;
    let moyen_nom = noms(conn, "moyen_paiement")?;

    let mut wb = Workbook::new();
    let entete = Format::new().set_bold().set_background_color(0xE8EEF7).set_border(FormatBorder::Thin);
    let money = Format::new().set_num_format("# ##0");
    let money_g = Format::new().set_bold().set_num_format("# ##0");
    let gras = Format::new().set_bold();

    // ---------- Feuille 1 : Ventes (une ligne par facture) ----------
    {
        let ws = wb.add_worksheet_with_constant_memory();
        ws.set_name("Ventes").map_err(xerr)?;
        let cols = ["N°", "Date", "Client", "Statut", "Objet", "HT", "TVA", "TTC", "Caisse"];
        entetes(ws, &cols, &entete)?;
        for (c, w) in [(0,16.0),(1,12.0),(2,26.0),(3,12.0),(4,24.0),(8,20.0)] { ws.set_column_width(c, w).ok(); }
        let mut stmt = conn.prepare(
            "SELECT d.numero, d.date, d.tiers_id, d.statut, d.objet, d.total_ht, d.total_tva, d.total_ttc,
                    (SELECT p.caisse_id FROM paiement p WHERE p.document_id=d.id AND p.sens='encaissement' LIMIT 1)
             FROM document d
             WHERE d.type_document='facture' AND d.sens='vente' AND d.statut<>'brouillon'
               AND (?1 IS NULL OR d.date>=?1) AND (?2 IS NULL OR d.date<=?2)
             ORDER BY d.date, d.numero")?;
        let mut rows = stmt.query(params![du, au])?;
        let mut row = 1u32;
        let (mut sht, mut stva, mut sttc) = (0f64, 0f64, 0f64);
        while let Some(r) = rows.next()? {
            if row > MAX_LIGNES { return Err(trop_de_lignes()); }
            let tiers_id: String = r.get(2)?;
            let caisse_id: Option<String> = r.get(8)?;
            let (ht, tva, ttc): (f64, f64, f64) = (r.get(5)?, r.get(6)?, r.get(7)?);
            ws.write(row, 0, r.get::<_, String>(0)?).map_err(xerr)?;
            ws.write(row, 1, r.get::<_, String>(1)?).map_err(xerr)?;
            ws.write(row, 2, tiers_nom.get(&tiers_id).map(String::as_str).unwrap_or("—")).map_err(xerr)?;
            ws.write(row, 3, statut_label(&r.get::<_, String>(3)?)).map_err(xerr)?;
            ws.write(row, 4, r.get::<_, Option<String>>(4)?.unwrap_or_default()).map_err(xerr)?;
            ws.write_number_with_format(row, 5, ht, &money).map_err(xerr)?;
            ws.write_number_with_format(row, 6, tva, &money).map_err(xerr)?;
            ws.write_number_with_format(row, 7, ttc, &money).map_err(xerr)?;
            ws.write(row, 8, caisse_id.and_then(|id| caisse_nom.get(&id).cloned()).unwrap_or_else(|| "—".into())).map_err(xerr)?;
            sht += ht; stva += tva; sttc += ttc; row += 1;
        }
        nb_ventes = (row - 1) as usize;
        ws.write_with_format(row, 4, "TOTAL", &gras).map_err(xerr)?;
        ws.write_number_with_format(row, 5, sht, &money_g).map_err(xerr)?;
        ws.write_number_with_format(row, 6, stva, &money_g).map_err(xerr)?;
        ws.write_number_with_format(row, 7, sttc, &money_g).map_err(xerr)?;
    }

    // ---------- Feuille 2 : Détail ventes (une ligne par article vendu) ----------
    {
        let ws = wb.add_worksheet_with_constant_memory();
        ws.set_name("Détail ventes").map_err(xerr)?;
        let cols = ["N° vente", "Date", "Article", "Qté", "Prix unitaire", "Remise %", "Total HT"];
        entetes(ws, &cols, &entete)?;
        for (c, w) in [(0,16.0),(1,12.0),(2,30.0),(4,14.0),(6,14.0)] { ws.set_column_width(c, w).ok(); }
        let mut stmt = conn.prepare(
            "SELECT d.numero, d.date, dl.designation, dl.quantite, dl.prix_unitaire, dl.remise, dl.total_ligne_ht
             FROM document d JOIN document_ligne dl ON dl.document_id=d.id
             WHERE d.type_document='facture' AND d.sens='vente' AND d.statut<>'brouillon'
               AND (?1 IS NULL OR d.date>=?1) AND (?2 IS NULL OR d.date<=?2)
             ORDER BY d.date, d.numero")?;
        let mut rows = stmt.query(params![du, au])?;
        let mut row = 1u32;
        while let Some(r) = rows.next()? {
            if row > MAX_LIGNES { return Err(trop_de_lignes()); }
            ws.write(row, 0, r.get::<_, String>(0)?).map_err(xerr)?;
            ws.write(row, 1, r.get::<_, String>(1)?).map_err(xerr)?;
            ws.write(row, 2, r.get::<_, String>(2)?).map_err(xerr)?;
            ws.write_number(row, 3, r.get::<_, f64>(3)?).map_err(xerr)?;
            ws.write_number_with_format(row, 4, r.get::<_, f64>(4)?, &money).map_err(xerr)?;
            ws.write_number(row, 5, r.get::<_, f64>(5)?).map_err(xerr)?;
            ws.write_number_with_format(row, 6, r.get::<_, f64>(6)?, &money).map_err(xerr)?;
            row += 1;
        }
    }

    // ---------- Feuille 3 : Mouvements (journal de caisse) ----------
    {
        let ws = wb.add_worksheet_with_constant_memory();
        ws.set_name("Mouvements").map_err(xerr)?;
        let cols = ["Date", "Tiers", "Sens", "Moyen", "Montant"];
        entetes(ws, &cols, &entete)?;
        for (c, w) in [(0,20.0),(1,26.0),(2,15.0),(3,18.0),(4,14.0)] { ws.set_column_width(c, w).ok(); }
        let mut stmt = conn.prepare(
            "SELECT date, tiers_id, sens, mode, moyen_paiement_id, montant FROM paiement
             WHERE (?1 IS NULL OR substr(date,1,10)>=?1) AND (?2 IS NULL OR substr(date,1,10)<=?2)
             ORDER BY date")?;
        let mut rows = stmt.query(params![du, au])?;
        let mut row = 1u32;
        let mut total = 0f64;
        while let Some(r) = rows.next()? {
            if row > MAX_LIGNES { return Err(trop_de_lignes()); }
            let tiers_id: String = r.get(1)?;
            let sens: String = r.get(2)?;
            let moyen = r.get::<_, Option<String>>(4)?.and_then(|id| moyen_nom.get(&id).cloned())
                .unwrap_or_else(|| mode_label(&r.get::<_, String>(3).unwrap_or_default()));
            let montant: f64 = r.get(5)?;
            let signe = if sens == "encaissement" { 1.0 } else { -1.0 };
            ws.write(row, 0, r.get::<_, String>(0)?).map_err(xerr)?;
            ws.write(row, 1, tiers_nom.get(&tiers_id).map(String::as_str).unwrap_or("—")).map_err(xerr)?;
            ws.write(row, 2, if sens == "encaissement" { "Encaissement" } else { "Décaissement" }).map_err(xerr)?;
            ws.write(row, 3, moyen).map_err(xerr)?;
            ws.write_number_with_format(row, 4, signe * montant, &money).map_err(xerr)?;
            total += signe * montant; row += 1;
        }
        ws.write_with_format(row, 3, "Total", &gras).map_err(xerr)?;
        ws.write_number_with_format(row, 4, total, &money_g).map_err(xerr)?;
    }

    // ---------- Feuille 4 : Sessions (peu volumineux : mode standard) ----------
    {
        let ws = wb.add_worksheet();
        ws.set_name("Sessions").map_err(xerr)?;
        let cols = ["Caisse", "Ouverte le", "Fermée le", "Fond", "Encaissé", "Décaissé", "Théorique", "Compté", "Écart", "Statut"];
        entetes(ws, &cols, &entete)?;
        for (c, w) in [(0,20.0),(1,18.0),(2,18.0)] { ws.set_column_width(c, w).ok(); }
        let sessions = session_caisse::lister(conn, None)?;
        let mut row = 1u32;
        for s in &sessions {
            let jour = &s.ouvert_le.get(0..10).unwrap_or("");
            if let Some(d) = du { if jour < &d { continue; } }
            if let Some(a) = au { if jour > &a { continue; } }
            ws.write(row, 0, caisse_nom.get(&s.caisse_id).map(String::as_str).unwrap_or("—")).map_err(xerr)?;
            ws.write(row, 1, &s.ouvert_le).map_err(xerr)?;
            ws.write(row, 2, s.ferme_le.as_deref().unwrap_or("")).map_err(xerr)?;
            ws.write_number_with_format(row, 3, s.fond_ouverture, &money).map_err(xerr)?;
            ws.write_number_with_format(row, 4, s.encaissements, &money).map_err(xerr)?;
            ws.write_number_with_format(row, 5, s.decaissements, &money).map_err(xerr)?;
            ws.write_number_with_format(row, 6, s.theorique, &money).map_err(xerr)?;
            if let Some(m) = s.montant_compte { ws.write_number_with_format(row, 7, m, &money).map_err(xerr)?; }
            if let Some(e) = s.ecart { ws.write_number_with_format(row, 8, e, &money).map_err(xerr)?; }
            ws.write(row, 9, if s.statut == "ouverte" { "Ouverte" } else { "Fermée" }).map_err(xerr)?;
            row += 1;
        }
    }

    // ---------- Feuille 5 : Bénéfices par mois × caisse (agrégé : petit) ----------
    {
        let ws = wb.add_worksheet();
        ws.set_name("Bénéfices").map_err(xerr)?;
        let cols = ["Mois", "Caisse", "Nb ventes", "CA HT", "CA TTC", "Coût d'achat", "Bénéfice"];
        entetes(ws, &cols, &entete)?;
        for (c, w) in [(0,12.0),(1,22.0)] { ws.set_column_width(c, w).ok(); }
        let lignes = rapport::benefices_par_mois_caisse(conn, du, au)?;
        let mut row = 1u32;
        let (mut a, mut b, mut c, mut d2) = (0f64, 0f64, 0f64, 0f64);
        for l in &lignes {
            ws.write(row, 0, &l.mois).map_err(xerr)?;
            ws.write(row, 1, &l.caisse_nom).map_err(xerr)?;
            ws.write_number(row, 2, l.nb_ventes as f64).map_err(xerr)?;
            ws.write_number_with_format(row, 3, l.ca_ht, &money).map_err(xerr)?;
            ws.write_number_with_format(row, 4, l.ca_ttc, &money).map_err(xerr)?;
            ws.write_number_with_format(row, 5, l.cout_achat, &money).map_err(xerr)?;
            ws.write_number_with_format(row, 6, l.benefice, &money).map_err(xerr)?;
            a += l.ca_ht; b += l.ca_ttc; c += l.cout_achat; d2 += l.benefice; row += 1;
        }
        ws.write_with_format(row, 1, "TOTAL", &gras).map_err(xerr)?;
        ws.write_number_with_format(row, 3, a, &money_g).map_err(xerr)?;
        ws.write_number_with_format(row, 4, b, &money_g).map_err(xerr)?;
        ws.write_number_with_format(row, 5, c, &money_g).map_err(xerr)?;
        ws.write_number_with_format(row, 6, d2, &money_g).map_err(xerr)?;
    }

    // Écriture directe sur disque. Si le fichier cible est déjà **ouvert dans
    // Excel** (verrou Windows), on réessaie avec un suffixe horaire au lieu
    // d'échouer.
    match wb.save(chemin) {
        Ok(()) => Ok((chemin.to_path_buf(), nb_ventes)),
        Err(_) => {
            let hhmmss = djigui_core::now()
                .get(11..19).unwrap_or("").replace(':', "");
            let tige = chemin.file_stem().and_then(|s| s.to_str()).unwrap_or("djigui-ventes");
            let alt = chemin.with_file_name(format!("{tige}-{hhmmss}.xlsx"));
            wb.save(&alt).map_err(xerr)?;
            Ok((alt, nb_ventes))
        }
    }
}

/// Écrit la ligne d'en-tête (ligne 0) d'une feuille.
fn entetes(ws: &mut Worksheet, cols: &[&str], fmt: &Format) -> CoreResult<()> {
    for (c, t) in cols.iter().enumerate() {
        ws.write_with_format(0, c as u16, *t, fmt).map_err(xerr)?;
    }
    Ok(())
}

fn xerr(e: rust_xlsxwriter::XlsxError) -> CoreError {
    CoreError::Rule(format!("export Excel : {e}"))
}

// ---------------------------------------------------------------------------
// Export iCalendar (.ics) de l'agenda — standard RFC 5545 (Google/Outlook/Apple)
// ---------------------------------------------------------------------------

/// Échappe un texte pour une valeur iCalendar (\\ , ; et sauts de ligne).
fn ics_echap(s: &str) -> String {
    s.replace('\\', "\\\\").replace('\n', "\\n").replace(',', "\\,").replace(';', "\\;")
}

/// « 2026-07-28 10:30 » → « 20260728T103000 » (heure locale, sans fuseau).
fn ics_datetime(s: &str) -> String {
    let d: String = s.chars().filter(|c| c.is_ascii_digit()).collect(); // 202607281030
    let date = d.get(0..8).unwrap_or("").to_string();
    let hm = d.get(8..12).unwrap_or("0000");
    format!("{date}T{hm}00")
}
/// « 2026-07-28 … » → « 20260728 » (date seule, pour les événements journée entière).
fn ics_date(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_digit()).take(8).collect()
}

/// Statut Djigui → STATUS iCalendar (valeurs autorisées : TENTATIVE/CONFIRMED/CANCELLED).
fn ics_statut(s: &str) -> &'static str {
    match s {
        "confirme" | "honore" => "CONFIRMED",
        "annule" => "CANCELLED",
        _ => "TENTATIVE", // planifie, reporte
    }
}

/// Construit le flux iCalendar des rendez-vous (bornés à `[du, au]` si fournis).
/// Renvoie (texte .ics, nombre d'événements).
pub fn ics_rendez_vous(conn: &Connection, du: Option<&str>, au: Option<&str>) -> CoreResult<(String, usize)> {
    let f = rendez_vous::FiltreRendezVous {
        du: du.map(str::to_string), au: au.map(str::to_string), ..Default::default()
    };
    let liste = rendez_vous::lister(conn, &f)?;
    let stamp = format!("{}Z", ics_datetime(&djigui_core::now())); // horodatage de génération
    let mut s = String::new();
    s.push_str("BEGIN:VCALENDAR\r\nVERSION:2.0\r\nPRODID:-//Djigui//Agenda//FR\r\nCALSCALE:GREGORIAN\r\nMETHOD:PUBLISH\r\n");
    for r in &liste {
        s.push_str("BEGIN:VEVENT\r\n");
        s.push_str(&format!("UID:{}@djigui\r\n", r.id));
        s.push_str(&format!("DTSTAMP:{stamp}\r\n"));
        if r.journee_entiere {
            s.push_str(&format!("DTSTART;VALUE=DATE:{}\r\n", ics_date(&r.debut)));
            if let Some(fin) = &r.fin {
                s.push_str(&format!("DTEND;VALUE=DATE:{}\r\n", ics_date(fin)));
            }
        } else {
            s.push_str(&format!("DTSTART:{}\r\n", ics_datetime(&r.debut)));
            if let Some(fin) = &r.fin {
                s.push_str(&format!("DTEND:{}\r\n", ics_datetime(fin)));
            }
        }
        s.push_str(&format!("SUMMARY:{}\r\n", ics_echap(&r.titre)));
        if let Some(lieu) = r.lieu.as_deref().filter(|x| !x.is_empty()) {
            s.push_str(&format!("LOCATION:{}\r\n", ics_echap(lieu)));
        }
        // Description = note + rattachements (client / responsable).
        let mut desc: Vec<String> = Vec::new();
        if let Some(n) = r.note.as_deref().filter(|x| !x.is_empty()) { desc.push(n.to_string()); }
        if let Some(t) = r.tiers_nom.as_deref() { desc.push(format!("Client : {t}")); }
        if let Some(u) = r.responsable_nom.as_deref() { desc.push(format!("Responsable : {u}")); }
        if !desc.is_empty() {
            s.push_str(&format!("DESCRIPTION:{}\r\n", ics_echap(&desc.join("\n"))));
        }
        s.push_str(&format!("STATUS:{}\r\n", ics_statut(&r.statut)));
        s.push_str("END:VEVENT\r\n");
    }
    s.push_str("END:VCALENDAR\r\n");
    Ok((s, liste.len()))
}
