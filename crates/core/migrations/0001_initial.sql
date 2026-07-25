-- Djigui Desktop — schéma initial
-- Conforme à la spec §5. Trois paris : tiers unifié, document unifié, stock en journal.
-- SQLite : uuid/enum -> TEXT, decimal -> NUMERIC, boolean -> INTEGER 0/1.
-- Le mode WAL et les FK sont activés à l'ouverture de la connexion (voir db.rs).

-- ---------------------------------------------------------------------------
-- 5.1 tiers  (un seul tiers, pas de client/fournisseur séparés — pari §3.1)
-- ---------------------------------------------------------------------------
CREATE TABLE tiers (
    id         TEXT PRIMARY KEY,
    code       TEXT NOT NULL UNIQUE,
    type_role  TEXT NOT NULL CHECK (type_role IN ('client','fournisseur','les_deux')),
    nom        TEXT NOT NULL,
    telephone  TEXT,
    adresse    TEXT,
    ninea      TEXT,
    solde      NUMERIC NOT NULL DEFAULT 0,   -- dérivé, tenu à jour à l'écriture (§6.4)
    actif      INTEGER NOT NULL DEFAULT 1,
    cree_le    TEXT NOT NULL
);

-- ---------------------------------------------------------------------------
-- 5.2 article
-- ---------------------------------------------------------------------------
CREATE TABLE article (
    id           TEXT PRIMARY KEY,
    code         TEXT NOT NULL UNIQUE,
    type         TEXT NOT NULL CHECK (type IN ('bien','service')),
    designation  TEXT NOT NULL,
    prix_vente   NUMERIC NOT NULL DEFAULT 0,
    prix_achat   NUMERIC,
    taux_tva     NUMERIC NOT NULL DEFAULT 0,
    gere_stock   INTEGER NOT NULL DEFAULT 0,  -- toujours 0 si type='service'
    stock_alerte NUMERIC,
    actif        INTEGER NOT NULL DEFAULT 1,
    -- garde-fou du pari §3.2 : un service ne gère jamais le stock
    CHECK (type <> 'service' OR gere_stock = 0)
);

-- ---------------------------------------------------------------------------
-- 5.3 depot
-- ---------------------------------------------------------------------------
CREATE TABLE depot (
    id         TEXT PRIMARY KEY,
    nom        TEXT NOT NULL,
    par_defaut INTEGER NOT NULL DEFAULT 0
);

-- ---------------------------------------------------------------------------
-- 5.4 document / document_ligne / facture_detail  (document unifié — pari §3.2)
-- ---------------------------------------------------------------------------
CREATE TABLE document (
    id                 TEXT PRIMARY KEY,
    numero             TEXT NOT NULL,
    type_document      TEXT NOT NULL CHECK (type_document IN
                         ('devis','facture','avoir','commande','livraison','proforma')),
    sens               TEXT NOT NULL CHECK (sens IN ('vente','achat')),
    tiers_id           TEXT NOT NULL REFERENCES tiers(id),
    depot_id           TEXT REFERENCES depot(id),
    date               TEXT NOT NULL,
    statut             TEXT NOT NULL DEFAULT 'brouillon' CHECK (statut IN
                         ('brouillon','valide','accepte','transforme','annule')),
    document_source_id TEXT REFERENCES document(id),  -- traçabilité transformation (§5.4)
    total_ht           NUMERIC NOT NULL DEFAULT 0,     -- dérivé des lignes
    total_tva          NUMERIC NOT NULL DEFAULT 0,     -- dérivé
    total_ttc          NUMERIC NOT NULL DEFAULT 0,     -- dérivé
    note               TEXT,
    cree_le            TEXT NOT NULL,
    UNIQUE (type_document, numero)
);
CREATE INDEX idx_document_tiers  ON document(tiers_id);
CREATE INDEX idx_document_type   ON document(type_document, sens);
CREATE INDEX idx_document_source ON document(document_source_id);

CREATE TABLE document_ligne (
    id             TEXT PRIMARY KEY,
    document_id    TEXT NOT NULL REFERENCES document(id) ON DELETE CASCADE,
    article_id     TEXT NOT NULL REFERENCES article(id),
    designation    TEXT NOT NULL,              -- copiée de l'article, éditable
    quantite       NUMERIC NOT NULL,
    prix_unitaire  NUMERIC NOT NULL,
    taux_tva       NUMERIC NOT NULL DEFAULT 0, -- copié de l'article, éditable
    remise         NUMERIC NOT NULL DEFAULT 0, -- %
    total_ligne_ht NUMERIC NOT NULL DEFAULT 0  -- dérivé
);
CREATE INDEX idx_ligne_document ON document_ligne(document_id);

-- extension 1-1, uniquement pour les documents de type 'facture' (§5.4)
CREATE TABLE facture_detail (
    document_id         TEXT PRIMARY KEY REFERENCES document(id) ON DELETE CASCADE,
    date_echeance       TEXT,
    conditions_paiement TEXT,
    mentions_legales    TEXT
);

-- ---------------------------------------------------------------------------
-- 5.5 mouvement_stock  (le stock est un journal — pari §3.3)
-- Un mouvement n'est JAMAIS modifié ni supprimé : on corrige par un inverse.
-- ---------------------------------------------------------------------------
CREATE TABLE mouvement_stock (
    id          TEXT PRIMARY KEY,
    article_id  TEXT NOT NULL REFERENCES article(id),
    depot_id    TEXT NOT NULL REFERENCES depot(id),
    document_id TEXT REFERENCES document(id),   -- nullable : inventaire, casse, transfert
    sens        TEXT NOT NULL CHECK (sens IN ('entree','sortie')),
    quantite    NUMERIC NOT NULL CHECK (quantite > 0), -- toujours positive ; 'sens' donne le signe
    motif       TEXT NOT NULL CHECK (motif IN
                  ('vente','achat','inventaire','casse','transfert','production')),
    date        TEXT NOT NULL
);
CREATE INDEX idx_mvt_article_depot ON mouvement_stock(article_id, depot_id);
CREATE INDEX idx_mvt_document      ON mouvement_stock(document_id);

-- ---------------------------------------------------------------------------
-- 5.6 caisse / paiement
-- ---------------------------------------------------------------------------
CREATE TABLE caisse (
    id    TEXT PRIMARY KEY,
    nom   TEXT NOT NULL,
    solde NUMERIC NOT NULL DEFAULT 0   -- dérivé, tenu à jour à l'écriture (§6.4)
);

CREATE TABLE paiement (
    id          TEXT PRIMARY KEY,
    tiers_id    TEXT NOT NULL REFERENCES tiers(id),
    caisse_id   TEXT NOT NULL REFERENCES caisse(id),
    document_id TEXT REFERENCES document(id),
    sens        TEXT NOT NULL CHECK (sens IN ('encaissement','decaissement')),
    montant     NUMERIC NOT NULL,
    mode        TEXT NOT NULL CHECK (mode IN ('espece','mobile_money','virement','cheque')),
    date        TEXT NOT NULL
);
CREATE INDEX idx_paiement_tiers  ON paiement(tiers_id);
CREATE INDEX idx_paiement_caisse ON paiement(caisse_id);

-- ---------------------------------------------------------------------------
-- 5.7 production
-- ---------------------------------------------------------------------------
CREATE TABLE ordre_production (
    id                  TEXT PRIMARY KEY,
    numero              TEXT NOT NULL,
    article_produit_id  TEXT NOT NULL REFERENCES article(id),
    quantite            NUMERIC NOT NULL,
    depot_id            TEXT NOT NULL REFERENCES depot(id),
    statut              TEXT NOT NULL DEFAULT 'brouillon' CHECK (statut IN
                          ('brouillon','en_cours','termine','annule')),
    date                TEXT NOT NULL
);

CREATE TABLE production_composant (
    id         TEXT PRIMARY KEY,
    ordre_id   TEXT NOT NULL REFERENCES ordre_production(id) ON DELETE CASCADE,
    article_id TEXT NOT NULL REFERENCES article(id),
    quantite   NUMERIC NOT NULL             -- par unité produite
);
CREATE INDEX idx_composant_ordre ON production_composant(ordre_id);

-- ---------------------------------------------------------------------------
-- 5.8 abonnement (facturation cyclique)
-- ---------------------------------------------------------------------------
CREATE TABLE abonnement (
    id                 TEXT PRIMARY KEY,
    tiers_id           TEXT NOT NULL REFERENCES tiers(id),
    document_modele_id TEXT NOT NULL REFERENCES document(id),
    frequence          TEXT NOT NULL CHECK (frequence IN ('mensuel','trimestriel','annuel')),
    prochaine_echeance TEXT NOT NULL,
    actif              INTEGER NOT NULL DEFAULT 1
);

-- ---------------------------------------------------------------------------
-- 5.9 parametres_entreprise (singleton — une seule ligne)
-- ---------------------------------------------------------------------------
CREATE TABLE parametres_entreprise (
    id              TEXT PRIMARY KEY,
    singleton       INTEGER NOT NULL DEFAULT 1 UNIQUE CHECK (singleton = 1), -- garantit 1 ligne
    raison_sociale  TEXT NOT NULL DEFAULT '',
    ninea           TEXT NOT NULL DEFAULT '',
    rccm            TEXT,
    adresse         TEXT NOT NULL DEFAULT '',
    telephone       TEXT NOT NULL DEFAULT '',
    email           TEXT,
    logo            TEXT,
    devise          TEXT NOT NULL DEFAULT 'FCFA',
    taux_tva_defaut NUMERIC NOT NULL DEFAULT 18,
    pied_facture    TEXT
);

-- ---------------------------------------------------------------------------
-- Configuration pilotée par la donnée (§3.2 / §6.1) — PAS de règles en dur.
-- Comportement de chaque type de document vis-à-vis du stock et des transformations.
-- ---------------------------------------------------------------------------
CREATE TABLE config_type_document (
    type_document   TEXT PRIMARY KEY,
    impacte_stock   INTEGER NOT NULL DEFAULT 0,  -- crée un mouvement à la validation ?
    mouvement_inverse INTEGER NOT NULL DEFAULT 0 -- avoir : inverse le sens normal
);
INSERT INTO config_type_document (type_document, impacte_stock, mouvement_inverse) VALUES
    ('devis',     0, 0),
    ('proforma',  0, 0),
    ('commande',  0, 0),
    ('livraison', 1, 0),
    ('facture',   1, 0),
    ('avoir',     1, 1);   -- impacte le stock, mouvement inverse

-- Transformations autorisées : (source -> cible), pilotées par la donnée (§6.2).
CREATE TABLE config_transformation (
    type_source TEXT NOT NULL,
    type_cible  TEXT NOT NULL,
    statut_source_requis TEXT NOT NULL, -- statut exigé sur la source pour transformer
    PRIMARY KEY (type_source, type_cible)
);
INSERT INTO config_transformation (type_source, type_cible, statut_source_requis) VALUES
    ('devis',    'facture',   'accepte'),
    ('proforma', 'facture',   'accepte'),
    ('commande', 'facture',   'valide'),
    ('commande', 'livraison', 'valide'),
    ('livraison','facture',   'valide');

-- ---------------------------------------------------------------------------
-- Paramètres globaux clé/valeur (ex. « gestion de stock active » §6.1).
-- ---------------------------------------------------------------------------
CREATE TABLE parametre_global (
    cle    TEXT PRIMARY KEY,
    valeur TEXT NOT NULL
);
INSERT INTO parametre_global (cle, valeur) VALUES
    ('gestion_stock_active', '1');
