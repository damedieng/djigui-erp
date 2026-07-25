//! Export Excel d'un projet : planning, Gantt, budget, ressources, jalons.
//!
//! **Le Gantt est dessiné avec les cellules** (une colonne = un jour, une
//! semaine ou un mois selon la durée, cellules colorées par niveau). C'est le
//! seul rendu robuste : les vrais graphiques Excel ne savent pas produire un
//! diagramme de Gantt lisible sans acrobaties, alors qu'un damier de cellules
//! s'imprime bien, se filtre, et reste modifiable par le commerçant.
//!
//! Mémoire constante (`add_worksheet_with_constant_memory`) : les lignes sont
//! écrites de gauche à droite et de haut en bas, jamais revisitées.

use chrono::{Datelike, Duration, NaiveDate};
use djigui_core::error::Result as CoreResult;
use djigui_core::CoreError;
use rusqlite::{params, Connection};
use rust_xlsxwriter::{Format, FormatAlign, FormatBorder, Workbook, Worksheet};
use std::path::Path;

// Palette : reprise des couleurs de l'écran pour que le fichier ressemble à ce
// que l'utilisateur voit à l'écran.
const BLEU_NUIT: u32 = 0x1F3A2E; // bandeaux de titre
const VERT: u32 = 0x2E7D52; // en-têtes de colonnes
const VERT_PALE: u32 = 0xEAF2ED; // lignes de synthèse
const NIVEAUX: [u32; 4] = [0x2E9E5B, 0xB8860B, 0x2F7FD1, 0x7B4FA8];
const JALON: u32 = 0xC0392B;
const WEEKEND: u32 = 0xEDEFEE;

struct Styles {
    titre: Format,
    sous_titre: Format,
    entete: Format,
    entete_frise: Format,
    texte: Format,
    gras: Format,
    money: Format,
    money_gras: Format,
    centre: Format,
    pct: Format,
    synthese_lbl: Format,
    synthese_val: Format,
    niveau: [Format; 4],
    barre: [Format; 4],
    cellule_vide: Format,
    weekend: Format,
    jalon: Format,
}

impl Styles {
    fn new() -> Self {
        let bord = FormatBorder::Thin;
        let base = || Format::new().set_border(bord).set_border_color(0xD5DBD8);
        Self {
            titre: Format::new()
                .set_bold()
                .set_font_size(15.0)
                .set_font_color(0xFFFFFF)
                .set_background_color(BLEU_NUIT)
                .set_align(FormatAlign::VerticalCenter),
            sous_titre: Format::new()
                .set_font_color(0xFFFFFF)
                .set_background_color(BLEU_NUIT)
                .set_align(FormatAlign::VerticalCenter),
            entete: Format::new()
                .set_bold()
                .set_font_color(0xFFFFFF)
                .set_background_color(VERT)
                .set_border(bord)
                .set_align(FormatAlign::Center)
                .set_align(FormatAlign::VerticalCenter)
                .set_text_wrap(),
            entete_frise: Format::new()
                .set_bold()
                .set_font_size(8.0)
                .set_font_color(0xFFFFFF)
                .set_background_color(VERT)
                .set_border(bord)
                .set_align(FormatAlign::Center),
            texte: base(),
            gras: base().set_bold(),
            money: base().set_num_format("# ##0"),
            money_gras: base().set_bold().set_num_format("# ##0"),
            centre: base().set_align(FormatAlign::Center),
            pct: base().set_num_format("0\\%").set_align(FormatAlign::Center),
            synthese_lbl: Format::new().set_bold().set_background_color(VERT_PALE).set_border(bord),
            synthese_val: Format::new()
                .set_background_color(VERT_PALE)
                .set_border(bord)
                .set_num_format("# ##0"),
            niveau: [
                base().set_bold().set_font_color(NIVEAUX[0]),
                base().set_bold().set_font_color(NIVEAUX[1]),
                base().set_font_color(NIVEAUX[2]),
                base().set_font_color(NIVEAUX[3]),
            ],
            barre: [
                Format::new().set_background_color(NIVEAUX[0]).set_border(bord).set_border_color(NIVEAUX[0]),
                Format::new().set_background_color(NIVEAUX[1]).set_border(bord).set_border_color(NIVEAUX[1]),
                Format::new().set_background_color(NIVEAUX[2]).set_border(bord).set_border_color(NIVEAUX[2]),
                Format::new().set_background_color(NIVEAUX[3]).set_border(bord).set_border_color(NIVEAUX[3]),
            ],
            cellule_vide: Format::new().set_border(bord).set_border_color(0xE4E8E6),
            weekend: Format::new().set_background_color(WEEKEND).set_border(bord).set_border_color(0xE4E8E6),
            jalon: Format::new()
                .set_background_color(JALON)
                .set_font_color(0xFFFFFF)
                .set_bold()
                .set_align(FormatAlign::Center)
                .set_border(bord),
        }
    }
}

// ---------------------------------------------------------------------------
// Données rassemblées depuis la base
// ---------------------------------------------------------------------------

struct Tache {
    id: String,
    nom: String,
    parent: Option<String>,
    debut: Option<NaiveDate>,
    fin: Option<NaiveDate>,
    statut: String,
    avancement: i64,
    budget: f64,
    niveau: usize,
    a_enfants: bool,
}

fn jour(s: &Option<String>) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.as_deref()?.get(0..10)?, "%Y-%m-%d").ok()
}

fn statut_tache(s: &str) -> &'static str {
    match s {
        "a_faire" => "À faire",
        "en_cours" => "En cours",
        "bloquee" => "Bloquée",
        "terminee" => "Terminée",
        _ => "—",
    }
}

/// Charge les activités et les remet dans l'ordre de l'arborescence, en
/// calculant le niveau et les dates agrégées des parents (bas → haut), comme
/// à l'écran.
fn charger_taches(conn: &Connection, projet_id: &str) -> CoreResult<Vec<Tache>> {
    let mut stmt = conn.prepare(
        "SELECT id, nom, tache_parente_id, date_debut_prevue, date_fin_prevue,
                statut, avancement, budget
         FROM tache WHERE projet_id = ?1 ORDER BY ordre, nom",
    )?;
    let brut: Vec<Tache> = stmt
        .query_map(params![projet_id], |r| {
            Ok(Tache {
                id: r.get(0)?,
                nom: r.get(1)?,
                parent: r.get(2)?,
                debut: jour(&r.get::<_, Option<String>>(3)?),
                fin: jour(&r.get::<_, Option<String>>(4)?),
                statut: r.get(5)?,
                avancement: r.get(6)?,
                budget: r.get(7)?,
                niveau: 1,
                a_enfants: false,
            })
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    // Mise à plat dans l'ordre hiérarchique.
    let mut ordonne: Vec<Tache> = Vec::with_capacity(brut.len());
    fn descendre(
        brut: &[Tache],
        parent: Option<&str>,
        niveau: usize,
        sortie: &mut Vec<usize>,
        niveaux: &mut Vec<usize>,
    ) {
        for (i, t) in brut.iter().enumerate() {
            if t.parent.as_deref() == parent {
                sortie.push(i);
                niveaux[i] = niveau;
                descendre(brut, Some(&t.id), niveau + 1, sortie, niveaux);
            }
        }
    }
    let mut indices = Vec::new();
    let mut niveaux = vec![1usize; brut.len()];
    descendre(&brut, None, 1, &mut indices, &mut niveaux);
    // Les orphelines (parent supprimé) ne doivent pas disparaître de l'export.
    for (i, _) in brut.iter().enumerate() {
        if !indices.contains(&i) {
            indices.push(i);
        }
    }

    for &i in &indices {
        let t = &brut[i];
        let a_enfants = brut.iter().any(|x| x.parent.as_deref() == Some(t.id.as_str()));
        ordonne.push(Tache {
            id: t.id.clone(),
            nom: t.nom.clone(),
            parent: t.parent.clone(),
            debut: t.debut,
            fin: t.fin,
            statut: t.statut.clone(),
            avancement: t.avancement,
            budget: t.budget,
            niveau: niveaux[i].min(4),
            a_enfants,
        });
    }

    // Remontée bas → haut : un parent prend l'étendue de ses enfants.
    for _ in 0..4 {
        for i in 0..ordonne.len() {
            if !ordonne[i].a_enfants {
                continue;
            }
            let id = ordonne[i].id.clone();
            let (mut d, mut f, mut b, mut som, mut nb) = (None, None, 0.0, 0i64, 0i64);
            for e in ordonne.iter().filter(|x| x.parent.as_deref() == Some(id.as_str())) {
                if let Some(x) = e.debut {
                    d = Some(d.map_or(x, |c: NaiveDate| c.min(x)));
                }
                if let Some(x) = e.fin {
                    f = Some(f.map_or(x, |c: NaiveDate| c.max(x)));
                }
                b += e.budget;
                som += e.avancement;
                nb += 1;
            }
            ordonne[i].debut = d;
            ordonne[i].fin = f;
            ordonne[i].budget = b;
            if nb > 0 {
                ordonne[i].avancement = som / nb;
            }
        }
    }
    Ok(ordonne)
}

/// Échelle de la frise : jour si le projet est court, sinon semaine, sinon mois.
enum Pas {
    Jour,
    Semaine,
    Mois,
}

fn colonnes_frise(d0: NaiveDate, d1: NaiveDate) -> (Pas, Vec<(NaiveDate, NaiveDate)>) {
    let jours = (d1 - d0).num_days() + 1;
    let pas = if jours <= 62 {
        Pas::Jour
    } else if jours <= 400 {
        Pas::Semaine
    } else {
        Pas::Mois
    };
    let mut cols = Vec::new();
    let mut c = d0;
    while c <= d1 {
        let fin = match pas {
            Pas::Jour => c,
            Pas::Semaine => c + Duration::days(6),
            Pas::Mois => {
                let (a, m) = (c.year(), c.month());
                let (a2, m2) = if m == 12 { (a + 1, 1) } else { (a, m + 1) };
                NaiveDate::from_ymd_opt(a2, m2, 1).unwrap() - Duration::days(1)
            }
        };
        cols.push((c, fin));
        c = fin + Duration::days(1);
    }
    (pas, cols)
}

// ---------------------------------------------------------------------------
// Écriture du classeur
// ---------------------------------------------------------------------------

pub fn ecrire_projet(conn: &Connection, chemin: &Path, projet_id: &str) -> CoreResult<std::path::PathBuf> {
    let s = Styles::new();
    let mut wb = Workbook::new();

    // ---- Projet ----
    let (nom, client, chef, dprev, fprev, budget_saisi, note): (
        String, Option<String>, Option<String>, Option<String>, Option<String>, f64, Option<String>,
    ) = conn
        .query_row(
            "SELECT p.nom, t.nom, u.nom, p.date_debut_prevue, p.date_fin_prevue, p.budget_global, p.note
             FROM projet p
             LEFT JOIN tiers t ON t.id = p.client_id
             LEFT JOIN utilisateur u ON u.id = p.chef_de_projet_id
             WHERE p.id = ?1",
            params![projet_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
        )
        .map_err(|_| CoreError::NotFound(format!("projet {projet_id}")))?;

    let taches = charger_taches(conn, projet_id)?;

    feuille_synthese(&mut wb, &s, &nom, &client, &chef, &dprev, &fprev, budget_saisi, &note, &taches, conn, projet_id)?;
    feuille_planning(&mut wb, &s, &taches, conn, projet_id)?;
    feuille_ressources_humaines(&mut wb, &s, conn, projet_id)?;
    feuille_ressources(&mut wb, &s, conn, projet_id)?;
    feuille_jalons(&mut wb, &s, conn, projet_id)?;

    wb.save(chemin).map_err(xerr)?;
    Ok(chemin.to_path_buf())
}

#[allow(clippy::too_many_arguments)]
fn feuille_synthese(
    wb: &mut Workbook,
    s: &Styles,
    nom: &str,
    client: &Option<String>,
    chef: &Option<String>,
    dprev: &Option<String>,
    fprev: &Option<String>,
    budget_saisi: f64,
    note: &Option<String>,
    taches: &[Tache],
    conn: &Connection,
    projet_id: &str,
) -> CoreResult<()> {
    let ws = wb.add_worksheet();
    ws.set_name("Synthèse").map_err(xerr)?;
    ws.set_column_width(0, 30.0).ok();
    ws.set_column_width(1, 26.0).ok();
    ws.set_column_width(2, 18.0).ok();

    ws.set_row_height(0, 30.0).ok();
    ws.merge_range(0, 0, 0, 2, &format!("  {nom}"), &s.titre).map_err(xerr)?;
    ws.set_row_height(1, 18.0).ok();
    ws.merge_range(1, 0, 1, 2, "  Fiche de projet — Djigui", &s.sous_titre).map_err(xerr)?;

    let mut l = 3u32;
    let couple = |ws: &mut Worksheet, l: &mut u32, k: &str, v: String| -> CoreResult<()> {
        ws.write_with_format(*l, 0, k, &s.synthese_lbl).map_err(xerr)?;
        ws.merge_range(*l, 1, *l, 2, &v, &s.texte).map_err(xerr)?;
        *l += 1;
        Ok(())
    };
    couple(ws, &mut l, "Client", client.clone().unwrap_or_else(|| "—".into()))?;
    couple(ws, &mut l, "Chef de projet", chef.clone().unwrap_or_else(|| "—".into()))?;
    couple(ws, &mut l, "Début prévu", fr(dprev))?;
    couple(ws, &mut l, "Fin prévue", fr(fprev))?;

    // Étendue réelle, calculée depuis les activités.
    let d0 = taches.iter().filter_map(|t| t.debut).min();
    let f1 = taches.iter().filter_map(|t| t.fin).max();
    couple(ws, &mut l, "Début réel (activités)", d0.map(|d| d.format("%d/%m/%Y").to_string()).unwrap_or_else(|| "—".into()))?;
    couple(ws, &mut l, "Fin réelle (activités)", f1.map(|d| d.format("%d/%m/%Y").to_string()).unwrap_or_else(|| "—".into()))?;
    if let Some(n) = note.as_deref().filter(|x| !x.trim().is_empty()) {
        couple(ws, &mut l, "Note", n.to_string())?;
    }

    l += 1;
    ws.merge_range(l, 0, l, 2, "BUDGET", &s.entete).map_err(xerr)?;
    l += 1;

    // Budget des feuilles seulement : un parent agrège déjà ses enfants.
    let budget_taches: f64 = taches.iter().filter(|t| !t.a_enfants).map(|t| t.budget).sum();
    let cout_res: f64 = conn.query_row(
        "SELECT COALESCE(SUM(cout_unitaire * quantite), 0) FROM ressource WHERE projet_id = ?1",
        params![projet_id], |r| r.get(0))?;
    let cout_mo: f64 = conn.query_row(
        "SELECT COALESCE(SUM(CASE WHEN i.type_taux='forfait' THEN i.taux
                                  ELSE a.heures_allouees * i.taux END), 0)
         FROM assignation a JOIN intervenant i ON i.id = a.intervenant_id
         JOIN tache t ON t.id = a.tache_id WHERE t.projet_id = ?1",
        params![projet_id], |r| r.get(0))?;
    let planifie = budget_taches + cout_res + cout_mo;

    let argent = |ws: &mut Worksheet, l: &mut u32, k: &str, v: f64, fort: bool| -> CoreResult<()> {
        ws.write_with_format(*l, 0, k, if fort { &s.synthese_lbl } else { &s.texte }).map_err(xerr)?;
        ws.merge_range(*l, 1, *l, 2, "", if fort { &s.synthese_val } else { &s.money }).map_err(xerr)?;
        ws.write_number_with_format(*l, 1, v, if fort { &s.money_gras } else { &s.money }).map_err(xerr)?;
        *l += 1;
        Ok(())
    };
    argent(ws, &mut l, "Budget saisi", budget_saisi, false)?;
    argent(ws, &mut l, "Budget des activités", budget_taches, false)?;
    argent(ws, &mut l, "Coût main-d'œuvre", cout_mo, false)?;
    argent(ws, &mut l, "Coût ressources", cout_res, false)?;
    argent(ws, &mut l, "Budget planifié (total)", planifie, true)?;
    argent(ws, &mut l, "Écart (saisi − planifié)", budget_saisi - planifie, true)?;

    l += 1;
    ws.merge_range(l, 0, l, 2, "AVANCEMENT", &s.entete).map_err(xerr)?;
    l += 1;
    let feuilles: Vec<&Tache> = taches.iter().filter(|t| !t.a_enfants).collect();
    let nb_term = feuilles.iter().filter(|t| t.statut == "terminee").count();
    ws.write_with_format(l, 0, "Activités", &s.synthese_lbl).map_err(xerr)?;
    ws.write_with_format(l, 1, format!("{} dont {} terminée(s)", feuilles.len(), nb_term), &s.texte).map_err(xerr)?;
    l += 1;
    let moy = if feuilles.is_empty() { 0 } else { feuilles.iter().map(|t| t.avancement).sum::<i64>() / feuilles.len() as i64 };
    ws.write_with_format(l, 0, "Avancement moyen", &s.synthese_lbl).map_err(xerr)?;
    ws.write_number_with_format(l, 1, moy as f64, &s.pct).map_err(xerr)?;
    Ok(())
}

fn feuille_planning(
    wb: &mut Workbook,
    s: &Styles,
    taches: &[Tache],
    conn: &Connection,
    projet_id: &str,
) -> CoreResult<()> {
    let ws = wb.add_worksheet();
    ws.set_name("Planning").map_err(xerr)?;

    // Jalons, pour les poser sur la frise.
    let mut stmt = conn.prepare(
        "SELECT nom, date_prevue, tache_id FROM jalon WHERE projet_id = ?1 ORDER BY date_prevue")?;
    let jalons: Vec<(String, Option<NaiveDate>, Option<String>)> = stmt
        .query_map(params![projet_id], |r| {
            Ok((r.get(0)?, jour(&r.get::<_, Option<String>>(1)?), r.get(2)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    const NB_COLS: u16 = 7; // Activité, Début, Fin, Jours, Statut, Avct, Budget
    let cols = ["Activité", "Début", "Fin", "Jours", "Statut", "Avct.", "Budget"];
    for (c, w) in [(0u16, 42.0), (1, 11.0), (2, 11.0), (3, 7.0), (4, 11.0), (5, 8.0), (6, 14.0)] {
        ws.set_column_width(c, w).ok();
    }

    let d0 = taches.iter().filter_map(|t| t.debut).min();
    let f1 = taches.iter().filter_map(|t| t.fin).max();
    let frise = match (d0, f1) {
        (Some(a), Some(b)) if b >= a => Some(colonnes_frise(a, b)),
        _ => None,
    };

    // Ligne 0 : titre. Ligne 1 : bande de regroupement. Ligne 2 : en-têtes.
    ws.set_row_height(0, 26.0).ok();
    let derniere = frise.as_ref().map_or(NB_COLS - 1, |(_, c)| NB_COLS + c.len() as u16 - 1);
    ws.merge_range(0, 0, 0, derniere, "  PLANNING DU PROJET", &s.titre).map_err(xerr)?;

    if let Some((pas, colonnes)) = &frise {
        // Bande supérieure : mois (ou année) au-dessus de la frise.
        let mut i = 0usize;
        while i < colonnes.len() {
            let etiquette = match pas {
                Pas::Mois => colonnes[i].0.format("%Y").to_string(),
                _ => format!("{}", mois_fr(colonnes[i].0)),
            };
            let mut j = i;
            while j + 1 < colonnes.len() {
                let suivante = match pas {
                    Pas::Mois => colonnes[j + 1].0.format("%Y").to_string(),
                    _ => format!("{}", mois_fr(colonnes[j + 1].0)),
                };
                if suivante != etiquette { break; }
                j += 1;
            }
            let c1 = NB_COLS + i as u16;
            let c2 = NB_COLS + j as u16;
            if c2 > c1 {
                ws.merge_range(1, c1, 1, c2, &etiquette, &s.entete_frise).map_err(xerr)?;
            } else {
                ws.write_with_format(1, c1, &etiquette, &s.entete_frise).map_err(xerr)?;
            }
            i = j + 1;
        }
    }

    for (i, c) in cols.iter().enumerate() {
        ws.write_with_format(2, i as u16, *c, &s.entete).map_err(xerr)?;
    }
    if let Some((pas, colonnes)) = &frise {
        for (i, (d, _)) in colonnes.iter().enumerate() {
            let lbl = match pas {
                Pas::Jour => d.format("%d").to_string(),
                Pas::Semaine => format!("S{}", d.iso_week().week()),
                Pas::Mois => mois_fr(*d).to_string(),
            };
            let c = NB_COLS + i as u16;
            ws.write_with_format(2, c, &lbl, &s.entete_frise).map_err(xerr)?;
            ws.set_column_width(c, if matches!(pas, Pas::Jour) { 3.0 } else { 5.5 }).ok();
        }
    }
    // Les en-têtes restent visibles quand on fait défiler.
    ws.set_freeze_panes(3, 1).map_err(xerr)?;

    let mut ligne = 3u32;
    for t in taches {
        let niv = t.niveau.clamp(1, 4) - 1;
        // Indentation par le texte : lisible partout, y compris à l'impression.
        let indent = "    ".repeat(niv);
        ws.write_with_format(ligne, 0, format!("{indent}{}", t.nom), &s.niveau[niv]).map_err(xerr)?;
        ws.write_with_format(ligne, 1, fr_date(t.debut), &s.centre).map_err(xerr)?;
        ws.write_with_format(ligne, 2, fr_date(t.fin), &s.centre).map_err(xerr)?;
        let jours = match (t.debut, t.fin) {
            (Some(a), Some(b)) => (b - a).num_days() + 1,
            _ => 0,
        };
        ws.write_with_format(ligne, 3, if jours > 0 { jours.to_string() } else { "—".into() }, &s.centre).map_err(xerr)?;
        ws.write_with_format(ligne, 4, statut_tache(&t.statut), &s.centre).map_err(xerr)?;
        ws.write_number_with_format(ligne, 5, t.avancement as f64, &s.pct).map_err(xerr)?;
        ws.write_number_with_format(ligne, 6, t.budget, if t.a_enfants { &s.money_gras } else { &s.money }).map_err(xerr)?;

        // La frise : une cellule colorée par unité de temps couverte.
        if let Some((pas, colonnes)) = &frise {
            let jalons_ici: Vec<&(String, Option<NaiveDate>, Option<String>)> =
                jalons.iter().filter(|(_, _, tid)| tid.as_deref() == Some(t.id.as_str())).collect();
            for (i, (cd, cf)) in colonnes.iter().enumerate() {
                let col = NB_COLS + i as u16;
                let dans_barre = matches!((t.debut, t.fin), (Some(a), Some(b)) if a <= *cf && b >= *cd);
                let jalon_ici = jalons_ici.iter().any(|(_, d, _)| matches!(d, Some(x) if x >= cd && x <= cf));
                if jalon_ici {
                    ws.write_with_format(ligne, col, "◆", &s.jalon).map_err(xerr)?;
                } else if dans_barre {
                    ws.write_with_format(ligne, col, "", &s.barre[niv]).map_err(xerr)?;
                } else if matches!(pas, Pas::Jour) && cd.weekday().num_days_from_monday() >= 5 {
                    ws.write_with_format(ligne, col, "", &s.weekend).map_err(xerr)?;
                } else {
                    ws.write_with_format(ligne, col, "", &s.cellule_vide).map_err(xerr)?;
                }
            }
        }
        ligne += 1;
    }

    // Jalons non rattachés à une activité : une ligne dédiée en bas.
    let libres: Vec<&(String, Option<NaiveDate>, Option<String>)> =
        jalons.iter().filter(|(_, d, tid)| tid.is_none() && d.is_some()).collect();
    if !libres.is_empty() {
        if let Some((_, colonnes)) = &frise {
            ligne += 1;
            ws.write_with_format(ligne, 0, "JALONS DU PROJET", &s.gras).map_err(xerr)?;
            for (i, (cd, cf)) in colonnes.iter().enumerate() {
                let col = NB_COLS + i as u16;
                let ici = libres.iter().any(|(_, d, _)| matches!(d, Some(x) if x >= cd && x <= cf));
                ws.write_with_format(ligne, col, if ici { "◆" } else { "" },
                    if ici { &s.jalon } else { &s.cellule_vide }).map_err(xerr)?;
            }
        }
    }

    // Légende, sous le tableau.
    ligne += 2;
    ws.write_with_format(ligne, 0, "Légende", &s.gras).map_err(xerr)?;
    ligne += 1;
    for (i, lbl) in ["Niveau 1", "Niveau 2", "Niveau 3", "Niveau 4"].iter().enumerate() {
        ws.write_with_format(ligne, i as u16, "", &s.barre[i]).map_err(xerr)?;
        ws.write_with_format(ligne + 1, i as u16, *lbl, &s.centre).map_err(xerr)?;
    }
    ws.write_with_format(ligne, 4, "◆", &s.jalon).map_err(xerr)?;
    ws.write_with_format(ligne + 1, 4, "Jalon", &s.centre).map_err(xerr)?;
    Ok(())
}

fn feuille_ressources_humaines(wb: &mut Workbook, s: &Styles, conn: &Connection, projet_id: &str) -> CoreResult<()> {
    let ws = wb.add_worksheet();
    ws.set_name("Ressources humaines").map_err(xerr)?;
    for (c, w) in [(0u16, 26.0), (1, 34.0), (2, 13.0), (3, 14.0), (4, 12.0), (5, 16.0)] {
        ws.set_column_width(c, w).ok();
    }
    ws.set_row_height(0, 26.0).ok();
    ws.merge_range(0, 0, 0, 5, "  RÉPARTITION PAR PERSONNE ET PAR ACTIVITÉ", &s.titre).map_err(xerr)?;
    for (i, c) in ["Personne", "Activité", "Mode", "Taux", "Quantité", "Coût"].iter().enumerate() {
        ws.write_with_format(2, i as u16, *c, &s.entete).map_err(xerr)?;
    }

    let mut stmt = conn.prepare(
        "SELECT i.nom, i.type, i.type_taux, i.taux, t.nom, a.heures_allouees
         FROM assignation a
         JOIN intervenant i ON i.id = a.intervenant_id
         JOIN tache t ON t.id = a.tache_id
         WHERE t.projet_id = ?1
         ORDER BY i.nom, t.ordre, t.nom")?;
    let lignes: Vec<(String, String, String, f64, String, f64)> = stmt
        .query_map(params![projet_id], |r| {
            Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?))
        })?
        .collect::<rusqlite::Result<Vec<_>>>()?;

    let mode = |t: &str| match t {
        "journalier" => "Journalier",
        "forfait" => "Forfait",
        _ => "Horaire",
    };
    let cout = |tt: &str, taux: f64, q: f64| if tt == "forfait" { taux } else { q * taux };

    let mut l = 3u32;
    let mut total = 0.0;
    let mut i = 0usize;
    while i < lignes.len() {
        let personne = lignes[i].0.clone();
        let mut j = i;
        let (mut sous_total, mut sous_qte) = (0.0, 0.0);
        while j < lignes.len() && lignes[j].0 == personne {
            let (nom, typ, tt, taux, tache, q) = &lignes[j];
            let c = cout(tt, *taux, *q);
            ws.write_with_format(l, 0, if j == i {
                format!("{nom}{}", if typ == "externe" { " (externe)" } else { "" })
            } else { String::new() }, &s.texte).map_err(xerr)?;
            ws.write_with_format(l, 1, tache, &s.texte).map_err(xerr)?;
            ws.write_with_format(l, 2, mode(tt), &s.centre).map_err(xerr)?;
            ws.write_number_with_format(l, 3, *taux, &s.money).map_err(xerr)?;
            ws.write_with_format(l, 4, if tt == "forfait" { "forfait".to_string() } else { format!("{q:.0}") }, &s.centre).map_err(xerr)?;
            ws.write_number_with_format(l, 5, c, &s.money).map_err(xerr)?;
            sous_total += c;
            sous_qte += q;
            total += c;
            l += 1;
            j += 1;
        }
        // Sous-total de la personne — demandé explicitement.
        ws.write_with_format(l, 0, format!("Total {personne}"), &s.synthese_lbl).map_err(xerr)?;
        for c in 1..=3u16 {
            ws.write_with_format(l, c, "", &s.synthese_lbl).map_err(xerr)?;
        }
        ws.write_with_format(l, 4, format!("{sous_qte:.0}"), &s.synthese_lbl).map_err(xerr)?;
        ws.write_number_with_format(l, 5, sous_total, &s.money_gras).map_err(xerr)?;
        l += 1;
        i = j;
    }
    if lignes.is_empty() {
        ws.write_with_format(3, 0, "Aucune affectation.", &s.texte).map_err(xerr)?;
    } else {
        l += 1;
        ws.write_with_format(l, 0, "TOTAL MAIN-D'ŒUVRE", &s.entete).map_err(xerr)?;
        for c in 1..=4u16 {
            ws.write_with_format(l, c, "", &s.entete).map_err(xerr)?;
        }
        ws.write_number_with_format(l, 5, total, &s.money_gras).map_err(xerr)?;
    }
    Ok(())
}

fn feuille_ressources(wb: &mut Workbook, s: &Styles, conn: &Connection, projet_id: &str) -> CoreResult<()> {
    let ws = wb.add_worksheet();
    ws.set_name("Ressources matérielles").map_err(xerr)?;
    for (c, w) in [(0u16, 34.0), (1, 16.0), (2, 30.0), (3, 14.0), (4, 11.0), (5, 16.0)] {
        ws.set_column_width(c, w).ok();
    }
    ws.set_row_height(0, 26.0).ok();
    ws.merge_range(0, 0, 0, 5, "  RESSOURCES MATÉRIELLES", &s.titre).map_err(xerr)?;
    for (i, c) in ["Libellé", "Type", "Rattachée à", "Coût unitaire", "Quantité", "Coût total"].iter().enumerate() {
        ws.write_with_format(2, i as u16, *c, &s.entete).map_err(xerr)?;
    }
    let mut stmt = conn.prepare(
        "SELECT r.libelle, r.type, t.nom, r.cout_unitaire, r.quantite
         FROM ressource r LEFT JOIN tache t ON t.id = r.tache_id
         WHERE r.projet_id = ?1 ORDER BY r.cree_le")?;
    let mut l = 3u32;
    let mut total = 0.0;
    let mut rows = stmt.query(params![projet_id])?;
    while let Some(r) = rows.next()? {
        let (cu, q): (f64, f64) = (r.get(3)?, r.get(4)?);
        ws.write_with_format(l, 0, r.get::<_, String>(0)?, &s.texte).map_err(xerr)?;
        ws.write_with_format(l, 1, r.get::<_, String>(1)?, &s.centre).map_err(xerr)?;
        ws.write_with_format(l, 2, r.get::<_, Option<String>>(2)?.unwrap_or_else(|| "Tout le projet".into()), &s.texte).map_err(xerr)?;
        ws.write_number_with_format(l, 3, cu, &s.money).map_err(xerr)?;
        ws.write_number_with_format(l, 4, q, &s.centre).map_err(xerr)?;
        ws.write_number_with_format(l, 5, cu * q, &s.money).map_err(xerr)?;
        total += cu * q;
        l += 1;
    }
    if l == 3 {
        ws.write_with_format(3, 0, "Aucune ressource.", &s.texte).map_err(xerr)?;
    } else {
        ws.write_with_format(l, 0, "TOTAL", &s.entete).map_err(xerr)?;
        for c in 1..=4u16 {
            ws.write_with_format(l, c, "", &s.entete).map_err(xerr)?;
        }
        ws.write_number_with_format(l, 5, total, &s.money_gras).map_err(xerr)?;
    }
    Ok(())
}

fn feuille_jalons(wb: &mut Workbook, s: &Styles, conn: &Connection, projet_id: &str) -> CoreResult<()> {
    let ws = wb.add_worksheet();
    ws.set_name("Jalons et livrables").map_err(xerr)?;
    for (c, w) in [(0u16, 34.0), (1, 30.0), (2, 13.0), (3, 13.0), (4, 14.0)] {
        ws.set_column_width(c, w).ok();
    }
    ws.set_row_height(0, 26.0).ok();
    ws.merge_range(0, 0, 0, 4, "  JALONS", &s.titre).map_err(xerr)?;
    for (i, c) in ["Jalon", "Activité liée", "Date prévue", "Date réelle", "Statut"].iter().enumerate() {
        ws.write_with_format(2, i as u16, *c, &s.entete).map_err(xerr)?;
    }
    let mut stmt = conn.prepare(
        "SELECT j.nom, t.nom, j.date_prevue, j.date_reelle, j.statut
         FROM jalon j LEFT JOIN tache t ON t.id = j.tache_id
         WHERE j.projet_id = ?1 ORDER BY j.date_prevue")?;
    let mut l = 3u32;
    let mut rows = stmt.query(params![projet_id])?;
    while let Some(r) = rows.next()? {
        ws.write_with_format(l, 0, r.get::<_, String>(0)?, &s.texte).map_err(xerr)?;
        ws.write_with_format(l, 1, r.get::<_, Option<String>>(1)?.unwrap_or_else(|| "—".into()), &s.texte).map_err(xerr)?;
        ws.write_with_format(l, 2, fr(&r.get::<_, Option<String>>(2)?), &s.centre).map_err(xerr)?;
        ws.write_with_format(l, 3, fr(&r.get::<_, Option<String>>(3)?), &s.centre).map_err(xerr)?;
        ws.write_with_format(l, 4, match r.get::<_, String>(4)?.as_str() {
            "atteint" => "Atteint", "manque" => "Manqué", _ => "À venir",
        }, &s.centre).map_err(xerr)?;
        l += 1;
    }
    if l == 3 {
        ws.write_with_format(3, 0, "Aucun jalon.", &s.texte).map_err(xerr)?;
        l = 4;
    }

    l += 2;
    ws.merge_range(l, 0, l, 4, "  LIVRABLES", &s.titre).map_err(xerr)?;
    l += 2;
    for (i, c) in ["Livrable", "Activité", "Attendu le", "Livré le", "Statut"].iter().enumerate() {
        ws.write_with_format(l, i as u16, *c, &s.entete).map_err(xerr)?;
    }
    l += 1;
    let mut stmt2 = conn.prepare(
        "SELECT v.nom, t.nom, v.date_attendue, v.date_livraison, v.statut
         FROM livrable v LEFT JOIN tache t ON t.id = v.tache_id
         WHERE v.projet_id = ?1 ORDER BY v.ordre, v.nom")?;
    let mut rows2 = stmt2.query(params![projet_id])?;
    let depart = l;
    while let Some(r) = rows2.next()? {
        ws.write_with_format(l, 0, r.get::<_, String>(0)?, &s.texte).map_err(xerr)?;
        ws.write_with_format(l, 1, r.get::<_, Option<String>>(1)?.unwrap_or_else(|| "—".into()), &s.texte).map_err(xerr)?;
        ws.write_with_format(l, 2, fr(&r.get::<_, Option<String>>(2)?), &s.centre).map_err(xerr)?;
        ws.write_with_format(l, 3, fr(&r.get::<_, Option<String>>(3)?), &s.centre).map_err(xerr)?;
        ws.write_with_format(l, 4, match r.get::<_, String>(4)?.as_str() {
            "livre" => "Livré", "accepte" => "Accepté", "refuse" => "Refusé",
            "en_cours" => "En cours", _ => "À produire",
        }, &s.centre).map_err(xerr)?;
        l += 1;
    }
    if l == depart {
        ws.write_with_format(l, 0, "Aucun livrable.", &s.texte).map_err(xerr)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------

fn fr(s: &Option<String>) -> String {
    match jour(s) {
        Some(d) => d.format("%d/%m/%Y").to_string(),
        None => "—".into(),
    }
}
fn fr_date(d: Option<NaiveDate>) -> String {
    d.map(|x| x.format("%d/%m/%Y").to_string()).unwrap_or_else(|| "—".into())
}
fn mois_fr(d: NaiveDate) -> &'static str {
    ["janv.", "févr.", "mars", "avr.", "mai", "juin",
     "juil.", "août", "sept.", "oct.", "nov.", "déc."][(d.month() - 1) as usize]
}
fn xerr(e: rust_xlsxwriter::XlsxError) -> CoreError {
    CoreError::Rule(format!("écriture du fichier Excel : {e}"))
}
