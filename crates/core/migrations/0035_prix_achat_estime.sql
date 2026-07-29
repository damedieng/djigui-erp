-- Djigui Desktop — migration 0035 : PRIX D'ACHAT ESTIMÉ.
--
-- Constat sur les vraies données de l'utilisateur (2026-07-27) : **25 articles
-- sur 25 n'avaient aucun prix d'achat**. Conséquence, le rapport de bénéfices
-- affichait un coût de zéro, donc une marge égale au chiffre d'affaires —
-- c'était faux, et personne ne pouvait le deviner à l'écran.
--
-- L'utilisateur a demandé de « seeder des données test », faute de vrais
-- chiffres d'achat. Danger identifié et traité ici : **un chiffre inventé sans
-- étiquette est plus dangereux qu'une case vide**, parce qu'il a l'air juste.
--
-- D'où ce drapeau : un prix estimé reste un prix estimé, il se voit à l'écran
-- (badge « prix estimé »), il est signalé sur les rapports de marge, et il
-- disparaît dès que le commerçant saisit son vrai prix d'achat.

ALTER TABLE article ADD COLUMN prix_achat_estime INTEGER NOT NULL DEFAULT 0;

-- Index partiel : l'écran « Compléter mes prix » ne s'intéresse qu'à ceux-là,
-- et ils sont censés devenir rares avec le temps.
CREATE INDEX idx_article_prix_estime ON article(prix_achat_estime)
    WHERE prix_achat_estime = 1;
