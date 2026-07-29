-- Djigui Desktop — migration 0031 : PRODUCTION (spec §5.7, étendue).
--
-- Couvre les trois cas d'usage validés avec l'utilisateur (2026-07-26) :
--   * cuisine / restauration  (recette d'un plat, pertes fréquentes)
--   * atelier / transformation (matières premières → produit fini, par lot)
--   * assemblage / kits        (articles en stock → article composé, instantané)
--
-- Décisions structurantes :
--   1. NOMENCLATURE réutilisable : on enregistre la recette UNE fois par article,
--      l'ordre de production la recopie (les composants restent modifiables au
--      cas par cas — la recette est un modèle, pas une contrainte).
--   2. VALORISATION par les composants consommés : le coût de l'ordre = somme des
--      composants sortis (au prix d'achat de l'article) + frais éventuels. Il
--      donne le prix de revient unitaire du produit fabriqué, qui alimente la
--      marge et le rapport bénéfices.
--   3. ÉCARTS : on distingue le PRÉVU du RÉEL (quantité produite et consommation).
--      L'écart est SIGNALÉ, jamais bloquant — cohérent avec tout le reste de
--      l'application (« la gestion ne doit jamais empêcher de produire »).
--
-- Le stock n'est touché QU'À LA CLÔTURE de l'ordre, par le journal des
-- mouvements (motif `production`), jamais en écriture directe.

-- ---------------------------------------------------------------------------
-- Nomenclature (la « recette ») — modèle réutilisable rattaché à un article
-- ---------------------------------------------------------------------------
CREATE TABLE nomenclature (
    id           TEXT PRIMARY KEY,
    -- L'article fabriqué grâce à cette recette.
    article_id   TEXT NOT NULL REFERENCES article(id),
    nom          TEXT NOT NULL,
    -- Une recette produit N unités (ex. « pâte à pain » = 20 baguettes).
    -- Les composants sont donc exprimés POUR CE LOT, pas par unité : c'est la
    -- façon dont les gens écrivent réellement une recette.
    quantite_produite NUMERIC NOT NULL DEFAULT 1 CHECK (quantite_produite > 0),
    note         TEXT,
    actif        INTEGER NOT NULL DEFAULT 1,
    cree_par     TEXT,
    cree_le      TEXT NOT NULL
);
CREATE INDEX idx_nomenclature_article ON nomenclature(article_id);

CREATE TABLE nomenclature_composant (
    id              TEXT PRIMARY KEY,
    nomenclature_id TEXT NOT NULL REFERENCES nomenclature(id),
    article_id      TEXT NOT NULL REFERENCES article(id),
    -- Quantité pour le lot complet (voir nomenclature.quantite_produite).
    quantite        NUMERIC NOT NULL,
    -- Perte technique attendue en % (épluchures, chutes de tissu, sciure…).
    -- Sert à proposer une consommation réaliste, pas à bloquer.
    perte_pct       NUMERIC NOT NULL DEFAULT 0,
    ordre           INTEGER NOT NULL DEFAULT 0,
    UNIQUE (nomenclature_id, article_id)
);
CREATE INDEX idx_nomcomp_nomenclature ON nomenclature_composant(nomenclature_id);

-- ---------------------------------------------------------------------------
-- Ordre de production (l'ordre de fabrication réel)
--
-- ⚠️ `ordre_production` et `production_composant` existent depuis la migration
-- 0001 (coquilles issues de la spec, jamais alimentées : aucun code n'écrivait
-- dedans). On ne modifie jamais une migration publiée → RECONSTRUCTION de table,
-- en recopiant les lignes éventuelles (INSERT … SELECT) plutôt qu'en les
-- perdant. L'ancienne `production_composant.quantite` valait « par unité
-- produite » : on la multiplie par la quantité de l'ordre pour obtenir la
-- nouvelle quantité prévue, qui porte sur l'ordre entier.
-- ---------------------------------------------------------------------------
ALTER TABLE ordre_production     RENAME TO ordre_production_ancien;
ALTER TABLE production_composant RENAME TO production_composant_ancien;
DROP INDEX IF EXISTS idx_composant_ordre;

CREATE TABLE ordre_production (
    id                 TEXT PRIMARY KEY,
    numero             TEXT NOT NULL UNIQUE,
    article_produit_id TEXT NOT NULL REFERENCES article(id),
    -- Recette d'origine, si l'ordre a été monté depuis une nomenclature.
    -- Conservée pour la traçabilité ; l'ordre reste autonome ensuite.
    nomenclature_id    TEXT REFERENCES nomenclature(id),
    depot_id           TEXT NOT NULL REFERENCES depot(id),
    -- Quantité qu'on prévoit de fabriquer.
    quantite           NUMERIC NOT NULL CHECK (quantite > 0),
    -- Quantité réellement obtenue, saisie à la clôture (NULL avant).
    quantite_produite  NUMERIC,
    statut             TEXT NOT NULL DEFAULT 'brouillon'
                       CHECK (statut IN ('brouillon','en_cours','termine','annule')),
    date               TEXT NOT NULL,
    -- Frais de fabrication à incorporer au coût (main-d'œuvre, énergie, cuisson).
    frais              NUMERIC NOT NULL DEFAULT 0,
    -- Renseignés à la clôture : coût total et prix de revient unitaire obtenus.
    cout_total         NUMERIC,
    cout_unitaire      NUMERIC,
    note               TEXT,
    motif_annulation   TEXT,
    cree_par           TEXT,
    cree_le            TEXT NOT NULL,
    cloture_par        TEXT,
    cloture_le         TEXT
);
CREATE INDEX idx_of_statut  ON ordre_production(statut);
CREATE INDEX idx_of_article ON ordre_production(article_produit_id);
CREATE INDEX idx_of_date    ON ordre_production(date);

CREATE TABLE production_composant (
    id              TEXT PRIMARY KEY,
    ordre_id        TEXT NOT NULL REFERENCES ordre_production(id),
    article_id      TEXT NOT NULL REFERENCES article(id),
    -- Ce qu'on prévoit de consommer pour la quantité de l'ordre.
    quantite_prevue NUMERIC NOT NULL,
    -- Ce qui a réellement été consommé (saisi à la clôture ; NULL = « comme prévu »).
    quantite_reelle NUMERIC,
    -- Coût unitaire figé à la clôture (photo du prix d'achat à cet instant) :
    -- le coût d'une fabrication passée ne doit pas bouger si le prix change.
    cout_unitaire   NUMERIC,
    ordre           INTEGER NOT NULL DEFAULT 0,
    UNIQUE (ordre_id, article_id)
);
CREATE INDEX idx_prodcomp_ordre ON production_composant(ordre_id);

-- Reprise des éventuelles lignes existantes, puis retrait des anciennes tables.
INSERT INTO ordre_production
    (id, numero, article_produit_id, depot_id, quantite, statut, date, cree_le)
SELECT id, numero, article_produit_id, depot_id, quantite, statut, date, date
  FROM ordre_production_ancien;

INSERT INTO production_composant
    (id, ordre_id, article_id, quantite_prevue)
SELECT c.id, c.ordre_id, c.article_id, c.quantite * o.quantite
  FROM production_composant_ancien c
  JOIN ordre_production_ancien o ON o.id = c.ordre_id;

DROP TABLE production_composant_ancien;
DROP TABLE ordre_production_ancien;

-- Numérotation des ordres : même mécanique que les pièces commerciales
-- (table sequence_numero, un compteur par exercice). Préfixe OF.
INSERT INTO config_prefixe_document (type_document, prefixe) VALUES ('production', 'OF');
