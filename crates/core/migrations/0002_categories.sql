-- Djigui Desktop — migration 0002 : catégories d'articles.
-- Ajout postérieur au §5.2 d'origine : les produits sont classés par catégorie
-- (cf. chips de la maquette caisse : Boissons, Épicerie, Hygiène, Divers).
-- Table dédiée (pas un simple texte) pour permettre gestion et filtrage propres.

CREATE TABLE categorie (
    id  TEXT PRIMARY KEY,
    nom TEXT NOT NULL UNIQUE
);

-- Lien article -> catégorie, nullable (un article peut rester non classé).
ALTER TABLE article ADD COLUMN categorie_id TEXT REFERENCES categorie(id);
CREATE INDEX idx_article_categorie ON article(categorie_id);

-- Catégories par défaut, alignées sur la maquette.
INSERT INTO categorie (id, nom) VALUES
    ('cat-boissons', 'Boissons'),
    ('cat-epicerie', 'Épicerie'),
    ('cat-hygiene',  'Hygiène'),
    ('cat-divers',   'Divers');
