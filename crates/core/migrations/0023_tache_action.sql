-- ---------------------------------------------------------------------------
-- Gestion de Projet — journal d'avancement / observations par tâche
-- ---------------------------------------------------------------------------
-- Chaque saisie d'avancement peut être accompagnée d'une observation (ce qui a
-- été fait, un blocage…). On garde l'historique : c'est le « journal d'actions »
-- de la spec, concret, sans multiplier les tâches.
CREATE TABLE tache_action (
    id             TEXT PRIMARY KEY,
    tache_id       TEXT NOT NULL REFERENCES tache(id),
    utilisateur_id TEXT,             -- id de l'acteur (comme cree_par ailleurs), pas de FK stricte
    date           TEXT NOT NULL,
    avancement     INTEGER,          -- % au moment de l'observation (nullable)
    observation    TEXT
);
CREATE INDEX idx_action_tache ON tache_action(tache_id);
