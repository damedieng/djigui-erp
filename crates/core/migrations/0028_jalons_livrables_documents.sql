-- Gestion de projet : jalons, livrables et documents joints.
--
-- ⚠️ BARRIÈRE SPEC RESPECTÉE : les jalons sont **locaux au projet**, sans aucun
-- lien avec l'agenda (`rendez_vous`). Le branchement éventuel se décidera
-- séparément avec l'utilisateur (SPEC_MODULE_GESTION_PROJET.md).

-- ---------------------------------------------------------------------------
-- Jalon : date clé du projet. Rattachable à une activité, mais autonome.
-- ---------------------------------------------------------------------------
CREATE TABLE jalon (
    id          TEXT PRIMARY KEY,
    projet_id   TEXT NOT NULL REFERENCES projet(id),
    tache_id    TEXT REFERENCES tache(id),      -- facultatif
    nom         TEXT NOT NULL,
    date_prevue TEXT NOT NULL,                  -- AAAA-MM-JJ
    date_reelle TEXT,                           -- renseignée quand atteint
    statut      TEXT NOT NULL DEFAULT 'a_venir'
                CHECK (statut IN ('a_venir','atteint','manque')),
    note        TEXT,
    ordre       INTEGER NOT NULL DEFAULT 0,
    cree_le     TEXT NOT NULL
);
CREATE INDEX idx_jalon_projet ON jalon(projet_id);

-- ---------------------------------------------------------------------------
-- Livrable : ce que le projet doit produire. Rattachable à une activité
-- et/ou à un jalon.
-- ---------------------------------------------------------------------------
CREATE TABLE livrable (
    id             TEXT PRIMARY KEY,
    projet_id      TEXT NOT NULL REFERENCES projet(id),
    tache_id       TEXT REFERENCES tache(id),   -- facultatif
    jalon_id       TEXT REFERENCES jalon(id),   -- facultatif
    nom            TEXT NOT NULL,
    description    TEXT,
    statut         TEXT NOT NULL DEFAULT 'a_produire'
                   CHECK (statut IN ('a_produire','en_cours','livre','accepte','refuse')),
    date_attendue  TEXT,
    date_livraison TEXT,
    ordre          INTEGER NOT NULL DEFAULT 0,
    cree_le        TEXT NOT NULL
);
CREATE INDEX idx_livrable_projet ON livrable(projet_id);

-- ---------------------------------------------------------------------------
-- Document joint. Le FICHIER est stocké SUR DISQUE (dossier `documents/` à
-- côté de la base) et seul son chemin est enregistré ici : mettre des pièces
-- jointes en base64 ferait exploser djigui.db (cf. leçon des images produit).
-- Rattachable au projet, à une activité, à un jalon ou à un livrable.
-- ---------------------------------------------------------------------------
CREATE TABLE document_joint (
    id          TEXT PRIMARY KEY,
    projet_id   TEXT NOT NULL REFERENCES projet(id),
    tache_id    TEXT REFERENCES tache(id),
    jalon_id    TEXT REFERENCES jalon(id),
    livrable_id TEXT REFERENCES livrable(id),
    nom         TEXT NOT NULL,          -- nom affiché (nom d'origine du fichier)
    chemin      TEXT NOT NULL,          -- chemin relatif au dossier de stockage
    taille      INTEGER NOT NULL DEFAULT 0,
    type_mime   TEXT,
    cree_par    TEXT,
    cree_le     TEXT NOT NULL
);
CREATE INDEX idx_docjoint_projet ON document_joint(projet_id);
