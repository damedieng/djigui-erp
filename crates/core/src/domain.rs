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
// Nature comptable OHADA d'un article (migration 0032). Pilote à la fois les
// listes des écrans (ce qui se vend / ce qui se consomme) et, à terme, les
// comptes employés : marchandise 601/701/31, matière première 602/32,
// produit fini 702/36 (+ 73 production stockée), service 706.
// Un négociant et un fabricant ne se comptabilisent pas pareil : c'est ce champ
// qui fait la différence.
enum_texte!(NatureComptable {
    Marchandise => "marchandise", MatierePremiere => "matiere_premiere",
    ProduitFini => "produit_fini", Service => "service",
});
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
// Ordre de fabrication. Le stock n'est touché qu'au passage en `termine`
// (sorties des composants + entrée du produit fini) — voir migration 0031.
enum_texte!(StatutOrdreProduction {
    Brouillon => "brouillon", EnCours => "en_cours",
    Termine => "termine", Annule => "annule",
});
enum_texte!(TypeIntervenant{ Interne => "interne", Externe => "externe" });
enum_texte!(TypeTaux { Horaire => "horaire", Journalier => "journalier", Forfait => "forfait" });

// --- Comptabilité (migration 0034) -----------------------------------------
// Djigui ne devine rien : le comptable crée ses comptes et écrit ses règles.
// Ces énumérations décrivent le vocabulaire de SON écran, pas une norme imposée.

// Place qu'un compte occupe dans une écriture. Le moteur connaît le schéma de
// chaque opération ; la règle du comptable ne fait que NOMMER les comptes.
enum_texte!(RoleCompte {
    Produit => "produit", Charge => "charge", Tiers => "tiers",
    Taxe => "taxe", Tresorerie => "tresorerie", Stock => "stock",
});
// Nature de l'opération à rattacher — sert de critère de règle et choisit le
// journal par défaut (vente → VT, achat → AC, encaissement → CA ou BQ…).
enum_texte!(DomaineComptable {
    Vente => "vente", Achat => "achat", Encaissement => "encaissement",
    Decaissement => "decaissement", Stock => "stock",
});
// Sens habituel du solde d'un compte. Indicatif : signale un solde anormal
// dans la balance, ne refuse jamais une écriture.
enum_texte!(SensCompte { Debit => "debit", Credit => "credit" });
// Pièce dont l'écriture est issue. `Manuel` = saisie directe du comptable ;
// une écriture n'est jamais modifiée, elle est contre-passée.
enum_texte!(OrigineEcriture {
    Document => "document", Paiement => "paiement", Mouvement => "mouvement",
    Manuel => "manuel", Contrepassation => "contrepassation",
});
