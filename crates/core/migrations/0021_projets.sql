-- ---------------------------------------------------------------------------
-- Module Gestion de Projet — Incrément 1 : projets + tâches
-- ---------------------------------------------------------------------------
-- Gestion PAR projet (v1 cloisonnée, pas de vue transversale multi-projets).
-- Les jalons, dépendances, assignations, ressources et journal d'actions
-- viendront dans les incréments suivants. AUCUN lien agenda ici (à valider).

CREATE TABLE projet (
    id                TEXT PRIMARY KEY,
    nom               TEXT NOT NULL,
    client_id         TEXT REFERENCES tiers(id),
    chef_de_projet_id TEXT REFERENCES utilisateur(id),
    date_debut_prevue TEXT,
    date_fin_prevue   TEXT,
    date_debut_reelle TEXT,
    date_fin_reelle   TEXT,
    statut            TEXT NOT NULL DEFAULT 'planifie' CHECK (statut IN
                        ('planifie','en_cours','suspendu','cloture')),
    budget_global     NUMERIC NOT NULL DEFAULT 0,
    note              TEXT,
    cree_par          TEXT,
    cree_le           TEXT NOT NULL
);
CREATE INDEX idx_projet_statut ON projet(statut);
CREATE INDEX idx_projet_client ON projet(client_id);

-- Tâche : hiérarchie à UN seul niveau (une tâche parente, pas de petite-fille).
CREATE TABLE tache (
    id                TEXT PRIMARY KEY,
    projet_id         TEXT NOT NULL REFERENCES projet(id),
    tache_parente_id  TEXT REFERENCES tache(id),
    nom               TEXT NOT NULL,
    description       TEXT,
    date_debut_prevue TEXT,
    date_fin_prevue   TEXT,
    date_debut_reelle TEXT,
    date_fin_reelle   TEXT,
    statut            TEXT NOT NULL DEFAULT 'a_faire' CHECK (statut IN
                        ('a_faire','en_cours','bloquee','terminee')),
    avancement        INTEGER NOT NULL DEFAULT 0,   -- 0..100, saisie manuelle
    ordre             INTEGER NOT NULL DEFAULT 0,
    cree_le           TEXT NOT NULL
);
CREATE INDEX idx_tache_projet ON tache(projet_id);
CREATE INDEX idx_tache_parente ON tache(tache_parente_id);
