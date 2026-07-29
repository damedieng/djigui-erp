-- Djigui Desktop — migration 0034 : COMPTABILITÉ (écran réservé au comptable).
--
-- Procédé validé avec l'utilisateur (2026-07-27), qui INVERSE l'approche
-- classique. Djigui ne devine RIEN en comptabilité :
--
--   1. Le COMPTABLE crée ses propres comptes (aucun plan imposé ; le plan OHADA
--      de base est proposé en un clic, jamais forcé).
--   2. Il écrit des RÈGLES multicritères (« les ventes de la catégorie Boissons
--      → 701 »). Il les pose UNE fois.
--   3. Les règles s'appliquent à TOUT L'HISTORIQUE DÉJÀ EN BASE — c'est le point
--      décisif : un comptable qui arrive dans six mois range le passé. Elles
--      s'appliquent ensuite d'elles-mêmes aux opérations futures.
--   4. Ce qu'aucune règle ne couvre tombe dans la corbeille « À ranger », qu'il
--      vide à la main. Cette corbeille est aussi son signal qu'une situation
--      nouvelle est apparue dans la boutique.
--
-- Règle d'or reprise de plan_comptable.md §0 : LA COMPTABILITÉ N'EMPÊCHE JAMAIS
-- DE VENDRE. Rien ici n'est branché sur le flux de caisse ; aucune écriture
-- n'est un prérequis d'une vente. Compte introuvable → compte d'attente 471 +
-- alerte jaune, jamais un refus.
--
-- Et en cas d'ambiguïté, c'est LE COMPTABLE QUI TRANCHE (décision utilisateur,
-- textuelle) : Djigui propose, il dispose.

-- ---------------------------------------------------------------------------
-- Les comptes — créés par le comptable, pas par nous
-- ---------------------------------------------------------------------------
CREATE TABLE compte (
    -- Le numéro EST la clé : c'est ainsi que les comptables les désignent.
    -- Texte et non entier : un compte peut valoir « 4011 » ou « 411CLI001 »,
    -- et « 06 » ne doit pas devenir « 6 ».
    numero      TEXT PRIMARY KEY,
    libelle     TEXT NOT NULL,
    -- Classe OHADA 1 à 8, déduite du premier chiffre à la création mais stockée
    -- (le comptable peut créer un compte hors norme, on ne le lui interdit pas).
    classe      INTEGER,
    -- Sens habituel du solde. Purement indicatif : sert à signaler un solde
    -- anormal dans la balance, jamais à refuser une écriture.
    sens_normal TEXT CHECK (sens_normal IN ('debit','credit')),
    -- Comptes de tiers (411/401) : on peut y rapprocher facture et règlement.
    lettrable   INTEGER NOT NULL DEFAULT 0,
    actif       INTEGER NOT NULL DEFAULT 1,
    note        TEXT,
    cree_par    TEXT,
    cree_le     TEXT NOT NULL
);
CREATE INDEX idx_compte_classe ON compte(classe);

-- Le SEUL compte que nous imposons, et pour une raison technique : il faut
-- toujours pouvoir écrire quelque part plutôt que de perdre une opération.
-- Le comptable le verra dans sa corbeille et le remplacera par le bon compte.
INSERT INTO compte (numero, libelle, classe, sens_normal, lettrable, cree_le)
VALUES ('471', 'Compte d''attente — à ranger', 4, 'debit', 0, datetime('now'));

-- ---------------------------------------------------------------------------
-- Les journaux — où se range chaque écriture
-- ---------------------------------------------------------------------------
CREATE TABLE journal_comptable (
    code    TEXT PRIMARY KEY,
    libelle TEXT NOT NULL,
    ordre   INTEGER NOT NULL DEFAULT 0,
    actif   INTEGER NOT NULL DEFAULT 1
);
INSERT INTO journal_comptable (code, libelle, ordre) VALUES
    ('VT', 'Journal des ventes',            1),
    ('AC', 'Journal des achats',            2),
    ('CA', 'Journal de caisse',             3),
    ('BQ', 'Journal de banque',             4),
    ('ST', 'Journal des stocks',            5),
    ('OD', 'Opérations diverses',           6);

-- ---------------------------------------------------------------------------
-- Les règles de rattachement — le cœur du procédé
--
-- UNE règle = « pour ce RÔLE, quand ces critères sont réunis, prends CE compte ».
--
-- Le rôle dit quelle place le compte occupe dans l'écriture. Le moteur connaît
-- le schéma de chaque opération (voir comptabilite.rs) ; la règle ne fait que
-- NOMMER les comptes. Djigui, lui, connaît déjà tous les montants.
--
--   produit     — ce que la vente rapporte            (701, 702, 706…)
--   charge      — ce que l'achat coûte                (601, 602…)
--   tiers       — client ou fournisseur               (411, 401)
--   taxe        — TVA collectée ou déductible         (4431, 4451)
--   tresorerie  — caisse ou banque                    (571, 521)
--   stock       — valeur du stock                     (31, 32, 36)
--
-- Tous les critères sont FACULTATIFS : NULL = « peu importe ». Une règle sans
-- aucun critère est le défaut du rôle. La règle retenue est la PLUS SPÉCIFIQUE
-- (le plus grand nombre de critères renseignés), `ordre` départageant les ex æquo.
-- ---------------------------------------------------------------------------
CREATE TABLE regle_comptable (
    id            TEXT PRIMARY KEY,
    nom           TEXT NOT NULL,
    role          TEXT NOT NULL CHECK (role IN
                     ('produit','charge','tiers','taxe','tresorerie','stock')),
    compte_numero TEXT NOT NULL REFERENCES compte(numero),

    -- Critères — tous facultatifs, combinables (recherche multicritère).
    -- Nature de l'opération : vente | achat | encaissement | decaissement | stock
    domaine           TEXT CHECK (domaine IN
                         ('vente','achat','encaissement','decaissement','stock')),
    categorie_id      TEXT REFERENCES categorie(id),
    article_id        TEXT REFERENCES article(id),
    -- marchandise | matiere_premiere | produit_fini | service (migration 0032) :
    -- c'est le critère qui distingue le négoce de la production.
    nature_comptable  TEXT,
    tiers_id          TEXT REFERENCES tiers(id),
    -- particulier | entreprise
    nature_tiers      TEXT,
    caisse_id         TEXT REFERENCES caisse(id),
    moyen_paiement_id TEXT REFERENCES moyen_paiement(id),
    -- espece | mobile_money | virement | cheque — pilote caisse (571) vs banque (521)
    famille_paiement  TEXT,
    depot_id          TEXT REFERENCES depot(id),
    taux_taxe         NUMERIC,
    montant_min       NUMERIC,
    montant_max       NUMERIC,
    libelle_contient  TEXT,

    -- Journal forcé ; sinon le moteur choisit selon le domaine.
    journal_code  TEXT REFERENCES journal_comptable(code),
    ordre         INTEGER NOT NULL DEFAULT 0,
    actif         INTEGER NOT NULL DEFAULT 1,
    note          TEXT,
    cree_par      TEXT,
    cree_le       TEXT NOT NULL
);
CREATE INDEX idx_regle_role    ON regle_comptable(role, actif);
CREATE INDEX idx_regle_domaine ON regle_comptable(domaine);

-- ---------------------------------------------------------------------------
-- Les écritures — partie double, toujours équilibrées
--
-- Invariant absolu : Σ débit = Σ crédit sur chaque écriture. Vérifié par le
-- cœur avant insertion (transaction) et couvert par les tests. C'est le seul
-- endroit de Djigui où l'on refuse d'écrire une donnée incohérente : une
-- écriture déséquilibrée n'est pas une souplesse, c'est une faute.
--
-- Une écriture n'est JAMAIS modifiée ni supprimée : on la contre-passe. Même
-- réflexe que les paiements (migration 0019) et que le journal de stock.
-- ---------------------------------------------------------------------------
CREATE TABLE ecriture (
    id           TEXT PRIMARY KEY,
    journal_code TEXT NOT NULL REFERENCES journal_comptable(code),
    date         TEXT NOT NULL,
    libelle      TEXT NOT NULL,
    -- Exercice = année de la date (AAAA). Dénormalisé pour filtrer vite.
    exercice     INTEGER NOT NULL,

    -- D'où vient l'écriture, pour ne jamais la produire deux fois et pour
    -- remonter à la pièce d'origine depuis le grand livre.
    origine_type TEXT NOT NULL CHECK (origine_type IN
                    ('document','paiement','mouvement','manuel','contrepassation')),
    origine_id   TEXT,

    -- Vrai tant qu'aucune ligne ne pointe sur le compte d'attente 471 : sert à
    -- alimenter la corbeille « À ranger » sans recalcul.
    complete     INTEGER NOT NULL DEFAULT 1,
    -- Écriture qui annule celle-ci (contre-passation), et réciproque.
    contrepasse_de TEXT REFERENCES ecriture(id),
    note         TEXT,
    cree_par     TEXT,
    cree_le      TEXT NOT NULL
);
CREATE INDEX idx_ecriture_date     ON ecriture(date);
CREATE INDEX idx_ecriture_journal  ON ecriture(journal_code, exercice);
CREATE INDEX idx_ecriture_complete ON ecriture(complete);
-- Une pièce ne doit produire qu'une écriture : garde-fou contre le double
-- comptage si le comptable relance le rattachement plusieurs fois.
CREATE UNIQUE INDEX idx_ecriture_origine
    ON ecriture(origine_type, origine_id)
    WHERE origine_id IS NOT NULL AND origine_type <> 'contrepassation';

CREATE TABLE ecriture_ligne (
    id            TEXT PRIMARY KEY,
    ecriture_id   TEXT NOT NULL REFERENCES ecriture(id),
    compte_numero TEXT NOT NULL REFERENCES compte(numero),
    libelle       TEXT,
    -- Une ligne porte soit un débit, soit un crédit ; jamais les deux
    -- (contrainte de table en fin de déclaration : SQLite n'accepte plus de
    -- colonne après une contrainte de table).
    debit         NUMERIC NOT NULL DEFAULT 0 CHECK (debit  >= 0),
    credit        NUMERIC NOT NULL DEFAULT 0 CHECK (credit >= 0),
    -- Rattachement au tiers pour la balance auxiliaire (contrôle croisé avec
    -- tiers.solde, que le module paiement tient déjà de son côté).
    tiers_id      TEXT REFERENCES tiers(id),
    -- Code de lettrage : rapproche une facture de son règlement. NULL = non lettré.
    lettrage      TEXT,
    -- Rôle ayant produit la ligne : permet au comptable de comprendre d'où elle
    -- vient et de rejouer une règle corrigée.
    role          TEXT,
    ordre         INTEGER NOT NULL DEFAULT 0,
    CHECK (debit = 0 OR credit = 0)
);
CREATE INDEX idx_ecrligne_ecriture ON ecriture_ligne(ecriture_id);
CREATE INDEX idx_ecrligne_compte   ON ecriture_ligne(compte_numero);
CREATE INDEX idx_ecrligne_tiers    ON ecriture_ligne(tiers_id);
CREATE INDEX idx_ecrligne_lettrage ON ecriture_ligne(lettrage);
