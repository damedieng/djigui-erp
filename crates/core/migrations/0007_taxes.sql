-- Djigui Desktop — migration 0007 : taxes multiples.
-- Généralise la TVA unique : une vente/un article peut porter PLUSIEURS taxes.
-- - `taxe`               : catalogue des taxes (TVA et autres), % ou montant fixe.
-- - `article_taxe`       : taxes appliquées par défaut à un article (0..n).
-- - `document_ligne_taxe`: snapshot des taxes calculées sur chaque ligne (figé,
--                          pour que l'historique ne bouge pas si une taxe change).
-- Compat : `document_ligne.taux_tva` reste comme repli quand une ligne n'a pas
-- de taxes explicites (documents existants, saisie simple).

CREATE TABLE taxe (
    id         TEXT PRIMARY KEY,
    nom        TEXT NOT NULL,
    taux       NUMERIC NOT NULL,
    type       TEXT NOT NULL DEFAULT 'pourcentage' CHECK (type IN ('pourcentage','fixe')),
    actif      INTEGER NOT NULL DEFAULT 1,
    par_defaut INTEGER NOT NULL DEFAULT 0
);

-- Reprend les taux de TVA existants (migration 0006) comme taxes de type pourcentage.
INSERT INTO taxe (id, nom, taux, type, actif, par_defaut)
SELECT lower(hex(randomblob(16))), 'TVA ' || libelle, valeur, 'pourcentage', 1, par_defaut
FROM taux_tva;

CREATE TABLE article_taxe (
    article_id TEXT NOT NULL REFERENCES article(id) ON DELETE CASCADE,
    taxe_id    TEXT NOT NULL REFERENCES taxe(id) ON DELETE CASCADE,
    PRIMARY KEY (article_id, taxe_id)
);

CREATE TABLE document_ligne_taxe (
    id       TEXT PRIMARY KEY,
    ligne_id TEXT NOT NULL REFERENCES document_ligne(id) ON DELETE CASCADE,
    nom      TEXT NOT NULL,
    type     TEXT NOT NULL,
    taux     NUMERIC NOT NULL,
    montant  NUMERIC NOT NULL
);
CREATE INDEX idx_ligne_taxe ON document_ligne_taxe(ligne_id);
