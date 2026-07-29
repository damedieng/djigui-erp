-- Djigui Desktop — migration 0036 : VALORISATION DU STOCK (CUMP).
--
-- Aujourd'hui le stock est tenu en **quantité** (12 sacs de riz) mais pas en
-- **valeur** (12 sacs × combien ?). Conséquence directe, constatée sur les
-- données réelles : la marge est calculée avec le prix d'achat **du jour**, pas
-- celui du jour de la vente. Si les prix bougent — et en Afrique de l'Ouest ils
-- bougent — la marge passée est fausse.
--
-- Méthode retenue : **CUMP**, coût unitaire moyen pondéré. À chaque entrée en
-- stock, on recalcule la moyenne de ce qui reste :
--
--     nouveau CUMP = (valeur du stock restant + valeur de l'entrée)
--                    ÷ (quantité restante + quantité entrée)
--
-- Pourquoi le CUMP et pas le FIFO : le commerçant ne sait pas dire quel sac de
-- riz précis il a vendu, et il n'a aucune envie de le savoir. Le CUMP donne un
-- coût unique par article, compréhensible, et c'est la méthode admise par le
-- SYSCOHADA au même titre que le FIFO.
--
-- ⚠️ Le coût est **figé sur le mouvement de sortie** : une vente passée ne se
-- revalorise jamais quand les prix changent. C'est déjà le réflexe pris pour la
-- production (migration 0031), on le généralise.

-- Coût unitaire porté par CHAQUE mouvement de stock.
--   * entrée : ce que l'unité a coûté (prix d'achat de la pièce, ou prix de
--     revient pour une production).
--   * sortie : le CUMP en vigueur à cet instant — la photo du coût.
-- NULL = mouvement antérieur à cette migration, ou coût inconnu : on ne
-- fabrique pas de chiffre, on laisse vide et l'écran le signale.
ALTER TABLE mouvement_stock ADD COLUMN cout_unitaire NUMERIC;

-- CUMP courant de l'article, recalculé à chaque entrée. Dénormalisé : le
-- recalculer depuis le journal à chaque affichage coûterait cher, et cette
-- colonne se reconstruit intégralement depuis les mouvements en cas de doute
-- (même principe que caisse.solde et tiers.solde, §6.4).
ALTER TABLE article ADD COLUMN cump NUMERIC;

-- Reprise de l'existant : à défaut de mieux, le CUMP de départ est le prix
-- d'achat connu. C'est une approximation assumée pour les mouvements déjà
-- enregistrés — les mouvements à venir, eux, seront valorisés pour de bon.
UPDATE article SET cump = prix_achat
 WHERE prix_achat IS NOT NULL AND prix_achat > 0;

CREATE INDEX idx_mouvement_cout ON mouvement_stock(article_id, date);
