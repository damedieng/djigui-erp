-- 0008 : la facturation cyclique s'appuie désormais sur des lignes propres à
-- l'abonnement (plus de « document modèle » à sélectionner). On reconstruit la
-- table `abonnement` pour retirer `document_modele_id` et ajouter un libellé,
-- puis on ajoute la table des lignes.

CREATE TABLE abonnement_new (
    id                 TEXT PRIMARY KEY,
    tiers_id           TEXT NOT NULL REFERENCES tiers(id),
    libelle            TEXT,
    frequence          TEXT NOT NULL CHECK (frequence IN ('mensuel','trimestriel','annuel')),
    prochaine_echeance TEXT NOT NULL,
    actif              INTEGER NOT NULL DEFAULT 1
);

INSERT INTO abonnement_new (id, tiers_id, libelle, frequence, prochaine_echeance, actif)
    SELECT id, tiers_id, NULL, frequence, prochaine_echeance, actif FROM abonnement;

DROP TABLE abonnement;
ALTER TABLE abonnement_new RENAME TO abonnement;

CREATE TABLE abonnement_ligne (
    id            TEXT PRIMARY KEY,
    abonnement_id TEXT NOT NULL REFERENCES abonnement(id) ON DELETE CASCADE,
    article_id    TEXT NOT NULL REFERENCES article(id),
    designation   TEXT NOT NULL,
    quantite      NUMERIC NOT NULL,
    prix_unitaire NUMERIC NOT NULL,
    taux_tva      NUMERIC NOT NULL DEFAULT 0,
    remise        NUMERIC NOT NULL DEFAULT 0
);
CREATE INDEX idx_abonnement_ligne ON abonnement_ligne(abonnement_id);
