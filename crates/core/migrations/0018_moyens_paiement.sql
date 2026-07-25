-- ---------------------------------------------------------------------------
-- Moyens de paiement configurables (Orange Money, Wave, Free Money…)
-- ---------------------------------------------------------------------------
-- L'utilisateur définit ses propres moyens (image + texte) dans les Paramètres.
-- Ils s'affichent à l'encaissement. Chaque moyen appartient à une FAMILLE
-- (espece / mobile_money / virement / cheque) qui pilote le comportement :
--   - `rendu_monnaie` = 1 → bloc « montant reçu / rendu » (typiquement l'espèce).
-- La colonne `paiement.mode` (CHECK famille) est conservée : on y écrit la
-- famille du moyen ; le moyen concret est tracé par `paiement.moyen_paiement_id`.
CREATE TABLE moyen_paiement (
    id            TEXT PRIMARY KEY,
    nom           TEXT NOT NULL UNIQUE,
    famille       TEXT NOT NULL CHECK (famille IN ('espece','mobile_money','virement','cheque')),
    image         TEXT,               -- data-URI base64 embarqué (hors-ligne), optionnel
    couleur       TEXT NOT NULL DEFAULT '#64748b',  -- repli pastille si pas d'image
    rendu_monnaie INTEGER NOT NULL DEFAULT 0,        -- 1 = calcule le rendu (espèce)
    actif         INTEGER NOT NULL DEFAULT 1,
    ordre         INTEGER NOT NULL DEFAULT 0
);

-- Moyen concret rattaché à un paiement (nullable : anciens paiements / repli famille).
ALTER TABLE paiement ADD COLUMN moyen_paiement_id TEXT REFERENCES moyen_paiement(id);

-- Seed des moyens par défaut (repli couleur, sans image).
INSERT INTO moyen_paiement (id, nom, famille, couleur, rendu_monnaie, actif, ordre) VALUES
  ('mp-espece',   'Espèce',       'espece',       '#16a34a', 1, 1, 0),
  ('mp-orange',   'Orange Money', 'mobile_money', '#f97316', 0, 1, 1),
  ('mp-wave',     'Wave',         'mobile_money', '#0ea5e9', 0, 1, 2),
  ('mp-free',     'Free Money',   'mobile_money', '#dc2626', 0, 1, 3),
  ('mp-virement', 'Virement',     'virement',     '#6366f1', 0, 1, 4),
  ('mp-cheque',   'Chèque',       'cheque',       '#64748b', 0, 1, 5);
