//! Modules métier à frontière nette (spec §3.4). Chaque module est isolé et
//! passe par le point d'autorisation unique pour toute vérification de droit.

pub mod abonnement;
pub mod article;
pub mod audit;
pub mod categorie;
pub mod dependance;
pub mod depot;
pub mod document;
pub mod inventaire;
pub mod jalon;
pub mod moyen_paiement;
pub mod notification;
pub mod paiement;
pub mod parametres;
pub mod projet;
pub mod rapport;
pub mod rendez_vous;
pub mod seed;
pub mod seeder;
pub mod session_caisse;
pub mod stock;
pub mod taux_tva;
pub mod taxe;
pub mod tiers;
pub mod utilisateur;
