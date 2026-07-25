-- ---------------------------------------------------------------------------
-- Seeder de catalogues métier (SEEDER-CATALOGUES.md)
-- ---------------------------------------------------------------------------
-- Pré-remplissage du catalogue par type de commerce, piloté par la DONNÉE
-- (JSON embarqués). Idempotent : les codes de seed servent de clé.

-- Référentiel d'unités (§4). Seedé une fois, indépendamment du type de commerce.
CREATE TABLE unite (
    code    TEXT PRIMARY KEY,
    libelle TEXT NOT NULL
);
INSERT INTO unite (code, libelle) VALUES
    ('piece','pièce'), ('paquet','paquet'), ('sachet','sachet'), ('boite','boîte'),
    ('carton','carton'), ('bouteille','bouteille'), ('bidon','bidon'), ('kg','kg'),
    ('g','g'), ('litre','litre'), ('metre','mètre'), ('paire','paire'),
    ('lot','lot'), ('heure','heure'), ('prestation','prestation');

-- Métadonnées de seed + affichage sur les catégories.
ALTER TABLE categorie ADD COLUMN code_seed TEXT;   -- clé d'idempotence (NULL = créée à la main)
ALTER TABLE categorie ADD COLUMN icone     TEXT;   -- icône Tabler (ex. ti-bottle)
ALTER TABLE categorie ADD COLUMN couleur   TEXT;   -- couleur d'accent (ex. #2563eb)
ALTER TABLE categorie ADD COLUMN ordre     INTEGER NOT NULL DEFAULT 100;

-- Métadonnées de seed + unité + image fichier sur les articles.
ALTER TABLE article ADD COLUMN code_seed        TEXT;   -- clé d'idempotence
ALTER TABLE article ADD COLUMN unite            TEXT;   -- référence unite.code
ALTER TABLE article ADD COLUMN image_chemin     TEXT;   -- chemin relatif (media/articles/...)
ALTER TABLE article ADD COLUMN image_origine    TEXT CHECK (image_origine IN ('seed','utilisateur'));
-- Prix « à compléter » : le schéma impose prix_vente NOT NULL, on marque donc
-- explicitement les articles seedés sans prix (prix_vente laissé à 0).
ALTER TABLE article ADD COLUMN prix_a_completer INTEGER NOT NULL DEFAULT 0;

-- Idempotence : un code_seed est unique quand il est renseigné.
CREATE UNIQUE INDEX idx_categorie_code_seed ON categorie(code_seed) WHERE code_seed IS NOT NULL;
CREATE UNIQUE INDEX idx_article_code_seed   ON article(code_seed)   WHERE code_seed IS NOT NULL;

-- Types de commerce déjà appliqués (pour proposer les autres et rester additif).
CREATE TABLE seed_applique (
    code_type   TEXT PRIMARY KEY,
    version     INTEGER NOT NULL,
    applique_le TEXT NOT NULL
);

-- Assujettissement TVA de l'entreprise : si faux, le seed force tva = 0 (§5).
ALTER TABLE parametres_entreprise ADD COLUMN assujetti_tva INTEGER NOT NULL DEFAULT 1;
