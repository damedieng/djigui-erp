-- Djigui Desktop — migration 0040 : ACTIVATION DES MODULES.
--
-- ⚠️ Ce n'est PAS un filtre d'affichage. À l'installation, la **formule vendue**
-- détermine les modules auxquels le client a droit : c'est une **donnée de
-- facturation**. Un simple `if` dans le menu ne conviendrait pas — il faut
-- pouvoir dire, des mois plus tard, ce qui a été souscrit, quand, et par qui.
--
-- # Deux niveaux, à ne jamais confondre
--
-- | Colonne    | Décidé par           | Nature                                  |
-- |------------|----------------------|-----------------------------------------|
-- | `souscrit` | l'installateur       | **facturation** — le client n'y touche pas |
-- | `actif`    | le client            | **confort** — il masque ce qu'il n'utilise pas |
--
-- Un client qui a souscrit « Production » mais ne fabrique pas encore peut le
-- masquer pour alléger son menu, et le rallumer plus tard : cela ne change rien
-- à ce qu'il paie.
--
-- # Ce qui reste toujours là
--
-- `socle = 1` marque les modules sans lesquels l'application n'a plus de sens
-- (articles, tiers, paramètres, utilisateurs). Ils ne se désactivent jamais.
--
-- # Ce que la désactivation NE fait PAS
--
-- Elle ne touche à **aucune donnée**. Masquer « Marchés » cache le menu ; les
-- marchés, soumissionnaires et avenants restent en base et réapparaissent
-- intacts à la réactivation.

CREATE TABLE module (
    code        TEXT PRIMARY KEY,
    libelle     TEXT NOT NULL,
    -- En langage d'utilisateur, pas de jargon : c'est ce qui s'affiche sur la
    -- carte du module et qui doit lui permettre de comprendre ce qu'il achète.
    description TEXT NOT NULL,
    icone       TEXT NOT NULL,
    -- Regroupement d'affichage sur l'écran des modules.
    famille     TEXT NOT NULL,
    ordre       INTEGER NOT NULL DEFAULT 0,
    -- Indispensable au fonctionnement : ni désactivable, ni « non souscrit ».
    socle       INTEGER NOT NULL DEFAULT 0,
    -- Le client a-t-il droit à ce module ? (formule vendue)
    souscrit    INTEGER NOT NULL DEFAULT 0,
    souscrit_le TEXT,
    souscrit_par TEXT,
    -- Parmi les modules souscrits, celui-ci est-il affiché ?
    actif       INTEGER NOT NULL DEFAULT 1,
    -- Modules nécessaires à celui-ci, séparés par des virgules. NULL = aucun.
    -- Sert à empêcher une configuration qui ne marcherait pas (la caisse sans
    -- le catalogue d'articles, par exemple).
    requiert    TEXT
);

-- La formule retenue à l'installation, pour mémoire : elle explique pourquoi
-- tel jeu de modules a été ouvert, et sert de point de départ si l'on veut la
-- changer plus tard.
INSERT INTO parametre_global (cle, valeur) VALUES ('formule_installee', '');

-- ---------------------------------------------------------------------------
-- Le catalogue. `souscrit` reste à 0 : c'est l'écran d'installation qui ouvre
-- les droits. Seul le socle est ouvert d'office, sans quoi l'application serait
-- inutilisable à la première ouverture.
-- ---------------------------------------------------------------------------
INSERT INTO module (code, libelle, description, icone, famille, ordre, socle, souscrit, actif, requiert) VALUES
 ('socle', 'Base', 'Articles, contacts, paramètres et utilisateurs. Le cœur de l''application, toujours présent.',
  'ti-cube', 'Base', 1, 1, 1, 1, NULL),

 ('caisse', 'Caisse', 'Vendre au comptoir, encaisser, tenir la caisse et faire les comptes du jour.',
  'ti-cash', 'Commerce', 10, 0, 0, 1, 'socle'),
 ('facturation', 'Facturation', 'Devis, factures, avoirs, bons de livraison et achats fournisseurs.',
  'ti-file-invoice', 'Commerce', 11, 0, 0, 1, 'socle'),
 ('abonnements', 'Abonnements', 'Facturer automatiquement des clients à échéance régulière.',
  'ti-repeat', 'Commerce', 12, 0, 0, 1, 'facturation'),
 ('magasins', 'Magasins & stock', 'Plusieurs points de vente ou dépôts, inventaires et mouvements de stock.',
  'ti-building-warehouse', 'Commerce', 13, 0, 0, 1, 'socle'),
 ('production', 'Production', 'Fabriquer à partir de recettes : ordres de fabrication et prix de revient.',
  'ti-tools', 'Commerce', 14, 0, 0, 1, 'socle'),

 ('projets', 'Projets', 'Suivre des projets : activités, planning, budget, équipe et jalons.',
  'ti-briefcase', 'Projets & Marchés', 20, 0, 0, 1, 'socle'),
 ('marches', 'Marchés', 'Passation et suivi des marchés : appels d''offres, attribution, avenants, réception.',
  'ti-gavel', 'Projets & Marchés', 21, 0, 0, 1, 'socle'),

 ('agenda', 'Agenda', 'Rendez-vous et échéances, en calendrier ou en liste.',
  'ti-calendar-event', 'Organisation', 30, 0, 0, 1, 'socle'),
 ('rapports', 'Rapports', 'Journaux de ventes et d''achats, marges, état du stock, encours clients.',
  'ti-chart-bar', 'Pilotage', 40, 0, 0, 1, 'socle'),
 ('comptabilite', 'Comptabilité', 'Plan comptable OHADA, écritures, grand livre et balance.',
  'ti-book-2', 'Pilotage', 41, 0, 0, 1, 'socle');
