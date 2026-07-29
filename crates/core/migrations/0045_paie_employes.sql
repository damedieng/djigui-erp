-- Djigui Desktop — migration 0045 : PAIE, SALARIÉS ET CONTRATS.
--
-- # Un salarié n'est pas un tiers
--
-- On aurait pu réutiliser la table `tiers`. C'eût été une erreur : un salarié
-- porte une situation de famille, un contrat, une ancienneté, un statut cadre
-- et des cotisations sociales — rien de tout cela n'a de sens pour un client ou
-- un fournisseur. Surtout, les deux ont des **cycles de vie et des règles
-- d'accès différents** : la fiche de paie d'un employé n'a pas à être visible du
-- caissier qui consulte les tiers toute la journée.
--
-- # Ce qui est CALCULÉ et jamais stocké
--
-- ⚠️ **Le nombre de parts fiscales n'est pas une colonne.** Il se déduit de la
-- situation matrimoniale et du nombre d'enfants (règle du CGI, plafonnée à 5
-- parts). Le stocker créerait une valeur qui cesserait d'être vraie dès la
-- naissance d'un enfant, sans que personne ne s'en aperçoive. Il est calculé
-- dans une fonction isolée et testable — la règle exacte doit pouvoir être
-- recalée sur le simulateur de la DGID sans toucher au reste.
--
-- Même raisonnement pour l'ancienneté : elle se déduit de `date_embauche`.
--
-- # Un seul contrat actif à la fois
--
-- Un salarié peut avoir plusieurs contrats successifs (un CDD prolongé, une
-- titularisation) : l'historique compte, notamment pour l'ancienneté et en cas
-- de litige. Mais **un seul peut être actif**, sinon le moteur de paie ne
-- saurait pas quel salaire de base retenir. Un index unique partiel le garantit
-- au niveau de la base, pas seulement dans le code.

CREATE TABLE employes (
    id                     TEXT PRIMARY KEY,
    -- Identifiant interne de l'entreprise. Unique : il sert de repère sur les
    -- bulletins, les déclarations et les virements.
    matricule              TEXT NOT NULL UNIQUE,
    nom                    TEXT NOT NULL,
    prenom                 TEXT,
    date_naissance         TEXT,
    lieu_naissance         TEXT,
    sexe                   TEXT CHECK (sexe IS NULL OR sexe IN ('m','f')),
    -- Pièce d'identité : facultative, jamais exigée (même principe que pour les
    -- tiers — contexte ouest-africain, voir 0027).
    cni                    TEXT,
    telephone              TEXT,
    adresse                TEXT,

    -- Situation de famille : elle décide des PARTS FISCALES, donc de l'impôt.
    situation_matrimoniale TEXT NOT NULL DEFAULT 'celibataire'
                           CHECK (situation_matrimoniale IN
                                  ('celibataire','marie','veuf','divorce')),
    -- Épouses **non salariées** à charge. Elles comptent pour les parts TRIMF,
    -- pas pour les parts d'impôt sur le revenu : ce sont deux comptes distincts,
    -- et les confondre fausserait les deux.
    nb_conjoints_a_charge  INTEGER NOT NULL DEFAULT 0 CHECK (nb_conjoints_a_charge >= 0),
    nb_enfants_charge      INTEGER NOT NULL DEFAULT 0 CHECK (nb_enfants_charge >= 0),

    -- ⚠️ Déclenche la retraite complémentaire (IPRES RCC). Ce n'est pas un
    -- titre honorifique : c'est une cotisation supplémentaire, salariale ET
    -- patronale.
    est_cadre              INTEGER NOT NULL DEFAULT 0,
    poste                  TEXT,
    -- Catégorie / classement conventionnel, s'il y en a un.
    categorie              TEXT,

    date_embauche          TEXT NOT NULL,
    -- Renseignée quand le salarié quitte l'entreprise. On ne supprime jamais
    -- un salarié : ses bulletins doivent rester consultables des années.
    date_sortie            TEXT,
    motif_sortie           TEXT,

    -- Numéros d'affiliation, indispensables aux déclarations.
    numero_ipres           TEXT,
    numero_css             TEXT,
    numero_ipm             TEXT,

    -- Comment le salaire lui est versé.
    mode_paiement          TEXT NOT NULL DEFAULT 'virement'
                           CHECK (mode_paiement IN ('virement','especes','cheque','mobile_money')),
    banque                 TEXT,
    numero_compte          TEXT,

    actif                  INTEGER NOT NULL DEFAULT 1,
    note                   TEXT,
    cree_le                TEXT NOT NULL,
    maj_le                 TEXT
);

CREATE INDEX idx_employes_actif ON employes(actif, nom);

CREATE TABLE contrats (
    id            TEXT PRIMARY KEY,
    employe_id    TEXT NOT NULL REFERENCES employes(id) ON DELETE CASCADE,
    type_contrat  TEXT NOT NULL DEFAULT 'cdi'
                  CHECK (type_contrat IN ('cdi','cdd','stage','apprentissage','journalier')),
    date_debut    TEXT NOT NULL,
    -- NULL pour un CDI. Pour un CDD, son absence est une anomalie juridique :
    -- signalée en alerte, jamais bloquée (un contrat peut être en cours de
    -- régularisation, et le terrain n'attend pas).
    date_fin      TEXT,

    salaire_base  NUMERIC NOT NULL DEFAULT 0,
    -- Complément contractuel versé au-delà du minimum conventionnel.
    sursalaire    NUMERIC NOT NULL DEFAULT 0,
    -- Horaire mensuel de référence, base du calcul des heures supplémentaires.
    heures_mois   NUMERIC NOT NULL DEFAULT 173.33,

    actif         INTEGER NOT NULL DEFAULT 1,
    motif_fin     TEXT,
    note          TEXT,
    cree_le       TEXT NOT NULL
);

-- ⚠️ Garantie AU NIVEAU DE LA BASE, pas seulement dans le code : deux contrats
-- actifs rendraient le salaire de base ambigu, et le moteur de paie choisirait
-- au hasard. Un index unique partiel est la seule barrière qu'aucun chemin de
-- code ne peut contourner.
CREATE UNIQUE INDEX idx_contrat_actif_unique
    ON contrats(employe_id) WHERE actif = 1;
CREATE INDEX idx_contrats_employe ON contrats(employe_id, date_debut DESC);

-- Avantages en nature attachés au contrat (logement, véhicule, nourriture…).
-- Leur évaluation vient de `ref_primes_reglementaires` (mig 0044) : ici on dit
-- seulement QUI en bénéficie, et éventuellement à quelle valeur déclarée.
CREATE TABLE contrat_avantages (
    id              TEXT PRIMARY KEY,
    contrat_id      TEXT NOT NULL REFERENCES contrats(id) ON DELETE CASCADE,
    code_avantage   TEXT NOT NULL,
    -- Valeur déclarée par l'employeur. NULL = on applique le barème de
    -- `ref_primes_reglementaires`.
    valeur_declaree NUMERIC,
    cree_le         TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_avantage_unique ON contrat_avantages(contrat_id, code_avantage);
