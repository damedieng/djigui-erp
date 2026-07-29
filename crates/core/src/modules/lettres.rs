//! Montant en toutes lettres — mention obligatoire sur une facture (N1 OHADA).
//!
//! # Pourquoi cette mention existe
//!
//! Elle protège contre l'altération d'un chiffre : ajouter un zéro à
//! « 125 000 » est l'affaire d'un instant, réécrire « cent vingt-cinq mille
//! francs CFA » pour qu'il dise autre chose ne l'est pas. Les deux écritures se
//! contredisent alors, et c'est **la mention en lettres qui fait foi** en cas de
//! litige. D'où une exigence : elle doit être **exacte**, pas approchante.
//!
//! # Aucune dépendance, aucune base
//!
//! Fonction pure : mêmes entrées, même sortie, toujours. Elle se teste
//! entièrement hors ligne, et c'est ce qui permet de couvrir les cas tordus de
//! l'orthographe française — qui sont nombreux.
//!
//! # Les règles françaises, qui sont des pièges
//!
//! | Règle | Exemple juste | Erreur courante |
//! |---|---|---|
//! | `vingt` et `cent` prennent un `s` **multipliés ET terminaux** | quatre-vingt**s** · deux cent**s** | quatre-vingt**s** un ❌ |
//! | …mais restent invariables s'ils sont suivis | quatre-vingt-un · deux cent trois | deux cent**s** trois ❌ |
//! | `mille` est **toujours** invariable | deux mille | deux mille**s** ❌ |
//! | « et un » de 21 à 71, **jamais** à 81 ni 91 | vingt **et** un · quatre-vingt-un | quatre-vingt **et** un ❌ |
//! | 71 et 91 se disent sur 60 et 80 | soixante **et onze** · quatre-vingt-**onze** | septante ❌ |
//! | `million`/`milliard` sont des **noms** : ils s'accordent | deux million**s** | deux million ❌ |
//!
//! On applique l'orthographe **traditionnelle** (« deux cent trois »), pas la
//! rectifiée de 1990 qui met des traits d'union partout : c'est celle qu'on lit
//! sur les documents commerciaux et bancaires de la zone.

/// Écrit un montant en toutes lettres, prêt à figurer sur une facture.
///
/// `devise` est le code affiché ailleurs sur la pièce (« FCFA », « EUR »…) ;
/// on en déduit le nom à écrire en toutes lettres.
///
/// ⚠️ **Arrondi au centime, puis à l'unité en franc CFA.** Le franc CFA n'a pas
/// de subdivision en circulation : écrire « et vingt-cinq centimes » sur une
/// facture sénégalaise n'aurait aucun sens et ferait douter du total.
pub fn montant_en_lettres(montant: f64, devise: &str) -> String {
    let (unite, sous_unite, decimales) = nom_devise(devise);

    // Le signe est porté par un mot, jamais par un « - » qui passerait
    // inaperçu : un avoir doit se lire comme un avoir.
    let negatif = montant < 0.0;
    let absolu = montant.abs();

    let (entier, centimes) = if decimales == 0 {
        (absolu.round() as u64, 0u64)
    } else {
        // On arrondit AU CENTIME d'abord, sinon 12.999 donnerait 12 et 99.
        let total_centimes = (absolu * 100.0).round() as u64;
        (total_centimes / 100, total_centimes % 100)
    };

    let mut texte = String::new();
    if negatif {
        texte.push_str("moins ");
    }
    texte.push_str(&nombre_en_lettres(entier));
    texte.push(' ');
    texte.push_str(if entier > 1 { unite.1 } else { unite.0 });

    if centimes > 0 {
        texte.push_str(" et ");
        texte.push_str(&nombre_en_lettres(centimes));
        texte.push(' ');
        texte.push_str(if centimes > 1 { sous_unite.1 } else { sous_unite.0 });
    }
    texte
}

/// (singulier, pluriel) de l'unité, de la sous-unité, et nombre de décimales.
fn nom_devise(code: &str) -> ((&'static str, &'static str), (&'static str, &'static str), u8) {
    match code.trim().to_uppercase().as_str() {
        // Le franc CFA ne circule pas en centimes : 0 décimale.
        "FCFA" | "XOF" | "XAF" | "CFA" | "F CFA" => {
            (("franc CFA", "francs CFA"), ("centime", "centimes"), 0)
        }
        "EUR" | "€" => (("euro", "euros"), ("centime", "centimes"), 2),
        "USD" | "$" => (("dollar", "dollars"), ("cent", "cents"), 2),
        "MAD" => (("dirham", "dirhams"), ("centime", "centimes"), 2),
        "GNF" => (("franc guinéen", "francs guinéens"), ("centime", "centimes"), 0),
        // Devise inconnue : on garde le code tel quel plutôt que d'inventer un
        // nom. Mieux vaut « cent vingt mille XYZ » qu'un mot faux sur une pièce
        // qui fait foi.
        _ => {
            let garde: &'static str = Box::leak(code.trim().to_string().into_boxed_str());
            ((garde, garde), ("centime", "centimes"), 0)
        }
    }
}

const UNITES: [&str; 20] = [
    "zéro", "un", "deux", "trois", "quatre", "cinq", "six", "sept", "huit", "neuf", "dix", "onze",
    "douze", "treize", "quatorze", "quinze", "seize", "dix-sept", "dix-huit", "dix-neuf",
];

/// Écrit un entier positif en toutes lettres.
pub fn nombre_en_lettres(n: u64) -> String {
    if n == 0 {
        return "zéro".into();
    }
    // Les paliers sont des NOMS : ils s'accordent (deux millions), contrairement
    // à « mille » qui est un adverbe multiplicatif et reste invariable.
    const PALIERS: [(u64, &str, &str); 5] = [
        (1_000_000_000_000_000_000, "trillion", "trillions"),
        (1_000_000_000_000, "billion", "billions"),
        (1_000_000_000, "milliard", "milliards"),
        (1_000_000, "million", "millions"),
        (1_000, "mille", "mille"),
    ];

    let mut reste = n;
    let mut morceaux: Vec<String> = Vec::new();

    for (valeur, singulier, pluriel) in PALIERS {
        if reste >= valeur {
            let combien = reste / valeur;
            reste %= valeur;
            if valeur == 1_000 {
                // « mille » et non « un mille » ; toujours invariable.
                if combien == 1 {
                    morceaux.push("mille".into());
                } else {
                    // ⚠️ `accord = false` : `mille` est un ADJECTIF NUMÉRAL, et
                    // `cent`/`vingt` restent invariables devant lui.
                    // « deux cent mille », « quatre-vingt mille ».
                    morceaux.push(format!("{} mille", sous_mille(combien, false)));
                }
            } else if combien == 1 {
                morceaux.push(format!("un {singulier}"));
            } else {
                // ⚠️ `accord = true` : `million`/`milliard` sont des NOMS, et
                // `cent`/`vingt` s'accordent devant eux.
                // « deux cents millions », « quatre-vingts millions ».
                morceaux.push(format!("{} {pluriel}", sous_mille_ou_plus(combien)));
            }
        }
    }
    if reste > 0 {
        // Dernier morceau du nombre : rien ne suit, l'accord est possible.
        morceaux.push(sous_mille(reste, true));
    }
    morceaux.join(" ")
}

/// Pour les multiplicateurs qui peuvent dépasser mille (ex. « deux mille trois
/// cents millions »).
fn sous_mille_ou_plus(n: u64) -> String {
    if n < 1000 {
        sous_mille(n, true)
    } else {
        nombre_en_lettres(n)
    }
}

/// Écrit un nombre de 1 à 999.
///
/// `accord` dit si `cent` et `vingt` peuvent prendre leur `s` : c'est le cas
/// quand rien ne les suit, ou quand ce qui suit est un **nom** (million,
/// milliard). C'est FAUX devant `mille`, qui est un adjectif numéral.
fn sous_mille(n: u64, accord: bool) -> String {
    debug_assert!(n < 1000);
    let centaines = n / 100;
    let reste = n % 100;

    let mut morceaux: Vec<String> = Vec::new();
    if centaines > 0 {
        if centaines == 1 {
            morceaux.push("cent".into());
        } else {
            // ⚠️ « cent » prend un `s` quand il est multiplié ET qu'il termine
            // le nombre : « deux cents », mais « deux cent trois ».
            morceaux.push(format!(
                "{} cent{}",
                UNITES[centaines as usize],
                if reste == 0 && accord { "s" } else { "" }
            ));
        }
    }
    if reste > 0 {
        morceaux.push(dizaines(reste, accord));
    }
    morceaux.join(" ")
}

/// Écrit un nombre de 1 à 99 — c'est ici que vivent tous les pièges.
///
/// `accord` : voir `sous_mille`. Il décide du `s` de « quatre-vingts ».
fn dizaines(n: u64, accord: bool) -> String {
    debug_assert!(n > 0 && n < 100);
    if n < 20 {
        return UNITES[n as usize].into();
    }
    let d = n / 10;
    let u = n % 10;

    match d {
        2..=6 => {
            let base = ["", "", "vingt", "trente", "quarante", "cinquante", "soixante"][d as usize];
            match u {
                0 => base.into(),
                // « et un » de 21 à 61. Le trait d'union ne s'y met pas.
                1 => format!("{base} et un"),
                _ => format!("{base}-{}", UNITES[u as usize]),
            }
        }
        // 70-79 se construit sur soixante : soixante-dix, soixante et onze…
        7 => match u {
            0 => "soixante-dix".into(),
            // ⚠️ 71 = « soixante et onze », avec « et », comme 21 ou 31.
            1 => "soixante et onze".into(),
            _ => format!("soixante-{}", UNITES[(10 + u) as usize]),
        },
        // 80-89 : « quatre-vingts » avec `s` SEULEMENT à 80 pile.
        8 => match u {
            // ⚠️ « quatre-vingts » avec `s` seulement si rien ne suit :
            // « quatre-vingts francs », mais « quatre-vingt mille ».
            0 => if accord { "quatre-vingts".into() } else { "quatre-vingt".to_string() },
            // ⚠️ Jamais « quatre-vingt ET un » : 81 se dit « quatre-vingt-un ».
            _ => format!("quatre-vingt-{}", UNITES[u as usize]),
        },
        // 90-99 se construit sur quatre-vingt, jamais de `s`, jamais de « et ».
        9 => format!("quatre-vingt-{}", UNITES[(10 + u) as usize]),
        _ => unreachable!("dizaine hors intervalle"),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_petits_nombres() {
        assert_eq!(nombre_en_lettres(0), "zéro");
        assert_eq!(nombre_en_lettres(1), "un");
        assert_eq!(nombre_en_lettres(7), "sept");
        assert_eq!(nombre_en_lettres(16), "seize");
        assert_eq!(nombre_en_lettres(17), "dix-sept");
    }

    /// Les pièges de l'orthographe française, un par un. Chacun de ces cas a
    /// une erreur courante associée — c'est pour ça qu'ils sont tous là.
    #[test]
    fn les_pieges_du_francais() {
        // « et un » de 21 à 71…
        assert_eq!(nombre_en_lettres(21), "vingt et un");
        assert_eq!(nombre_en_lettres(31), "trente et un");
        assert_eq!(nombre_en_lettres(61), "soixante et un");
        assert_eq!(nombre_en_lettres(71), "soixante et onze");
        // …mais JAMAIS à 81 ni 91.
        assert_eq!(nombre_en_lettres(81), "quatre-vingt-un");
        assert_eq!(nombre_en_lettres(91), "quatre-vingt-onze");

        // 70 et 90 se construisent sur 60 et 80 (français de France, pas
        // « septante » / « nonante »).
        assert_eq!(nombre_en_lettres(70), "soixante-dix");
        assert_eq!(nombre_en_lettres(77), "soixante-dix-sept");
        assert_eq!(nombre_en_lettres(90), "quatre-vingt-dix");
        assert_eq!(nombre_en_lettres(99), "quatre-vingt-dix-neuf");

        // « quatre-vingts » : `s` UNIQUEMENT à 80 pile.
        assert_eq!(nombre_en_lettres(80), "quatre-vingts");
        assert_eq!(nombre_en_lettres(82), "quatre-vingt-deux");
        assert_eq!(nombre_en_lettres(180), "cent quatre-vingts");

        // « cent » : `s` s'il est multiplié ET terminal.
        assert_eq!(nombre_en_lettres(100), "cent");
        assert_eq!(nombre_en_lettres(200), "deux cents");
        assert_eq!(nombre_en_lettres(203), "deux cent trois");
        assert_eq!(nombre_en_lettres(180_000), "cent quatre-vingt mille");

        // « mille » est TOUJOURS invariable.
        assert_eq!(nombre_en_lettres(1_000), "mille");
        assert_eq!(nombre_en_lettres(2_000), "deux mille");
        assert_eq!(nombre_en_lettres(200_000), "deux cent mille");

        // ⚠️⚠️ LES DEUX BUGS DE LA PREMIÈRE VERSION, attrapés par ces tests.
        // `cent` et `vingt` restent INVARIABLES devant `mille` (adjectif
        // numéral) mais S'ACCORDENT devant `million`/`milliard` (noms).
        assert_eq!(nombre_en_lettres(180_000), "cent quatre-vingt mille");
        assert_eq!(nombre_en_lettres(80_000), "quatre-vingt mille");
        assert_eq!(nombre_en_lettres(500_000), "cinq cent mille");
        assert_eq!(nombre_en_lettres(300_000), "trois cent mille");
        // …et devant un nom, l'accord revient.
        assert_eq!(nombre_en_lettres(80_000_000), "quatre-vingts millions");
        assert_eq!(nombre_en_lettres(500_000_000), "cinq cents millions");
        assert_eq!(nombre_en_lettres(200_000_000_000), "deux cents milliards");
        // Le mélange des deux dans un même nombre.
        assert_eq!(
            nombre_en_lettres(80_080_080),
            "quatre-vingts millions quatre-vingt mille quatre-vingts"
        );

        // million / milliard sont des noms : ils s'accordent.
        assert_eq!(nombre_en_lettres(1_000_000), "un million");
        assert_eq!(nombre_en_lettres(2_000_000), "deux millions");
        assert_eq!(nombre_en_lettres(1_000_000_000), "un milliard");
        assert_eq!(nombre_en_lettres(3_000_000_000), "trois milliards");
    }

    #[test]
    fn les_montants_realistes_du_terrain() {
        assert_eq!(
            montant_en_lettres(125_000.0, "FCFA"),
            "cent vingt-cinq mille francs CFA"
        );
        assert_eq!(
            montant_en_lettres(1_500_000.0, "FCFA"),
            "un million cinq cent mille francs CFA"
        );
        assert_eq!(
            montant_en_lettres(6_903.0, "FCFA"),
            "six mille neuf cent trois francs CFA"
        );
        // Un marché de travaux : l'ordre de grandeur où une faute se paie cher.
        assert_eq!(
            montant_en_lettres(285_450_000.0, "FCFA"),
            "deux cent quatre-vingt-cinq millions quatre cent cinquante mille francs CFA"
        );
    }

    /// ⚠️ Le franc CFA ne circule PAS en centimes : écrire « et cinquante
    /// centimes » sur une facture sénégalaise ferait douter du total.
    #[test]
    fn le_franc_cfa_n_a_pas_de_centimes() {
        assert_eq!(montant_en_lettres(1_000.49, "FCFA"), "mille francs CFA");
        assert_eq!(montant_en_lettres(1_000.5, "FCFA"), "mille un francs CFA");
        assert!(!montant_en_lettres(999.99, "FCFA").contains("centime"));
    }

    #[test]
    fn les_devises_a_centimes_les_ecrivent() {
        assert_eq!(montant_en_lettres(12.34, "EUR"), "douze euros et trente-quatre centimes");
        assert_eq!(montant_en_lettres(1.0, "EUR"), "un euro");
        assert_eq!(montant_en_lettres(1.01, "EUR"), "un euro et un centime");
        assert_eq!(montant_en_lettres(2.0, "USD"), "deux dollars");
    }

    /// L'arrondi doit se faire AU CENTIME avant de séparer : sinon 12,999
    /// donnerait « douze euros et quatre-vingt-dix-neuf centimes » au lieu de
    /// treize euros.
    #[test]
    fn l_arrondi_se_fait_avant_la_separation() {
        assert_eq!(montant_en_lettres(12.999, "EUR"), "treize euros");
        assert_eq!(montant_en_lettres(0.999, "EUR"), "un euro");
    }

    #[test]
    fn le_singulier_et_le_pluriel_de_l_unite() {
        assert_eq!(montant_en_lettres(1.0, "FCFA"), "un franc CFA");
        assert_eq!(montant_en_lettres(2.0, "FCFA"), "deux francs CFA");
        // Zéro prend le singulier (« zéro franc »), pas le pluriel.
        assert_eq!(montant_en_lettres(0.0, "FCFA"), "zéro franc CFA");
    }

    /// Un avoir est un montant négatif : il doit se LIRE comme tel. Un simple
    /// « - » en tête passerait inaperçu sur une pièce imprimée.
    #[test]
    fn un_avoir_se_lit_comme_un_avoir() {
        assert_eq!(montant_en_lettres(-5_000.0, "FCFA"), "moins cinq mille francs CFA");
    }

    /// Une devise qu'on ne connaît pas : on garde le code plutôt que d'inventer
    /// un nom faux sur une pièce qui fait foi.
    #[test]
    fn une_devise_inconnue_garde_son_code() {
        assert_eq!(montant_en_lettres(100.0, "ZZZ"), "cent ZZZ");
    }

    /// Filet de sécurité : aucun montant plausible ne doit faire paniquer la
    /// fonction ni produire de texte vide — elle imprime une mention légale.
    #[test]
    fn aucun_montant_plausible_ne_casse() {
        let mut n: u64 = 1;
        while n < 900_000_000_000_000 {
            let t = montant_en_lettres(n as f64, "FCFA");
            assert!(!t.is_empty(), "texte vide pour {n}");
            assert!(!t.contains("  "), "double espace pour {n} : {t}");
            assert!(!t.starts_with(' ') && !t.ends_with(' '), "espace en bord pour {n}");
            n = n.saturating_mul(7).saturating_add(13);
        }
        for m in [0.0, 0.4, 999.5, 1_000_000.0, f64::from(i32::MAX)] {
            assert!(!montant_en_lettres(m, "FCFA").is_empty());
        }
    }
}
