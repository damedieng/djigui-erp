-- ---------------------------------------------------------------------------
-- Image de catégorie (optionnelle)
-- ---------------------------------------------------------------------------
-- Permet d'illustrer une catégorie (data-URI base64, comme les articles). Sert
-- notamment à afficher l'image en fond des tuiles de catégorie à la caisse.
-- Facultatif : NULL = pas d'image (repli sur l'affichage par défaut).
ALTER TABLE categorie ADD COLUMN image TEXT;
