-- Djigui Desktop — migration 0005 : code-barres article.
-- Distinct du `code` interne : le code-barres (EAN/UPC) sert au scan et à la
-- recherche en caisse. Nullable, non unique (certains produits n'en ont pas ;
-- on évite de bloquer sur des doublons de codes-barres génériques).

ALTER TABLE article ADD COLUMN code_barre TEXT;
CREATE INDEX idx_article_code_barre ON article(code_barre);
