-- 0009 : facturation « contrat » — exonération de TVA par client, référence de
-- dossier + objet sur les factures, et abonnements à durée limitée (nombre
-- d'échéances) avec libellé numéroté.

-- Client exonéré de TVA : ses factures sont émises sans taxe.
ALTER TABLE tiers ADD COLUMN exonere_tva INTEGER NOT NULL DEFAULT 0;

-- Référence de dossier + objet, affichés sur la facture.
ALTER TABLE document ADD COLUMN reference_dossier TEXT;
ALTER TABLE document ADD COLUMN objet TEXT;

-- Abonnement : contrat à échéances limitées + dossier/objet propagés.
ALTER TABLE abonnement ADD COLUMN reference_dossier TEXT;
ALTER TABLE abonnement ADD COLUMN objet TEXT;
ALTER TABLE abonnement ADD COLUMN nb_echeances INTEGER;              -- NULL = illimité
ALTER TABLE abonnement ADD COLUMN echeances_faites INTEGER NOT NULL DEFAULT 0;
