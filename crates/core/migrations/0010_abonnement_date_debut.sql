-- 0010 : l'abonnement porte une DATE DE DÉBUT de facturation fixe. Les échéances
-- se calculent à partir d'elle (début + N périodes). `prochaine_echeance` reste
-- comme cache de la prochaine échéance due (début + échéances déjà émises).

ALTER TABLE abonnement ADD COLUMN date_debut TEXT;

-- Reprise : pour les abonnements existants, la date de début = la prochaine
-- échéance connue reculée du nombre d'échéances déjà émises serait idéale, mais
-- faute de mieux on part de la prochaine échéance courante.
UPDATE abonnement SET date_debut = prochaine_echeance
 WHERE date_debut IS NULL OR date_debut = '';
