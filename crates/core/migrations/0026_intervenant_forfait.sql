-- ---------------------------------------------------------------------------
-- Gestion de Projet — mode de coût « forfait » pour les intervenants
-- ---------------------------------------------------------------------------
-- Parfois le coût d'une personne est FORFAITAIRE (montant fixe) et non
-- heures × taux. On ajoute 'forfait' aux types de taux. Le coût d'une
-- assignation en forfait = le montant (taux), quel que soit le nombre d'heures.
-- SQLite ne permet pas d'altérer un CHECK : on reconstruit la table.
CREATE TABLE intervenant_new (
    id             TEXT PRIMARY KEY,
    nom            TEXT NOT NULL,
    type           TEXT NOT NULL DEFAULT 'interne' CHECK (type IN ('interne','externe')),
    utilisateur_id TEXT,
    societe        TEXT,
    role           TEXT,
    type_taux      TEXT NOT NULL DEFAULT 'horaire' CHECK (type_taux IN ('horaire','journalier','forfait')),
    taux           NUMERIC NOT NULL DEFAULT 0,
    actif          INTEGER NOT NULL DEFAULT 1,
    cree_le        TEXT NOT NULL
);
INSERT INTO intervenant_new (id, nom, type, utilisateur_id, societe, role, type_taux, taux, actif, cree_le)
    SELECT id, nom, type, utilisateur_id, societe, role, type_taux, taux, actif, cree_le FROM intervenant;
DROP TABLE intervenant;
ALTER TABLE intervenant_new RENAME TO intervenant;
CREATE INDEX idx_intervenant_user ON intervenant(utilisateur_id);
