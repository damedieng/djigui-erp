-- ---------------------------------------------------------------------------
-- Inventaires (comptage daté et verrouillé) — § stock / magasins
-- ---------------------------------------------------------------------------
-- Un inventaire est un enregistrement figé : à sa validation, on crée les
-- ajustements de stock et on conserve le détail (théorique / compté / écart).
-- Il n'est plus modifiable ensuite.
CREATE TABLE inventaire (
    id             TEXT PRIMARY KEY,
    depot_id       TEXT NOT NULL,
    utilisateur_id TEXT,
    date           TEXT NOT NULL,
    statut         TEXT NOT NULL DEFAULT 'valide' CHECK (statut IN ('brouillon','valide')),
    note           TEXT
);
CREATE INDEX idx_inventaire_depot ON inventaire(depot_id);

CREATE TABLE inventaire_ligne (
    id              TEXT PRIMARY KEY,
    inventaire_id   TEXT NOT NULL REFERENCES inventaire(id),
    article_id      TEXT NOT NULL,
    designation     TEXT NOT NULL,
    stock_theorique NUMERIC NOT NULL,
    stock_compte    NUMERIC NOT NULL,
    ecart           NUMERIC NOT NULL
);
CREATE INDEX idx_inventaire_ligne ON inventaire_ligne(inventaire_id);
