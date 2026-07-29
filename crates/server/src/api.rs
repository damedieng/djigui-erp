//! Couche API HTTP/JSON. Traduit les appels réseau en opérations du cœur métier.
//! Aucune règle métier ici : uniquement du transport et de la traduction d'erreurs.

use crate::state::AppState;
use axum::{
    extract::{FromRequestParts, Path, Query, State},
    http::request::Parts,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use djigui_core::modules::{
    abonnement, article, audit, categorie, comptabilite, dependance, depot, document, inventaire, jalon,
    activation, calendrier, marche, moyen_paiement, notification, paiement, prix_estime,
    paie_employe, paie_parametres, parametres, production, projet, rapport, rendez_vous, sauvegarde, seed, seeder, session_caisse,
    stock, taux_tva, taxe, tiers, utilisateur,
};
use djigui_core::CoreError;
use serde::Deserialize;

/// Utilisateur courant, extrait de l'en-tête `X-Utilisateur-Id` envoyé par l'UI.
/// Jamais bloquant : absent = action non authentifiée (id `None`).
pub struct Acteur(pub Option<String>);

#[axum::async_trait]
impl<S: Send + Sync> FromRequestParts<S> for Acteur {
    type Rejection = std::convert::Infallible;
    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let id = parts
            .headers
            .get("X-Utilisateur-Id")
            .and_then(|v| v.to_str().ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty());
        Ok(Acteur(id))
    }
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/api/sante", get(sante))
        .route("/api/parametres", get(get_parametres).put(put_parametres))
        .route("/api/config", get(get_config))
        .route("/api/config/:cle", axum::routing::put(put_config))
        .route("/api/taux-tva", get(liste_taux).post(cree_taux))
        .route("/api/taux-tva/:valeur", axum::routing::delete(supprime_taux))
        .route("/api/taxes", get(liste_taxes).post(cree_taxe))
        .route("/api/taxes/:id", axum::routing::put(modifie_taxe).delete(supprime_taxe))
        .route("/api/taxes/:id/actif", post(taxe_actif))
        .route("/api/tiers", get(liste_tiers).post(cree_tiers))
        .route("/api/tiers/:id", get(get_tiers).put(modifie_tiers).delete(supprime_tiers))
        .route("/api/tiers/lot/desactiver", post(tiers_desactiver_lot))
        .route("/api/tiers/lot/role", post(tiers_role_lot))
        .route("/api/articles", get(liste_articles).post(cree_article))
        .route("/api/articles/page", get(liste_articles_page))
        .route("/api/articles/:id", get(get_article).put(modifie_article).delete(supprime_article))
        .route("/api/articles/lot/categorie", post(articles_categorie_lot))
        .route("/api/articles/lot/desactiver", post(articles_desactiver_lot))
        .route("/api/articles/lot/supprimer", post(articles_supprimer_lot))
        .route("/api/categories", get(liste_categories).post(cree_categorie))
        .route("/api/categories/:id", axum::routing::put(modifie_categorie).delete(supprime_categorie))
        .route("/api/depots", get(liste_depots).post(cree_depot))
        .route("/api/depots/:id", axum::routing::put(renomme_depot))
        .route("/api/depots/:id/defaut", post(depot_defaut))
        .route("/api/stock/depot/:id", get(etat_stock_depot))
        .route("/api/stock/transfert", post(stock_transfert))
        .route("/api/stock/inventaire", post(stock_inventaire))
        .route("/api/inventaires", get(liste_inventaires).post(cree_inventaire))
        .route("/api/inventaires/:id", get(get_inventaire))
        .route("/api/documents", get(liste_documents).post(cree_document))
        .route("/api/documents/:id", get(get_document).delete(supprime_document))
        .route("/api/documents/:id/valider", post(valider_document))
        .route("/api/documents/:id/transformer", post(transformer_document))
        .route("/api/documents/:id/annuler", post(annuler_document))
        .route("/api/moyens-paiement", get(liste_moyens).post(cree_moyen))
        .route("/api/moyens-paiement/:id", axum::routing::put(modifie_moyen).delete(supprime_moyen))
        .route("/api/moyens-paiement/lot/actif", post(moyens_actif_lot))
        .route("/api/moyens-paiement/ordre", post(moyens_reordonner))
        .route("/api/dev/seed-articles", post(seed_articles))
        .route("/api/imprimantes", get(liste_imprimantes))
        .route("/api/imprimer", post(imprimer_ticket))
        .route("/api/etat/tickets-attente", axum::routing::put(maj_tickets_attente))
        .route("/api/paiements", get(liste_paiements).post(cree_paiement))
        .route("/api/caisses", get(liste_caisses).post(cree_caisse))
        .route("/api/caisses/:id", axum::routing::put(modifie_caisse).delete(supprime_caisse))
        .route("/api/caisses/:id/session", get(session_ouverte_caisse))
        .route("/api/sessions", get(liste_sessions))
        .route("/api/sessions/ouvrir", post(ouvrir_session))
        .route("/api/sessions/:id/fermer", post(fermer_session))
        .route("/api/recalcul-soldes", post(recalcul_soldes))
        .route("/api/rendez-vous", get(liste_rdv).post(cree_rdv))
        .route("/api/rendez-vous/:id", axum::routing::put(modifie_rdv).delete(supprime_rdv))
        .route("/api/rendez-vous/lot/statut", post(rdv_statut_lot))
        .route("/api/rendez-vous/lot/supprimer", post(rdv_supprimer_lot))
        .route("/api/rendez-vous/export-ics", post(export_ics))
        .route("/api/projets", get(liste_projets).post(cree_projet))
        .route("/api/projets/:id", get(get_projet).put(modifie_projet).delete(supprime_projet))
        .route("/api/projets/:id/statut", post(projet_statut))
        .route("/api/projets/:id/taches", get(liste_taches))
        .route("/api/taches", post(cree_tache))
        .route("/api/taches/:id", axum::routing::put(modifie_tache).delete(supprime_tache))
        .route("/api/taches/lot/statut", post(taches_statut_lot))
        .route("/api/taches/lot/supprimer", post(taches_supprimer_lot))
        .route("/api/taches/:id/actions", get(liste_actions).post(cree_action))
        .route("/api/projets/:id/assignations", get(liste_assignations))
        .route("/api/assignations", post(cree_assignation))
        .route("/api/assignations/:id", axum::routing::put(modifie_assignation).delete(supprime_assignation))
        .route("/api/intervenants", get(liste_intervenants).post(cree_intervenant))
        .route("/api/intervenants/:id", axum::routing::put(modifie_intervenant).delete(supprime_intervenant))
        // Liens de précédence — flèches du Gantt (migration 0029)
        .route("/api/projets/:id/dependances", get(liste_dependances))
        .route("/api/dependances", post(cree_dependance))
        .route("/api/dependances/:id", axum::routing::delete(supprime_dependance))
        .route("/api/projets/:id/coherence", get(coherence_projet))
        .route("/api/projets/:id/harmoniser", post(harmonise_projet))
        .route("/api/projets/:id/export-xlsx", post(export_projet_xlsx))
        // Notifications du jour (recalculées à chaque appel)
        .route("/api/notifications", get(liste_notifications))
        .route("/api/notifications/lues", post(notifications_lues))
        .route("/api/notifications/reafficher", post(notifications_reafficher))
        // Jalons / livrables / documents joints (migration 0028)
        .route("/api/projets/:id/jalons", get(liste_jalons))
        .route("/api/jalons", post(cree_jalon))
        .route("/api/jalons/:id", axum::routing::put(modifie_jalon))
        .route("/api/jalons/lot/statut", post(jalons_statut_lot))
        .route("/api/jalons/lot/supprimer", post(jalons_supprimer_lot))
        .route("/api/projets/:id/livrables", get(liste_livrables))
        .route("/api/livrables", post(cree_livrable))
        .route("/api/livrables/:id", axum::routing::put(modifie_livrable))
        .route("/api/livrables/lot/statut", post(livrables_statut_lot))
        .route("/api/livrables/lot/supprimer", post(livrables_supprimer_lot))
        .route("/api/projets/:id/documents-joints", get(liste_documents_joints))
        .route("/api/documents-joints", post(cree_document_joint))
        .route("/api/documents-joints/:id", axum::routing::delete(supprime_document_joint))
        .route("/api/documents-joints/:id/ouvrir", post(ouvre_document_joint))
        .route("/api/projets/:id/ressources", get(liste_ressources))
        .route("/api/ressources", post(cree_ressource))
        .route("/api/ressources/:id", axum::routing::put(modifie_ressource).delete(supprime_ressource))
        // Production : nomenclatures (recettes) et ordres de fabrication (mig 0031)
        .route("/api/nomenclatures", get(liste_nomenclatures).post(cree_nomenclature))
        .route(
            "/api/nomenclatures/:id",
            get(get_nomenclature)
                .put(modifie_nomenclature)
                .delete(supprime_nomenclature),
        )
        .route("/api/ordres-production", get(liste_ordres).post(cree_ordre))
        .route(
            "/api/ordres-production/:id",
            get(get_ordre).put(modifie_ordre).delete(supprime_ordre),
        )
        .route("/api/ordres-production/:id/statut", post(ordre_statut))
        .route("/api/ordres-production/:id/cloturer", post(ordre_cloturer))
        .route("/api/ordres-production/:id/annuler", post(ordre_annuler))
        .route("/api/ordres-production/lot/statut", post(ordres_statut_lot))
        .route("/api/ordres-production/lot/supprimer", post(ordres_supprimer_lot))
        // Comptabilité — écran réservé au comptable (mig 0034). Le comptable
        // crée SES comptes, écrit SES règles, et range l'historique existant.
        .route("/api/comptes", get(liste_comptes).post(cree_compte))
        .route(
            "/api/comptes/:numero",
            get(get_compte).put(modifie_compte).delete(supprime_compte),
        )
        .route("/api/comptes/plan-ohada", post(installe_plan_ohada))
        .route("/api/regles-comptables", get(liste_regles).post(cree_regle))
        .route(
            "/api/regles-comptables/:id",
            axum::routing::put(modifie_regle).delete(supprime_regle),
        )
        .route("/api/regles-comptables/lot/supprimer", post(regles_supprimer_lot))
        .route("/api/comptabilite/operations", get(liste_operations))
        .route("/api/comptabilite/rattacher", post(rattache_operations))
        .route("/api/comptabilite/rattacher-tout", post(rattache_tout))
        .route("/api/comptabilite/ecritures", get(liste_ecritures))
        .route("/api/comptabilite/ecritures/:id", get(get_ecriture))
        .route("/api/comptabilite/ecritures/:id/contrepasser", post(contrepasse_ecriture))
        .route("/api/comptabilite/ecritures/:id/rejouer", post(rejoue_ecriture))
        .route("/api/comptabilite/rejouer-incompletes", post(rejoue_incompletes))
        .route("/api/comptabilite/lignes/:id/compte", post(affecte_compte_ligne))
        .route("/api/comptabilite/grand-livre/:numero", get(get_grand_livre))
        .route("/api/comptabilite/balance", get(get_balance))
        .route("/api/comptabilite/lettrer", post(lettre_lignes))
        .route("/api/comptabilite/delettrer", post(delettre_lignes))
        // Passation et suivi des marchés (mig 0037)
        .route("/api/marches", get(liste_marches).post(cree_marche))
        .route("/api/marches/:id", get(get_marche).put(modifie_marche).delete(supprime_marche))
        .route("/api/marches/:id/statut", post(marche_statut))
        .route("/api/marches/:id/annuler", post(marche_annuler))
        .route("/api/marches/lot/statut", post(marches_statut_lot))
        .route("/api/marches/lot/supprimer", post(marches_supprimer_lot))
        .route("/api/marches/:id/soumissionnaires", post(cree_soumissionnaire))
        .route(
            "/api/soumissionnaires/:id",
            axum::routing::put(modifie_soumissionnaire).delete(supprime_soumissionnaire),
        )
        .route("/api/soumissionnaires/:id/attribuer", post(attribue_marche))
        .route("/api/marches/:id/avenants", post(cree_avenant))
        .route(
            "/api/avenants/:id",
            axum::routing::put(modifie_avenant).delete(supprime_avenant),
        )
        .route("/api/avenants/:id/statut", post(avenant_statut))
        .route("/api/marches/:id/receptions", post(cree_reception))
        .route(
            "/api/receptions/:id",
            axum::routing::put(modifie_reception).delete(supprime_reception),
        )
        .route("/api/receptions/:id/lever-reserves", post(reception_lever_reserves))
        // Activation des modules (migration 0040). ⚠️ `souscrit` est une donnée
        // de FACTURATION posée à l'installation ; `actif` est le confort du
        // client. Les deux ne se pilotent pas par la même route.
        // Calendriers superposés : l'agenda affiche PAR-DESSUS ses rendez-vous
        // les échéances des autres modules. ⚠️ LECTURE SEULE — aucune route
        // d'écriture ici, on modifie dans l'écran d'origine.
        .route("/api/calendrier", get(liste_evenements))
        .route("/api/calendrier/sources", get(liste_calendriers))
        // Sauvegarde chiffrée (mig 0042). Les copies partent vers des dossiers
        // choisis par l'utilisateur ; aucun envoi vers un service en ligne.
        .route("/api/sauvegarde/parametres", get(sauvegarde_parametres).put(sauvegarde_modifier))
        .route("/api/sauvegarde/mot-de-passe", post(sauvegarde_mot_de_passe))
        .route("/api/sauvegarde/licence", post(sauvegarde_licence))
        .route("/api/sauvegarde/destinations", get(sauvegarde_destinations).post(sauvegarde_ajout_destination))
        .route(
            "/api/sauvegarde/destinations/:id",
            axum::routing::put(sauvegarde_maj_destination).delete(sauvegarde_suppr_destination),
        )
        .route("/api/sauvegarde/executer", post(sauvegarde_executer))
        .route("/api/sauvegarde/journal", get(sauvegarde_journal))
        .route("/api/sauvegarde/parcourir", get(sauvegarde_parcourir))
        .route("/api/sauvegarde/suggestions", get(sauvegarde_suggestions))
        .route("/api/sauvegarde/choisir-dossier", post(sauvegarde_choisir_dossier))
        .route("/api/sauvegarde/apercu", post(sauvegarde_apercu))
        .route("/api/sauvegarde/restaurer", post(sauvegarde_restaurer))
        // Paie & RH — paramètres légaux (mig 0044). ⚠️ Aucun taux n'est dans
        // le code : ces routes sont le SEUL moyen de les faire évoluer.
        .route("/api/paie/parametres", get(paie_lire_parametres))
        .route("/api/paie/parametres/periode", post(paie_nouvelle_periode))
        .route("/api/paie/parametres/corriger", post(paie_corriger_periode))
        .route("/api/paie/parametres/verifie", post(paie_marquer_verifie))
        .route("/api/paie/employeur", axum::routing::put(paie_enregistrer_employeur))
        // Salariés & contrats (mig 0045).
        .route("/api/paie/employes", get(paie_liste_employes).post(paie_cree_employe))
        .route("/api/paie/employes/:id",
               get(paie_get_employe).put(paie_modifie_employe).delete(paie_supprime_employe))
        .route("/api/paie/employes/:id/depart", post(paie_depart))
        .route("/api/paie/employes/:id/reintegrer", post(paie_reintegrer))
        .route("/api/paie/employes/:id/contrats", get(paie_liste_contrats))
        .route("/api/paie/employes/lot/depart", post(paie_depart_lot))
        .route("/api/paie/contrats", post(paie_cree_contrat))
        .route("/api/paie/contrats/:id", axum::routing::put(paie_modifie_contrat))
        .route("/api/modules", get(liste_modules))
        .route("/api/modules/formules", get(liste_formules))
        .route("/api/modules/formule", post(applique_formule))
        .route("/api/modules/:code/actif", post(module_actif))
        .route("/api/marches/phases", get(marches_par_phase))
        .route("/api/marches/export-suivi", post(export_suivi_marches))
        .route("/api/marches/:id/incidents", post(cree_incident))
        .route("/api/incidents/:id", axum::routing::delete(supprime_incident))
        .route("/api/incidents/:id/clore", post(clot_incident))
        .route("/api/marche-etapes/:id", axum::routing::put(modifie_etape))
        .route("/api/marche-etapes/:id/statut", post(etape_statut))
        .route("/api/marche-etapes/:id/plan-replanification", get(plan_replanif))
        .route("/api/marche-etapes/:id/replanifier", post(replanif))
        .route("/api/marche-types", get(liste_types_marche).post(cree_type_marche))
        .route(
            "/api/marche-types/:id",
            axum::routing::put(modifie_type_marche).delete(supprime_type_marche),
        )
        // Rapports (§7) — le langage du commerçant, pas celui du comptable.
        .route("/api/rapports/benefices", get(rapport_benefices))
        .route("/api/rapports/journal-ventes", get(rapport_journal_ventes))
        .route("/api/rapports/journal-achats", get(rapport_journal_achats))
        .route("/api/rapports/marges", get(rapport_marges))
        .route("/api/rapports/stock", get(rapport_stock))
        .route("/api/rapports/encours-clients", get(rapport_encours_clients))
        .route("/api/rapports/encours-fournisseurs", get(rapport_encours_fournisseurs))
        // Continuité de la numérotation (N1 OHADA). Rapport, jamais blocage :
        // un trou a parfois une explication légitime, mais l'utilisateur doit
        // le connaître avant qu'un contrôleur ne le lui montre.
        .route("/api/rapports/numerotation", get(rapport_numerotation))
        // Prix d'achat estimés (mig 0035) : des chiffres de démonstration assumés.
        .route("/api/prix/apercu", get(prix_apercu))
        .route("/api/prix/estimer", post(prix_estimer))
        .route("/api/prix/effacer-estimations", post(prix_effacer))
        .route("/api/prix/a-completer", get(prix_a_completer))
        .route("/api/prix/reels", post(prix_reels))
        .route("/api/export/xlsx/enregistrer", post(export_xlsx_fichier))
        .route("/api/abonnements", get(liste_abonnements).post(cree_abonnement))
        .route("/api/abonnements/:id", axum::routing::put(modifie_abonnement).delete(supprime_abonnement))
        .route("/api/abonnements/generer", post(generer_abonnements))
        .route("/api/login", post(login))
        .route("/api/utilisateurs", get(liste_utilisateurs).post(cree_utilisateur))
        .route("/api/utilisateurs/:id", axum::routing::put(modifie_utilisateur).delete(supprime_utilisateur))
        .route("/api/journal-audit", get(liste_audit))
        .route("/api/catalogues", get(liste_catalogues))
        .route("/api/catalogues/appliquer", post(applique_catalogues))
        .route("/api/catalogues/:code", get(detail_catalogue))
        .with_state(state)
}

async fn sante() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "service": "djigui-server", "statut": "ok" }))
}

// ---- Paramètres entreprise (singleton) -------------------------------------

async fn get_parametres(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(parametres::lire(&conn)?))
}

async fn put_parametres(
    State(s): State<AppState>,
    Json(p): Json<parametres::ParametresEntreprise>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    parametres::enregistrer(&conn, &p)?;
    Ok(Json(p))
}

// ---- Taux de TVA ------------------------------------------------------------

async fn liste_taux(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(taux_tva::lister(&conn)?))
}

async fn cree_taux(
    State(s): State<AppState>,
    Json(nt): Json<taux_tva::NouveauTaux>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok((StatusCode::CREATED, Json(taux_tva::creer(&conn, &nt)?)))
}

async fn supprime_taux(
    State(s): State<AppState>,
    Path(valeur): Path<f64>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    taux_tva::supprimer(&conn, valeur)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- Paramètres globaux (clé/valeur) ---------------------------------------

async fn get_config(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(parametres::lister_globaux(&conn)?))
}

#[derive(Deserialize)]
struct CorpsConfig {
    valeur: String,
}

async fn put_config(
    State(s): State<AppState>,
    Path(cle): Path<String>,
    Json(b): Json<CorpsConfig>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    parametres::ecrire_global(&conn, &cle, &b.valeur)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- Taxes (multi-taxes) ----------------------------------------------------

#[derive(Deserialize)]
struct FiltreTaxes {
    #[serde(default)]
    tous: bool,
}

async fn liste_taxes(
    State(s): State<AppState>,
    Query(q): Query<FiltreTaxes>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let taxes = if q.tous { taxe::lister_tous(&conn)? } else { taxe::lister(&conn)? };
    Ok(Json(taxes))
}

#[derive(Deserialize)]
struct CorpsActif {
    actif: bool,
}

async fn taxe_actif(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(b): Json<CorpsActif>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    taxe::definir_actif(&conn, &id, b.actif)?;
    Ok(StatusCode::NO_CONTENT)
}
async fn cree_taxe(
    State(s): State<AppState>,
    Json(nt): Json<taxe::NouvelleTaxe>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok((StatusCode::CREATED, Json(taxe::creer(&conn, &nt)?)))
}
async fn modifie_taxe(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(nt): Json<taxe::NouvelleTaxe>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(taxe::modifier(&conn, &id, &nt)?))
}
async fn supprime_taxe(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    taxe::supprimer(&conn, &id)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- Tiers ------------------------------------------------------------------

#[derive(Deserialize)]
struct FiltreTiers {
    #[serde(default)]
    filtre: tiers::Filtre,
}

async fn liste_tiers(
    State(s): State<AppState>,
    Query(q): Query<FiltreTiers>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(tiers::lister(&conn, q.filtre)?))
}

async fn cree_tiers(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(nt): Json<tiers::NouveauTiers>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let t = tiers::creer(&conn, &nt)?;
    journaliser(&conn, &acteur, "creation", "tiers", Some(&t.id), Some(&t.nom));
    Ok((StatusCode::CREATED, Json(t)))
}

async fn get_tiers(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(tiers::lire(&conn, &id)?))
}

async fn modifie_tiers(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(nt): Json<tiers::NouveauTiers>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let t = tiers::modifier(&conn, &id, &nt)?;
    journaliser(&conn, &acteur, "modification", "tiers", Some(&t.id), Some(&t.nom));
    Ok(Json(t))
}

async fn supprime_tiers(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    tiers::desactiver(&conn, &id)?;
    journaliser(&conn, &acteur, "desactivation", "tiers", Some(&id), None);
    Ok(StatusCode::NO_CONTENT)
}

// ---- Tiers : traitement par lot --------------------------------------------

#[derive(Deserialize)]
struct LotIds {
    ids: Vec<String>,
}

#[derive(Deserialize)]
struct LotRole {
    ids: Vec<String>,
    type_role: djigui_core::domain::TypeRole,
}

async fn tiers_desactiver_lot(
    State(s): State<AppState>,
    Json(b): Json<LotIds>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(tiers::desactiver_lot(&conn, &b.ids)?))
}

async fn tiers_role_lot(
    State(s): State<AppState>,
    Json(b): Json<LotRole>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(tiers::changer_role_lot(&conn, &b.ids, b.type_role)?))
}

// ---- Articles ---------------------------------------------------------------

#[derive(Deserialize)]
struct FiltreArticles {
    #[serde(default)]
    filtre: article::Filtre,
}

async fn liste_articles(
    State(s): State<AppState>,
    Query(q): Query<FiltreArticles>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(article::lister(&conn, q.filtre)?))
}

#[derive(Deserialize)]
struct SeedParams {
    #[serde(default = "seed_defaut")]
    n: usize,
}
fn seed_defaut() -> usize { 2000 }

/// Endpoint de développement : génère un jeu « supermarché » (§ perf).
async fn seed_articles(
    State(s): State<AppState>,
    Query(p): Query<SeedParams>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let cree = seed::generer(&conn, p.n)?;
    Ok(Json(serde_json::json!({ "crees": cree })))
}

async fn liste_articles_page(
    State(s): State<AppState>,
    Query(req): Query<article::RequeteListe>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(article::lister_page(&conn, &req)?))
}

// ---- Impression native des tickets (§ caisse) ------------------------------

/// Clé du paramètre global stockant l'imprimante ticket choisie.
const CLE_IMPRIMANTE: &str = "imprimante_ticket";
/// Clé du mode d'impression : "standard" (pilote) ou "thermique" (ESC/POS).
const CLE_MODE: &str = "imprimante_mode";

/// Liste les imprimantes installées sur le poste (pour le choix en Paramètres).
async fn liste_imprimantes() -> impl IntoResponse {
    Json(crate::impression::lister())
}

#[derive(Deserialize)]
struct EtatTickets {
    n: i64,
}

/// L'UI publie le nombre de tickets en attente ; la coquille desktop le lit pour
/// confirmer avant la fermeture de la fenêtre (§ garde-fou caisse).
async fn maj_tickets_attente(Json(b): Json<EtatTickets>) -> impl IntoResponse {
    crate::TICKETS_EN_ATTENTE.store(b.n.max(0), std::sync::atomic::Ordering::Relaxed);
    StatusCode::NO_CONTENT
}

// ---- Caisse & paiements (§5.6 / §6.4) --------------------------------------

#[derive(Deserialize)]
struct FiltrePaiements {
    #[serde(default)]
    tiers_id: Option<String>,
}

async fn liste_paiements(
    State(s): State<AppState>,
    Query(q): Query<FiltrePaiements>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(paiement::lister(&conn, q.tiers_id.as_deref())?))
}

async fn cree_paiement(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(np): Json<paiement::NouveauPaiement>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let p = paiement::enregistrer(&conn, &np)?;
    marquer_auteur(&conn, "paiement", &p.id, &acteur);
    let detail = format!("{} {} — {}", p.sens, p.montant, p.mode);
    journaliser(&conn, &acteur, "paiement", "paiement", Some(&p.id), Some(&detail));
    Ok((StatusCode::CREATED, Json(p)))
}

async fn liste_caisses(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(paiement::lister_caisses(&conn)?))
}

#[derive(Deserialize)]
struct CorpsCaisse {
    nom: String,
    #[serde(default)]
    depot_id: Option<String>,
}

async fn cree_caisse(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(b): Json<CorpsCaisse>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let c = paiement::creer_caisse(&conn, &b.nom, b.depot_id.as_deref())?;
    journaliser(&conn, &acteur, "creation", "caisse", Some(&c.id), Some(&c.nom));
    Ok((StatusCode::CREATED, Json(c)))
}

async fn modifie_caisse(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(b): Json<CorpsCaisse>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let c = paiement::modifier_caisse(&conn, &id, &b.nom, b.depot_id.as_deref())?;
    journaliser(&conn, &acteur, "modification", "caisse", Some(&c.id), Some(&c.nom));
    Ok(Json(c))
}

async fn supprime_caisse(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    paiement::supprimer_caisse(&conn, &id)?;
    journaliser(&conn, &acteur, "suppression", "caisse", Some(&id), None);
    Ok(StatusCode::NO_CONTENT)
}

// ---- Sessions de caisse ----------------------------------------------------

async fn session_ouverte_caisse(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(session_caisse::session_ouverte(&conn, &id)?))
}

async fn liste_sessions(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(session_caisse::lister(&conn, None)?))
}

async fn ouvrir_session(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(o): Json<session_caisse::Ouverture>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let sess = session_caisse::ouvrir(&conn, &o, acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "ouverture", "session_caisse", Some(&sess.id),
                Some(&format!("fond {}", sess.fond_ouverture)));
    Ok((StatusCode::CREATED, Json(sess)))
}

async fn fermer_session(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(f): Json<session_caisse::Fermeture>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let sess = session_caisse::fermer(&conn, &id, &f)?;
    journaliser(&conn, &acteur, "fermeture", "session_caisse", Some(&sess.id),
                Some(&format!("écart {}", sess.ecart.unwrap_or(0.0))));
    Ok(Json(sess))
}

/// Utilitaire de réparation : recalcule tous les soldes depuis les journaux (§6.4).
async fn recalcul_soldes(
    State(s): State<AppState>,
    acteur: Acteur,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let n = paiement::recalculer_soldes(&conn)?;
    journaliser(&conn, &acteur, "recalcul_soldes", "caisse", None, None);
    Ok(Json(serde_json::json!({ "soldes_repares": n })))
}

// ---- Gestion de projet — Incrément 1 (migration 0021) ---------------------

#[derive(Deserialize)]
struct FiltreProjets {
    #[serde(default)]
    statut: Option<djigui_core::domain::StatutProjet>,
}

async fn liste_projets(
    State(s): State<AppState>,
    Query(f): Query<FiltreProjets>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(projet::lister(&conn, f.statut)?))
}

async fn get_projet(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(projet::lire(&conn, &id)?))
}

async fn cree_projet(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(n): Json<projet::NouveauProjet>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let p = projet::creer(&conn, &n, acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "creation", "projet", Some(&p.id), Some(&p.nom));
    Ok((StatusCode::CREATED, Json(p)))
}

async fn modifie_projet(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(n): Json<projet::NouveauProjet>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let p = projet::modifier(&conn, &id, &n)?;
    journaliser(&conn, &acteur, "modification", "projet", Some(&p.id), Some(&p.nom));
    Ok(Json(p))
}

#[derive(Deserialize)]
struct CorpsStatutProjet {
    statut: djigui_core::domain::StatutProjet,
}

async fn projet_statut(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(b): Json<CorpsStatutProjet>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let p = projet::changer_statut(&conn, &id, b.statut)?;
    journaliser(&conn, &acteur, "statut", "projet", Some(&p.id), Some(&p.statut));
    Ok(Json(p))
}

async fn supprime_projet(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    projet::supprimer(&conn, &id)?;
    journaliser(&conn, &acteur, "suppression", "projet", Some(&id), None);
    Ok(StatusCode::NO_CONTENT)
}

async fn liste_taches(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(projet::lister_taches(&conn, &id)?))
}

async fn cree_tache(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(n): Json<projet::NouvelleTache>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let t = projet::creer_tache(&conn, &n)?;
    journaliser(&conn, &acteur, "creation", "tache", Some(&t.id), Some(&t.nom));
    Ok((StatusCode::CREATED, Json(t)))
}

async fn modifie_tache(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(n): Json<projet::NouvelleTache>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let t = projet::modifier_tache(&conn, &id, &n)?;
    journaliser(&conn, &acteur, "modification", "tache", Some(&t.id), Some(&t.nom));
    Ok(Json(t))
}

async fn supprime_tache(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    projet::supprimer_taches(&conn, &[id.clone()])?;
    journaliser(&conn, &acteur, "suppression", "tache", Some(&id), None);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct LotTachesStatut {
    ids: Vec<String>,
    statut: djigui_core::domain::StatutTache,
}

async fn taches_statut_lot(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(b): Json<LotTachesStatut>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let n = projet::changer_statut_taches(&conn, &b.ids, b.statut)?;
    journaliser(&conn, &acteur, "statut", "tache", None, Some(&format!("{n} tâche(s)")));
    Ok(Json(serde_json::json!({ "touches": n })))
}

async fn taches_supprimer_lot(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(b): Json<LotIds>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let n = projet::supprimer_taches(&conn, &b.ids)?;
    journaliser(&conn, &acteur, "suppression", "tache", None, Some(&format!("{n} tâche(s)")));
    Ok(Json(serde_json::json!({ "touches": n })))
}

async fn liste_actions(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(projet::lister_actions(&conn, &id)?))
}

async fn cree_action(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(n): Json<projet::NouvelleAction>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let a = projet::creer_action(&conn, &id, &n, acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "avancement", "tache", Some(&id),
                a.avancement.map(|v| format!("{v}%")).as_deref());
    Ok((StatusCode::CREATED, Json(a)))
}

async fn liste_assignations(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(projet::lister_assignations(&conn, &id)?))
}

async fn cree_assignation(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(n): Json<projet::NouvelleAssignation>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let a = projet::creer_assignation(&conn, &n)?;
    journaliser(&conn, &acteur, "assignation", "tache", Some(&a.tache_id), a.intervenant_nom.as_deref());
    Ok((StatusCode::CREATED, Json(a)))
}

#[derive(Deserialize)]
struct MajAssignation {
    #[serde(default)]
    heures_allouees: f64,
}

async fn modifie_assignation(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(b): Json<MajAssignation>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let a = projet::modifier_assignation(&conn, &id, b.heures_allouees)?;
    journaliser(&conn, &acteur, "assignation", "assignation", Some(&id), None);
    Ok(Json(a))
}

async fn supprime_assignation(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    projet::supprimer_assignation(&conn, &id)?;
    journaliser(&conn, &acteur, "desassignation", "assignation", Some(&id), None);
    Ok(StatusCode::NO_CONTENT)
}

async fn liste_intervenants(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(projet::lister_intervenants(&conn)?))
}

async fn cree_intervenant(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(n): Json<projet::NouvelIntervenant>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let i = projet::creer_intervenant(&conn, &n)?;
    journaliser(&conn, &acteur, "creation", "intervenant", Some(&i.id), Some(&i.nom));
    Ok((StatusCode::CREATED, Json(i)))
}

async fn modifie_intervenant(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(n): Json<projet::NouvelIntervenant>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let i = projet::modifier_intervenant(&conn, &id, &n)?;
    journaliser(&conn, &acteur, "modification", "intervenant", Some(&i.id), Some(&i.nom));
    Ok(Json(i))
}

async fn supprime_intervenant(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    projet::supprimer_intervenant(&conn, &id)?;
    journaliser(&conn, &acteur, "suppression", "intervenant", Some(&id), None);
    Ok(StatusCode::NO_CONTENT)
}

// ---- Notifications ---------------------------------------------------------
//
// Recalculées à chaque appel : aucune alerte fantôme. Seul l'état « lu » est
// stocké, sur une clé qui change si la situation s'aggrave.

async fn liste_notifications(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(notification::lister(&conn)?))
}

async fn notifications_lues(
    State(s): State<AppState>,
    Json(b): Json<LotCles>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(serde_json::json!({ "traites": notification::marquer_lues(&conn, &b.cles)? })))
}

async fn notifications_reafficher(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(serde_json::json!({ "efface": notification::tout_reafficher(&conn)? })))
}

#[derive(Deserialize)]
struct LotCles {
    cles: Vec<String>,
}

// ---- Liens de précédence (migration 0029) ---------------------------------
//
// ⚠️ L'harmonisation des dates n'est JAMAIS déclenchée automatiquement :
// `GET /coherence` signale et donne l'aperçu, `POST /harmoniser` applique — et
// seulement sur action explicite de l'utilisateur.

async fn liste_dependances(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(dependance::lister(&conn, &id)?))
}

async fn cree_dependance(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(n): Json<dependance::NouvelleDependance>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let d = dependance::creer(&conn, &n)?;
    journaliser(&conn, &acteur, "creation", "dependance", Some(&d.id),
                Some(&format!("{} → {}", d.predecesseur_nom, d.tache_nom)));
    Ok((StatusCode::CREATED, Json(d)))
}

async fn supprime_dependance(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    dependance::supprimer(&conn, &id)?;
    journaliser(&conn, &acteur, "suppression", "dependance", Some(&id), None);
    Ok(StatusCode::NO_CONTENT)
}

/// Signalement + aperçu. Ne modifie rien.
async fn coherence_projet(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(serde_json::json!({
        "violations": dependance::violations(&conn, &id)?,
        "changements": dependance::plan_harmonisation(&conn, &id)?,
    })))
}

/// Applique l'harmonisation — action explicite de l'utilisateur uniquement.
async fn harmonise_projet(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let changements = dependance::harmoniser(&conn, &id)?;
    journaliser(&conn, &acteur, "modification", "projet", Some(&id),
                Some(&format!("harmonisation des dates : {} activité(s)", changements.len())));
    Ok(Json(serde_json::json!({ "changements": changements })))
}

// ---- Jalons / livrables / documents joints (migration 0028) ----------------
//
// Rappel barrière spec : le jalon reste LOCAL au projet, aucun lien agenda.

#[derive(Deserialize)]
struct LotStatutJalon {
    ids: Vec<String>,
    statut: djigui_core::domain::StatutJalon,
}

#[derive(Deserialize)]
struct LotStatutLivrable {
    ids: Vec<String>,
    statut: djigui_core::domain::StatutLivrable,
}

async fn liste_jalons(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(jalon::lister_jalons(&conn, &id)?))
}

async fn cree_jalon(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(n): Json<jalon::NouveauJalon>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let j = jalon::creer_jalon(&conn, &n)?;
    journaliser(&conn, &acteur, "creation", "jalon", Some(&j.id), Some(&j.nom));
    Ok((StatusCode::CREATED, Json(j)))
}

async fn modifie_jalon(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(n): Json<jalon::NouveauJalon>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let j = jalon::modifier_jalon(&conn, &id, &n)?;
    journaliser(&conn, &acteur, "modification", "jalon", Some(&j.id), Some(&j.nom));
    Ok(Json(j))
}

async fn jalons_statut_lot(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(b): Json<LotStatutJalon>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let n = jalon::changer_statut_jalons(&conn, &b.ids, b.statut)?;
    journaliser(&conn, &acteur, "modification", "jalon", None, Some(b.statut.as_str()));
    Ok(Json(serde_json::json!({ "traites": n })))
}

async fn jalons_supprimer_lot(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(b): Json<LotIds>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let n = jalon::supprimer_jalons(&conn, &b.ids)?;
    journaliser(&conn, &acteur, "suppression", "jalon", None, None);
    Ok(Json(serde_json::json!({ "traites": n })))
}

// ---- Production : nomenclatures et ordres de fabrication (mig 0031) --------

#[derive(Deserialize)]
struct FiltreNomenclatures {
    article_id: Option<String>,
    #[serde(default)]
    actives_seulement: bool,
}

async fn liste_nomenclatures(
    State(s): State<AppState>,
    Query(q): Query<FiltreNomenclatures>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(production::lister_nomenclatures(
        &conn,
        q.article_id.as_deref(),
        q.actives_seulement,
    )?))
}

async fn get_nomenclature(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(production::lire_nomenclature(&conn, &id)?))
}

async fn cree_nomenclature(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(n): Json<production::NouvelleNomenclature>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let r = production::creer_nomenclature(&conn, &n, acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "creation", "nomenclature", Some(&r.id), Some(&r.nom));
    Ok((StatusCode::CREATED, Json(r)))
}

async fn modifie_nomenclature(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(n): Json<production::NouvelleNomenclature>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let r = production::modifier_nomenclature(&conn, &id, &n)?;
    journaliser(&conn, &acteur, "modification", "nomenclature", Some(&r.id), Some(&r.nom));
    Ok(Json(r))
}

async fn supprime_nomenclature(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    production::supprimer_nomenclature(&conn, &id)?;
    journaliser(&conn, &acteur, "suppression", "nomenclature", Some(&id), None);
    Ok(StatusCode::NO_CONTENT)
}

async fn liste_ordres(
    State(s): State<AppState>,
    Query(f): Query<production::FiltreOrdres>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(production::lister_ordres(&conn, &f)?))
}

async fn get_ordre(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(production::lire_ordre(&conn, &id)?))
}

async fn cree_ordre(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(n): Json<production::NouvelOrdre>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let o = production::creer_ordre(&conn, &n, acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "creation", "ordre_production", Some(&o.id), Some(&o.numero));
    Ok((StatusCode::CREATED, Json(o)))
}

async fn modifie_ordre(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(n): Json<production::NouvelOrdre>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let o = production::modifier_ordre(&conn, &id, &n)?;
    journaliser(&conn, &acteur, "modification", "ordre_production", Some(&o.id), Some(&o.numero));
    Ok(Json(o))
}

async fn supprime_ordre(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    production::supprimer_ordre(&conn, &id)?;
    journaliser(&conn, &acteur, "suppression", "ordre_production", Some(&id), None);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct CorpsStatutOrdre {
    statut: djigui_core::domain::StatutOrdreProduction,
}

async fn ordre_statut(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(b): Json<CorpsStatutOrdre>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let o = production::changer_statut(&conn, &id, b.statut)?;
    journaliser(&conn, &acteur, "modification", "ordre_production", Some(&o.id), Some(b.statut.as_str()));
    Ok(Json(o))
}

/// Clôture : c'est elle qui écrit les mouvements de stock, donc elle est
/// journalisée avec le numéro de l'ordre.
async fn ordre_cloturer(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(c): Json<production::Cloture>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let o = production::cloturer(&conn, &id, &c, acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "cloture", "ordre_production", Some(&o.id), Some(&o.numero));
    Ok(Json(o))
}

#[derive(Deserialize)]
struct CorpsMotif {
    motif: String,
}

async fn ordre_annuler(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(b): Json<CorpsMotif>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let o = production::annuler(&conn, &id, &b.motif)?;
    journaliser(&conn, &acteur, "annulation", "ordre_production", Some(&o.id), Some(&b.motif));
    Ok(Json(o))
}

#[derive(Deserialize)]
struct LotStatutOrdre {
    ids: Vec<String>,
    statut: djigui_core::domain::StatutOrdreProduction,
}

async fn ordres_statut_lot(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(b): Json<LotStatutOrdre>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let n = production::changer_statut_lot(&conn, &b.ids, b.statut)?;
    journaliser(&conn, &acteur, "modification", "ordre_production", None, Some(b.statut.as_str()));
    Ok(Json(serde_json::json!({ "traites": n })))
}

async fn ordres_supprimer_lot(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(b): Json<LotIds>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let n = production::supprimer_lot(&conn, &b.ids)?;
    journaliser(&conn, &acteur, "suppression", "ordre_production", None, None);
    Ok(Json(serde_json::json!({ "traites": n })))
}

async fn liste_livrables(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(jalon::lister_livrables(&conn, &id)?))
}

async fn cree_livrable(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(n): Json<jalon::NouveauLivrable>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let l = jalon::creer_livrable(&conn, &n)?;
    journaliser(&conn, &acteur, "creation", "livrable", Some(&l.id), Some(&l.nom));
    Ok((StatusCode::CREATED, Json(l)))
}

async fn modifie_livrable(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(n): Json<jalon::NouveauLivrable>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let l = jalon::modifier_livrable(&conn, &id, &n)?;
    journaliser(&conn, &acteur, "modification", "livrable", Some(&l.id), Some(&l.nom));
    Ok(Json(l))
}

async fn livrables_statut_lot(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(b): Json<LotStatutLivrable>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let n = jalon::changer_statut_livrables(&conn, &b.ids, b.statut)?;
    journaliser(&conn, &acteur, "modification", "livrable", None, Some(b.statut.as_str()));
    Ok(Json(serde_json::json!({ "traites": n })))
}

async fn livrables_supprimer_lot(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(b): Json<LotIds>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let n = jalon::supprimer_livrables(&conn, &b.ids)?;
    journaliser(&conn, &acteur, "suppression", "livrable", None, None);
    Ok(Json(serde_json::json!({ "traites": n })))
}

// -- Documents joints --------------------------------------------------------
// Le navigateur envoie le contenu en base64 ; le SERVEUR écrit le fichier sur
// disque et ne met que le chemin en base.

/// Taille maximale d'une pièce jointe. Garde-fou mémoire : le corps JSON est
/// entièrement chargé, et le base64 pèse ~4/3 du fichier.
const MAX_PIECE_JOINTE: usize = 20 * 1024 * 1024;

#[derive(Deserialize)]
struct EnvoiDocument {
    projet_id: String,
    #[serde(default)]
    tache_id: Option<String>,
    #[serde(default)]
    jalon_id: Option<String>,
    #[serde(default)]
    livrable_id: Option<String>,
    nom: String,
    #[serde(default)]
    type_mime: Option<String>,
    /// Contenu du fichier encodé en base64 (sans préfixe data:).
    contenu_base64: String,
}

async fn liste_documents_joints(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(jalon::lister_documents(&conn, &id)?))
}

async fn cree_document_joint(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(e): Json<EnvoiDocument>,
) -> Result<impl IntoResponse, ApiError> {
    use base64::Engine;
    // Un data-URI éventuel est toléré : on ne garde que ce qui suit la virgule.
    let brut = e.contenu_base64.rsplit(',').next().unwrap_or("");
    let octets = base64::engine::general_purpose::STANDARD
        .decode(brut.trim())
        .map_err(|_| ApiError(CoreError::Rule("fichier illisible (encodage invalide)".into())))?;
    if octets.len() > MAX_PIECE_JOINTE {
        return Err(ApiError(CoreError::Rule(format!(
            "fichier trop lourd ({} Mo) — maximum {} Mo",
            octets.len() / 1_048_576,
            MAX_PIECE_JOINTE / 1_048_576
        ))));
    }

    // Nom de fichier assaini : on ne fait JAMAIS confiance au nom fourni par le
    // client (une barre oblique ou « .. » permettrait d'écrire hors du dossier).
    let nom_affiche = e.nom.trim();
    let extension = std::path::Path::new(nom_affiche)
        .extension()
        .and_then(|x| x.to_str())
        .filter(|x| x.len() <= 10 && x.chars().all(|c| c.is_ascii_alphanumeric()))
        .unwrap_or("bin");
    let relatif = format!("{}.{extension}", uuid::Uuid::new_v4());

    let dossier = s.dossier_documents.join(&e.projet_id);
    std::fs::create_dir_all(&dossier)
        .map_err(|err| ApiError(CoreError::Rule(format!("dossier de stockage inaccessible : {err}"))))?;
    let taille = octets.len() as i64;
    std::fs::write(dossier.join(&relatif), &octets)
        .map_err(|err| ApiError(CoreError::Rule(format!("écriture du fichier impossible : {err}"))))?;

    let chemin = format!("{}/{relatif}", e.projet_id);
    let conn = s.conn.lock().unwrap();
    let d = jalon::creer_document(
        &conn,
        &jalon::NouveauDocument {
            projet_id: e.projet_id.clone(),
            tache_id: e.tache_id,
            jalon_id: e.jalon_id,
            livrable_id: e.livrable_id,
            nom: nom_affiche.to_string(),
            chemin,
            taille,
            type_mime: e.type_mime,
        },
        acteur.0.as_deref(),
    )?;
    journaliser(&conn, &acteur, "creation", "document_joint", Some(&d.id), Some(&d.nom));
    Ok((StatusCode::CREATED, Json(d)))
}

async fn supprime_document_joint(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let chemin = {
        let conn = s.conn.lock().unwrap();
        let chemin = jalon::supprimer_document(&conn, &id)?;
        journaliser(&conn, &acteur, "suppression", "document_joint", Some(&id), None);
        chemin
    };
    // Le fichier est effacé après coup : un fichier orphelin est bénin, une
    // fiche pointant vers un fichier disparu le serait moins.
    let _ = std::fs::remove_file(s.dossier_documents.join(&chemin));
    Ok(StatusCode::NO_CONTENT)
}

/// Ouvre la pièce jointe avec l'application par défaut du poste. Le WebView2
/// (Tauri) ne sait pas télécharger : c'est le serveur qui ouvre le fichier,
/// exactement comme pour l'export .xlsx.
async fn ouvre_document_joint(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let doc = {
        let conn = s.conn.lock().unwrap();
        jalon::lire_document(&conn, &id)?
    };
    let complet = s.dossier_documents.join(&doc.chemin);
    if !complet.is_file() {
        return Err(ApiError(CoreError::Rule(format!(
            "le fichier « {} » est introuvable sur ce poste",
            doc.nom
        ))));
    }
    ouvrir_fichier(&complet.to_string_lossy());
    Ok(Json(serde_json::json!({ "ouvert": doc.nom })))
}

async fn liste_ressources(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(projet::lister_ressources(&conn, &id)?))
}

async fn cree_ressource(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(n): Json<projet::NouvelleRessource>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let r = projet::creer_ressource(&conn, &n)?;
    journaliser(&conn, &acteur, "creation", "ressource", Some(&r.id), Some(&r.libelle));
    Ok((StatusCode::CREATED, Json(r)))
}

async fn modifie_ressource(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(n): Json<projet::NouvelleRessource>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let r = projet::modifier_ressource(&conn, &id, &n)?;
    journaliser(&conn, &acteur, "modification", "ressource", Some(&r.id), Some(&r.libelle));
    Ok(Json(r))
}

async fn supprime_ressource(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    projet::supprimer_ressource(&conn, &id)?;
    journaliser(&conn, &acteur, "suppression", "ressource", Some(&id), None);
    Ok(StatusCode::NO_CONTENT)
}

// ---- Agenda / rendez-vous (migration 0020) --------------------------------

async fn liste_rdv(
    State(s): State<AppState>,
    Query(f): Query<rendez_vous::FiltreRendezVous>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(rendez_vous::lister(&conn, &f)?))
}

async fn cree_rdv(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(n): Json<rendez_vous::NouveauRendezVous>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let r = rendez_vous::creer(&conn, &n, acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "creation", "rendez_vous", Some(&r.id), Some(&r.titre));
    Ok((StatusCode::CREATED, Json(r)))
}

async fn modifie_rdv(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(n): Json<rendez_vous::NouveauRendezVous>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let r = rendez_vous::modifier(&conn, &id, &n)?;
    journaliser(&conn, &acteur, "modification", "rendez_vous", Some(&r.id), Some(&r.titre));
    Ok(Json(r))
}

async fn supprime_rdv(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    rendez_vous::supprimer(&conn, &id)?;
    journaliser(&conn, &acteur, "suppression", "rendez_vous", Some(&id), None);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct LotRdvStatut {
    ids: Vec<String>,
    statut: djigui_core::domain::StatutRendezVous,
}

async fn rdv_statut_lot(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(b): Json<LotRdvStatut>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let n = rendez_vous::changer_statut_lot(&conn, &b.ids, b.statut)?;
    journaliser(&conn, &acteur, "statut", "rendez_vous", None, Some(&format!("{n} RDV")));
    Ok(Json(serde_json::json!({ "touches": n })))
}

async fn rdv_supprimer_lot(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(b): Json<LotIds>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let n = rendez_vous::supprimer_lot(&conn, &b.ids)?;
    journaliser(&conn, &acteur, "suppression", "rendez_vous", None, Some(&format!("{n} RDV")));
    Ok(Json(serde_json::json!({ "touches": n })))
}

/// Export iCalendar (.ics) de l'agenda → fichier dans « Téléchargements », ouvert
/// dans l'appli calendrier par défaut (importable dans Google Agenda / Outlook /
/// Apple). Contourne le blocage des téléchargements du WebView2.
async fn export_ics(
    State(s): State<AppState>,
    Query(p): Query<Periode>,
) -> Result<impl IntoResponse, ApiError> {
    let (texte, nb) = {
        let conn = s.conn.lock().unwrap();
        crate::export::ics_rendez_vous(&conn, borne(&p.du), borne(&p.au))?
    };
    let dossier = dossier_telechargements();
    let base = dossier.join("djigui-agenda.ics");
    // Écriture ; si le fichier est verrouillé (ouvert), on ajoute un suffixe horaire.
    let chemin = match std::fs::write(&base, texte.as_bytes()) {
        Ok(()) => base,
        Err(_) => {
            let hhmmss = djigui_core::now().get(11..19).unwrap_or("").replace(':', "");
            let alt = dossier.join(format!("djigui-agenda-{hhmmss}.ics"));
            std::fs::write(&alt, texte.as_bytes())
                .map_err(|e| CoreError::Rule(format!("écriture du fichier impossible : {e}")))?;
            alt
        }
    };
    let chemin_str = chemin.to_string_lossy().to_string();
    ouvrir_fichier(&chemin_str);
    Ok(Json(serde_json::json!({ "chemin": chemin_str, "nb": nb })))
}

/// Filtre de période commun (dates « AAAA-MM-JJ », incluses ; absentes = sans borne).
#[derive(Deserialize)]
struct Periode {
    #[serde(default)]
    du: Option<String>,
    #[serde(default)]
    au: Option<String>,
}
fn borne(s: &Option<String>) -> Option<&str> {
    s.as_deref().map(str::trim).filter(|x| !x.is_empty())
}

/// Rapport bénéfices par mois × caisse (JSON pour l'onglet Bénéfices), borné à la période.
async fn rapport_benefices(
    State(s): State<AppState>,
    Query(p): Query<Periode>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(rapport::benefices_par_mois_caisse(&conn, borne(&p.du), borne(&p.au))?))
}

/// Enregistre le classeur .xlsx sur le disque (dossier Téléchargements) et
/// l'ouvre dans l'application par défaut. Contourne le blocage des
/// téléchargements par le WebView2 en desktop : le serveur local a accès au
/// disque, l'UI reçoit juste le chemin du fichier créé. Écriture en **mémoire
/// constante**, directement sur le fichier (voir `export.rs`).
async fn export_xlsx_fichier(
    State(s): State<AppState>,
    Query(p): Query<Periode>,
) -> Result<impl IntoResponse, ApiError> {
    let dossier = dossier_telechargements();
    let date = djigui_core::now()[..10].to_string();
    let suffixe = match (borne(&p.du), borne(&p.au)) {
        (Some(d), Some(a)) => format!("{d}_{a}"),
        (Some(d), None) => format!("depuis-{d}"),
        (None, Some(a)) => format!("jusqu-{a}"),
        (None, None) => date,
    };
    let souhaite = dossier.join(format!("djigui-ventes-{suffixe}.xlsx"));
    let (chemin, nb_ventes) = {
        let conn = s.conn.lock().unwrap();
        crate::export::ecrire_classeur(&conn, &souhaite, borne(&p.du), borne(&p.au))?
    };
    let chemin_str = chemin.to_string_lossy().to_string();
    ouvrir_fichier(&chemin_str);
    Ok(Json(serde_json::json!({ "chemin": chemin_str, "nb_ventes": nb_ventes })))
}

/// Exporte le projet en classeur Excel (planning + Gantt en cellules, budget,
/// ressources, jalons). Même contrainte que l'export des ventes : le WebView2
/// ne sait pas télécharger, donc le serveur écrit le fichier dans
/// « Téléchargements » puis l'ouvre.
async fn export_projet_xlsx(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let dossier = dossier_telechargements();
    let (nom, chemin) = {
        let conn = s.conn.lock().unwrap();
        let nom: String = conn
            .query_row("SELECT nom FROM projet WHERE id = ?1", rusqlite::params![id], |r| r.get(0))
            .map_err(|_| ApiError(CoreError::NotFound(format!("projet {id}"))))?;
        let souhaite = dossier.join(format!("djigui-projet-{}.xlsx", nom_fichier(&nom)));
        // Si le fichier est déjà ouvert dans Excel, Windows le verrouille :
        // on réessaie avec un suffixe horaire plutôt que d'échouer.
        let chemin = match crate::export_projet::ecrire_projet(&conn, &souhaite, &id) {
            Ok(c) => c,
            Err(_) => {
                let hhmmss = djigui_core::now()[11..19].replace(':', "");
                let alt = dossier.join(format!("djigui-projet-{}-{hhmmss}.xlsx", nom_fichier(&nom)));
                crate::export_projet::ecrire_projet(&conn, &alt, &id)?
            }
        };
        (nom, chemin)
    };
    let chemin_str = chemin.to_string_lossy().to_string();
    ouvrir_fichier(&chemin_str);
    Ok(Json(serde_json::json!({ "chemin": chemin_str, "projet": nom })))
}

/// Nom de fichier sûr : on ne laisse passer que lettres, chiffres et tirets.
fn nom_fichier(nom: &str) -> String {
    let net: String = nom
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '-' })
        .collect();
    let net = net.trim_matches('-').to_string();
    if net.is_empty() { "projet".into() } else { net.chars().take(60).collect() }
}

/// Dossier « Téléchargements » de l'utilisateur, avec replis raisonnables.
fn dossier_telechargements() -> std::path::PathBuf {
    if let Ok(profil) = std::env::var("USERPROFILE") {
        let dl = std::path::Path::new(&profil).join("Downloads");
        if dl.is_dir() {
            return dl;
        }
        return std::path::Path::new(&profil).to_path_buf();
    }
    std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
}

/// Ouvre un fichier avec l'application par défaut (best-effort, non bloquant).
fn ouvrir_fichier(chemin: &str) {
    #[cfg(windows)]
    {
        // `cmd /C start "" "<chemin>"` ouvre le .xlsx dans Excel (ou l'app associée).
        let _ = std::process::Command::new("cmd")
            .args(["/C", "start", "", chemin])
            .spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("xdg-open").arg(chemin).spawn();
    }
}

async fn liste_audit(
    State(s): State<AppState>,
    Query(q): Query<AuditQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(audit::lister(&conn, q.limite)?))
}

#[derive(Deserialize)]
struct AuditQuery {
    #[serde(default)]
    limite: Option<i64>,
}

// ---- Seeder de catalogues métier -------------------------------------------

async fn liste_catalogues(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(seeder::types_disponibles(&conn)?))
}

async fn detail_catalogue(
    State(s): State<AppState>,
    Path(code): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(seeder::detail(&conn, &code)?))
}

async fn applique_catalogues(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(selections): Json<Vec<seeder::SelectionCatalogue>>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let r = seeder::appliquer(&conn, &selections)?;
    let types: Vec<&str> = selections.iter().map(|s| s.code.as_str()).collect();
    let detail = format!("{} : {} articles", types.join(", "), r.articles_crees);
    journaliser(&conn, &acteur, "seed_catalogue", "article", None, Some(&detail));
    Ok(Json(r))
}

// ---- Utilisateurs & authentification ---------------------------------------

async fn login(
    State(s): State<AppState>,
    Json(ids): Json<utilisateur::Identifiants>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(utilisateur::authentifier(&conn, &ids)?))
}

async fn liste_utilisateurs(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(utilisateur::lister(&conn)?))
}

async fn cree_utilisateur(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(nu): Json<utilisateur::NouvelUtilisateur>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let u = utilisateur::creer(&conn, &nu)?;
    journaliser(&conn, &acteur, "creation", "utilisateur", Some(&u.id), Some(&u.login));
    Ok((StatusCode::CREATED, Json(u)))
}

async fn modifie_utilisateur(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(m): Json<utilisateur::MajUtilisateur>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let u = utilisateur::modifier(&conn, &id, &m)?;
    journaliser(&conn, &acteur, "modification", "utilisateur", Some(&u.id), Some(&u.login));
    Ok(Json(u))
}

async fn supprime_utilisateur(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    utilisateur::desactiver(&conn, &id)?;
    journaliser(&conn, &acteur, "desactivation", "utilisateur", Some(&id), None);
    Ok(StatusCode::NO_CONTENT)
}

// ---- Facturation cyclique / abonnements (§5.8) -----------------------------

async fn liste_abonnements(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(abonnement::lister(&conn)?))
}

async fn cree_abonnement(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(na): Json<abonnement::NouvelAbonnement>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let a = abonnement::creer(&conn, &na)?;
    journaliser(&conn, &acteur, "creation", "abonnement", Some(&a.id), a.libelle.as_deref());
    Ok((StatusCode::CREATED, Json(a)))
}

#[derive(Deserialize)]
struct MajAbonnement {
    #[serde(flatten)]
    champs: abonnement::NouvelAbonnement,
    #[serde(default = "vrai")]
    actif: bool,
}
fn vrai() -> bool { true }

async fn modifie_abonnement(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(b): Json<MajAbonnement>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let a = abonnement::modifier(&conn, &id, &b.champs, b.actif)?;
    journaliser(&conn, &acteur, "modification", "abonnement", Some(&a.id), a.libelle.as_deref());
    Ok(Json(a))
}

async fn supprime_abonnement(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    abonnement::supprimer(&conn, &id)?;
    journaliser(&conn, &acteur, "suppression", "abonnement", Some(&id), None);
    Ok(StatusCode::NO_CONTENT)
}

/// Génère les factures des abonnements échus (à la date du jour). Retourne les
/// pièces créées (brouillon) : numéro + total, pour le retour utilisateur.
async fn generer_abonnements(
    State(s): State<AppState>,
    acteur: Acteur,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let aujourdhui = djigui_core::now()[..10].to_string();
    let crees = abonnement::generer_echeances_dues(&conn, &aujourdhui)?;
    // Auteur + audit sur chaque facture générée.
    for d in &crees {
        marquer_auteur(&conn, "document", &d.id, &acteur);
        journaliser(&conn, &acteur, "generation_abonnement", "document", Some(&d.id), Some(&d.numero));
    }
    let apercu: Vec<_> = crees
        .iter()
        .map(|d| serde_json::json!({ "id": d.id, "numero": d.numero, "total_ttc": d.total_ttc }))
        .collect();
    Ok(Json(serde_json::json!({ "generees": apercu.len(), "factures": apercu })))
}

#[derive(Deserialize)]
struct CorpsImpression {
    /// Ticket déjà mis en forme (une ligne source = une ligne imprimée).
    texte: String,
    /// Imprimante cible ; à défaut on prend celle stockée dans la config.
    #[serde(default)]
    imprimante: Option<String>,
    /// Mode : "standard" (pilote) ou "thermique" (ESC/POS) ; défaut = config.
    #[serde(default)]
    mode: Option<String>,
}

/// Imprime un ticket en arrière-plan sur l'imprimante configurée, selon le mode
/// (standard = via pilote / thermique = ESC/POS brut).
async fn imprimer_ticket(
    State(s): State<AppState>,
    Json(b): Json<CorpsImpression>,
) -> Result<impl IntoResponse, ApiError> {
    let (nom, mode) = {
        let conn = s.conn.lock().unwrap();
        let cfg = parametres::lister_globaux(&conn)?;
        let nom = b.imprimante
            .filter(|n| !n.trim().is_empty())
            .or_else(|| cfg.get(CLE_IMPRIMANTE).cloned())
            .filter(|n| !n.trim().is_empty())
            .ok_or_else(|| CoreError::Rule(
                "aucune imprimante configurée (Paramètres → Impression)".into(),
            ))?;
        let mode = b.mode
            .filter(|m| !m.trim().is_empty())
            .or_else(|| cfg.get(CLE_MODE).cloned())
            .unwrap_or_else(|| "standard".into());
        (nom, mode)
    };
    crate::impression::imprimer_ticket(&nom, &b.texte, &mode).map_err(CoreError::Rule)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn cree_article(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(na): Json<article::NouvelArticle>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let a = article::creer(&conn, &na)?;
    journaliser(&conn, &acteur, "creation", "article", Some(&a.id), Some(&a.designation));
    Ok((StatusCode::CREATED, Json(a)))
}

async fn get_article(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(article::lire(&conn, &id)?))
}

async fn modifie_article(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(na): Json<article::NouvelArticle>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let a = article::modifier(&conn, &id, &na)?;
    journaliser(&conn, &acteur, "modification", "article", Some(&a.id), Some(&a.designation));
    Ok(Json(a))
}

async fn supprime_article(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    article::desactiver(&conn, &id)?;
    journaliser(&conn, &acteur, "desactivation", "article", Some(&id), None);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct LotArticlesCategorie {
    ids: Vec<String>,
    #[serde(default)]
    categorie_id: Option<String>,
}

async fn articles_categorie_lot(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(b): Json<LotArticlesCategorie>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let cat = b.categorie_id.as_deref().filter(|s| !s.is_empty());
    let n = article::affecter_categorie_lot(&conn, &b.ids, cat)?;
    journaliser(&conn, &acteur, "categorie_lot", "article", None, Some(&format!("{n} articles")));
    Ok(Json(serde_json::json!({ "traites": n })))
}

async fn articles_desactiver_lot(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(b): Json<LotIds>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let n = article::desactiver_lot(&conn, &b.ids)?;
    journaliser(&conn, &acteur, "desactivation_lot", "article", None, Some(&format!("{n} articles")));
    Ok(Json(serde_json::json!({ "traites": n })))
}

async fn articles_supprimer_lot(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(b): Json<LotIds>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let r = article::supprimer_lot(&conn, &b.ids)?;
    journaliser(&conn, &acteur, "suppression_lot", "article", None,
                Some(&format!("{} supprimés, {} archivés", r.supprimes, r.archives)));
    Ok(Json(r))
}

// ---- Catégories -------------------------------------------------------------

async fn liste_categories(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(categorie::lister(&conn)?))
}

async fn cree_categorie(
    State(s): State<AppState>,
    Json(nc): Json<categorie::NouvelleCategorie>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok((StatusCode::CREATED, Json(categorie::creer(&conn, &nc)?)))
}

async fn modifie_categorie(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(nc): Json<categorie::NouvelleCategorie>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(categorie::modifier(&conn, &id, &nc)?))
}

async fn supprime_categorie(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    categorie::supprimer(&conn, &id)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- Dépôts -----------------------------------------------------------------

async fn liste_depots(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(depot::lister(&conn)?))
}

async fn cree_depot(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(nd): Json<depot::NouveauDepot>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let d = depot::creer(&conn, &nd)?;
    journaliser(&conn, &acteur, "creation", "magasin", Some(&d.id), Some(&d.nom));
    Ok((StatusCode::CREATED, Json(d)))
}

#[derive(Deserialize)]
struct CorpsDepot {
    nom: String,
}

async fn renomme_depot(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(b): Json<CorpsDepot>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let d = depot::renommer(&conn, &id, &b.nom)?;
    journaliser(&conn, &acteur, "modification", "magasin", Some(&d.id), Some(&d.nom));
    Ok(Json(d))
}

async fn depot_defaut(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    depot::definir_defaut(&conn, &id)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- Stock : transfert & inventaire par magasin ----------------------------

async fn etat_stock_depot(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(stock::etat_depot(&conn, &id)?))
}

#[derive(Deserialize)]
struct CorpsTransfert {
    article_id: String,
    source_depot: String,
    dest_depot: String,
    quantite: f64,
}

async fn stock_transfert(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(b): Json<CorpsTransfert>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    stock::transferer(&conn, &b.article_id, &b.source_depot, &b.dest_depot, b.quantite)?;
    journaliser(&conn, &acteur, "transfert", "stock", Some(&b.article_id),
                Some(&format!("{} unités", b.quantite)));
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct CorpsInventaire {
    article_id: String,
    depot_id: String,
    stock_physique: f64,
}

async fn stock_inventaire(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(b): Json<CorpsInventaire>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let mvt = stock::ajuster_inventaire(&conn, &b.article_id, &b.depot_id, b.stock_physique)?;
    if mvt.is_some() {
        journaliser(&conn, &acteur, "inventaire", "stock", Some(&b.article_id),
                    Some(&format!("compté {}", b.stock_physique)));
    }
    Ok(Json(serde_json::json!({ "ajuste": mvt.is_some() })))
}

// ---- Inventaires (comptage daté et verrouillé) -----------------------------

#[derive(Deserialize)]
struct FiltreInventaires {
    #[serde(default)]
    depot_id: Option<String>,
}

async fn liste_inventaires(
    State(s): State<AppState>,
    Query(q): Query<FiltreInventaires>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(inventaire::lister(&conn, q.depot_id.as_deref())?))
}

async fn get_inventaire(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(inventaire::lire(&conn, &id)?))
}

async fn cree_inventaire(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(ni): Json<inventaire::NouvelInventaire>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let inv = inventaire::enregistrer(&conn, &ni, acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "inventaire", "inventaire", Some(&inv.id),
                Some(&format!("{} lignes, écart {}", inv.nb_lignes, inv.total_ecart)));
    Ok((StatusCode::CREATED, Json(inv)))
}

// ---- Documents --------------------------------------------------------------

async fn liste_documents(
    State(s): State<AppState>,
    Query(f): Query<document::FiltreDocuments>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(document::lister(&conn, &f)?))
}

async fn cree_document(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(nd): Json<document::NouveauDocument>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let doc = document::creer(&conn, &nd)?;
    marquer_auteur(&conn, "document", &doc.id, &acteur);
    journaliser(&conn, &acteur, "creation", "document", Some(&doc.id), Some(&doc.numero));
    Ok((StatusCode::CREATED, Json(doc)))
}

/// Pose l'auteur (`cree_par`) sur une pièce, si un utilisateur est connu.
fn marquer_auteur(conn: &rusqlite::Connection, table: &str, id: &str, acteur: &Acteur) {
    if let Some(uid) = &acteur.0 {
        let sql = format!("UPDATE {table} SET cree_par = ?2 WHERE id = ?1");
        let _ = conn.execute(&sql, rusqlite::params![id, uid]);
    }
}

/// Écrit une entrée d'audit (best-effort : n'échoue jamais la requête).
fn journaliser(conn: &rusqlite::Connection, acteur: &Acteur, action: &str, entite: &str,
               id: Option<&str>, detail: Option<&str>) {
    let _ = audit::enregistrer(conn, acteur.0.as_deref(), action, entite, id, detail);
}

async fn get_document(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(document::lire(&conn, &id)?))
}

async fn supprime_document(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    // On lit le numéro avant suppression pour un journal lisible.
    let numero = document::lire(&conn, &id).ok().map(|d| d.numero);
    document::supprimer(&conn, &id)?;
    journaliser(&conn, &acteur, "suppression", "document", Some(&id), numero.as_deref());
    Ok(StatusCode::NO_CONTENT)
}

async fn valider_document(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let doc = document::valider(&conn, &id)?;
    journaliser(&conn, &acteur, "validation", "document", Some(&doc.id), Some(&doc.numero));
    Ok(Json(doc))
}

#[derive(Deserialize)]
struct CorpsTransformer {
    type_cible: djigui_core::domain::TypeDocument,
}

async fn transformer_document(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(b): Json<CorpsTransformer>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let cible = document::transformer(&conn, &id, b.type_cible)?;
    marquer_auteur(&conn, "document", &cible.id, &acteur);
    journaliser(&conn, &acteur, "transformation", "document", Some(&cible.id), Some(&cible.numero));
    Ok((StatusCode::CREATED, Json(cible)))
}

/// Garde-fou de RÔLE MACHINE : refuse tout ce qui touche à la sauvegarde quand
/// cet ordinateur n'est pas le serveur (mig 0042).
///
/// ⚠️ Rappel de l'utilisateur : « seule la machine serveur peut faire ça ».
/// Le cœur refuse déjà d'EXÉCUTER une sauvegarde depuis un poste secondaire ;
/// ce garde-fou étend le refus à tout ce qui la PRÉPARE — choisir un dossier,
/// ajouter ou modifier une destination. Sans lui, un poste client pourrait
/// configurer des destinations qu'il n'écrirait jamais : l'utilisateur croirait
/// sa sauvegarde en place alors que rien ne partirait.
fn exiger_serveur(conn: &rusqlite::Connection, quoi: &str) -> Result<(), ApiError> {
    let p = djigui_core::modules::sauvegarde::lire_parametres(conn)?;
    if !p.cette_machine_est_serveur {
        return Err(ApiError(CoreError::Forbidden(format!(
            "Cet ordinateur n'est pas le serveur Djigui : {quoi} se fait depuis le poste qui              héberge les données."
        ))));
    }
    Ok(())
}

/// Garde-fou : exige que l'acteur soit un **Admin** actif. Sinon `Forbidden`.
fn exiger_admin(conn: &rusqlite::Connection, acteur: &Acteur) -> Result<(), ApiError> {
    exiger_admin_pour(conn, acteur, "annuler une vente encaissée")
}

/// Même garde-fou, mais le refus **nomme l'action refusée**. Un message générique
/// (« accès refusé ») laisse l'utilisateur deviner ce qu'il vient de tenter.
fn exiger_admin_pour(
    conn: &rusqlite::Connection,
    acteur: &Acteur,
    motif: &str,
) -> Result<(), ApiError> {
    use djigui_core::domain::RoleUtilisateur;
    let id = acteur.0.as_deref().ok_or_else(|| {
        ApiError(CoreError::Unauthorized("connexion requise pour cette action".into()))
    })?;
    let u = utilisateur::lire(conn, id)
        .map_err(|_| ApiError(CoreError::Unauthorized("utilisateur inconnu".into())))?;
    if u.role != RoleUtilisateur::Admin || !u.actif {
        return Err(ApiError(CoreError::Forbidden(format!(
            "seul un administrateur peut {motif}"
        ))));
    }
    Ok(())
}

/// L'écran comptable est **réservé**. Aujourd'hui le rôle « comptable » n'existe
/// pas dans Djigui (rôles Admin/Caissier) : on exige donc l'administrateur, qui
/// est la personne à qui le commerçant confie ce genre d'accès. Le jour où un
/// rôle dédié sera créé, c'est ici qu'il s'ajoutera — nulle part ailleurs.
fn exiger_comptable(conn: &rusqlite::Connection, acteur: &Acteur) -> Result<(), ApiError> {
    use djigui_core::domain::RoleUtilisateur;
    let id = acteur.0.as_deref().ok_or_else(|| {
        ApiError(CoreError::Unauthorized("connexion requise pour cette action".into()))
    })?;
    let u = utilisateur::lire(conn, id)
        .map_err(|_| ApiError(CoreError::Unauthorized("utilisateur inconnu".into())))?;
    if u.role != RoleUtilisateur::Admin || !u.actif {
        return Err(ApiError(CoreError::Forbidden(
            "l'écran comptable est réservé au comptable (compte administrateur)".into(),
        )));
    }
    Ok(())
}

#[derive(Deserialize)]
struct CorpsAnnulation {
    motif: String,
}

/// Annulation d'une vente encaissée (contre-passation) — **Admin uniquement**.
async fn annuler_document(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(b): Json<CorpsAnnulation>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_admin(&conn, &acteur)?;
    let doc = document::annuler(&conn, &id, &b.motif, acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "annulation", "document", Some(&doc.id),
                Some(&format!("{} — {}", doc.numero, b.motif.trim())));
    Ok(Json(doc))
}

// ---- Passation et suivi des marchés (migration 0037) -----------------------

async fn liste_marches(
    State(s): State<AppState>,
    Query(f): Query<marche::FiltreMarches>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(marche::lister(&conn, &f)?))
}

async fn get_marche(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(marche::lire(&conn, &id)?))
}

async fn cree_marche(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(n): Json<marche::NouveauMarche>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let m = marche::creer(&conn, &n, acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "creation", "marche", Some(&m.id), Some(&m.numero));
    Ok((StatusCode::CREATED, Json(m)))
}

async fn modifie_marche(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(n): Json<marche::NouveauMarche>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let m = marche::modifier(&conn, &id, &n)?;
    journaliser(&conn, &acteur, "modification", "marche", Some(&m.id), Some(&m.numero));
    Ok(Json(m))
}

async fn supprime_marche(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    marche::supprimer(&conn, &id)?;
    journaliser(&conn, &acteur, "suppression", "marche", Some(&id), None);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct CorpsStatut {
    statut: String,
}

async fn marche_statut(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(b): Json<CorpsStatut>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let m = marche::changer_statut(&conn, &id, &b.statut)?;
    journaliser(&conn, &acteur, "modification", "marche", Some(&m.id), Some(&b.statut));
    Ok(Json(m))
}

async fn marche_annuler(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(b): Json<CorpsMotif>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let m = marche::annuler(&conn, &id, &b.motif, acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "annulation", "marche", Some(&m.id), Some(&b.motif));
    Ok(Json(m))
}

#[derive(Deserialize)]
struct LotStatutMarche {
    ids: Vec<String>,
    statut: String,
}

async fn marches_statut_lot(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(b): Json<LotStatutMarche>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let n = marche::changer_statut_lot(&conn, &b.ids, &b.statut)?;
    journaliser(&conn, &acteur, "modification", "marche", None,
                Some(&format!("{n} marché(s) → {}", b.statut)));
    Ok(Json(serde_json::json!({ "modifies": n })))
}

/// Suppression groupée : rend compte des supprimés ET des conservés (marchés
/// qui ont une histoire). L'écran doit pouvoir le DIRE, pas juste compter.
async fn marches_supprimer_lot(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(b): Json<LotIds>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let r = marche::supprimer_lot(&conn, &b.ids)?;
    journaliser(&conn, &acteur, "suppression", "marche", None,
                Some(&format!("{} supprimé(s), {} conservé(s)", r.supprimes, r.conserves)));
    Ok(Json(r))
}

async fn cree_soumissionnaire(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(n): Json<marche::NouveauSoumissionnaire>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let x = marche::ajouter_soumissionnaire(&conn, &id, &n, acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "creation", "soumissionnaire", Some(&x.id), Some(&x.nom));
    Ok((StatusCode::CREATED, Json(x)))
}

async fn modifie_soumissionnaire(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(n): Json<marche::NouveauSoumissionnaire>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let x = marche::modifier_soumissionnaire(&conn, &id, &n)?;
    journaliser(&conn, &acteur, "modification", "soumissionnaire", Some(&x.id), Some(&x.nom));
    Ok(Json(x))
}

async fn supprime_soumissionnaire(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    marche::supprimer_soumissionnaire(&conn, &id)?;
    journaliser(&conn, &acteur, "suppression", "soumissionnaire", Some(&id), None);
    Ok(StatusCode::NO_CONTENT)
}

// --- Avenants ---------------------------------------------------------------

async fn cree_avenant(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(n): Json<marche::NouvelAvenant>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let x = marche::ajouter_avenant(&conn, &id, &n, acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "creation", "marche_avenant", Some(&x.id),
                Some(&format!("avenant n° {} — {}", x.numero, x.objet)));
    Ok((StatusCode::CREATED, Json(x)))
}

async fn modifie_avenant(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(n): Json<marche::NouvelAvenant>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let x = marche::modifier_avenant(&conn, &id, &n)?;
    journaliser(&conn, &acteur, "modification", "marche_avenant", Some(&x.id), Some(&x.objet));
    Ok(Json(x))
}

/// L'approbation est l'acte qui engage : elle est tracée nommément.
async fn avenant_statut(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(b): Json<CorpsStatut>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let x = marche::statut_avenant(&conn, &id, &b.statut, acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "modification", "marche_avenant", Some(&x.id),
                Some(&format!("avenant n° {} — {}", x.numero, b.statut)));
    Ok(Json(x))
}

async fn supprime_avenant(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    marche::supprimer_avenant(&conn, &id)?;
    journaliser(&conn, &acteur, "suppression", "marche_avenant", Some(&id), None);
    Ok(StatusCode::NO_CONTENT)
}

// --- Réceptions -------------------------------------------------------------

async fn cree_reception(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(n): Json<marche::NouvelleReception>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let x = marche::ajouter_reception(&conn, &id, &n, acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "creation", "marche_reception", Some(&x.id),
                Some(&format!("réception {} du {}", x.type_reception, x.date_reception)));
    Ok((StatusCode::CREATED, Json(x)))
}

async fn modifie_reception(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(n): Json<marche::NouvelleReception>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let x = marche::modifier_reception(&conn, &id, &n)?;
    journaliser(&conn, &acteur, "modification", "marche_reception", Some(&x.id), None);
    Ok(Json(x))
}

#[derive(Deserialize)]
struct CorpsLevee {
    #[serde(default)]
    date: Option<String>,
}

/// Lever les réserves : c'est ce geste qui libère la retenue de garantie.
async fn reception_lever_reserves(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(b): Json<CorpsLevee>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let x = marche::lever_reserves(&conn, &id, b.date.as_deref())?;
    journaliser(&conn, &acteur, "modification", "marche_reception", Some(&x.id),
                Some("levée des réserves"));
    Ok(Json(x))
}

async fn supprime_reception(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    marche::supprimer_reception(&conn, &id)?;
    journaliser(&conn, &acteur, "suppression", "marche_reception", Some(&id), None);
    Ok(StatusCode::NO_CONTENT)
}

/// Attribuer : un seul geste, parce que c'est un seul acte dans la réalité.
async fn attribue_marche(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let m = marche::attribuer(&conn, &id)?;
    journaliser(&conn, &acteur, "modification", "marche", Some(&m.id),
                Some(&format!("attribution — {}", m.numero)));
    Ok(Json(m))
}

async fn modifie_etape(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(m): Json<marche::MajEtape>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(marche::modifier_etape(&conn, &id, &m)?))
}

#[derive(Deserialize)]
struct CorpsStatutEtape {
    statut: String,
    /// Renseigné pour franchir une étape hors de son rang. Tracé nominativement.
    #[serde(default)]
    motif_derogation: Option<String>,
    /// Date réelle de l'acte et ce qui s'est dit : franchir une étape est un
    /// acte daté, pas un simple clic.
    #[serde(default)]
    date_effective: Option<String>,
    #[serde(default)]
    observations: Option<String>,
}

/// Changement de statut d'une étape, **règle d'enchaînement comprise**.
/// La réponse dit ce que le geste a entraîné ailleurs : rouvrir une étape
/// franchie remet en cause tout ce qui en découle, l'écran doit l'annoncer.
async fn etape_statut(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(b): Json<CorpsStatutEtape>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let r = marche::changer_statut_etape_saisie(
        &conn, &id, &b.statut, acteur.0.as_deref(),
        &marche::SaisieEtape {
            date_effective: b.date_effective.clone(),
            observations: b.observations.clone(),
            motif_derogation: b.motif_derogation.clone(),
        })?;
    let mut detail = b.statut.clone();
    if let Some(m) = &b.motif_derogation {
        detail = format!("{detail} — DÉROGATION : {m}");
    }
    if !r.etapes_rouvertes.is_empty() {
        detail = format!("{detail} — a rouvert : {}", r.etapes_rouvertes.join(", "));
    }
    journaliser(&conn, &acteur, "modification", "marche_etape", Some(&r.etape.id), Some(&detail));
    Ok(Json(r))
}

/// Les échéances de la période, tous calendriers demandés confondus.
async fn liste_evenements(
    State(s): State<AppState>,
    Query(f): Query<calendrier::FiltreCalendrier>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(calendrier::evenements(&conn, &f)?))
}

/// Les calendriers proposables : seulement ceux dont le module est visible.
async fn liste_calendriers(
    State(s): State<AppState>,
    Query(f): Query<calendrier::FiltreCalendrier>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(calendrier::disponibles(&conn, &f)?))
}

// --- Activation des modules -------------------------------------------------

async fn liste_modules(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(serde_json::json!({
        "modules": activation::lister(&conn)?,
        "formule_installee": activation::formule_installee(&conn),
    })))
}

async fn liste_formules() -> impl IntoResponse {
    Json(activation::formules())
}

/// Pose les droits selon la formule vendue. **Acte d'installation** : il
/// remplace l'état de souscription, il ne s'y ajoute pas.
async fn applique_formule(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(c): Json<activation::ChoixFormule>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let mods = activation::appliquer_formule(&conn, &c, acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "modification", "module", None,
                Some(&format!("formule « {} » — {} module(s) souscrit(s)",
                              c.formule, mods.iter().filter(|m| m.souscrit).count())));
    Ok(Json(mods))
}

/// Le client masque ou réaffiche un module qu'il a souscrit.
/// ⚠️ Réutilise le `CorpsActif` déjà défini plus haut : ne pas le redéclarer. Tracé : c'est
/// utile de savoir qu'un module a été masqué quand l'utilisateur appelle en
/// disant « mon menu a disparu ».
async fn module_actif(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(code): Path<String>,
    Json(b): Json<CorpsActif>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let m = activation::changer_actif(&conn, &code, b.actif)?;
    journaliser(&conn, &acteur, "modification", "module", Some(&m.code),
                Some(if b.actif { "affiché" } else { "masqué" }));
    Ok(Json(m))
}

/// Le tableau de suivi par phase : où les marchés s'arrêtent.
async fn marches_par_phase(
    State(s): State<AppState>,
    Query(f): Query<marche::FiltreMarches>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(marche::tableau_phases(&conn, &f)?))
}

/// Export du suivi. Comme les autres exports : le SERVEUR écrit le fichier dans
/// Téléchargements puis l'ouvre — le WebView2 de Tauri ne sait pas télécharger.
async fn export_suivi_marches(
    State(s): State<AppState>,
    acteur: Acteur,
) -> Result<impl IntoResponse, ApiError> {
    let dossier = dossier_telechargements();
    let jour = djigui_core::now()[..10].to_string();
    let chemin = {
        let conn = s.conn.lock().unwrap();
        let souhaite = dossier.join(format!("djigui-suivi-marches-{jour}.xlsx"));
        // Fichier déjà ouvert dans Excel = verrou Windows : on réessaie avec un
        // suffixe horaire plutôt que d'échouer sous le nez de l'utilisateur.
        let c = match crate::export_marches::ecrire_suivi(&conn, &souhaite) {
            Ok(c) => c,
            Err(_) => {
                let hhmmss = djigui_core::now()[11..19].replace(':', "");
                crate::export_marches::ecrire_suivi(
                    &conn, &dossier.join(format!("djigui-suivi-marches-{jour}-{hhmmss}.xlsx")))?
            }
        };
        journaliser(&conn, &acteur, "export", "marche", None, Some("suivi des marchés (.xlsx)"));
        c
    };
    let chemin_str = chemin.to_string_lossy().to_string();
    ouvrir_fichier(&chemin_str);
    Ok(Json(serde_json::json!({ "chemin": chemin_str })))
}

// --- Incidents de procédure : infructueux et recours ------------------------

async fn cree_incident(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(n): Json<marche::NouvelIncident>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let x = marche::declarer_incident(&conn, &id, &n, acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "creation", "marche_incident", Some(&x.id),
                Some(&format!("{} — {}", x.type_incident, x.motif)));
    Ok((StatusCode::CREATED, Json(x)))
}

#[derive(Deserialize)]
struct CorpsDecision {
    decision: String,
}

async fn clot_incident(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(b): Json<CorpsDecision>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let x = marche::clore_incident(&conn, &id, &b.decision)?;
    journaliser(&conn, &acteur, "modification", "marche_incident", Some(&x.id),
                Some(&format!("clos — {}", b.decision)));
    Ok(Json(x))
}

async fn supprime_incident(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    marche::supprimer_incident(&conn, &id)?;
    journaliser(&conn, &acteur, "suppression", "marche_incident", Some(&id), None);
    Ok(StatusCode::NO_CONTENT)
}

/// Aperçu **sans écriture** : Djigui ne recalcule jamais les dates tout seul.
async fn plan_replanif(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(marche::plan_replanification(&conn, &id)?))
}

async fn replanif(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let n = marche::replanifier(&conn, &id)?;
    journaliser(&conn, &acteur, "modification", "marche_etape", Some(&id),
                Some(&format!("{n} étape(s) replanifiée(s)")));
    Ok(Json(serde_json::json!({ "replanifiees": n })))
}

#[derive(Deserialize)]
struct FiltreTypesMarche {
    #[serde(default)]
    actifs_seulement: bool,
}

async fn liste_types_marche(
    State(s): State<AppState>,
    Query(q): Query<FiltreTypesMarche>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(marche::lister_types(&conn, q.actifs_seulement)?))
}

async fn cree_type_marche(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(t): Json<marche::NouveauType>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let x = marche::creer_type(&conn, &t, acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "creation", "marche_type", Some(&x.id), Some(&x.libelle));
    Ok((StatusCode::CREATED, Json(x)))
}

async fn modifie_type_marche(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(t): Json<marche::NouveauType>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let x = marche::modifier_type(&conn, &id, &t)?;
    journaliser(&conn, &acteur, "modification", "marche_type", Some(&x.id), Some(&x.libelle));
    Ok(Json(x))
}

async fn supprime_type_marche(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    marche::supprimer_type(&conn, &id)?;
    journaliser(&conn, &acteur, "suppression", "marche_type", Some(&id), None);
    Ok(StatusCode::NO_CONTENT)
}

// ---- Rapports (§7) ---------------------------------------------------------

async fn rapport_journal_ventes(
    State(s): State<AppState>,
    Query(p): Query<rapport::Periode>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(rapport::journal(&conn, "vente", &p)?))
}

async fn rapport_journal_achats(
    State(s): State<AppState>,
    Query(p): Query<rapport::Periode>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(rapport::journal(&conn, "achat", &p)?))
}

async fn rapport_marges(
    State(s): State<AppState>,
    Query(p): Query<rapport::Periode>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(rapport::marges_par_article(&conn, &p)?))
}

async fn rapport_stock(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(rapport::etat_stock(&conn)?))
}

async fn rapport_encours_clients(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(rapport::encours(&conn, "vente")?))
}

async fn rapport_encours_fournisseurs(
    State(s): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(rapport::encours(&conn, "achat")?))
}

async fn rapport_numerotation(
    State(s): State<AppState>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(rapport::continuite_numerotation(&conn)?))
}

// ---- Prix d'achat estimés (migration 0035) ---------------------------------
//
// Un chiffre inventé sans étiquette est plus dangereux qu'une case vide : ces
// routes posent des prix de démonstration, mais toujours marqués comme tels et
// toujours réversibles.

async fn prix_apercu(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(prix_estime::apercu(&conn)?))
}

async fn prix_estimer(
    State(s): State<AppState>,
    acteur: Acteur,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let r = prix_estime::appliquer(&conn)?;
    journaliser(&conn, &acteur, "modification", "article", None,
                Some(&format!("{} prix d'achat estimés", r.estimes)));
    Ok(Json(r))
}

async fn prix_effacer(
    State(s): State<AppState>,
    acteur: Acteur,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let n = prix_estime::effacer_estimations(&conn)?;
    journaliser(&conn, &acteur, "modification", "article", None,
                Some(&format!("{n} estimations effacées")));
    Ok(Json(serde_json::json!({ "effaces": n })))
}

async fn prix_a_completer(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(prix_estime::a_completer(&conn)?))
}

#[derive(Deserialize)]
struct CorpsPrixReels {
    prix: Vec<prix_estime::PrixReel>,
}

async fn prix_reels(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(b): Json<CorpsPrixReels>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let n = prix_estime::definir_prix_reels(&conn, &b.prix)?;
    journaliser(&conn, &acteur, "modification", "article", None,
                Some(&format!("{n} prix d'achat saisis")));
    Ok(Json(serde_json::json!({ "enregistres": n })))
}

// ---- Comptabilité — écran du comptable (migration 0034) --------------------
//
// Rappel du procédé : Djigui ne décide de rien. Le comptable crée ses comptes,
// écrit ses règles multicritères, et les applique à tout l'historique déjà en
// base. Ce qu'aucune règle ne couvre reste dans la corbeille « À ranger ».

#[derive(Deserialize)]
struct FiltreComptes {
    #[serde(default)]
    actifs_seulement: bool,
}

async fn liste_comptes(
    State(s): State<AppState>,
    Query(q): Query<FiltreComptes>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(comptabilite::lister_comptes(&conn, q.actifs_seulement)?))
}

async fn get_compte(
    State(s): State<AppState>,
    Path(numero): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(comptabilite::lire_compte(&conn, &numero)?))
}

async fn cree_compte(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(c): Json<comptabilite::NouveauCompte>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_comptable(&conn, &acteur)?;
    let r = comptabilite::creer_compte(&conn, &c, acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "creation", "compte", Some(&r.numero), Some(&r.libelle));
    Ok((StatusCode::CREATED, Json(r)))
}

async fn modifie_compte(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(numero): Path<String>,
    Json(c): Json<comptabilite::NouveauCompte>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_comptable(&conn, &acteur)?;
    let r = comptabilite::modifier_compte(&conn, &numero, &c)?;
    journaliser(&conn, &acteur, "modification", "compte", Some(&r.numero), Some(&r.libelle));
    Ok(Json(r))
}

async fn supprime_compte(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(numero): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_comptable(&conn, &acteur)?;
    comptabilite::supprimer_compte(&conn, &numero)?;
    journaliser(&conn, &acteur, "suppression", "compte", Some(&numero), None);
    Ok(StatusCode::NO_CONTENT)
}

/// Plan OHADA de base — **proposé**, jamais imposé. Les comptes déjà créés par
/// le comptable ne sont pas touchés.
async fn installe_plan_ohada(
    State(s): State<AppState>,
    acteur: Acteur,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_comptable(&conn, &acteur)?;
    let n = comptabilite::installer_plan_ohada(&conn, acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "creation", "plan_comptable", None, Some(&format!("{n} comptes")));
    Ok(Json(serde_json::json!({ "ajoutes": n })))
}

async fn liste_regles(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(comptabilite::lister_regles(&conn)?))
}

async fn cree_regle(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(r): Json<comptabilite::NouvelleRegle>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_comptable(&conn, &acteur)?;
    let g = comptabilite::creer_regle(&conn, &r, acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "creation", "regle_comptable", Some(&g.id), Some(&g.nom));
    Ok((StatusCode::CREATED, Json(g)))
}

async fn modifie_regle(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(r): Json<comptabilite::NouvelleRegle>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_comptable(&conn, &acteur)?;
    let g = comptabilite::modifier_regle(&conn, &id, &r)?;
    journaliser(&conn, &acteur, "modification", "regle_comptable", Some(&g.id), Some(&g.nom));
    Ok(Json(g))
}

async fn supprime_regle(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_comptable(&conn, &acteur)?;
    comptabilite::supprimer_regle(&conn, &id)?;
    journaliser(&conn, &acteur, "suppression", "regle_comptable", Some(&id), None);
    Ok(StatusCode::NO_CONTENT)
}

async fn regles_supprimer_lot(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(b): Json<LotIds>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_comptable(&conn, &acteur)?;
    let n = comptabilite::supprimer_regles(&conn, &b.ids)?;
    journaliser(&conn, &acteur, "suppression", "regle_comptable", None, Some(&format!("{n} règles")));
    Ok(Json(serde_json::json!({ "supprimees": n })))
}

/// La corbeille « À ranger » — recherche multicritère.
async fn liste_operations(
    State(s): State<AppState>,
    Query(f): Query<comptabilite::FiltreOperations>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(comptabilite::lister_operations(&conn, &f)?))
}

#[derive(Deserialize)]
struct CorpsRattachement {
    operations: Vec<comptabilite::RefOperation>,
}

async fn rattache_operations(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(b): Json<CorpsRattachement>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_comptable(&conn, &acteur)?;
    let r = comptabilite::rattacher(&conn, &b.operations, acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "creation", "ecriture", None,
                Some(&format!("{} écriture(s)", r.creees)));
    Ok(Json(r))
}

/// « Ranger tout l'historique » : le geste qu'on fait une fois les règles posées.
async fn rattache_tout(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(f): Json<comptabilite::FiltreOperations>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_comptable(&conn, &acteur)?;
    let r = comptabilite::rattacher_selon(&conn, &f, acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "creation", "ecriture", None,
                Some(&format!("rattachement en lot : {} écriture(s)", r.creees)));
    Ok(Json(r))
}

#[derive(Deserialize)]
struct FiltreEcritures {
    #[serde(default)]
    du: Option<String>,
    #[serde(default)]
    au: Option<String>,
    #[serde(default)]
    journal: Option<String>,
    #[serde(default)]
    incompletes_seulement: bool,
}

async fn liste_ecritures(
    State(s): State<AppState>,
    Query(q): Query<FiltreEcritures>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(comptabilite::lister_ecritures(
        &conn,
        q.du.as_deref(),
        q.au.as_deref(),
        q.journal.as_deref(),
        q.incompletes_seulement,
    )?))
}

async fn get_ecriture(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(comptabilite::lire_ecriture(&conn, &id)?))
}

#[derive(Deserialize)]
struct CorpsContrepassation {
    #[serde(default)]
    date: Option<String>,
    #[serde(default)]
    motif: Option<String>,
}

async fn contrepasse_ecriture(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(b): Json<CorpsContrepassation>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_comptable(&conn, &acteur)?;
    let e = comptabilite::contrepasser(&conn, &id, b.date.as_deref(), b.motif.as_deref(), acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "annulation", "ecriture", Some(&id), Some(&e.libelle));
    Ok(Json(e))
}

/// Rejouer après avoir écrit la règle qui manquait — le geste qui suit
/// naturellement la découverte d'une opération en compte d'attente.
async fn rejoue_ecriture(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_comptable(&conn, &acteur)?;
    let r = comptabilite::rejouer(&conn, &id, acteur.0.as_deref())?;
    Ok(Json(r))
}

async fn rejoue_incompletes(
    State(s): State<AppState>,
    acteur: Acteur,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_comptable(&conn, &acteur)?;
    let r = comptabilite::rejouer_incompletes(&conn, acteur.0.as_deref())?;
    journaliser(&conn, &acteur, "modification", "ecriture", None,
                Some(&format!("rejeu : {} écriture(s)", r.creees)));
    Ok(Json(r))
}

#[derive(Deserialize)]
struct CorpsCompte {
    compte_numero: String,
}

/// Affectation manuelle d'un compte à une ligne : la sortie de secours quand
/// aucune règle ne convient. **C'est le comptable qui tranche.**
async fn affecte_compte_ligne(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(b): Json<CorpsCompte>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_comptable(&conn, &acteur)?;
    comptabilite::affecter_ligne(&conn, &id, &b.compte_numero)?;
    journaliser(&conn, &acteur, "modification", "ecriture_ligne", Some(&id), Some(&b.compte_numero));
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct PeriodeQuery {
    #[serde(default)]
    du: Option<String>,
    #[serde(default)]
    au: Option<String>,
}

async fn get_grand_livre(
    State(s): State<AppState>,
    Path(numero): Path<String>,
    Query(q): Query<PeriodeQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(comptabilite::grand_livre(&conn, &numero, q.du.as_deref(), q.au.as_deref())?))
}

async fn get_balance(
    State(s): State<AppState>,
    Query(q): Query<PeriodeQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(comptabilite::balance(&conn, q.du.as_deref(), q.au.as_deref())?))
}

#[derive(Deserialize)]
struct CorpsLettrage {
    lignes: Vec<String>,
    #[serde(default)]
    code: Option<String>,
}

async fn lettre_lignes(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(b): Json<CorpsLettrage>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_comptable(&conn, &acteur)?;
    let code = comptabilite::lettrer(&conn, &b.lignes, b.code.as_deref())?;
    Ok(Json(serde_json::json!({ "code": code })))
}

async fn delettre_lignes(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(b): Json<CorpsLettrage>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_comptable(&conn, &acteur)?;
    let n = comptabilite::delettrer(&conn, &b.lignes)?;
    Ok(Json(serde_json::json!({ "delettrees": n })))
}

// ---- Moyens de paiement configurables (migration 0018) ---------------------

async fn liste_moyens(
    State(s): State<AppState>,
    Query(q): Query<FiltreMoyens>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let liste = if q.actifs_seulement {
        moyen_paiement::lister_actifs(&conn)?
    } else {
        moyen_paiement::lister(&conn)?
    };
    Ok(Json(liste))
}

#[derive(Deserialize)]
struct FiltreMoyens {
    #[serde(default)]
    actifs_seulement: bool,
}

async fn cree_moyen(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(m): Json<moyen_paiement::NouveauMoyen>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let mp = moyen_paiement::creer(&conn, &m)?;
    journaliser(&conn, &acteur, "creation", "moyen_paiement", Some(&mp.id), Some(&mp.nom));
    Ok((StatusCode::CREATED, Json(mp)))
}

async fn modifie_moyen(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(m): Json<moyen_paiement::NouveauMoyen>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let mp = moyen_paiement::modifier(&conn, &id, &m)?;
    journaliser(&conn, &acteur, "modification", "moyen_paiement", Some(&mp.id), Some(&mp.nom));
    Ok(Json(mp))
}

async fn supprime_moyen(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    moyen_paiement::supprimer(&conn, &id)?;
    journaliser(&conn, &acteur, "suppression", "moyen_paiement", Some(&id), None);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct LotMoyensActif {
    ids: Vec<String>,
    actif: bool,
}

async fn moyens_actif_lot(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(b): Json<LotMoyensActif>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let n = moyen_paiement::definir_actif(&conn, &b.ids, b.actif)?;
    let action = if b.actif { "activation" } else { "desactivation" };
    journaliser(&conn, &acteur, action, "moyen_paiement", None, Some(&format!("{n} moyen(s)")));
    Ok(Json(serde_json::json!({ "touches": n })))
}

#[derive(Deserialize)]
struct OrdreMoyens {
    ids: Vec<String>,
}

async fn moyens_reordonner(
    State(s): State<AppState>,
    Json(b): Json<OrdreMoyens>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    moyen_paiement::reordonner(&conn, &b.ids)?;
    Ok(StatusCode::NO_CONTENT)
}

// ---- Traduction des erreurs métier en HTTP ---------------------------------

struct ApiError(CoreError);

impl From<CoreError> for ApiError {
    fn from(e: CoreError) -> Self {
        ApiError(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (code, msg) = match &self.0 {
            CoreError::NotFound(m) => (StatusCode::NOT_FOUND, m.clone()),
            CoreError::Rule(m) => (StatusCode::UNPROCESSABLE_ENTITY, m.clone()),
            CoreError::Forbidden(m) => (StatusCode::FORBIDDEN, m.clone()),
            CoreError::Unauthorized(m) => (StatusCode::UNAUTHORIZED, m.clone()),
            CoreError::Db(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };
        (code, Json(serde_json::json!({ "erreur": msg }))).into_response()
    }
}

// ---------------------------------------------------------------------------
// Sauvegarde chiffrée (migration 0042)
// ---------------------------------------------------------------------------
//
// ⚠️ Le mot de passe de sauvegarde **n'est jamais stocké** : il transite à
// chaque appel qui en a besoin, et la base n'en garde qu'une empreinte de
// vérification. Un mot de passe rangé en base à côté des données qu'il protège
// ne protégerait rien.

async fn sauvegarde_parametres(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(serde_json::json!({
        "parametres": sauvegarde::lire_parametres(&conn)?,
        "destinations": sauvegarde::lister_destinations(&conn)?,
        "journal": sauvegarde::lister_journal(&conn, 20)?,
    })))
}

async fn sauvegarde_modifier(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(m): Json<sauvegarde::MajParametres>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_admin_pour(&conn, &acteur, "modifier les réglages de sauvegarde")?;
    let p = sauvegarde::modifier_parametres(&conn, &m)?;
    journaliser(&conn, &acteur, "modification", "sauvegarde", None,
                Some(&format!("réglages — automatique : {}, copies conservées : {}",
                              if p.activee { "oui" } else { "non" }, p.copies_a_conserver)));
    Ok(Json(p))
}

#[derive(Deserialize)]
struct CorpsMotDePasse {
    /// `null` ou absent = retirer le mot de passe et revenir à la clé intégrée.
    mot_de_passe: Option<String>,
}

async fn sauvegarde_mot_de_passe(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(c): Json<CorpsMotDePasse>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_admin_pour(&conn, &acteur, "changer la protection des sauvegardes")?;
    let p = sauvegarde::definir_mot_de_passe(&conn, c.mot_de_passe.as_deref())?;
    // On trace le CHANGEMENT, jamais la valeur.
    journaliser(&conn, &acteur, "modification", "sauvegarde", None,
                Some(if p.mot_de_passe_defini {
                    "protection des sauvegardes : mot de passe posé"
                } else {
                    "protection des sauvegardes : retour à la clé intégrée"
                }));
    Ok(Json(p))
}

async fn sauvegarde_destinations(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(sauvegarde::lister_destinations(&conn)?))
}

async fn sauvegarde_ajout_destination(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(d): Json<sauvegarde::NouvelleDestination>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_admin_pour(&conn, &acteur, "ajouter un dossier de sauvegarde")?;
    exiger_serveur(&conn, "la configuration des sauvegardes")?;
    let cree = sauvegarde::ajouter_destination(&conn, &d)?;
    journaliser(&conn, &acteur, "creation", "sauvegarde_destination", Some(&cree.id),
                Some(&format!("{} → {}", cree.libelle, cree.chemin)));
    Ok((StatusCode::CREATED, Json(cree)))
}

async fn sauvegarde_maj_destination(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(d): Json<sauvegarde::NouvelleDestination>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_admin_pour(&conn, &acteur, "modifier un dossier de sauvegarde")?;
    exiger_serveur(&conn, "la configuration des sauvegardes")?;
    let maj = sauvegarde::modifier_destination(&conn, &id, &d)?;
    journaliser(&conn, &acteur, "modification", "sauvegarde_destination", Some(&id),
                Some(&format!("{} → {}", maj.libelle, maj.chemin)));
    Ok(Json(maj))
}

async fn sauvegarde_suppr_destination(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_admin_pour(&conn, &acteur, "retirer un dossier de sauvegarde")?;
    exiger_serveur(&conn, "la configuration des sauvegardes")?;
    sauvegarde::supprimer_destination(&conn, &id)?;
    journaliser(&conn, &acteur, "suppression", "sauvegarde_destination", Some(&id),
                Some("les copies déjà écrites dans ce dossier sont conservées"));
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct CorpsExecuter {
    #[serde(default)]
    mot_de_passe: Option<String>,
    /// `"manuelle"` (bouton) ou `"fermeture"` (arrêt de l'application).
    #[serde(default = "declencheur_manuel")]
    declencheur: String,
}

fn declencheur_manuel() -> String {
    "manuelle".into()
}

async fn sauvegarde_executer(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(c): Json<CorpsExecuter>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let travail = s.dossier_travail();
    let r = sauvegarde::executer(
        &conn,
        &s.dossier_documents,
        &travail,
        &c.declencheur,
        c.mot_de_passe.as_deref(),
    )?;
    journaliser(&conn, &acteur, "sauvegarde", "sauvegarde", None, Some(&r.message));
    Ok(Json(r))
}

async fn sauvegarde_journal(State(s): State<AppState>) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    Ok(Json(sauvegarde::lister_journal(&conn, 50)?))
}

#[derive(Deserialize)]
struct CorpsArchive {
    chemin: String,
    #[serde(default)]
    mot_de_passe: Option<String>,
}

/// Ce qu'on peut dire d'un fichier **sans** son mot de passe : sa date, s'il est
/// protégé, combien de documents il contient. L'écran de restauration s'en sert
/// pour annoncer ce qui va être remis en place avant de demander confirmation.
async fn sauvegarde_apercu(
    Json(c): Json<CorpsArchive>,
) -> Result<impl IntoResponse, ApiError> {
    Ok(Json(sauvegarde::apercu(std::path::Path::new(&c.chemin))?))
}

/// ⚠️⚠️ Remplace les données en service. Réservé à l'administrateur.
///
/// La connexion ouverte pointe encore sur l'ancien fichier après l'opération :
/// la réponse le dit explicitement, et l'écran impose le redémarrage.
async fn sauvegarde_restaurer(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(c): Json<CorpsArchive>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_admin_pour(&conn, &acteur, "restaurer une sauvegarde")?;
    // Trace AVANT l'opération : si la restauration réussit, elle emporte le
    // journal d'audit de la base courante — la ligne écrite après ne survivrait
    // pas. Celle-ci part avec l'ancienne base, mise de côté et donc consultable.
    journaliser(&conn, &acteur, "restauration", "sauvegarde", None,
                Some(&format!("restauration demandée depuis {}", c.chemin)));

    let r = sauvegarde::restaurer(
        std::path::Path::new(&c.chemin),
        &s.chemin_base,
        &s.dossier_documents,
        c.mot_de_passe.as_deref(),
    )?;
    Ok(Json(r))
}

#[derive(Deserialize)]
struct CorpsLicence {
    licence: String,
}

/// Enregistre la clé de licence remise au client à l'installation.
///
/// Elle devient le **secret de chiffrement des sauvegardes** (mig 0042) : propre
/// à chaque client, donc absente de l'exécutable, et pourtant récupérable —
/// elle figure sur les documents d'installation et chez SODEVITEL.
async fn sauvegarde_licence(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(c): Json<CorpsLicence>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_admin_pour(&conn, &acteur, "enregistrer la clé de licence")?;
    let p = sauvegarde::definir_licence(&conn, &c.licence)?;
    // On trace la SAISIE, en ne gardant que la fin de la clé : de quoi vérifier
    // plus tard laquelle a été posée, sans l'inscrire en clair dans un journal
    // que la sauvegarde elle-même emportera.
    journaliser(&conn, &acteur, "modification", "sauvegarde", None,
                Some(&format!("licence enregistrée (…{})",
                              p.licence_fin.clone().unwrap_or_default())));
    Ok(Json(p))
}

#[derive(Deserialize)]
struct QueryChemin {
    #[serde(default)]
    chemin: Option<String>,
}

/// Explorateur de dossiers **de la machine serveur**.
///
/// ⚠️ Réservé à l'administrateur : cette route révèle l'arborescence de la
/// machine sur le réseau local. Elle ne lit aucun fichier, seulement des noms
/// de dossiers, mais c'est déjà une information à ne pas exposer largement.
async fn sauvegarde_parcourir(
    State(s): State<AppState>,
    acteur: Acteur,
    Query(q): Query<QueryChemin>,
) -> Result<impl IntoResponse, ApiError> {
    {
        let conn = s.conn.lock().unwrap();
        exiger_admin_pour(&conn, &acteur, "parcourir les dossiers du serveur")?;
    }
    Ok(Json(sauvegarde::parcourir(q.chemin.as_deref())?))
}

/// Endroits probables (Drive, clé USB, Documents), pour éviter d'avoir à taper
/// un chemin quand on ne sait pas ce qu'est un chemin.
async fn sauvegarde_suggestions(
    State(s): State<AppState>,
    acteur: Acteur,
) -> Result<impl IntoResponse, ApiError> {
    {
        let conn = s.conn.lock().unwrap();
        exiger_admin_pour(&conn, &acteur, "consulter les dossiers proposés")?;
    }
    Ok(Json(sauvegarde::suggestions()))
}

/// Ouvre le **vrai sélecteur de dossiers de Windows** sur la machine serveur.
///
/// Demande de l'utilisateur : « utilise l'explorateur Windows, c'est plus
/// simple ». Voir `dossier_natif` pour la raison du choix serveur plutôt que
/// coquille Tauri.
///
/// La boîte est modale : on la lance sur un thread bloquant pour ne pas figer
/// tout le serveur pendant que l'utilisateur cherche son dossier.
async fn sauvegarde_choisir_dossier(
    State(s): State<AppState>,
    acteur: Acteur,
) -> Result<impl IntoResponse, ApiError> {
    {
        let conn = s.conn.lock().unwrap();
        exiger_admin_pour(&conn, &acteur, "choisir un dossier de sauvegarde")?;
        exiger_serveur(&conn, "le choix du dossier de sauvegarde")?;
    }
    let choix = tokio::task::spawn_blocking(crate::dossier_natif::choisir)
        .await
        .map_err(|e| ApiError(CoreError::Rule(format!("sélecteur interrompu : {e}"))))?
        .map_err(|e| ApiError(CoreError::Rule(e.to_string())))?;
    // `chemin: null` = annulation. L'écran ne doit alors rien changer ni rien
    // afficher : annuler n'est pas un échec.
    Ok(Json(serde_json::json!({ "chemin": choix })))
}

// ---------------------------------------------------------------------------
// Paie & RH — paramètres légaux (migration 0044)
// ---------------------------------------------------------------------------
//
// ⚠️ Réservé à l'administrateur : ces valeurs décident du salaire net de tout
// le monde et des sommes déclarées à l'administration.

#[derive(Deserialize)]
struct QueryDate {
    /// Date d'application des paramètres voulus. Absente = aujourd'hui.
    ///
    /// ⚠️ Ce n'est PAS un filtre d'affichage : c'est ce qui permet de retrouver
    /// les taux d'un mois passé pour réimprimer un bulletin à l'identique.
    #[serde(default)]
    date: Option<String>,
}

async fn paie_lire_parametres(
    State(s): State<AppState>,
    Query(q): Query<QueryDate>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    let date = q.date.unwrap_or_else(|| djigui_core::now()[..10].to_string());
    Ok(Json(paie_parametres::jeu_complet(&conn, &date)?))
}

async fn paie_nouvelle_periode(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(n): Json<paie_parametres::NouvellePeriode>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_admin_pour(&conn, &acteur, "modifier les paramètres de paie")?;
    let posees = paie_parametres::nouvelle_periode(&conn, &n)?;
    journaliser(&conn, &acteur, "creation", "paie_parametres", None,
                Some(&format!("nouvelle période « {} » au {} — {} valeur(s)",
                              n.table, n.date_debut, posees)));
    Ok(Json(serde_json::json!({ "lignes_posees": posees })))
}

#[derive(Deserialize)]
struct CorpsCorrection {
    table: String,
    date_debut: String,
    lignes: serde_json::Value,
}

/// Corrige la période EN COURS au lieu d'en ouvrir une nouvelle.
///
/// ⚠️ Le geste normal est d'ouvrir une nouvelle période. Celui-ci n'existe que
/// pour les valeurs installées d'origine — indicatives et non certifiées —
/// qu'il serait absurde de « fermer » alors qu'elles n'ont jamais servi.
/// D'où le garde-fou : dès qu'un bulletin existe, on refuse.
async fn paie_corriger_periode(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(c): Json<CorpsCorrection>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_admin_pour(&conn, &acteur, "corriger les paramètres de paie")?;

    // Table absente tant que l'étape « bulletins » n'est pas livrée : dans ce
    // cas il n'existe évidemment aucun bulletin.
    let nb_bulletins: i64 = conn
        .query_row("SELECT COUNT(*) FROM bulletins_paie", [], |r| r.get(0))
        .unwrap_or(0);
    if nb_bulletins > 0 {
        return Err(ApiError(CoreError::Rule(
            "Des bulletins ont déjà été calculés avec ces valeurs. Pour changer un taux,              ouvrez une NOUVELLE PÉRIODE : corriger celle en cours réécrirait des bulletins              déjà remis aux salariés."
                .into(),
        )));
    }
    let n = paie_parametres::corriger_periode_courante(&conn, &c.table, &c.date_debut, &c.lignes)?;
    journaliser(&conn, &acteur, "modification", "paie_parametres", None,
                Some(&format!("correction de « {} » au {} — {} valeur(s)",
                              c.table, c.date_debut, n)));
    Ok(Json(serde_json::json!({ "lignes_posees": n })))
}

#[derive(Deserialize)]
struct CorpsVerifie {
    table: String,
    date_debut: String,
}

async fn paie_marquer_verifie(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(c): Json<CorpsVerifie>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_admin_pour(&conn, &acteur, "valider les paramètres de paie")?;
    let n = paie_parametres::marquer_verifie(&conn, &c.table, &c.date_debut)?;
    // On trace QUI confirme avoir confronté les valeurs au texte en vigueur :
    // c'est une prise de responsabilité, elle doit laisser une trace.
    journaliser(&conn, &acteur, "modification", "paie_parametres", None,
                Some(&format!("« {} » du {} confirmé conforme ({} ligne(s))",
                              c.table, c.date_debut, n)));
    Ok(Json(serde_json::json!({ "lignes": n })))
}

async fn paie_enregistrer_employeur(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(m): Json<paie_parametres::MajEmployeur>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_admin_pour(&conn, &acteur, "modifier les paramètres employeur")?;
    let p = paie_parametres::enregistrer_employeur(&conn, &m)?;
    journaliser(&conn, &acteur, "modification", "paie_parametres", None,
                Some("paramètres employeur (accident du travail, IPRES/CSS/IPM, majorations)"));
    Ok(Json(p))
}

// ---- Paie & RH : salariés et contrats (migration 0045) ---------------------
//
// ⚠️ Réservé à l'administrateur : les salaires ne regardent pas le caissier.

#[derive(Deserialize)]
struct QueryFiltreEmployes {
    #[serde(default)]
    filtre: Option<paie_employe::Filtre>,
}

async fn paie_liste_employes(
    State(s): State<AppState>,
    acteur: Acteur,
    Query(q): Query<QueryFiltreEmployes>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_admin_pour(&conn, &acteur, "consulter les salariés")?;
    Ok(Json(paie_employe::lister(&conn, q.filtre.unwrap_or_default())?))
}

async fn paie_get_employe(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_admin_pour(&conn, &acteur, "consulter un salarié")?;
    Ok(Json(paie_employe::lire(&conn, &id)?))
}

async fn paie_cree_employe(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(e): Json<paie_employe::NouvelEmploye>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_admin_pour(&conn, &acteur, "enregistrer un salarié")?;
    let cree = paie_employe::creer(&conn, &e)?;
    journaliser(&conn, &acteur, "creation", "employe", Some(&cree.id),
                Some(&format!("{} ({})", cree.nom_complet, cree.matricule)));
    Ok((StatusCode::CREATED, Json(cree)))
}

async fn paie_modifie_employe(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(e): Json<paie_employe::NouvelEmploye>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_admin_pour(&conn, &acteur, "modifier un salarié")?;
    let maj = paie_employe::modifier(&conn, &id, &e)?;
    journaliser(&conn, &acteur, "modification", "employe", Some(&id),
                Some(&format!("{} ({})", maj.nom_complet, maj.matricule)));
    Ok(Json(maj))
}

async fn paie_supprime_employe(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_admin_pour(&conn, &acteur, "supprimer un salarié")?;
    paie_employe::supprimer(&conn, &id)?;
    journaliser(&conn, &acteur, "suppression", "employe", Some(&id), None);
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct CorpsDepart {
    date_sortie: String,
    motif: String,
}

async fn paie_depart(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(c): Json<CorpsDepart>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_admin_pour(&conn, &acteur, "enregistrer un départ")?;
    let e = paie_employe::enregistrer_depart(&conn, &id, &c.date_sortie, &c.motif)?;
    journaliser(&conn, &acteur, "modification", "employe", Some(&id),
                Some(&format!("départ le {} — {}", c.date_sortie, c.motif)));
    Ok(Json(e))
}

async fn paie_reintegrer(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_admin_pour(&conn, &acteur, "réintégrer un salarié")?;
    let e = paie_employe::reintegrer(&conn, &id)?;
    journaliser(&conn, &acteur, "modification", "employe", Some(&id),
                Some("réintégration — un nouveau contrat reste à enregistrer"));
    Ok(Json(e))
}

async fn paie_liste_contrats(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_admin_pour(&conn, &acteur, "consulter les contrats")?;
    Ok(Json(paie_employe::contrats(&conn, &id)?))
}

#[derive(Deserialize)]
struct CorpsDepartLot {
    ids: Vec<String>,
    date_sortie: String,
    motif: String,
}

async fn paie_depart_lot(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(c): Json<CorpsDepartLot>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_admin_pour(&conn, &acteur, "enregistrer des départs")?;
    let r = paie_employe::depart_lot(&conn, &c.ids, &c.date_sortie, &c.motif)?;
    journaliser(&conn, &acteur, "modification", "employe", None, Some(&r.message));
    Ok(Json(r))
}

async fn paie_cree_contrat(
    State(s): State<AppState>,
    acteur: Acteur,
    Json(c): Json<paie_employe::NouveauContrat>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_admin_pour(&conn, &acteur, "enregistrer un contrat")?;
    let cree = paie_employe::creer_contrat(&conn, &c)?;
    journaliser(&conn, &acteur, "creation", "contrat", Some(&cree.id),
                Some(&format!("{} du {} — salaire {}", cree.type_contrat,
                              cree.date_debut, cree.salaire_base)));
    Ok((StatusCode::CREATED, Json(cree)))
}

async fn paie_modifie_contrat(
    State(s): State<AppState>,
    acteur: Acteur,
    Path(id): Path<String>,
    Json(c): Json<paie_employe::NouveauContrat>,
) -> Result<impl IntoResponse, ApiError> {
    let conn = s.conn.lock().unwrap();
    exiger_admin_pour(&conn, &acteur, "modifier un contrat")?;
    let maj = paie_employe::modifier_contrat(&conn, &id, &c)?;
    journaliser(&conn, &acteur, "modification", "contrat", Some(&id), None);
    Ok(Json(maj))
}
