-- ---------------------------------------------------------------------------
-- Gestion de Projet — assignations (personne ↔ tâche + heures)
-- ---------------------------------------------------------------------------
-- Qui travaille sur quelle tâche, avec un volume d'heures allouées. Sert à la
-- planification par ressource humaine (Gantt d'une personne + détection des
-- chevauchements = surcharge).
CREATE TABLE assignation (
    id              TEXT PRIMARY KEY,
    tache_id        TEXT NOT NULL REFERENCES tache(id),
    utilisateur_id  TEXT NOT NULL,          -- id de l'utilisateur (pas de FK stricte, cf. autres tables)
    heures_allouees NUMERIC NOT NULL DEFAULT 0
);
CREATE INDEX idx_assign_tache ON assignation(tache_id);
CREATE INDEX idx_assign_user  ON assignation(utilisateur_id);
