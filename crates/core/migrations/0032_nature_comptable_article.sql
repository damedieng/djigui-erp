-- Djigui Desktop — migration 0032 : NATURE COMPTABLE DES ARTICLES.
--
-- Décision utilisateur (2026-07-26) : un article porte sa **nature comptable
-- OHADA**. Ce champ unique sert DEUX choses à la fois :
--
--   1. L'ÉCRAN — la caisse ne doit proposer que ce qui se vend (on ne vend pas
--      un sac de farine à un client de boulangerie), et les recettes ne doivent
--      proposer que ce qui se consomme.
--   2. LA COMPTABILITÉ — en OHADA, un négociant et un fabricant ne se
--      comptabilisent pas pareil, et c'est cette nature qui décide des comptes :
--
--        marchandise      achetée pour être revendue EN L'ÉTAT
--                         → 601 achats / 701 ventes / 31 stock marchandises
--                         → résultat lu en MARGE COMMERCIALE
--        matiere_premiere achetée pour être TRANSFORMÉE
--                         → 602 achats / 32 stock matières premières
--        produit_fini     FABRIQUÉ par l'entreprise
--                         → 702 ventes / 36 stock produits finis
--                         → 73 production stockée (ce qu'on a fabriqué et gardé
--                           est un PRODUIT de l'exercice, pas une charge)
--        service          ni stock ni transformation → 706 ventes de services
--
-- Ranger un article dans la mauvaise nature fausse le compte de résultat : de la
-- farine comptabilisée en 601 donne une marge commerciale fausse et fait
-- disparaître la production stockée. D'où un champ explicite, pas une déduction.
--
-- ⚠️ Comme partout ailleurs : ce champ ne bloque jamais rien. Il oriente les
-- listes et, plus tard, les écritures. Un article mal classé se reclasse.

ALTER TABLE article ADD COLUMN nature_comptable TEXT NOT NULL DEFAULT 'marchandise'
    CHECK (nature_comptable IN ('marchandise','matiere_premiere','produit_fini','service'));

-- Reprise de l'existant : personne ne doit ressaisir son catalogue. On déduit la
-- nature de ce que l'article FAIT DÉJÀ dans la base, du plus spécifique au plus
-- général (l'ordre des UPDATE compte : le dernier écrit gagne).

-- 1. Consommé dans une recette ou un ordre de fabrication → matière première.
--    ⚠️ MAIS seulement si Djigui est certain que l'article ne se vend pas :
--    aucun prix de vente ET jamais apparu dans un document. Sans ce garde-fou,
--    le sac de ciment revendu entier ET reconditionné disparaîtrait de la
--    caisse à la première migration — un dégât bien pire que l'absence de
--    classement. Mêmes conditions qu'à l'exécution (`classer_matiere_premiere`).
UPDATE article SET nature_comptable = 'matiere_premiere'
 WHERE (id IN (SELECT article_id FROM nomenclature_composant)
     OR id IN (SELECT article_id FROM production_composant))
   AND prix_vente = 0
   AND NOT EXISTS (SELECT 1 FROM document_ligne dl WHERE dl.article_id = article.id);

-- 2. Fabriqué (recette ou ordre) → produit fini. Prioritaire sur le cas 1 : un
--    semi-fini est consommé ET fabriqué ; c'est sa fabrication qui le définit,
--    car c'est elle qui porte le coût de production.
UPDATE article SET nature_comptable = 'produit_fini'
 WHERE id IN (SELECT article_id FROM nomenclature)
    OR id IN (SELECT article_produit_id FROM ordre_production);

-- 3. Un service reste un service, quoi qu'il arrive (il ne peut pas être stocké).
UPDATE article SET nature_comptable = 'service' WHERE type = 'service';

-- Le reste conserve le défaut 'marchandise' : c'est le cas de l'immense majorité
-- des articles d'une boutique, et le comportement d'avant cette migration.
