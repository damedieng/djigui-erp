-- ---------------------------------------------------------------------------
-- Traçabilité (audit) : qui a fait quoi, et quand
-- ---------------------------------------------------------------------------
-- Journal central de toutes les actions sensibles. La vérité de « qui a agi »
-- est ici. En complément, les pièces clés portent aussi leur auteur (`cree_par`).
CREATE TABLE journal_audit (
    id              TEXT PRIMARY KEY,
    date            TEXT NOT NULL,
    utilisateur_id  TEXT,                 -- NULL = action système / non authentifiée
    utilisateur_nom TEXT NOT NULL DEFAULT '',
    action          TEXT NOT NULL,        -- ex. « creation », « validation », « suppression »
    entite          TEXT NOT NULL,        -- ex. « document », « paiement », « tiers »
    entite_id       TEXT,
    detail          TEXT
);
CREATE INDEX idx_audit_date ON journal_audit(date);
CREATE INDEX idx_audit_utilisateur ON journal_audit(utilisateur_id);

-- Auteur direct sur les pièces clés (posé à la création).
ALTER TABLE document ADD COLUMN cree_par TEXT;
ALTER TABLE paiement ADD COLUMN cree_par TEXT;
