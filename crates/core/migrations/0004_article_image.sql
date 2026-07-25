-- Djigui Desktop — migration 0004 : image d'article.
-- Un article peut porter une image (photo produit), stockée en data-URI base64
-- (cohérent avec le logo entreprise §5.9, également stocké en base64/chemin).
-- Nullable : l'image reste optionnelle.

ALTER TABLE article ADD COLUMN image TEXT;
