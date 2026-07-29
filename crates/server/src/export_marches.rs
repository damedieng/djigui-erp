//! Export Excel du **suivi des marchés** : le portefeuille vu par phase, avec
//! les goulots d'étranglement.
//!
//! Demande de l'utilisateur (`ameliorations.md`) : « une vue globale sur les
//! marchés […] et doit permettre l'export au format Excel mais en montrant tout
//! le temps les goulots d'étranglement ».
//!
//! # Ce que le fichier montre
//!
//! Trois feuilles, dans l'ordre où l'on se pose les questions :
//! 1. **Suivi par phase** — le tableau de bord : combien de marchés et combien
//!    d'argent sont arrêtés à chaque phase, le temps qu'on y passe **comparé à
//!    ce que la procédure prévoyait**, et la désignation des goulots.
//! 2. **Marchés** — une ligne par marché, avec sa phase, son étape du moment et
//!    son ancienneté. C'est la feuille qu'on trie et qu'on filtre.
//! 3. **Étapes** — le détail acte par acte : prévu, réel, écart, qui a validé.
//!
//! # Le goulot
//!
//! Il ne se mesure **pas au nombre de marchés** : une phase chargée où tout
//! avance vite n'est pas un problème. Il se mesure au **temps passé**, comparé
//! à la durée que la procédure elle-même avait prévue. Le seuil n'est donc pas
//! inventé — il vient des dates du marché.

use djigui_core::error::Result as CoreResult;
use djigui_core::modules::marche;
use djigui_core::CoreError;
use rusqlite::Connection;
use rust_xlsxwriter::{Format, FormatAlign, FormatBorder, Workbook, Worksheet};
use std::path::{Path, PathBuf};

// Mêmes couleurs qu'à l'écran : le fichier doit ressembler à ce que
// l'utilisateur a sous les yeux.
const BLEU_NUIT: u32 = 0x1F3A2E;
const VERT: u32 = 0x2E7D52;
const VERT_PALE: u32 = 0xEAF2ED;
const ROUGE: u32 = 0xB23A2C;
const ROUGE_PALE: u32 = 0xFAE9E6;
const AMBRE_PALE: u32 = 0xFBF1DD;
const VERT_TRES_PALE: u32 = 0xE5F2EA;
const VERT_FONCE: u32 = 0x0E5A39;

struct Styles {
    titre: Format,
    entete: Format,
    texte: Format,
    gras: Format,
    montant: Format,
    entier: Format,
    date: Format,
    goulot: Format,
    goulot_txt: Format,
    alerte: Format,
    total: Format,
    total_montant: Format,
    /// Écart favorable : à l'heure ou en avance.
    avance: Format,
    /// Retard **en cours** : l'étape n'est pas faite et l'échéance est passée.
    /// Italique pour le distinguer d'un retard constaté.
    retard_encours: Format,
}

fn styles() -> Styles {
    let bord = FormatBorder::Thin;
    Styles {
        titre: Format::new()
            .set_bold()
            .set_font_size(14)
            .set_font_color(0xFFFFFF)
            .set_background_color(BLEU_NUIT)
            .set_align(FormatAlign::VerticalCenter),
        entete: Format::new()
            .set_bold()
            .set_font_color(0xFFFFFF)
            .set_background_color(VERT)
            .set_border(bord)
            .set_align(FormatAlign::Center)
            .set_text_wrap(),
        texte: Format::new().set_border(bord),
        gras: Format::new().set_bold().set_border(bord),
        // Format sénégalais : séparateur de milliers, pas de décimale — les
        // francs CFA n'ont pas de centimes.
        montant: Format::new().set_border(bord).set_num_format("# ##0"),
        entier: Format::new().set_border(bord).set_align(FormatAlign::Center),
        date: Format::new().set_border(bord).set_align(FormatAlign::Center),
        goulot: Format::new()
            .set_border(bord)
            .set_background_color(ROUGE_PALE)
            .set_font_color(ROUGE)
            .set_bold()
            .set_align(FormatAlign::Center),
        goulot_txt: Format::new()
            .set_border(bord)
            .set_background_color(ROUGE_PALE)
            .set_font_color(ROUGE)
            .set_bold(),
        alerte: Format::new().set_border(bord).set_background_color(AMBRE_PALE),
        total: Format::new().set_bold().set_border(bord).set_background_color(VERT_PALE),
        total_montant: Format::new()
            .set_bold()
            .set_border(bord)
            .set_background_color(VERT_PALE)
            .set_num_format("# ##0"),
        avance: Format::new()
            .set_border(bord)
            .set_background_color(VERT_TRES_PALE)
            .set_font_color(VERT_FONCE)
            .set_bold()
            .set_align(FormatAlign::Center),
        retard_encours: Format::new()
            .set_border(bord)
            .set_background_color(ROUGE_PALE)
            .set_font_color(ROUGE)
            .set_bold()
            .set_italic()
            .set_align(FormatAlign::Center),
    }
}

fn xerr(e: rust_xlsxwriter::XlsxError) -> CoreError {
    CoreError::Rule(format!("export Excel : {e}"))
}

fn titre(f: &mut Worksheet, s: &Styles, texte: &str, largeur: u16) -> CoreResult<()> {
    f.merge_range(0, 0, 0, largeur.saturating_sub(1), texte, &s.titre).map_err(xerr)?;
    f.set_row_height(0, 26.0).map_err(xerr)?;
    Ok(())
}

fn entetes(f: &mut Worksheet, s: &Styles, ligne: u32, cols: &[(&str, f64)]) -> CoreResult<()> {
    for (i, (nom, largeur)) in cols.iter().enumerate() {
        f.write_string_with_format(ligne, i as u16, *nom, &s.entete).map_err(xerr)?;
        f.set_column_width(i as u16, *largeur).map_err(xerr)?;
    }
    f.set_row_height(ligne, 30.0).map_err(xerr)?;
    Ok(())
}

/// Écrit le classeur. Renvoie le chemin réellement utilisé.
pub fn ecrire_suivi(conn: &Connection, chemin: &Path) -> CoreResult<PathBuf> {
    let s = styles();
    let mut wb = Workbook::new();

    let colonnes = marche::tableau_phases(conn, &marche::FiltreMarches::default())?;

    // ---------------------------------------------------------------------
    // Feuille 1 — Suivi par phase : la réponse à « où ça coince ? »
    // ---------------------------------------------------------------------
    {
        let f = wb.add_worksheet();
        f.set_name("Suivi par phase").map_err(xerr)?;
        titre(f, &s, "Suivi des marchés par phase — où les dossiers s'arrêtent", 7)?;
        entetes(f, &s, 2, &[
            ("Phase", 24.0),
            ("Marchés", 10.0),
            ("Montant engagé", 18.0),
            ("Durée prévue (j)", 15.0),
            ("Temps réel moyen (j)", 18.0),
            ("Écart (j)", 11.0),
            ("Constat", 46.0),
        ])?;

        let mut l = 3u32;
        let (mut tot_nb, mut tot_montant) = (0i64, 0f64);
        for c in &colonnes {
            let ecart = c.jours_reels_moy - c.jours_prevus_moy;
            let (fmt_nb, fmt_txt) = if c.goulot {
                (&s.goulot, &s.goulot_txt)
            } else {
                (&s.entier, &s.texte)
            };
            f.write_string_with_format(l, 0, &c.libelle, if c.goulot { &s.goulot_txt } else { &s.gras })
                .map_err(xerr)?;
            f.write_number_with_format(l, 1, c.nb as f64, fmt_nb).map_err(xerr)?;
            f.write_number_with_format(l, 2, c.montant_total, &s.montant).map_err(xerr)?;
            f.write_number_with_format(l, 3, c.jours_prevus_moy as f64, &s.entier).map_err(xerr)?;
            f.write_number_with_format(l, 4, c.jours_reels_moy as f64, fmt_nb).map_err(xerr)?;
            f.write_number_with_format(l, 5, ecart as f64, fmt_nb).map_err(xerr)?;
            // Le constat en toutes lettres : un tableau de chiffres se lit mal
            // en réunion, une phrase se comprend tout de suite.
            let constat = if c.nb == 0 {
                "Aucun marché à cette phase.".to_string()
            } else if c.goulot {
                format!(
                    "GOULOT — on y reste {} j alors que la procédure en prévoyait {}. Le plus ancien : {}.",
                    c.jours_reels_moy,
                    c.jours_prevus_moy,
                    c.marches.first().map(|m| format!("{} ({} j)", m.numero, m.jours_dans_phase))
                        .unwrap_or_default()
                )
            } else if c.jours_prevus_moy == 0 {
                "Pas de durée prévue : impossible de dire si c'est long.".to_string()
            } else {
                format!("Dans les temps ({} j pour {} prévus).", c.jours_reels_moy, c.jours_prevus_moy)
            };
            f.write_string_with_format(l, 6, &constat, fmt_txt).map_err(xerr)?;
            tot_nb += c.nb;
            tot_montant += c.montant_total;
            l += 1;
        }
        f.write_string_with_format(l, 0, "TOTAL", &s.total).map_err(xerr)?;
        f.write_number_with_format(l, 1, tot_nb as f64, &s.total).map_err(xerr)?;
        f.write_number_with_format(l, 2, tot_montant, &s.total_montant).map_err(xerr)?;
        for c in 3..7 {
            f.write_string_with_format(l, c, "", &s.total).map_err(xerr)?;
        }

        // Une note de lecture : sans elle, « goulot » reste un mot.
        l += 2;
        f.write_string(l, 0, "Comment lire ce tableau").map_err(xerr)?;
        l += 1;
        for texte in [
            "Un goulot n'est pas la phase qui a le plus de marchés : c'est celle où l'on reste le plus longtemps.",
            "La durée prévue vient de votre propre procédure (les durées d'étapes du type de marché).",
            "Une phase est signalée en rouge dès que le temps réel dépasse le temps prévu.",
            "Le temps dans une phase court dès la fin de la phase précédente : l'attente avant de commencer compte.",
        ] {
            f.write_string(l, 0, texte).map_err(xerr)?;
            l += 1;
        }
    }

    // ---------------------------------------------------------------------
    // Feuille 2 — Marchés : la feuille qu'on trie et qu'on filtre
    // ---------------------------------------------------------------------
    {
        let f = wb.add_worksheet();
        f.set_name("Marchés").map_err(xerr)?;
        titre(f, &s, "Marchés en cours — position dans la procédure", 11)?;
        entetes(f, &s, 2, &[
            ("N°", 15.0),
            ("Objet", 42.0),
            ("Type", 18.0),
            ("Phase", 18.0),
            ("Étape du moment", 26.0),
            ("Attributaire", 26.0),
            ("Montant courant", 17.0),
            ("Avancement", 12.0),
            ("Jours dans la phase", 15.0),
            ("Prévu (j)", 11.0),
            ("Points à vérifier", 50.0),
        ])?;

        let mut l = 3u32;
        for c in &colonnes {
            for m in &c.marches {
                let depasse = c.jours_prevus_moy > 0 && m.jours_dans_phase > m.jours_prevus_phase;
                let base = if depasse { &s.alerte } else { &s.texte };
                f.write_string_with_format(l, 0, &m.numero, base).map_err(xerr)?;
                f.write_string_with_format(l, 1, &m.objet, base).map_err(xerr)?;
                f.write_string_with_format(l, 2, m.type_libelle.as_deref().unwrap_or("—"), base).map_err(xerr)?;
                f.write_string_with_format(l, 3, &c.libelle, base).map_err(xerr)?;
                f.write_string_with_format(l, 4, m.etape_courante.as_deref().unwrap_or("—"), base).map_err(xerr)?;
                f.write_string_with_format(l, 5, m.attributaire_nom.as_deref().unwrap_or("Pas encore attribué"), base).map_err(xerr)?;
                f.write_number_with_format(l, 6, m.montant_courant, &s.montant).map_err(xerr)?;
                f.write_string_with_format(l, 7, format!("{} %", m.avancement), &s.entier).map_err(xerr)?;
                f.write_number_with_format(l, 8, m.jours_dans_phase as f64,
                    if depasse { &s.goulot } else { &s.entier }).map_err(xerr)?;
                f.write_number_with_format(l, 9, m.jours_prevus_phase as f64, &s.entier).map_err(xerr)?;
                // Toutes les alertes du marché dans une cellule : c'est là que
                // l'utilisateur cherche « qu'est-ce qui ne va pas ».
                let mut points = m.alertes.clone();
                if m.reserves_ouvertes {
                    points.push("Réserves de réception non levées.".into());
                }
                if let Some(r) = &m.recours_en_cours {
                    points.push(format!("Recours en cours : {r}"));
                }
                f.write_string_with_format(l, 10, points.join(" / "), base).map_err(xerr)?;
                l += 1;
            }
        }
        // Filtre automatique : la feuille est faite pour être triée.
        if l > 3 {
            f.autofilter(2, 0, l - 1, 10).map_err(xerr)?;
        }
        f.set_freeze_panes(3, 0).map_err(xerr)?;
    }

    // ---------------------------------------------------------------------
    // Feuille 3 — Étapes : le détail acte par acte
    // ---------------------------------------------------------------------
    {
        let f = wb.add_worksheet();
        f.set_name("Étapes").map_err(xerr)?;
        titre(f, &s, "Déroulé détaillé des procédures", 10)?;
        entetes(f, &s, 2, &[
            ("Marché", 15.0),
            ("Objet", 34.0),
            ("#", 5.0),
            ("Étape", 30.0),
            ("Phase", 18.0),
            ("Prévue le", 12.0),
            ("Faite le", 12.0),
            ("Écart (j)", 10.0),
            ("État", 13.0),
            ("Validée par", 18.0),
        ])?;

        let mut l = 3u32;
        for c in &colonnes {
            for carte in &c.marches {
                let m = marche::lire(conn, &carte.id)?;
                for (i, e) in m.etapes.iter().enumerate() {
                    // L'écart n'a de sens que si les deux dates existent.
                    let ecart = match (&e.date_prevue, &e.date_effective) {
                        (Some(p), Some(r)) => Some(jours(p, r)),
                        _ => None,
                    };
                    let en_retard = ecart.map(|x| x > 0).unwrap_or(false) || e.retard_jours.is_some();
                    let base = if en_retard { &s.alerte } else { &s.texte };
                    f.write_string_with_format(l, 0, &m.numero, base).map_err(xerr)?;
                    f.write_string_with_format(l, 1, &m.objet, base).map_err(xerr)?;
                    f.write_number_with_format(l, 2, (i + 1) as f64, &s.entier).map_err(xerr)?;
                    f.write_string_with_format(l, 3, &e.libelle, base).map_err(xerr)?;
                    f.write_string_with_format(l, 4,
                        e.phase.as_deref().map(marche::libelle_phase).unwrap_or("—"), base).map_err(xerr)?;
                    f.write_string_with_format(l, 5, e.date_prevue.as_deref().unwrap_or("—"), &s.date).map_err(xerr)?;
                    f.write_string_with_format(l, 6, e.date_effective.as_deref().unwrap_or("—"), &s.date).map_err(xerr)?;
                    match ecart {
                        Some(x) => f.write_number_with_format(l, 7, x as f64,
                            if x > 0 { &s.goulot } else { &s.entier }).map_err(xerr)?,
                        None => f.write_string_with_format(l, 7, "—", &s.entier).map_err(xerr)?,
                    };
                    let etat = match e.statut.as_str() {
                        "termine" => "Terminée",
                        "en_cours" => "En cours",
                        "annule" => "Annulée",
                        "reporte" => "Reportée",
                        _ if e.verrouillee => "Verrouillée",
                        _ => "À faire",
                    };
                    f.write_string_with_format(l, 8, etat, base).map_err(xerr)?;
                    f.write_string_with_format(l, 9, e.valide_par.as_deref().unwrap_or(""), base).map_err(xerr)?;
                    l += 1;
                }
            }
        }
        if l > 3 {
            f.autofilter(2, 0, l - 1, 9).map_err(xerr)?;
        }
        f.set_freeze_panes(3, 0).map_err(xerr)?;
    }

    // ---------------------------------------------------------------------
    // Feuilles 4+ — UNE PAR TYPE DE MARCHÉ
    //
    // Pourquoi séparer : dans un même type, tous les marchés suivent la MÊME
    // procédure, donc les colonnes s'alignent et une colonne « Retard » se lit
    // de haut en bas — c'est elle qui désigne l'étape qui fait dérailler tous
    // les dossiers. Mélanger Travaux (8 étapes) et Services (7) rendrait le
    // tableau illisible.
    // ---------------------------------------------------------------------
    for t in marche::lister_types(conn, false)? {
        if t.etapes.is_empty() {
            continue;   // un type sans procédure n'a pas de colonnes à aligner
        }
        let mut f_marches = Vec::new();
        for m in marche::lister(conn, &marche::FiltreMarches {
            type_id: Some(t.id.clone()),
            ..Default::default()
        })? {
            f_marches.push(marche::lire(conn, &m.id)?);
        }
        if f_marches.is_empty() {
            continue;   // pas de feuille vide : elle n'apprendrait rien
        }
        feuille_type(&mut wb, &s, &t, &f_marches)?;
    }

    wb.save(chemin).map_err(xerr)?;
    Ok(chemin.to_path_buf())
}

/// Nom de feuille accepté par Excel : 31 caractères, sans `[]:*?/\`.
fn nom_feuille(libelle: &str) -> String {
    let net: String = libelle
        .chars()
        .map(|c| if "[]:*?/\\".contains(c) { '-' } else { c })
        .collect();
    net.chars().take(31).collect()
}

/// Une feuille pour un type de marché : un tableau croisé marchés × étapes,
/// chaque étape éclatée en **Prévue / Réalisé / Retard**.
fn feuille_type(
    wb: &mut Workbook,
    s: &Styles,
    t: &marche::TypeMarche,
    marches: &[marche::Marche],
) -> CoreResult<()> {
    const FIXES: u16 = 3; // N°, Objet, Statut
    let etapes = &t.etapes;
    let f = wb.add_worksheet();
    f.set_name(&nom_feuille(&t.libelle)).map_err(xerr)?;

    let derniere = FIXES + etapes.len() as u16 * 3; // + colonne « Retard cumulé »
    titre(f, s, &format!("{} — déroulé comparé des procédures", t.libelle), derniere + 1)?;

    // --- En-tête sur DEUX niveaux ---
    // Ligne 2 : le nom de l'étape, fusionné sur ses trois sous-colonnes.
    // Ligne 3 : Prévue | Réalisé | Retard.
    for (i, (nom, largeur)) in [("N° marché", 15.0), ("Objet", 38.0), ("Statut", 12.0)]
        .iter().enumerate()
    {
        f.merge_range(2, i as u16, 3, i as u16, nom, &s.entete).map_err(xerr)?;
        f.set_column_width(i as u16, *largeur).map_err(xerr)?;
    }
    for (k, e) in etapes.iter().enumerate() {
        let c0 = FIXES + k as u16 * 3;
        f.merge_range(2, c0, 2, c0 + 2, &format!("{}. {}", k + 1, e.libelle), &s.entete)
            .map_err(xerr)?;
        for (j, sous) in ["Prévue", "Réalisé", "Retard"].iter().enumerate() {
            f.write_string_with_format(3, c0 + j as u16, *sous, &s.entete).map_err(xerr)?;
            f.set_column_width(c0 + j as u16, if j == 2 { 9.0 } else { 11.5 }).map_err(xerr)?;
        }
    }
    f.merge_range(2, derniere, 3, derniere, "Retard cumulé", &s.entete).map_err(xerr)?;
    f.set_column_width(derniere, 13.0).map_err(xerr)?;
    f.set_row_height(2, 30.0).map_err(xerr)?;
    f.set_row_height(3, 18.0).map_err(xerr)?;

    // --- Une ligne par marché ---
    // On accumule au passage de quoi écrire les lignes de synthèse : elles
    // répondent à « quelle étape fait dériver TOUS les dossiers ? ».
    let mut somme = vec![0i64; etapes.len()];
    let mut compte = vec![0i64; etapes.len()];
    let mut nb_retard = vec![0i64; etapes.len()];

    let mut l = 4u32;
    for m in marches {
        f.write_string_with_format(l, 0, &m.numero, &s.texte).map_err(xerr)?;
        f.write_string_with_format(l, 1, &m.objet, &s.texte).map_err(xerr)?;
        f.write_string_with_format(l, 2, libelle_statut(&m.statut), &s.texte).map_err(xerr)?;

        let mut cumul = 0i64;
        for (k, modele) in etapes.iter().enumerate() {
            let c0 = FIXES + k as u16 * 3;
            // On retrouve l'étape du marché par filiation ; à défaut par rang,
            // car une procédure recopiée garde l'ordre même si le lien manque.
            let etape = m.etapes.iter()
                .find(|x| x.etape_modele_id.as_deref() == Some(modele.id.as_str()))
                .or_else(|| m.etapes.get(k));
            let Some(e) = etape else {
                for j in 0..3 {
                    f.write_string_with_format(l, c0 + j, "", &s.texte).map_err(xerr)?;
                }
                continue;
            };
            f.write_string_with_format(l, c0, e.date_prevue.as_deref().unwrap_or("—"), &s.date)
                .map_err(xerr)?;
            f.write_string_with_format(l, c0 + 1, e.date_effective.as_deref().unwrap_or("—"), &s.date)
                .map_err(xerr)?;
            match e.ecart_jours {
                Some(j) => {
                    // Rouge dès qu'il y a retard ; le retard EN COURS se
                    // distingue en italique — réel, mais pas encore constaté.
                    let fmt = if j > 0 {
                        if e.ecart_en_cours { &s.retard_encours } else { &s.goulot }
                    } else {
                        &s.avance
                    };
                    f.write_number_with_format(l, c0 + 2, j as f64, fmt).map_err(xerr)?;
                    somme[k] += j;
                    compte[k] += 1;
                    if j > 0 {
                        nb_retard[k] += 1;
                        cumul += j;
                    }
                }
                None => { f.write_string_with_format(l, c0 + 2, "—", &s.entier).map_err(xerr)?; }
            }
        }
        // Le cumul ne compte que les retards : une avance sur une étape ne
        // « rattrape » pas un retard sur une autre, les jalons sont distincts.
        f.write_number_with_format(l, derniere, cumul as f64,
            if cumul > 0 { &s.goulot } else { &s.entier }).map_err(xerr)?;
        l += 1;
    }

    // --- Lignes de synthèse ---
    f.write_string_with_format(l, 0, "Retard moyen (j)", &s.total).map_err(xerr)?;
    f.write_string_with_format(l, 1, "moyenne des écarts constatés et en cours", &s.total).map_err(xerr)?;
    f.write_string_with_format(l, 2, "", &s.total).map_err(xerr)?;
    for (k, _) in etapes.iter().enumerate() {
        let c0 = FIXES + k as u16 * 3;
        f.write_string_with_format(l, c0, "", &s.total).map_err(xerr)?;
        f.write_string_with_format(l, c0 + 1, "", &s.total).map_err(xerr)?;
        let moy = if compte[k] > 0 { somme[k] as f64 / compte[k] as f64 } else { 0.0 };
        f.write_number_with_format(l, c0 + 2, (moy * 10.0).round() / 10.0,
            if moy > 0.0 { &s.goulot } else { &s.total }).map_err(xerr)?;
    }
    f.write_string_with_format(l, derniere, "", &s.total).map_err(xerr)?;

    l += 1;
    f.write_string_with_format(l, 0, "Marchés en retard", &s.total).map_err(xerr)?;
    f.write_string_with_format(l, 1, &format!("sur {} marché(s) de ce type", marches.len()), &s.total)
        .map_err(xerr)?;
    f.write_string_with_format(l, 2, "", &s.total).map_err(xerr)?;
    for (k, _) in etapes.iter().enumerate() {
        let c0 = FIXES + k as u16 * 3;
        f.write_string_with_format(l, c0, "", &s.total).map_err(xerr)?;
        f.write_string_with_format(l, c0 + 1, "", &s.total).map_err(xerr)?;
        // Une moyenne peut être tirée par un seul dossier très en retard :
        // le NOMBRE de marchés touchés dit si le problème est systématique.
        f.write_number_with_format(l, c0 + 2, nb_retard[k] as f64,
            if nb_retard[k] > 0 { &s.goulot } else { &s.total }).map_err(xerr)?;
    }
    f.write_string_with_format(l, derniere, "", &s.total).map_err(xerr)?;

    // Note de lecture : sans elle, « + » et « − » restent à deviner.
    l += 2;
    for texte in [
        "Retard : + = en retard, − = en avance, 0 = fait le jour prévu.",
        "Une valeur en ITALIQUE est un retard EN COURS : l'étape n'est pas encore faite et son échéance est passée.",
        "Lisez une colonne « Retard » de haut en bas : c'est elle qui montre l'étape qui fait dériver tous les dossiers.",
        "« Retard cumulé » n'additionne que les retards : une avance sur une étape ne rattrape pas un retard sur une autre.",
    ] {
        f.write_string(l, 0, texte).map_err(xerr)?;
        l += 1;
    }

    f.set_freeze_panes(4, FIXES).map_err(xerr)?;
    Ok(())
}

fn libelle_statut(code: &str) -> &str {
    match code {
        "en_cours" => "En cours",
        "realise" => "Réalisé",
        "suspendu" => "Suspendu",
        "annule" => "Annulé",
        autre => autre,
    }
}

/// Jours entre deux dates « AAAA-MM-JJ ». 0 si l'une est illisible : mieux vaut
/// un zéro qu'un export qui échoue sur une date mal saisie.
fn jours(a: &str, b: &str) -> i64 {
    use chrono::NaiveDate;
    match (
        NaiveDate::parse_from_str(&a[..a.len().min(10)], "%Y-%m-%d"),
        NaiveDate::parse_from_str(&b[..b.len().min(10)], "%Y-%m-%d"),
    ) {
        (Ok(x), Ok(y)) => (y - x).num_days(),
        _ => 0,
    }
}
