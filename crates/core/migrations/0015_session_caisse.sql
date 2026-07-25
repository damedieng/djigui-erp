-- ---------------------------------------------------------------------------
-- Sessions de caisse (ouverture / fermeture) — § caisse
-- ---------------------------------------------------------------------------
-- Une session = une « journée de caisse » : ouverte avec un fond, fermée après
-- comptage. Les paiements encaissés/décaissés y sont rattachés pour calculer
-- l'écart (compté − théorique) à la fermeture.
CREATE TABLE session_caisse (
    id             TEXT PRIMARY KEY,
    caisse_id      TEXT NOT NULL,
    utilisateur_id TEXT,                     -- qui a ouvert la session
    fond_ouverture NUMERIC NOT NULL DEFAULT 0,
    ouvert_le      TEXT NOT NULL,
    ferme_le       TEXT,                     -- NULL tant qu'ouverte
    montant_compte NUMERIC,                  -- espèces comptées à la fermeture
    ecart          NUMERIC,                  -- compté − théorique
    statut         TEXT NOT NULL DEFAULT 'ouverte' CHECK (statut IN ('ouverte','fermee')),
    note           TEXT
);
CREATE INDEX idx_session_caisse_statut ON session_caisse(caisse_id, statut);

-- Rattachement des règlements à la session ouverte (posé automatiquement).
ALTER TABLE paiement ADD COLUMN session_caisse_id TEXT;
