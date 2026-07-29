-- Djigui Desktop — migration 0037 : PASSATION ET SUIVI DES MARCHÉS.
--
-- Source : module « Passation de Marché » de l'application OLAC (base db_olac,
-- tables marche_*), lu le 2026-07-27. Spec : claude_spec/SPEC_MODULE_MARCHES.md.
--
-- Périmètre arrêté avec l'utilisateur : on reprend le modèle d'OLAC, on ajoute
-- les **soumissionnaires**, les **avenants** et la **réception** (absents
-- d'OLAC), et on **exclut** le lien avec la comptabilité.
--
-- L'idée structurante, reprise d'OLAC parce qu'elle est juste : **le type de
-- marché porte sa procédure**. On choisit « Travaux » et les étapes se
-- déroulent seules, dates calculées par cumul des durées. C'est exactement le
-- rapport modèle → instance déjà employé entre `nomenclature` (la recette) et
-- `ordre_production` : le modèle amorce, l'instance vit ensuite sa vie.

-- ---------------------------------------------------------------------------
-- Les familles de marché et leur procédure
-- ---------------------------------------------------------------------------
CREATE TABLE marche_type (
    id          TEXT PRIMARY KEY,
    code        TEXT NOT NULL UNIQUE,
    libelle     TEXT NOT NULL,
    description TEXT,
    actif       INTEGER NOT NULL DEFAULT 1,
    cree_par    TEXT,
    cree_le     TEXT NOT NULL
);

-- Le modèle de procédure : ce que le type impose comme étapes.
CREATE TABLE marche_etape_modele (
    id                 TEXT PRIMARY KEY,
    type_id            TEXT NOT NULL REFERENCES marche_type(id),
    libelle            TEXT NOT NULL,
    description        TEXT,
    ordre              INTEGER NOT NULL DEFAULT 0,
    -- Durée prévue en jours ; sert à calculer les dates par cumul.
    duree_prevue_jours INTEGER NOT NULL DEFAULT 0,
    -- Une étape facultative n'entre pas dans le calcul d'avancement.
    obligatoire        INTEGER NOT NULL DEFAULT 1,
    actif              INTEGER NOT NULL DEFAULT 1
);
CREATE INDEX idx_etape_modele_type ON marche_etape_modele(type_id, ordre);

-- ---------------------------------------------------------------------------
-- Le marché
-- ---------------------------------------------------------------------------
CREATE TABLE marche (
    id                     TEXT PRIMARY KEY,
    numero                 TEXT NOT NULL UNIQUE,
    objet                  TEXT NOT NULL,
    -- Détaché (NULL) si le type est supprimé : le marché garde ses étapes déjà
    -- instanciées. Supprimer = détacher, jamais détruire l'historique.
    type_id                TEXT REFERENCES marche_type(id),
    montant_estime         NUMERIC NOT NULL DEFAULT 0,
    -- Renseigné à l'attribution ; l'écart avec l'estimé est LE chiffre qu'on regarde.
    montant_attribue       NUMERIC,
    monnaie                TEXT NOT NULL DEFAULT 'FCFA',
    statut                 TEXT NOT NULL DEFAULT 'en_cours'
                           CHECK (statut IN ('en_cours','realise','annule','suspendu')),
    date_lancement         TEXT NOT NULL,
    date_cloture_prevue    TEXT,
    date_cloture_effective TEXT,
    -- Le titulaire retenu, une fois l'attribution prononcée. OLAC ne le stockait
    -- pas ; Djigui a déjà les tiers avec leur identité légale (mig 0027).
    attributaire_id        TEXT REFERENCES tiers(id),
    -- Un marché finance souvent une activité de projet. Simple lien, AUCUNE
    -- propagation de dates (barrière « cascade » de la spec Gestion de projet).
    projet_id              TEXT REFERENCES projet(id),
    responsable_id         TEXT REFERENCES utilisateur(id),
    lieu_execution         TEXT,
    observations           TEXT,
    motif_annulation       TEXT,
    annule_par             TEXT,
    annule_le              TEXT,
    cree_par               TEXT,
    cree_le                TEXT NOT NULL
);
CREATE INDEX idx_marche_statut ON marche(statut);
CREATE INDEX idx_marche_date   ON marche(date_lancement);
CREATE INDEX idx_marche_projet ON marche(projet_id);

-- ---------------------------------------------------------------------------
-- Les étapes réellement suivies
--
-- ⚠️ `libelle` est **recopié** du modèle, pas joint : corriger une procédure ne
-- doit jamais réécrire l'histoire des marchés déjà lancés. Même règle que les
-- composants d'un ordre de fabrication (mig 0031).
-- ---------------------------------------------------------------------------
CREATE TABLE marche_etape (
    id              TEXT PRIMARY KEY,
    marche_id       TEXT NOT NULL REFERENCES marche(id),
    -- Traçabilité seulement ; NULL si l'étape a été ajoutée à la main.
    etape_modele_id TEXT REFERENCES marche_etape_modele(id),
    libelle         TEXT NOT NULL,
    description     TEXT,
    ordre           INTEGER NOT NULL DEFAULT 0,
    date_prevue     TEXT,
    date_effective  TEXT,
    statut          TEXT NOT NULL DEFAULT 'en_attente'
                    CHECK (statut IN ('en_attente','en_cours','termine','annule','reporte')),
    obligatoire     INTEGER NOT NULL DEFAULT 1,
    observations    TEXT,
    -- Qui a validé l'étape et quand : c'est la valeur probante du module.
    valide_par      TEXT,
    valide_le       TEXT,
    cree_le         TEXT NOT NULL
);
CREATE INDEX idx_marche_etape ON marche_etape(marche_id, ordre);
CREATE INDEX idx_etape_statut ON marche_etape(statut, date_prevue);

-- ---------------------------------------------------------------------------
-- Les soumissionnaires (ajout demandé — absent d'OLAC)
--
-- Le dépouillement des offres. Un soumissionnaire peut être un tiers connu ou
-- une entreprise croisée une seule fois : `tiers_id` est donc FACULTATIF et le
-- nom est toujours saisi. On ne force pas à créer une fiche tiers pour recevoir
-- une offre — ce serait alourdir la saisie pour rien.
-- ---------------------------------------------------------------------------
CREATE TABLE marche_soumissionnaire (
    id              TEXT PRIMARY KEY,
    marche_id       TEXT NOT NULL REFERENCES marche(id),
    tiers_id        TEXT REFERENCES tiers(id),
    nom             TEXT NOT NULL,
    ninea           TEXT,
    telephone       TEXT,
    -- Montant proposé HT et TTC (le TTC sert à comparer ce que ça coûte vraiment).
    montant_offre   NUMERIC,
    montant_offre_ttc NUMERIC,
    -- Délai d'exécution proposé, en jours : souvent aussi décisif que le prix.
    delai_jours     INTEGER,
    -- Notes technique et financière du dépouillement (facultatives, /100).
    note_technique  NUMERIC,
    note_financiere NUMERIC,
    -- recu → conforme|non_conforme après examen → retenu (l'attributaire) ou ecarte.
    statut          TEXT NOT NULL DEFAULT 'recu'
                    CHECK (statut IN ('recu','conforme','non_conforme','retenu','ecarte')),
    motif           TEXT,
    observations    TEXT,
    date_depot      TEXT,
    cree_par        TEXT,
    cree_le         TEXT NOT NULL
);
CREATE INDEX idx_soum_marche ON marche_soumissionnaire(marche_id);

-- ---------------------------------------------------------------------------
-- Les avenants (ajout demandé — absent d'OLAC)
--
-- Un marché qui gonfle ou s'allonge en cours d'exécution : c'est constant. On
-- garde le montant initial INTACT sur le marché et on empile les avenants ;
-- le montant courant se déduit. On ne réécrit jamais le marché d'origine.
-- ---------------------------------------------------------------------------
CREATE TABLE marche_avenant (
    id              TEXT PRIMARY KEY,
    marche_id       TEXT NOT NULL REFERENCES marche(id),
    numero          INTEGER NOT NULL,
    objet           TEXT NOT NULL,
    -- Variation de montant : positive (augmentation) ou négative (diminution).
    montant_variation NUMERIC NOT NULL DEFAULT 0,
    -- Allongement du délai en jours, éventuellement négatif.
    delai_jours     INTEGER NOT NULL DEFAULT 0,
    date_avenant    TEXT NOT NULL,
    motif           TEXT,
    statut          TEXT NOT NULL DEFAULT 'projet'
                    CHECK (statut IN ('projet','approuve','rejete')),
    approuve_par    TEXT,
    approuve_le     TEXT,
    cree_par        TEXT,
    cree_le         TEXT NOT NULL,
    UNIQUE (marche_id, numero)
);
CREATE INDEX idx_avenant_marche ON marche_avenant(marche_id);

-- ---------------------------------------------------------------------------
-- Les réceptions (ajout demandé — absent d'OLAC)
--
-- Réception provisoire puis définitive, avec les réserves et la garantie. C'est
-- la fin réelle d'un marché de travaux, et la levée des réserves conditionne la
-- libération de la retenue de garantie.
-- ---------------------------------------------------------------------------
CREATE TABLE marche_reception (
    id                TEXT PRIMARY KEY,
    marche_id         TEXT NOT NULL REFERENCES marche(id),
    type_reception    TEXT NOT NULL DEFAULT 'provisoire'
                      CHECK (type_reception IN ('provisoire','definitive','partielle')),
    date_reception    TEXT NOT NULL,
    -- prononcee | avec_reserves | refusee : l'état réel du procès-verbal.
    resultat          TEXT NOT NULL DEFAULT 'prononcee'
                      CHECK (resultat IN ('prononcee','avec_reserves','refusee')),
    reserves          TEXT,
    -- Date à laquelle les réserves ont été levées (NULL = encore ouvertes).
    date_levee_reserves TEXT,
    -- Durée de garantie en mois à compter de la réception provisoire.
    garantie_mois     INTEGER,
    montant_retenue_garantie NUMERIC,
    observations      TEXT,
    receptionne_par   TEXT,
    cree_par          TEXT,
    cree_le           TEXT NOT NULL
);
CREATE INDEX idx_reception_marche ON marche_reception(marche_id);

-- ---------------------------------------------------------------------------
-- Pièces jointes : on ÉTEND `document_joint` (mig 0028) plutôt que de créer une
-- table. Les fichiers restent sur disque, nom régénéré en UUID, 20 Mo max —
-- toute la mécanique éprouvée est déjà là.
-- ---------------------------------------------------------------------------
ALTER TABLE document_joint ADD COLUMN marche_id TEXT REFERENCES marche(id);
ALTER TABLE document_joint ADD COLUMN marche_etape_id TEXT REFERENCES marche_etape(id);
CREATE INDEX idx_docjoint_marche ON document_joint(marche_id);

-- Numérotation : même mécanique que les factures et les ordres de fabrication
-- (sequence_numero, un compteur par exercice). Préfixe MA → MA-2026-0001.
INSERT INTO config_prefixe_document (type_document, prefixe) VALUES ('marche', 'MA');

-- Fenêtre pendant laquelle une étape validée reste modifiable. OLAC la codait
-- en dur à 30 jours ; ici c'est un réglage — un pays, une administration, un
-- client n'ont pas les mêmes usages.
INSERT INTO parametre_global (cle, valeur)
VALUES ('marche_delai_modification_suivi_jours', '30');

-- ---------------------------------------------------------------------------
-- Familles et procédures par défaut, reprises d'OLAC (données réelles observées)
-- ---------------------------------------------------------------------------
INSERT INTO marche_type (id, code, libelle, description, actif, cree_le) VALUES
 ('mt-travaux',    'TRAVAUX',     'Travaux',                    'Construction, réhabilitation, aménagement', 1, datetime('now')),
 ('mt-fournitures','FOURNITURES', 'Fournitures',                'Achat de biens et d''équipements',          1, datetime('now')),
 ('mt-services',   'SERVICES',    'Services',                   'Prestations de services courants',          1, datetime('now')),
 ('mt-intellect',  'INTELLECT',   'Prestations intellectuelles','Études, assistance, maîtrise d''œuvre',     1, datetime('now'));

INSERT INTO marche_etape_modele (id, type_id, libelle, ordre, duree_prevue_jours, obligatoire, actif) VALUES
 -- Travaux
 ('em-tr-1','mt-travaux','Préparation du dossier d''appel d''offres', 1, 10, 1, 1),
 ('em-tr-2','mt-travaux','Publication de l''avis',                    2,  3, 1, 1),
 ('em-tr-3','mt-travaux','Réception des offres',                      3, 21, 1, 1),
 ('em-tr-4','mt-travaux','Ouverture des plis',                        4,  1, 1, 1),
 ('em-tr-5','mt-travaux','Évaluation des offres',                     5,  7, 1, 1),
 ('em-tr-6','mt-travaux','Attribution provisoire',                    6,  7, 1, 1),
 ('em-tr-7','mt-travaux','Signature du contrat',                      7, 10, 1, 1),
 ('em-tr-8','mt-travaux','Notification et ordre de service',          8,  5, 1, 1),
 -- Fournitures
 ('em-fo-1','mt-fournitures','Préparation du dossier', 1,  7, 1, 1),
 ('em-fo-2','mt-fournitures','Publication de l''avis', 2,  3, 1, 1),
 ('em-fo-3','mt-fournitures','Réception des offres',   3, 15, 1, 1),
 ('em-fo-4','mt-fournitures','Ouverture des plis',     4,  1, 1, 1),
 ('em-fo-5','mt-fournitures','Évaluation des offres',  5,  5, 1, 1),
 ('em-fo-6','mt-fournitures','Attribution',            6, 10, 1, 1),
 ('em-fo-7','mt-fournitures','Signature du contrat',   7,  7, 1, 1),
 ('em-fo-8','mt-fournitures','Livraison',              8, 30, 1, 1),
 -- Services
 ('em-se-1','mt-services','Préparation du dossier', 1,  7, 1, 1),
 ('em-se-2','mt-services','Publication',            2,  3, 1, 1),
 ('em-se-3','mt-services','Réception des offres',   3, 15, 1, 1),
 ('em-se-4','mt-services','Ouverture des plis',     4,  1, 1, 1),
 ('em-se-5','mt-services','Évaluation',             5,  5, 1, 1),
 ('em-se-6','mt-services','Attribution',            6, 10, 1, 1),
 ('em-se-7','mt-services','Contractualisation',     7,  8, 1, 1),
 -- Prestations intellectuelles
 ('em-in-1','mt-intellect','Termes de référence',            1, 10, 1, 1),
 ('em-in-2','mt-intellect','Publication',                    2,  5, 1, 1),
 ('em-in-3','mt-intellect','Manifestations d''intérêt',      3, 15, 1, 1),
 ('em-in-4','mt-intellect','Liste restreinte',               4,  5, 1, 1),
 ('em-in-5','mt-intellect','Demande de propositions',        5, 21, 1, 1),
 ('em-in-6','mt-intellect','Évaluation technique',           6, 10, 1, 1),
 ('em-in-7','mt-intellect','Évaluation financière',          7,  5, 1, 1),
 ('em-in-8','mt-intellect','Attribution',                    8, 10, 1, 1),
 ('em-in-9','mt-intellect','Négociation et signature',       9,  8, 1, 1);
