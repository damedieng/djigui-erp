-- ---------------------------------------------------------------------------
-- Rattachement caisse → magasin (dépôt)
-- ---------------------------------------------------------------------------
-- Une caisse appartient à un magasin : les ventes encaissées à cette caisse
-- déduisent le stock DE CE MAGASIN. NULL = repli sur le dépôt par défaut.
ALTER TABLE caisse ADD COLUMN depot_id TEXT;
