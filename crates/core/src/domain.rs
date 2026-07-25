//! Types du domaine partagés entre modules. Les enums reflètent exactement les
//! contraintes CHECK du schéma (§5). Sérialisés en `snake_case` pour l'API JSON.

use serde::{Deserialize, Serialize};

macro_rules! enum_texte {
    ($(#[$m:meta])* $nom:ident { $($variant:ident => $txt:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
        #[serde(rename_all = "snake_case")]
        pub enum $nom { $($variant),+ }

        impl $nom {
            pub fn as_str(&self) -> &'static str {
                match self { $(Self::$variant => $txt),+ }
            }
            pub fn parse(s: &str) -> Option<Self> {
                match s { $($txt => Some(Self::$variant),)+ _ => None }
            }
        }
        impl rusqlite::ToSql for $nom {
            fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> {
                Ok(self.as_str().into())
            }
        }
    };
}

enum_texte!(TypeRole { Client => "client", Fournisseur => "fournisseur", LesDeux => "les_deux" });
// Nature d'un tiers : commande les mentions attendues sur la facture
// (NINEA/RCCM pour une entreprise, prénom/CNI pour un particulier).
// Aucune de ces mentions n'est obligatoire — voir migration 0027.
enum_texte!(NatureTiers { Particulier => "particulier", Entreprise => "entreprise" });
enum_texte!(TypeArticle { Bien => "bien", Service => "service" });
enum_texte!(TypeTaxe { Pourcentage => "pourcentage", Fixe => "fixe" });
enum_texte!(TypeDocument {
    Devis => "devis", Facture => "facture", Avoir => "avoir",
    Commande => "commande", Livraison => "livraison", Proforma => "proforma",
});
enum_texte!(SensDocument { Vente => "vente", Achat => "achat" });
enum_texte!(StatutDocument {
    Brouillon => "brouillon", Valide => "valide", Accepte => "accepte",
    Transforme => "transforme", Annule => "annule",
});
enum_texte!(SensMouvement { Entree => "entree", Sortie => "sortie" });
enum_texte!(MotifMouvement {
    Vente => "vente", Achat => "achat", Inventaire => "inventaire",
    Casse => "casse", Transfert => "transfert", Production => "production",
});
enum_texte!(SensPaiement { Encaissement => "encaissement", Decaissement => "decaissement" });
enum_texte!(ModePaiement {
    Espece => "espece", MobileMoney => "mobile_money",
    Virement => "virement", Cheque => "cheque",
});
enum_texte!(RoleUtilisateur { Admin => "admin", Caissier => "caissier" });
enum_texte!(FrequenceAbonnement {
    Mensuel => "mensuel", Trimestriel => "trimestriel", Annuel => "annuel",
});
enum_texte!(StatutRendezVous {
    Planifie => "planifie", Confirme => "confirme",
    Honore => "honore", Annule => "annule", Reporte => "reporte",
});
// Jalon : date clé du projet. Autonome, sans lien agenda (barrière spec).
enum_texte!(StatutJalon {
    AVenir => "a_venir", Atteint => "atteint", Manque => "manque",
});
// Livrable : ce que le projet doit produire.
enum_texte!(StatutLivrable {
    AProduire => "a_produire", EnCours => "en_cours",
    Livre => "livre", Accepte => "accepte", Refuse => "refuse",
});
enum_texte!(StatutProjet {
    Planifie => "planifie", EnCours => "en_cours", Suspendu => "suspendu", Cloture => "cloture",
});
enum_texte!(StatutTache {
    AFaire => "a_faire", EnCours => "en_cours", Bloquee => "bloquee", Terminee => "terminee",
});
enum_texte!(TypeRessource {
    Materiel => "materiel", Budget => "budget", SousTraitance => "sous_traitance",
});
enum_texte!(TypeIntervenant { Interne => "interne", Externe => "externe" });
enum_texte!(TypeTaux { Horaire => "horaire", Journalier => "journalier", Forfait => "forfait" });
