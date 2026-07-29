-- Djigui Desktop — migration 0038 : ENCHAÎNEMENT DES ÉTAPES D'UN MARCHÉ.
--
-- Constat de l'utilisateur (2026-07-28) : « en matière de suivi de marché les
-- étapes sont liées fortement, mais tel que c'est fait ce n'est pas bon, car la
-- saisie est plate — quelqu'un peut annuler l'ouverture des plis et continuer
-- les autres étapes. Ça ne se passe pas comme ça. »
--
-- Il a raison, et c'est un défaut de conception de la 0037 : `changer_statut_etape`
-- ne regardait AUCUNE autre étape. On pouvait annuler l'ouverture des plis et
-- valider l'attribution — un dossier juridiquement indéfendable.
--
-- # Le principe
--
-- Une procédure de passation n'est pas une liste de cases à cocher : c'est une
-- **chaîne d'actes**, où chacun fonde le suivant. Ouvrir les plis fonde
-- l'évaluation, qui fonde l'attribution, qui fonde le contrat. Casser un maillon
-- invalide tout ce qui en découle.
--
-- # Ce qui NE change pas
--
-- Le module ne bloque toujours pas sur les **dates** : une étape en retard
-- n'empêche rien, le terrain n'attend pas. Le verrou porte uniquement sur
-- l'**ordre des actes**, qui est d'une autre nature.
--
-- # La porte de sortie (décision utilisateur)
--
-- Verrou **avec dérogation motivée** : un bouton « Passer outre » existe, mais il
-- exige un motif et laisse une trace nominative. Sans lui, il deviendrait
-- impossible de saisir un dossier déjà commencé sur papier — ce que l'utilisateur
-- fait réellement (constaté sur ses données : des étapes datées avant le
-- lancement du marché).

-- ---------------------------------------------------------------------------
-- Dérogation : franchir une étape hors de son rang, en l'assumant
-- ---------------------------------------------------------------------------
ALTER TABLE marche_etape ADD COLUMN derogation INTEGER NOT NULL DEFAULT 0;
ALTER TABLE marche_etape ADD COLUMN motif_derogation TEXT;
ALTER TABLE marche_etape ADD COLUMN derogation_par TEXT;
ALTER TABLE marche_etape ADD COLUMN derogation_le TEXT;

-- ---------------------------------------------------------------------------
-- Incidents de procédure : infructueux et recours
--
-- Deux situations bien réelles que le modèle plat ne savait pas dire :
--   • **infructueux** : aucune offre, ou aucune conforme. La procédure repart
--     à la publication — mais la première tentative doit rester au dossier.
--   • **recours** : un candidat conteste l'attribution. La procédure est gelée
--     à cette étape jusqu'à décision. C'est un arrêt SUBI, pas un oubli : il ne
--     doit pas être compté comme un simple retard.
--
-- Une seule table pour les deux : ce sont deux évènements qui interrompent la
-- chaîne, se datent, se motivent et se closent. Les séparer dupliquerait tout.
-- ---------------------------------------------------------------------------
CREATE TABLE marche_incident (
    id            TEXT PRIMARY KEY,
    marche_id     TEXT NOT NULL REFERENCES marche(id),
    -- L'étape où la procédure s'est arrêtée. Détachée (NULL) si l'étape est
    -- supprimée : l'incident reste au dossier, il fait partie de l'histoire.
    etape_id      TEXT REFERENCES marche_etape(id),
    type_incident TEXT NOT NULL CHECK (type_incident IN ('infructueux','recours')),
    date_incident TEXT NOT NULL,
    -- Pourquoi : « aucune offre reçue », « offres toutes non conformes »,
    -- « recours de l'entreprise X sur les critères d'évaluation »…
    motif         TEXT NOT NULL,
    -- Qui conteste, pour un recours. NULL pour un infructueux.
    auteur_recours TEXT,
    -- ouvert → clos. Tant qu'un recours est ouvert, la procédure est gelée.
    statut        TEXT NOT NULL DEFAULT 'ouvert'
                  CHECK (statut IN ('ouvert','clos')),
    -- La suite donnée : « recours rejeté », « procédure relancée le … ».
    decision      TEXT,
    date_decision TEXT,
    -- Numéro de tentative rouverte par un infructueux (1 = procédure initiale).
    tentative     INTEGER NOT NULL DEFAULT 1,
    cree_par      TEXT,
    cree_le       TEXT NOT NULL
);
CREATE INDEX idx_incident_marche ON marche_incident(marche_id, statut);

-- Numéro de tentative porté par le marché : incrémenté à chaque relance après
-- un appel d'offres infructueux. Sert à dire « 2ᵉ tentative » à l'écran.
ALTER TABLE marche ADD COLUMN tentative INTEGER NOT NULL DEFAULT 1;
