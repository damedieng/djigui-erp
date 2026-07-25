-- Djigui Desktop — migration 0003 : compteurs de numérotation des pièces.
-- Numérotation « par type + exercice » (§5.4/§9). Un compteur par
-- (type_document, exercice) ; le serveur étant écrivain unique, l'incrément est
-- naturellement sérialisé, sans risque de doublon.

CREATE TABLE sequence_numero (
    type_document TEXT NOT NULL,
    exercice      INTEGER NOT NULL,   -- année (ex. 2026)
    dernier       INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (type_document, exercice)
);

-- Préfixe lisible par type de document (piloté par la donnée, pas en dur).
CREATE TABLE config_prefixe_document (
    type_document TEXT PRIMARY KEY,
    prefixe       TEXT NOT NULL
);
INSERT INTO config_prefixe_document (type_document, prefixe) VALUES
    ('devis',     'DV'),
    ('facture',   'FA'),
    ('avoir',     'AV'),
    ('commande',  'BC'),
    ('livraison', 'BL'),
    ('proforma',  'PF');
