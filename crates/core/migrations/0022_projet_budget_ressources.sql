-- ---------------------------------------------------------------------------
-- Gestion de Projet — budget par tâche (remontée bas→haut) + ressources
-- ---------------------------------------------------------------------------
-- Le budget « planifié » se calcule des feuilles vers le haut : une tâche
-- parente n'a pas de budget propre, elle vaut la somme de ses enfants ; le
-- budget planifié du projet = somme des tâches feuilles. Le budget SAISI au
-- projet (projet.budget_global) est conservé à part (comparaison / écart).
-- La hiérarchie passe à plusieurs niveaux (garde-fou applicatif : max 4).

ALTER TABLE tache ADD COLUMN budget NUMERIC NOT NULL DEFAULT 0;  -- budget de la feuille

-- Ressources : matériel / budget / sous-traitance, au niveau projet ou tâche.
CREATE TABLE ressource (
    id            TEXT PRIMARY KEY,
    projet_id     TEXT NOT NULL REFERENCES projet(id),
    tache_id      TEXT REFERENCES tache(id),
    type          TEXT NOT NULL CHECK (type IN ('materiel','budget','sous_traitance')),
    libelle       TEXT NOT NULL,
    cout_unitaire NUMERIC NOT NULL DEFAULT 0,
    quantite      NUMERIC NOT NULL DEFAULT 1,
    cree_le       TEXT NOT NULL
);
CREATE INDEX idx_ressource_projet ON ressource(projet_id);
CREATE INDEX idx_ressource_tache  ON ressource(tache_id);
