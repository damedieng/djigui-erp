-- Djigui Desktop — migration 0041 : NE PAS COUPER L'ACCÈS À L'EXISTANT.
--
-- La migration 0040 crée le catalogue des modules avec `souscrit = 0` partout,
-- en attendant que l'installateur pose la formule vendue. Correct pour une
-- installation neuve — **désastreux sur une base déjà en service** : au premier
-- démarrage après mise à jour, le menu d'un client qui travaillait depuis des
-- mois se réduirait au socle. Ses ventes, sa caisse, ses marchés disparaîtraient
-- de l'écran sans explication.
--
-- Constaté immédiatement sur la base de travail : `visibles: ['socle']`.
--
-- **Règle qui s'applique ici : une mise à jour n'enlève jamais un accès.**
-- On ouvre donc tout, et c'est l'écran « Modules » qui sert ensuite à
-- restreindre selon la formule réellement vendue. Le sens de l'erreur compte :
-- un module ouvert par excès se referme d'un clic, un module fermé par erreur
-- fait croire à une perte de données et déclenche un appel paniqué.
--
-- ⚠️ On ne modifie pas la 0040 : elle est déjà appliquée. On ajoute.

UPDATE module SET souscrit = 1, souscrit_le = datetime('now')
 WHERE souscrit = 0;

-- La formule reste vide tant que personne ne l'a choisie : l'écran affichera
-- « non définie », ce qui invite justement à la poser. On ne prétend pas que le
-- client a souscrit la formule complète — on constate seulement que tout est
-- ouvert en attendant.
