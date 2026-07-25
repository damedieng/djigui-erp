-- ---------------------------------------------------------------------------
-- Agenda / rendez-vous (backlog fonctionnel — module organisation)
-- ---------------------------------------------------------------------------
-- Un rendez-vous a un titre, une plage horaire, un statut, et peut être rattaché
-- (optionnellement) à un tiers (client/fournisseur), à un responsable
-- (utilisateur), à un lieu et à une note libre. `debut`/`fin` sont des
-- horodatages « AAAA-MM-JJ HH:MM » (fin optionnelle). Traçabilité via `cree_par`.
CREATE TABLE rendez_vous (
    id             TEXT PRIMARY KEY,
    titre          TEXT NOT NULL,
    debut          TEXT NOT NULL,                 -- 'AAAA-MM-JJ HH:MM'
    fin            TEXT,                           -- optionnel
    journee_entiere INTEGER NOT NULL DEFAULT 0,    -- 1 = toute la journée (heure ignorée)
    statut         TEXT NOT NULL DEFAULT 'planifie' CHECK (statut IN
                     ('planifie','confirme','honore','annule','reporte')),
    tiers_id       TEXT REFERENCES tiers(id),
    responsable_id TEXT REFERENCES utilisateur(id),
    lieu           TEXT,
    note           TEXT,
    cree_par       TEXT,
    cree_le        TEXT NOT NULL
);
CREATE INDEX idx_rdv_debut  ON rendez_vous(debut);
CREATE INDEX idx_rdv_tiers   ON rendez_vous(tiers_id);
CREATE INDEX idx_rdv_statut  ON rendez_vous(statut);
