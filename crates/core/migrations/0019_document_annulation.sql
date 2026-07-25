-- ---------------------------------------------------------------------------
-- Annulation d'une vente déjà encaissée (contre-passation, bonnes pratiques)
-- ---------------------------------------------------------------------------
-- On n'efface JAMAIS l'historique : annuler passe le document en statut 'annule'
-- (déjà autorisé par le CHECK d'origine), réintègre le stock par des mouvements
-- inverses, inverse le solde du tiers, et CONTRE-PASSE le paiement par un
-- décaissement lié (l'argent est rendu au client → la caisse reste juste).
-- Réservé à l'Admin ; le motif et l'auteur sont tracés.
ALTER TABLE document ADD COLUMN motif_annulation TEXT;
ALTER TABLE document ADD COLUMN annule_par       TEXT;
ALTER TABLE document ADD COLUMN annule_le        TEXT;

-- Paiement de contre-passation : référence le paiement d'origine annulé.
ALTER TABLE paiement ADD COLUMN annulation_de TEXT REFERENCES paiement(id);
