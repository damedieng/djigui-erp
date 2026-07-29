-- Djigui Desktop — migration 0033 : CORRECTION du classement de la 0032.
--
-- Constaté sur des données réelles (restaurant, 2026-07-26) : la 0032 rangeait
-- 19 plats en `service` — riz et huile compris, alors que ce sont des
-- ingrédients. Cause : dans les catalogues métier, `article.type` vaut
-- `service` dès que l'article ne gère pas le stock (voir seeder). Ce n'est donc
-- PAS un indicateur fiable de « prestation de service », et la règle « un
-- service reste un service » de la 0032, appliquée en dernier, écrasait les
-- deux classements utiles.
--
-- Effet réel du bug : le riz et l'huile seraient restés proposés à la caisse,
-- et le plat fabriqué n'aurait pas été reconnu comme produit fini — exactement
-- l'inverse de ce qu'on voulait.
--
-- Correction : on refait le classement dans le BON ordre de priorité, du plus
-- faible au plus fort — ce que l'article FAIT prime sur la façon dont il a été
-- créé :
--        service  <  matière première (il est consommé)  <  produit fini (il est fabriqué)
--
-- Les garde-fous de la 0032 sont conservés à l'identique : on ne déclasse jamais
-- un article qui a un prix de vente ou qui a déjà été vendu.

-- 1. Base : le type déclaré à la création (le plus faible des indices).
UPDATE article SET nature_comptable = 'service'     WHERE type = 'service';
UPDATE article SET nature_comptable = 'marchandise' WHERE type <> 'service';

-- 2. Consommé en fabrication → matière première (si sûr qu'il ne se vend pas).
UPDATE article SET nature_comptable = 'matiere_premiere'
 WHERE (id IN (SELECT article_id FROM nomenclature_composant)
     OR id IN (SELECT article_id FROM production_composant))
   AND prix_vente = 0
   AND NOT EXISTS (SELECT 1 FROM document_ligne dl WHERE dl.article_id = article.id);

-- 3. Fabriqué → produit fini. Priorité maximale : un plat cuisiné est un produit
--    fini, quoi qu'ait dit son type à la création. Sans garde-fou ici : ce qu'on
--    fabrique se vend, et `produit_fini` reste visible à la caisse.
UPDATE article SET nature_comptable = 'produit_fini'
 WHERE id IN (SELECT article_id FROM nomenclature)
    OR id IN (SELECT article_produit_id FROM ordre_production);
