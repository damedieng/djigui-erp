-- Notifications quotidiennes : SEUL l'état « lu » est stocké.
--
-- Les alertes elles-mêmes sont recalculées à chaque affichage depuis les
-- données réelles (projets, jalons, stock, caisse…). Les stocker créerait des
-- alertes fantômes : une facture réglée ou une tâche terminée laisserait
-- traîner une notification devenue fausse.
--
-- La clé porte le fond de l'alerte (ex. « projet-retard:<id>:<jours> ») : si la
-- situation s'aggrave, la clé change et la notification réapparaît, même si
-- l'utilisateur avait masqué la précédente.
CREATE TABLE notification_lue (
    cle    TEXT PRIMARY KEY,
    lu_le  TEXT NOT NULL
);
