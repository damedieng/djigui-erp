//! Modules métier à frontière nette (spec §3.4). Chaque module est isolé et
//! passe par le point d'autorisation unique pour toute vérification de droit.

pub mod abonnement;
pub mod activation;
pub mod article;
pub mod audit;
pub mod calendrier;
pub mod categorie;
pub mod comptabilite;
pub mod dependance;
pub mod depot;
pub mod document;
pub mod inventaire;
pub mod jalon;
pub mod lettres;
pub mod marche;
pub mod moyen_paiement;
pub mod notification;
pub mod paiement;
pub mod paie_employe;
pub mod paie_parametres;
pub mod parametres;
pub mod prix_estime;
pub mod production;
pub mod projet;
pub mod rapport;
pub mod rendez_vous;
pub mod sauvegarde;
pub mod seed;
pub mod seeder;
pub mod session_caisse;
pub mod stock;
pub mod taux_tva;
pub mod taxe;
pub mod tiers;
pub mod utilisateur;
