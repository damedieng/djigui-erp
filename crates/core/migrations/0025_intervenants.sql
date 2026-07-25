-- ---------------------------------------------------------------------------
-- Gestion de Projet — intervenants (ressources humaines interne / externe)
-- ---------------------------------------------------------------------------
-- Un intervenant est une personne planifiable : soit INTERNE (rattachée à un
-- compte utilisateur), soit EXTERNE (consultant, prestataire : nom + société,
-- sans compte). Chacun porte un taux, HORAIRE ou JOURNALIER au choix. Les
-- assignations pointent désormais sur l'intervenant → le coût main-d'œuvre
-- (heures × taux) alimente les dépenses du projet.
CREATE TABLE intervenant (
    id             TEXT PRIMARY KEY,
    nom            TEXT NOT NULL,
    type           TEXT NOT NULL DEFAULT 'interne' CHECK (type IN ('interne','externe')),
    utilisateur_id TEXT,                 -- rempli si type='interne'
    societe        TEXT,                 -- pour les externes
    role           TEXT,
    type_taux      TEXT NOT NULL DEFAULT 'horaire' CHECK (type_taux IN ('horaire','journalier')),
    taux           NUMERIC NOT NULL DEFAULT 0,
    actif          INTEGER NOT NULL DEFAULT 1,
    cree_le        TEXT NOT NULL
);
CREATE INDEX idx_intervenant_user ON intervenant(utilisateur_id);

-- Les assignations pointent sur l'intervenant (l'ancienne colonne utilisateur_id
-- reste, ignorée, pour ne pas reconstruire la table).
ALTER TABLE assignation ADD COLUMN intervenant_id TEXT;

-- Rétro-compatibilité : crée un intervenant interne pour chaque personne déjà
-- assignée, puis relie les assignations existantes.
INSERT INTO intervenant (id, nom, type, utilisateur_id, type_taux, taux, actif, cree_le)
    SELECT DISTINCT lower(hex(randomblob(16))),
           COALESCE(u.nom, a.utilisateur_id), 'interne', a.utilisateur_id, 'horaire', 0, 1, datetime('now')
    FROM assignation a LEFT JOIN utilisateur u ON u.id = a.utilisateur_id
    WHERE a.utilisateur_id IS NOT NULL;
UPDATE assignation SET intervenant_id = (
    SELECT i.id FROM intervenant i WHERE i.utilisateur_id = assignation.utilisateur_id LIMIT 1)
    WHERE utilisateur_id IS NOT NULL;

-- Reconstruit `assignation` sans l'ancienne colonne utilisateur_id (NOT NULL).
CREATE TABLE assignation_new (
    id              TEXT PRIMARY KEY,
    tache_id        TEXT NOT NULL REFERENCES tache(id),
    intervenant_id  TEXT,
    heures_allouees NUMERIC NOT NULL DEFAULT 0
);
INSERT INTO assignation_new (id, tache_id, intervenant_id, heures_allouees)
    SELECT id, tache_id, intervenant_id, heures_allouees FROM assignation;
DROP TABLE assignation;
ALTER TABLE assignation_new RENAME TO assignation;
CREATE INDEX idx_assign_tache  ON assignation(tache_id);
CREATE INDEX idx_assign_interv ON assignation(intervenant_id);
