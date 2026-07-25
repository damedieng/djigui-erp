-- Identité légale (conformité OHADA) — TOUT est FACULTATIF.
-- Règle métier ferme : aucun de ces champs ne bloque un enregistrement ni une
-- vente. Un champ manquant déclenche une alerte jaune côté UI, jamais un refus
-- (contexte Afrique de l'Ouest : beaucoup de clients n'ont ni NINEA ni CNI).

-- --------------------------------------------------------------------------
-- Tiers : distinguer un particulier d'une entreprise. Le NINEA/RCCM n'a de
-- sens que pour une entreprise ; le prénom et la CNI que pour un particulier.
-- --------------------------------------------------------------------------
ALTER TABLE tiers ADD COLUMN nature TEXT NOT NULL DEFAULT 'particulier'
    CHECK (nature IN ('particulier','entreprise'));
ALTER TABLE tiers ADD COLUMN prenom TEXT;   -- particulier
ALTER TABLE tiers ADD COLUMN cni    TEXT;   -- particulier, jamais exigée
ALTER TABLE tiers ADD COLUMN rccm   TEXT;   -- entreprise (le ninea existe déjà)

-- Les tiers déjà saisis qui portent un NINEA sont manifestement des entreprises.
UPDATE tiers SET nature = 'entreprise'
 WHERE ninea IS NOT NULL AND TRIM(ninea) <> '';

-- --------------------------------------------------------------------------
-- Entreprise émettrice : mentions légales attendues sur une facture OHADA.
-- ninea et rccm existent déjà (0001_initial).
-- --------------------------------------------------------------------------
ALTER TABLE parametres_entreprise ADD COLUMN forme_juridique TEXT;
ALTER TABLE parametres_entreprise ADD COLUMN capital         NUMERIC;
ALTER TABLE parametres_entreprise ADD COLUMN regime_fiscal   TEXT;
