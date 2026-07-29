-- Djigui Desktop — migration 0044 : PAIE, SOCLE DES PARAMÈTRES LÉGAUX.
--
-- ⚠️⚠️ RÈGLE ABSOLUE DU MODULE PAIE : **AUCUN taux, plafond, tranche ou montant
-- légal n'est écrit dans le code.** Tout vit ici, en données. Le moteur de
-- calcul lit ces tables ; il ne connaît aucun chiffre.
--
-- Demande de l'utilisateur, mot pour mot : « il faut prévoir des interfaces qui
-- permettent de paramétrer cela, car la législation peut changer ». Chaque
-- table de cette migration a donc son écran (« Paramètres de paie »).
--
-- # Pourquoi le versionnement par période est indispensable
--
-- Les paramètres sociaux et fiscaux changent à chaque loi de finances. Si l'on
-- écrasait un taux, **tous les bulletins passés se recalculeraient faux** : un
-- bulletin de janvier réimprimé en juin utiliserait les taux de juin. Chaque
-- ligne porte donc `date_debut` / `date_fin`.
--
-- ⚠️ **On ne modifie JAMAIS un taux : on ferme sa période et on en ouvre une
-- nouvelle.** L'écran ne propose d'ailleurs pas de « modifier », mais
-- « nouvelle période » — le geste protège l'historique par construction.
--
-- # Les valeurs de départ ne sont PAS certifiées
--
-- Elles viennent de l'annexe A de la spécification, qui les donne pour
-- indicatives et signale des divergences entre sources publiques (plafond IPRES
-- 360 000 ou 432 000 ? abattement plafonné à 900 000 ou 1 800 000 ? TRIMF
-- mensuelle ou annuelle ?). D'où la colonne `a_verifier` : tant qu'elle vaut 1,
-- l'écran affiche un bandeau d'avertissement. **Ce n'est pas un détail** — un
-- bulletin faux se paie en redressement, et le commerçant ne peut pas deviner
-- que le logiciel a inventé un chiffre.

-- ---------------------------------------------------------------------------
-- Cotisations sociales : IPRES, CSS, IPM
-- ---------------------------------------------------------------------------
CREATE TABLE ref_parametres_sociaux (
    id              TEXT PRIMARY KEY,
    -- ipres_rg  : retraite, régime général — tout le monde
    -- ipres_rcc : retraite complémentaire — CADRES uniquement
    -- css_pf    : prestations familiales — patronal exclusif
    -- css_at    : accident du travail    — patronal, taux selon le secteur
    -- ipm       : couverture maladie
    organisme       TEXT NOT NULL
                    CHECK (organisme IN ('ipres_rg','ipres_rcc','css_pf','css_at','ipm')),
    libelle         TEXT NOT NULL,
    taux_salarial   NUMERIC NOT NULL DEFAULT 0,
    taux_patronal   NUMERIC NOT NULL DEFAULT 0,
    -- Assiette plafonnée. NULL = pas de plafond.
    plafond_mensuel NUMERIC,
    -- L'IPRES RCC ne s'applique qu'aux cadres, et seulement sur la fraction
    -- comprise entre le plafond du régime général et le sien.
    reserve_cadre   INTEGER NOT NULL DEFAULT 0,
    date_debut      TEXT NOT NULL,
    date_fin        TEXT,
    a_verifier      INTEGER NOT NULL DEFAULT 1,
    note            TEXT
);
CREATE INDEX idx_ref_social_periode ON ref_parametres_sociaux(organisme, date_debut);

-- ---------------------------------------------------------------------------
-- Barème de l'impôt sur le revenu — tranches ANNUELLES
-- ---------------------------------------------------------------------------
--
-- ⚠️ Les bornes sont **annuelles**. Le moteur annualise le net imposable
-- mensuel (× 12), applique le barème, puis redivise par 12. Appliquer un barème
-- annuel à un salaire mensuel donnerait un impôt douze fois trop faible.
CREATE TABLE ref_bareme_irpp (
    id          TEXT PRIMARY KEY,
    borne_inf   NUMERIC NOT NULL,
    borne_sup   NUMERIC,          -- NULL = dernière tranche, sans plafond
    taux        NUMERIC NOT NULL,
    date_debut  TEXT NOT NULL,
    date_fin    TEXT,
    a_verifier  INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX idx_ref_irpp_periode ON ref_bareme_irpp(date_debut, borne_inf);

-- ---------------------------------------------------------------------------
-- Réduction pour charge de famille
-- ---------------------------------------------------------------------------
--
-- ⚠️⚠️ **CE N'EST PAS LE QUOTIENT FAMILIAL FRANÇAIS.** On ne divise pas le
-- revenu par les parts. Le Sénégal calcule l'IR sur le revenu ENTIER, puis
-- **soustrait** une réduction de l'IR brut. Cette réduction est un pourcentage
-- **borné par un plancher et un plafond** — un taux seul ne suffirait pas.
CREATE TABLE ref_reductions_famille (
    id             TEXT PRIMARY KEY,
    nb_parts       NUMERIC NOT NULL,
    taux_reduction NUMERIC NOT NULL,
    reduction_min  NUMERIC NOT NULL DEFAULT 0,   -- plancher ANNUEL
    reduction_max  NUMERIC NOT NULL DEFAULT 0,   -- plafond ANNUEL
    date_debut     TEXT NOT NULL,
    date_fin       TEXT,
    a_verifier     INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX idx_ref_famille_periode ON ref_reductions_famille(date_debut, nb_parts);

-- ---------------------------------------------------------------------------
-- TRIMF — taxe représentative de l'impôt du minimum fiscal
-- ---------------------------------------------------------------------------
--
-- Forfait par tranche de rémunération, multiplié par le nombre de parts TRIMF
-- (le salarié + ses épouses non salariées).
--
-- ⚠️ `periodicite` est un CHAMP et non une hypothèse : les sources publiques
-- divergent sur le caractère mensuel ou annuel du barème, et la fourchette
-- citée va de 900 à 36 000 F. Deviner ferait un bulletin faux pour tout le
-- monde. L'écran pose la question explicitement.
CREATE TABLE ref_trimf (
    id          TEXT PRIMARY KEY,
    borne_inf   NUMERIC NOT NULL,
    borne_sup   NUMERIC,
    montant     NUMERIC NOT NULL,
    periodicite TEXT NOT NULL DEFAULT 'annuel' CHECK (periodicite IN ('mensuel','annuel')),
    date_debut  TEXT NOT NULL,
    date_fin    TEXT,
    a_verifier  INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX idx_ref_trimf_periode ON ref_trimf(date_debut, borne_inf);

-- ---------------------------------------------------------------------------
-- Primes réglementées et avantages en nature
-- ---------------------------------------------------------------------------
--
-- Deux usages dans une même table, parce qu'ils se paramètrent pareil :
--   • plafond d'exonération d'une prime (transport…) ;
--   • mode d'évaluation d'un avantage en nature (logement, véhicule, nourriture).
CREATE TABLE ref_primes_reglementaires (
    id                  TEXT PRIMARY KEY,
    code                TEXT NOT NULL,
    libelle             TEXT NOT NULL,
    -- 'exoneration' : la prime est exonérée jusqu'au plafond
    -- 'avantage'    : l'avantage est évalué et AJOUTÉ au brut
    usage_prime         TEXT NOT NULL DEFAULT 'exoneration'
                        CHECK (usage_prime IN ('exoneration','avantage')),
    plafond_exoneration NUMERIC,
    -- 'forfait' (valeur en francs) | 'pourcentage' (valeur en % du brut)
    mode_evaluation     TEXT NOT NULL DEFAULT 'forfait'
                        CHECK (mode_evaluation IN ('forfait','pourcentage')),
    valeur              NUMERIC NOT NULL DEFAULT 0,
    date_debut          TEXT NOT NULL,
    date_fin            TEXT,
    a_verifier          INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX idx_ref_prime_periode ON ref_primes_reglementaires(code, date_debut);

-- ---------------------------------------------------------------------------
-- Abattement forfaitaire pour frais professionnels
-- ---------------------------------------------------------------------------
--
-- ⚠️ `cumul_avec_ipres` : point de doctrine NON TRANCHÉ, signalé par la spec.
-- Selon la lecture du CGI, l'abattement de 30 % et la déduction des cotisations
-- IPRES peuvent NE PAS se cumuler (l'abattement représentant déjà la part
-- retraite). C'est un **interrupteur**, pas une décision prise à votre place :
-- il change le net de TOUS les salariés.
CREATE TABLE ref_abattement_frais_pro (
    id               TEXT PRIMARY KEY,
    taux             NUMERIC NOT NULL,
    plafond_annuel   NUMERIC,
    cumul_avec_ipres INTEGER NOT NULL DEFAULT 1,
    date_debut       TEXT NOT NULL,
    date_fin         TEXT,
    a_verifier       INTEGER NOT NULL DEFAULT 1
);

-- ---------------------------------------------------------------------------
-- Paramètres de l'employeur (singleton)
-- ---------------------------------------------------------------------------
CREATE TABLE parametres_entreprise_paie (
    singleton            INTEGER PRIMARY KEY CHECK (singleton = 1),
    -- Le taux « accident du travail » dépend du SECTEUR DE RISQUE de
    -- l'entreprise (1 % à 5 %) : il ne peut pas venir d'une table nationale,
    -- il est propre à chaque employeur.
    secteur_risque_at    TEXT,
    taux_at_retenu       NUMERIC NOT NULL DEFAULT 0,
    numero_ipres         TEXT,
    numero_css           TEXT,
    ipm_nom              TEXT,
    numero_ipm           TEXT,
    -- Base de proratisation des absences. 26 est l'usage courant, mais c'est
    -- un choix d'entreprise : il figure au règlement intérieur.
    jours_ouvrables_mois INTEGER NOT NULL DEFAULT 26,
    -- Heure supplémentaire : majorations. Elles relèvent de la convention
    -- collective, donc paramétrables comme le reste.
    majoration_hs1       NUMERIC NOT NULL DEFAULT 15,
    majoration_hs2       NUMERIC NOT NULL DEFAULT 40,
    majoration_nuit      NUMERIC NOT NULL DEFAULT 60,
    majoration_ferie     NUMERIC NOT NULL DEFAULT 100,
    -- CFCE : contribution forfaitaire à la charge de l'employeur.
    taux_cfce            NUMERIC NOT NULL DEFAULT 3,
    maj_le               TEXT NOT NULL
);

INSERT INTO parametres_entreprise_paie (singleton, maj_le) VALUES (1, datetime('now'));

-- ---------------------------------------------------------------------------
-- VALEURS DE DÉPART — ⚠️ INDICATIVES, TOUTES MARQUÉES « À VÉRIFIER »
-- ---------------------------------------------------------------------------
--
-- Elles servent à ce que l'écran ne soit pas vide au premier démarrage et à ce
-- que le moteur soit testable. Elles ne valent PAS validation : `a_verifier = 1`
-- fait afficher un bandeau tant que l'utilisateur ne les a pas confrontées au
-- CGI en vigueur et au simulateur de la DGID.

-- Barème IR (bornes annuelles).
INSERT INTO ref_bareme_irpp (id, borne_inf, borne_sup, taux, date_debut) VALUES
 ('ir-1',         0,   630000,  0, '2026-01-01'),
 ('ir-2',    630001,  1500000, 20, '2026-01-01'),
 ('ir-3',   1500001,  4000000, 30, '2026-01-01'),
 ('ir-4',   4000001,  8000000, 35, '2026-01-01'),
 ('ir-5',   8000001, 13500000, 37, '2026-01-01'),
 ('ir-6',  13500001,     NULL, 40, '2026-01-01');

-- Réduction pour charge de famille (min/max ANNUELS).
INSERT INTO ref_reductions_famille (id, nb_parts, taux_reduction, reduction_min, reduction_max, date_debut) VALUES
 ('rf-10', 1.0,  0,      0,       0, '2026-01-01'),
 ('rf-15', 1.5, 10, 100000,  300000, '2026-01-01'),
 ('rf-20', 2.0, 15, 200000,  650000, '2026-01-01'),
 ('rf-25', 2.5, 20, 300000, 1100000, '2026-01-01'),
 ('rf-30', 3.0, 25, 400000, 1650000, '2026-01-01'),
 ('rf-35', 3.5, 30, 500000, 2030000, '2026-01-01'),
 ('rf-40', 4.0, 35, 600000, 2490000, '2026-01-01'),
 ('rf-45', 4.5, 40, 700000, 2755000, '2026-01-01'),
 ('rf-50', 5.0, 45, 800000, 3180000, '2026-01-01');

-- Cotisations. ⚠️ Les plafonds sont les points de divergence les plus cités.
INSERT INTO ref_parametres_sociaux
  (id, organisme, libelle, taux_salarial, taux_patronal, plafond_mensuel, reserve_cadre, date_debut, note) VALUES
 ('so-ipres-rg', 'ipres_rg', 'IPRES — régime général', 5.6, 8.4, 432000, 0, '2026-01-01',
  'Plafond mensuel : 360 000 ou 432 000 selon la source. À CONFIRMER auprès de l''IPRES.'),
 ('so-ipres-rcc','ipres_rcc','IPRES — retraite complémentaire des cadres', 2.4, 3.6, 1296000, 1, '2026-01-01',
  'Ne s''applique QU''AUX CADRES, sur la fraction entre le plafond du régime général et celui-ci.'),
 ('so-css-pf',   'css_pf',   'CSS — prestations familiales', 0, 7, 63000, 0, '2026-01-01',
  'Patronal exclusif.'),
 ('so-css-at',   'css_at',   'CSS — accident du travail', 0, 1, 63000, 0, '2026-01-01',
  'Patronal exclusif. Le taux réel (1 % à 5 %) dépend du SECTEUR DE RISQUE : il se règle dans « Mon entreprise ».'),
 ('so-ipm',      'ipm',      'IPM — couverture maladie', 3, 3, 250000, 0, '2026-01-01',
  'Varie selon l''IPM de rattachement : à caler sur votre convention.');

-- TRIMF. ⚠️ Périodicité et montants NON CERTIFIÉS — voir le commentaire de la table.
INSERT INTO ref_trimf (id, borne_inf, borne_sup, montant, periodicite, date_debut) VALUES
 ('tr-1',       0,   599999,   900, 'annuel', '2026-01-01'),
 ('tr-2',  600000,   999999,  3600, 'annuel', '2026-01-01'),
 ('tr-3', 1000000,  1999999,  4800, 'annuel', '2026-01-01'),
 ('tr-4', 2000000,  6999999, 12000, 'annuel', '2026-01-01'),
 ('tr-5', 7000000, 11999999, 18000, 'annuel', '2026-01-01'),
 ('tr-6',12000000,     NULL, 36000, 'annuel', '2026-01-01');

-- Abattement pour frais professionnels.
INSERT INTO ref_abattement_frais_pro (id, taux, plafond_annuel, cumul_avec_ipres, date_debut) VALUES
 ('ab-1', 30, 900000, 1, '2026-01-01');

-- Primes et avantages.
INSERT INTO ref_primes_reglementaires
  (id, code, libelle, usage_prime, plafond_exoneration, mode_evaluation, valeur, date_debut) VALUES
 ('pr-transport', 'transport', 'Prime de transport', 'exoneration', 20800, 'forfait', 0, '2026-01-01'),
 ('pr-logement',  'av_logement',  'Avantage en nature — logement',  'avantage', NULL, 'pourcentage', 15, '2026-01-01'),
 ('pr-vehicule',  'av_vehicule',  'Avantage en nature — véhicule',  'avantage', NULL, 'forfait',  25000, '2026-01-01'),
 ('pr-nourriture','av_nourriture','Avantage en nature — nourriture','avantage', NULL, 'forfait',  20000, '2026-01-01');

-- ---------------------------------------------------------------------------
-- Le module au catalogue
-- ---------------------------------------------------------------------------
--
-- `souscrit = 0` : c'est l'écran d'installation qui ouvre les droits selon la
-- formule vendue. Le module apparaît malgré tout dans la VITRINE de l'écran
-- Modules — grisé — pour montrer ce que le système sait faire.
INSERT INTO module (code, libelle, description, icone, famille, ordre, socle, souscrit, actif, requiert)
VALUES ('paie', 'Paie & RH',
        'Salariés, contrats et bulletins de paie conformes au droit social sénégalais.',
        'ti-users-group', 'Pilotage', 45, 0, 0, 1, 'socle');
