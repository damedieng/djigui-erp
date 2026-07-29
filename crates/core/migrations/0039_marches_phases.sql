-- Djigui Desktop — migration 0039 : PHASES DE LA PROCÉDURE.
--
-- But : pouvoir afficher TOUS les marchés côte à côte dans un tableau de suivi
-- (vue Kanban), alors que les procédures n'ont ni le même nombre d'étapes ni
-- les mêmes libellés — 7 pour Services, 8 pour Travaux et Fournitures, 9 pour
-- les Prestations intellectuelles.
--
-- La solution est un **dénominateur commun** : cinq grandes phases dans
-- lesquelles toutes les procédures se rangent, plus une pour l'exécution.
--
--   preparation      → dossier d'appel d'offres, termes de référence
--   consultation     → publication, réception des offres, liste restreinte
--   evaluation       → ouverture des plis, évaluation technique et financière
--   attribution      → attribution provisoire ou définitive
--   contractualisation → signature, notification, ordre de service
--   execution        → livraison, exécution, réception
--
-- ⚠️ POURQUOI UN CHAMP ET NON UNE DEVINETTE PAR MOTS-CLÉS : l'utilisateur crée
-- ses propres types de marché avec ses propres libellés. Une heuristique sur le
-- texte marcherait sur les 4 procédures livrées et casserait sur la cinquième.
-- La phase est donc une **donnée**, préremplie ici et modifiable dans l'écran
-- « Types de marché ».
--
-- `NULL` est autorisé et signifie « même phase que l'étape précédente » : une
-- étape ajoutée au milieu d'une procédure appartient naturellement à la phase
-- en cours, et l'utilisateur n'a rien à renseigner pour que ce soit juste.

ALTER TABLE marche_etape_modele ADD COLUMN phase TEXT;
ALTER TABLE marche_etape        ADD COLUMN phase TEXT;

-- ---------------------------------------------------------------------------
-- Pré-remplissage des 4 procédures livrées, par identifiant d'étape.
-- On vise les `id` (em-tr-1…) et non les libellés : c'est exact, et cela ne
-- touche pas une étape que l'utilisateur aurait déjà renommée.
-- ---------------------------------------------------------------------------
UPDATE marche_etape_modele SET phase = 'preparation'
 WHERE id IN ('em-tr-1','em-fo-1','em-se-1','em-in-1');

UPDATE marche_etape_modele SET phase = 'consultation'
 WHERE id IN ('em-tr-2','em-tr-3',
              'em-fo-2','em-fo-3',
              'em-se-2','em-se-3',
              'em-in-2','em-in-3','em-in-4','em-in-5');

UPDATE marche_etape_modele SET phase = 'evaluation'
 WHERE id IN ('em-tr-4','em-tr-5',
              'em-fo-4','em-fo-5',
              'em-se-4','em-se-5',
              'em-in-6','em-in-7');

UPDATE marche_etape_modele SET phase = 'attribution'
 WHERE id IN ('em-tr-6','em-fo-6','em-se-6','em-in-8');

UPDATE marche_etape_modele SET phase = 'contractualisation'
 WHERE id IN ('em-tr-7','em-tr-8',
              'em-fo-7',
              'em-se-7',
              'em-in-9');

UPDATE marche_etape_modele SET phase = 'execution'
 WHERE id IN ('em-fo-8');

-- ---------------------------------------------------------------------------
-- Les étapes DÉJÀ INSTANCIÉES dans les marchés en cours.
-- D'abord par filiation (le cas normal), puis par mots-clés pour celles qui ont
-- été ajoutées à la main. Le repli par mots-clés est acceptable ICI parce qu'il
-- s'agit d'un rattrapage ponctuel sur l'existant, pas d'une règle permanente.
-- ---------------------------------------------------------------------------
UPDATE marche_etape
   SET phase = (SELECT m.phase FROM marche_etape_modele m WHERE m.id = marche_etape.etape_modele_id)
 WHERE etape_modele_id IS NOT NULL;

UPDATE marche_etape SET phase = 'preparation'
 WHERE phase IS NULL AND (lower(libelle) LIKE '%prépar%' OR lower(libelle) LIKE '%prepar%'
                       OR lower(libelle) LIKE '%termes de référence%' OR lower(libelle) LIKE '%dossier%');

UPDATE marche_etape SET phase = 'consultation'
 WHERE phase IS NULL AND (lower(libelle) LIKE '%publication%' OR lower(libelle) LIKE '%avis%'
                       OR lower(libelle) LIKE '%réception des offres%' OR lower(libelle) LIKE '%manifestation%'
                       OR lower(libelle) LIKE '%liste restreinte%' OR lower(libelle) LIKE '%proposition%');

UPDATE marche_etape SET phase = 'evaluation'
 WHERE phase IS NULL AND (lower(libelle) LIKE '%ouverture%' OR lower(libelle) LIKE '%évaluation%'
                       OR lower(libelle) LIKE '%evaluation%' OR lower(libelle) LIKE '%dépouill%');

UPDATE marche_etape SET phase = 'attribution'
 WHERE phase IS NULL AND lower(libelle) LIKE '%attribution%';

UPDATE marche_etape SET phase = 'contractualisation'
 WHERE phase IS NULL AND (lower(libelle) LIKE '%signature%' OR lower(libelle) LIKE '%contrat%'
                       OR lower(libelle) LIKE '%notification%' OR lower(libelle) LIKE '%ordre de service%'
                       OR lower(libelle) LIKE '%négociation%');

UPDATE marche_etape SET phase = 'execution'
 WHERE phase IS NULL AND (lower(libelle) LIKE '%livraison%' OR lower(libelle) LIKE '%exécution%'
                       OR lower(libelle) LIKE '%réception%');

CREATE INDEX idx_marche_etape_phase ON marche_etape(marche_id, phase);
