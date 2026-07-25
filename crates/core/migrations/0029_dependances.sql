-- Liens de précédence entre activités (flèches du Gantt).
--
-- ⚠️ Décision utilisateur (2026-07-25) : la propagation en cascade EXISTE mais
-- n'est JAMAIS automatique. On signale l'incohérence, l'utilisateur clique
-- « Harmoniser les dates » et voit un aperçu avant application. Même principe
-- que le bandeau « Ajuster la fin » déjà en place.
--
-- v1 : uniquement le lien fin → début (95 % des cas réels). Les autres types
-- (début→début, fin→fin, début→fin) viendront si le besoin se confirme ; la
-- colonne `type` est là pour ne pas avoir à refaire la table.

CREATE TABLE dependance (
    id             TEXT PRIMARY KEY,
    -- Le successeur : l'activité qui dépend d'une autre.
    tache_id       TEXT NOT NULL REFERENCES tache(id),
    -- Le prédécesseur : celle qui doit être finie avant.
    predecesseur_id TEXT NOT NULL REFERENCES tache(id),
    type           TEXT NOT NULL DEFAULT 'fin_debut'
                   CHECK (type IN ('fin_debut','debut_debut','fin_fin','debut_fin')),
    -- Décalage en jours : « B commence 3 jours après la fin de A ». Peut être
    -- négatif (chevauchement volontaire).
    decalage       INTEGER NOT NULL DEFAULT 0,
    cree_le        TEXT NOT NULL,
    -- Un même lien ne peut exister qu'une fois.
    UNIQUE (tache_id, predecesseur_id)
);
CREATE INDEX idx_dep_tache ON dependance(tache_id);
CREATE INDEX idx_dep_pred  ON dependance(predecesseur_id);
