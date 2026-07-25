-- Djigui Desktop — migration 0006 : taux de TVA paramétrables.
-- Les taux sont désormais gérés dans les paramètres et proposés à la création
-- d'un article (au lieu d'une liste codée en dur). Un seul taux « par défaut ».

CREATE TABLE taux_tva (
    valeur     NUMERIC PRIMARY KEY,       -- ex. 18, 10, 0
    libelle    TEXT NOT NULL,             -- ex. "18 %", "0 % (exonéré)"
    par_defaut INTEGER NOT NULL DEFAULT 0 -- un seul à 1
);

INSERT INTO taux_tva (valeur, libelle, par_defaut) VALUES
    (18, '18 %', 1),
    (10, '10 %', 0),
    (0,  '0 % (exonéré)', 0);
