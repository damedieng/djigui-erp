-- Djigui Desktop — migration 0042 : SAUVEGARDE AUTOMATIQUE CHIFFRÉE.
--
-- # Pourquoi cette migration existe
--
-- Jusqu'ici, la seule copie des données de l'utilisateur était le fichier
-- `djigui.db` posé sur une machine. Un disque qui lâche, un ordinateur volé,
-- un ransomware — et dix ans de ventes, de marchés et bientôt de bulletins de
-- paie disparaissent. Une sauvegarde n'est pas un confort : c'est la seule
-- chose qui distingue une panne d'une fermeture d'entreprise.
--
-- # Trois décisions structurantes, prises avec l'utilisateur
--
-- 1. **L'utilisateur choisit OÙ.** Pas un dossier imposé : une LISTE de
--    destinations (disque local, dossier Google Drive Desktop, clé USB, partage
--    réseau). Une copie au même endroit que l'original ne protège de rien ; il
--    en faut au moins une qui parte physiquement ailleurs.
--    D'où une table `sauvegarde_destination`, pas une colonne `chemin`.
--
-- 2. **Le fichier est CHIFFRÉ.** Une sauvegarde circule : elle traîne sur une
--    clé USB, se synchronise chez Google, se copie sur le poste du comptable.
--    En clair, n'importe qui l'ouvrirait avec un lecteur SQLite gratuit et y
--    lirait les salaires, les marges et les coordonnées des clients.
--    Trois modes :
--      • `licence`    — **LE MODE NORMAL, décision de l'utilisateur.** La clé de
--                       chiffrement est dérivée de la **clé de licence remise au
--                       client à l'installation**. C'est le meilleur des deux
--                       mondes : le secret est **propre à chaque client** (il ne
--                       se trouve donc pas dans l'exécutable, contrairement à la
--                       clé intégrée), et il reste **récupérable** — la licence
--                       figure sur les documents d'installation et chez
--                       SODEVITEL. Un client qui perd sa machine ET sa base peut
--                       redemander sa licence et rouvrir ses sauvegardes.
--      • `integree`   — clé DJIGUI embarquée dans le logiciel. Sert uniquement
--                       **avant la saisie de la licence** (installation toute
--                       neuve) : sans elle, une machine non encore activée ne
--                       pourrait pas se sauvegarder du tout.
--                       ⚠️ Protège des curieux, PAS d'un technicien déterminé.
--      • `motdepasse` — phrase choisie par le client, pour qui veut un secret
--                       que même SODEVITEL ne détient pas.
--                       ⚠️ Perdue = sauvegardes définitivement illisibles.
--
--    ⚠️ **Chaque archive garde le mode avec lequel elle a été écrite.** Saisir
--    la licence plus tard ne rend donc pas illisibles les copies faites avant :
--    elles s'ouvrent toujours avec la clé intégrée. Aucune archive n'est jamais
--    réécrite — toucher à la seule copie de secours serait le pire moment pour
--    prendre un risque.
--    Le fichier porte son mode dans son entête EN CLAIR : la restauration sait
--    donc quoi demander avant même de tenter de déchiffrer.
--
-- 3. **Seule la machine serveur sauvegarde.** Elle seule détient le fichier de
--    base et le dossier des documents ; les postes clients passent par le
--    réseau et n'ont rien à copier. Sans ce verrou, le jour où le mode client
--    arrivera, trois postes écriraient trois archives concurrentes dans le
--    même dossier Drive — et la plus incomplète gagnerait.
--
-- # Ce qu'on sauvegarde
--
-- La base **ET** le dossier `documents/`. Une base sans ses pièces jointes
-- restaure des factures qui pointent dans le vide : ce n'est pas une
-- sauvegarde, c'est une illusion de sauvegarde.
--
-- # Ce qu'on ne fait pas
--
-- Aucun envoi vers un service en ligne, aucun compte, aucun OAuth. Djigui
-- ÉCRIT DES FICHIERS dans des dossiers. Si l'un de ces dossiers se trouve être
-- celui que Google Drive Desktop synchronise, la copie part dans le nuage sans
-- que Djigui n'ait jamais parlé à Google. Cela fonctionne hors ligne, sans
-- compte, et ne casse pas le jour où une interface distante change.

-- ---------------------------------------------------------------------------
-- Réglages généraux (singleton)
-- ---------------------------------------------------------------------------
CREATE TABLE parametres_sauvegarde (
    singleton                INTEGER PRIMARY KEY CHECK (singleton = 1),

    -- Interrupteur général de la sauvegarde automatique.
    activee                  INTEGER NOT NULL DEFAULT 1,

    -- ⚠️ VERROU DE RÔLE. Tant que ce drapeau est à 0, cette machine ne
    -- sauvegarde RIEN, même si des destinations sont configurées. Il vaut 1
    -- par défaut car aujourd'hui chaque installation est autonome (elle est
    -- donc son propre serveur) ; le jour du mode client, l'installateur le
    -- passera à 0 sur les postes.
    cette_machine_est_serveur INTEGER NOT NULL DEFAULT 1,

    -- Déclencheur retenu avec l'utilisateur : à la fermeture de l'application,
    -- quand la journée est finie et que la base est au repos.
    a_la_fermeture           INTEGER NOT NULL DEFAULT 1,

    -- Rotation. Une sauvegarde qui ne s'efface jamais finit par remplir le
    -- disque, et un disque plein empêche la sauvegarde SUIVANTE : le mécanisme
    -- se saborde lui-même. On garde les N plus récentes par destination.
    copies_a_conserver       INTEGER NOT NULL DEFAULT 10 CHECK (copies_a_conserver >= 1),

    -- Mode de chiffrement. Défaut 'integree' : une installation neuve n'a pas
    -- encore de licence saisie, et elle doit pouvoir se sauvegarder dès le
    -- premier jour. La saisie de la licence bascule automatiquement en
    -- 'licence' — c'est le mode normal en exploitation.
    mode_cle                 TEXT NOT NULL DEFAULT 'integree'
                             CHECK (mode_cle IN ('licence', 'integree', 'motdepasse')),

    -- Sel du mot de passe, et EMPREINTE de vérification (jamais le mot de
    -- passe lui-même). L'empreinte sert seulement à dire « ce n'est pas le bon
    -- mot de passe » AVANT de lancer une restauration de plusieurs minutes.
    -- Elle ne permet pas de retrouver le mot de passe.
    sel_mot_de_passe         TEXT,
    empreinte_mot_de_passe   TEXT,

    -- Dernière tentative, pour l'affichage « dernière sauvegarde : … ».
    derniere_sauvegarde      TEXT,
    dernier_statut           TEXT,

    maj_le                   TEXT NOT NULL
);

INSERT INTO parametres_sauvegarde (singleton, maj_le)
VALUES (1, datetime('now'));

-- Clé de licence remise au client à l'installation. Vide tant qu'elle n'a pas
-- été saisie. Elle sert AUJOURD'HUI de secret de chiffrement des sauvegardes ;
-- elle servira plus tard à la couche de licence gratuit/payant prévue par la
-- spec (§3.4). On la range dans les paramètres globaux plutôt que dans la table
-- de sauvegarde : elle ne lui appartient pas, elle identifie l'installation.
INSERT INTO parametre_global (cle, valeur) VALUES ('licence_installation', '');

-- ---------------------------------------------------------------------------
-- Où écrire les copies
-- ---------------------------------------------------------------------------
CREATE TABLE sauvegarde_destination (
    id       TEXT PRIMARY KEY,
    -- Nom en langage d'utilisateur : « Clé USB bleue », « Dossier Drive ».
    -- C'est ce nom qui apparaîtra dans le message d'échec ; « E:\ » ne dit
    -- rien à quelqu'un six mois plus tard.
    libelle  TEXT NOT NULL,
    chemin   TEXT NOT NULL,
    actif    INTEGER NOT NULL DEFAULT 1,
    ordre    INTEGER NOT NULL DEFAULT 0,

    -- Résultat de la dernière écriture sur CETTE destination. Une clé USB
    -- débranchée depuis trois semaines doit se voir d'un coup d'œil, sinon on
    -- se croit sauvegardé alors qu'une seule copie sur trois part encore.
    dernier_essai   TEXT,
    dernier_statut  TEXT,
    dernier_message TEXT,

    cree_le  TEXT NOT NULL
);

CREATE UNIQUE INDEX idx_sauvegarde_destination_chemin ON sauvegarde_destination(chemin);

-- ---------------------------------------------------------------------------
-- Journal des sauvegardes
-- ---------------------------------------------------------------------------
--
-- On journalise aussi les ÉCHECS, et c'est le point important : une sauvegarde
-- qui échoue en silence est pire que pas de sauvegarde du tout, parce qu'elle
-- installe une fausse tranquillité.
CREATE TABLE sauvegarde_journal (
    id            TEXT PRIMARY KEY,
    horodatage    TEXT NOT NULL,

    -- 'fermeture' | 'manuelle' — d'où venait le déclenchement.
    declencheur   TEXT NOT NULL,

    nom_fichier   TEXT,
    taille_octets INTEGER,

    -- 'succes' | 'partiel' | 'echec'. « partiel » = au moins une destination
    -- a reçu la copie, une autre non. Ce n'est ni un succès ni un échec, et
    -- confondre les trois empêcherait de comprendre ce qui se passe.
    statut        TEXT NOT NULL CHECK (statut IN ('succes', 'partiel', 'echec')),

    nb_destinations_ok    INTEGER NOT NULL DEFAULT 0,
    nb_destinations_echec INTEGER NOT NULL DEFAULT 0,

    -- Relecture de contrôle : on rouvre l'archive écrite et on vérifie qu'elle
    -- se déchiffre. Une copie qu'on n'a pas su rouvrir n'est pas une copie.
    verifiee      INTEGER NOT NULL DEFAULT 0,

    -- Message en langage clair, destiné à l'utilisateur, pas une trace technique.
    message       TEXT
);

CREATE INDEX idx_sauvegarde_journal_date ON sauvegarde_journal(horodatage DESC);
