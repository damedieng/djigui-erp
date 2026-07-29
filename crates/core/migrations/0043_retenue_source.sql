-- Djigui Desktop — migration 0043 : RETENUE À LA SOURCE (précompte).
--
-- # Ce que c'est, et pourquoi ce n'est PAS une taxe
--
-- Une taxe (TVA…) **s'ajoute** au montant et **s'ajoute** à ce que le client
-- verse. La retenue à la source fait l'inverse : elle **figure sur la facture**
-- mais **se soustrait** de ce que le client nous paie. Le client garde cette
-- part et la reverse lui-même au Trésor, au nom du fournisseur.
--
--     Total TTC                      118 000
--     − Retenue à la source (5 %)     −5 000
--     ─────────────────────────────────────
--     NET À PAYER                    113 000   ← ce que le client verse
--
-- Le fournisseur a bien encaissé 118 000 au sens fiscal : 113 000 en banque et
-- 5 000 sous forme de **créance sur le Trésor**, qu'il imputera sur son impôt.
--
-- ⚠️ **Le moteur de taxes ne pouvait pas l'exprimer.** Il ne sait produire que
-- des montants qui s'ajoutent, ligne par ligne. La retenue porte sur la pièce
-- entière et va dans l'autre sens : d'où des colonnes propres.
--
-- # Conséquence sur le solde du tiers — le point délicat
--
-- Le client ne DOIT que le net. Si l'encours continuait de compter le TTC, la
-- facture resterait éternellement « partiellement impayée » du montant de la
-- retenue, et l'écran de relance réclamerait au client une somme qu'il n'a
-- jamais eu à verser.
--
-- ⚠️⚠️ Ce changement touche **DEUX endroits qui doivent rester des miroirs
-- exacts** (leçon du bug des soldes de juillet) :
--   1. `document::valider` / `annuler`, qui écrivent le solde au fil de l'eau ;
--   2. `paiement::recalculer_soldes`, qui le reconstruit depuis les journaux.
-- Si l'un compte le TTC et l'autre le net, le « recalcul » réparerait les
-- soldes en les faussant — pire que le bug d'origine.
--
-- # Le taux est FIGÉ sur la pièce
--
-- Comme les taxes de ligne : corriger le taux d'un tiers ne doit jamais
-- réécrire une facture déjà émise. On recopie donc le taux au moment de la
-- création, et c'est cette copie qui fait foi.

-- Taux applicable au tiers. NULL = aucune retenue (le cas courant) — et c'est
-- volontairement NULL et non 0 : « pas concerné » et « concerné au taux zéro »
-- ne se disent pas pareil dans un dossier.
ALTER TABLE tiers ADD COLUMN retenue_source_taux NUMERIC;

-- Snapshot sur la pièce : taux appliqué et montant calculé.
ALTER TABLE document ADD COLUMN retenue_taux NUMERIC NOT NULL DEFAULT 0;
ALTER TABLE document ADD COLUMN montant_retenue NUMERIC NOT NULL DEFAULT 0;

-- Sur quelle base calculer la retenue : 'ht' ou 'ttc'.
--
-- ⚠️ **Paramétrable, jamais codé en dur.** L'assiette diffère selon la nature
-- de la retenue et selon le pays OHADA ; figer « HT » dans le code obligerait à
-- recompiler pour un client dont l'administration dit autre chose.
-- Défaut 'ht' : c'est l'assiette la plus fréquente pour les retenues sur
-- prestations en zone UEMOA.
INSERT INTO parametre_global (cle, valeur) VALUES ('retenue_base', 'ht');
