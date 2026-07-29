# Faut-il construire la comptabilité complète (N2) dans Djigui ?

> **À qui s'adresse ce document** : à toi, qui n'as pas de notions de comptabilité.
> Tout est expliqué en langage courant avant d'être chiffré. Aucun jargon n'est
> employé sans être défini la première fois.
>
> **Question posée** : combien de temps prend le niveau N2 (« Socle + écritures
> et journaux ») décrit dans `plan_comptable.md`, et est-ce que ça vaut la peine ?
>
> **Réponse courte, en une phrase** : ça représente environ **8 à 11 sessions de
> travail** (contre 2 à 3 pour le socle seul), et **ça ne vaut la peine que si tu
> vises les entreprises structurées** — pour le commerçant de quartier, c'est du
> travail que son comptable refait de toute façon dans son propre logiciel.
>
> Rédigé le 2026-07-27, à partir du code réel et des données réelles de `djigui.db`.

---

## 1. D'abord, comprendre ce qu'est une « écriture comptable »

C'est le seul concept à comprendre pour décider. Prends cinq minutes, tout le
reste du document en découle.

### 1.1 Ce que Djigui sait faire aujourd'hui

Quand tu vends un plat 5 000 F payé en espèces, Djigui enregistre trois faits :

- une facture de 5 000 F au nom du client,
- un encaissement de 5 000 F dans la caisse,
- une sortie de stock du plat.

C'est du **langage de commerçant**. C'est juste, c'est complet, et ça te suffit
pour tenir ta boutique.

### 1.2 Ce que la comptabilité exige en plus

L'administration et le comptable ne lisent pas « une vente de plat ». Ils lisent
un **plan comptable** : une liste normalisée d'environ 250 boîtes numérotées,
identique dans les 17 pays de la zone **OHADA** (l'organisation qui harmonise le
droit des affaires en Afrique de l'Ouest et Centrale). Chaque boîte a un numéro
et un nom :

| Numéro | Nom de la boîte | En français courant |
|---|---|---|
| 411 | Clients | ce que tes clients te doivent |
| 401 | Fournisseurs | ce que tu dois à tes fournisseurs |
| 571 | Caisse | l'argent liquide chez toi |
| 521 | Banque | l'argent sur ton compte |
| 701 | Ventes de marchandises | ton chiffre d'affaires |
| 601 | Achats de marchandises | ce que tu as acheté pour revendre |
| 4431 | TVA collectée | la TVA que tu as encaissée pour l'État |
| 4451 | TVA déductible | la TVA que tu as payée à tes fournisseurs |

La règle de base : **chaque opération doit toucher au moins deux boîtes, et les
montants doivent s'équilibrer exactement**. C'est ça, une écriture. On dit
« partie double ». L'idée est vieille de 500 ans et c'est un filet de sécurité :
si les deux colonnes ne tombent pas juste, c'est qu'il y a une erreur quelque part.

Les deux colonnes s'appellent **débit** (à gauche) et **crédit** (à droite). Ne
cherche pas de sens moral à ces mots : ce sont juste les noms de la colonne de
gauche et de la colonne de droite. Retiens seulement : **total gauche = total droite**.

### 1.3 La même vente, en écritures

Ta vente de 5 000 F devient **deux écritures** :

```
Écriture n°1 — la facture (journal des ventes)
  411 Clients .............. gauche  5 000     (le client me doit 5 000)
  701 Ventes ............... droite  4 237     (mon chiffre d'affaires)
  4431 TVA collectée ....... droite    763     (la TVA que je dois à l'État)
                                    -------
  gauche 5 000  =  droite 5 000  ✓ équilibré

Écriture n°2 — l'encaissement (journal de caisse)
  571 Caisse ............... gauche  5 000     (l'argent est entré)
  411 Clients .............. droite  5 000     (le client ne me doit plus rien)
```

Tu vois le principe : **la même vente est racontée deux fois**, une fois en
langage de commerçant (ce que Djigui fait déjà) et une fois en langage comptable
(ce que N2 ajouterait). Le travail de N2, c'est d'écrire le **traducteur
automatique** entre les deux.

### 1.4 Et ensuite, à quoi ça sert ?

Une fois les écritures produites, on peut sortir trois documents que tout
comptable connaît :

- **Le journal** — la liste de toutes les écritures dans l'ordre du temps.
  « Qu'est-ce qui s'est passé le 12 juillet ? »
- **Le grand livre** — les mêmes écritures, mais triées par boîte.
  « Montre-moi tout ce qui est passé par la caisse ce mois-ci. »
- **La balance** — le solde de chaque boîte à une date donnée.
  « Où j'en suis, boîte par boîte ? » C'est le document à partir duquel on
  fabrique le bilan et le compte de résultat.

**C'est tout.** N2, c'est : le plan comptable + le traducteur automatique + ces
trois documents + le **lettrage** (cocher qu'un règlement correspond bien à telle
facture, pour savoir ce qui reste impayé).

---

## 2. Ce que N2 représente concrètement en développement

Voici le découpage réel, avec pour chaque bloc ce qu'il faut construire.

### Bloc A — Le plan comptable (table `compte`)
Créer la table des ~250 boîtes et la **livrer pré-remplie** avec le plan OHADA
officiel, pour que tu n'aies rien à saisir. Écran de consultation, possibilité
d'ajouter des sous-comptes (par exemple un compte 411 par gros client).

*Poids : petit à moyen. Le vrai travail est de **saisir correctement les 250
comptes officiels** — c'est fastidieux et ça ne se devine pas, il faut le texte
de référence AUDCIF sous les yeux.*

### Bloc B — Le paramétrage (le point sensible)
Chaque chose vendue doit savoir dans quelle boîte elle atterrit. Un plat va en
701 ou en 702 ? Une prestation de service en 706. Le riz acheté en 601 ou 602 ?

**On a déjà fait la moitié du travail** sans le savoir : le champ
`nature_comptable` posé hier (marchandise / matière première / produit fini /
service) est exactement ce qui décide de ces comptes. C'est le meilleur signal
du dossier.

Reste à ajouter : compte par défaut global, affinable par catégorie, puis par
article ; comptes des taxes, des tiers, des caisses, des moyens de paiement.
Avec la règle absolue de la spec : **compte non renseigné → compte d'attente 471
+ alerte jaune, jamais un refus de vendre**.

*Poids : moyen. Beaucoup d'écrans de paramétrage, peu de logique.*

### Bloc C — Le moteur d'écritures (le cœur)
Le traducteur automatique. Pour chaque fait de gestion, produire l'écriture
équilibrée correspondante :

- facture de vente validée → écriture n°1 ci-dessus
- facture d'achat → l'inverse
- encaissement / décaissement → écriture n°2
- **annulation → contre-passation** (on n'efface jamais une écriture, on en
  écrit une seconde en sens inverse)
- transfert entre caisses, mouvements de stock valorisés

Avec un invariant contrôlé en base et testé : **somme gauche = somme droite,
toujours**.

*Poids : **gros**. C'est le bloc le plus risqué : chaque cas particulier déjà
géré par Djigui (client exonéré de TVA, multi-taxes, remises, avoirs,
annulations, transformations commande→facture) doit avoir sa traduction. Le
module `document.rs` fait déjà 895 lignes de cas métier ; chacun doit être
couvert.*

### Bloc D — Les restitutions
Journal, grand livre, balance générale, balance par tiers. Écrans de
consultation avec filtres de période, plus export.

Bonus gratuit : la **balance par tiers** doit tomber exactement sur les soldes
clients que Djigui tient déjà de son côté. C'est un **contrôle croisé** qui
détecterait des bugs qu'on ne voit pas aujourd'hui.

*Poids : moyen. Techniquement proche de ce qui existe déjà (`rapport.rs`,
l'export Excel).*

### Bloc E — Le lettrage
Rapprocher chaque règlement de la facture qu'il paie, manuellement et
automatiquement. Écran de pointage.

*Poids : moyen. L'écran est délicat à rendre utilisable.*

### Bloc F — Le prérequis caché : la valorisation du stock
**C'est le piège du dossier.** Aujourd'hui Djigui compte le stock en
**quantités** (12 sacs de riz) mais pas en **valeur** (12 sacs × combien ?).

Vérifié à l'instant sur ta base : **25 articles sur 25 n'ont aucun prix
d'achat**. Conséquence, ton rapport de bénéfices affiche aujourd'hui un coût de
zéro, donc une marge égale au chiffre d'affaires — **c'est faux**, et une
comptabilité construite là-dessus serait fausse aussi.

Il faut donc calculer un coût moyen (« CUMP » : le prix moyen payé pour ce que
tu as en stock, recalculé à chaque achat).

*Poids : moyen. **Et ce bloc est indispensable quoi qu'il arrive**, même si tu
renonces à N2 — parce que sans lui tu ne connais pas ta vraie marge.*

---

## 3. Le chiffrage

Base de comparaison honnête, prise sur du travail réellement livré ici :

| Repère réel | Taille livrée | Durée réelle |
|---|---|---|
| Module Production (26/07) | 1 395 lignes cœur + 915 lignes d'écran + 1 migration + API | ~1 session |
| Module Gestion de projet | ~1 219 lignes cœur + 2 520 lignes d'écran + 9 migrations | ~4 sessions |
| Nature comptable des articles (26/07) | 2 migrations + modifications ciblées | ~0,5 session |

Une « session » = une journée de travail soutenue avec moi, du cadrage au test
sur tes vraies données.

### N2 complet

| Bloc | Sessions |
|---|---|
| F. Valorisation du stock (CUMP) — prérequis | 1,5 |
| A. Plan comptable OHADA seedé | 1 |
| B. Paramétrage des comptes | 1,5 |
| C. Moteur d'écritures | **3** |
| D. Journaux, grand livre, balance | 2 |
| E. Lettrage | 1 |
| Tests sur tes vraies données + corrections | 1 |
| **Total** | **11 sessions** |

**Fourchette réaliste : 8 à 11 sessions.** 8 si on accepte de couvrir seulement
les ventes et encaissements (et pas les achats fournisseurs, ni les avoirs), 11
pour la couverture complète.

À comparer avec le **socle seul** (montant en toutes lettres, droit de timbre,
retenue à la source, écran comptable, export vers le logiciel du comptable) :
**2 à 3 sessions**.

### Pourquoi le moteur d'écritures est estimé à 3 sessions et pas 1

Parce que ce n'est pas une fonctionnalité, c'est une **couche transversale**.
Chaque comportement existant doit être retraduit. Ta base contient déjà les cas
qui vont poser problème :

- 13 factures validées, **1 facture annulée** (FA-2026-0012) → il faut la
  contre-passation ;
- 1 commande transformée en facture → ne doit produire **aucune** écriture au
  stade commande, sinon tout est compté deux fois ;
- 1 facture brouillon → ne doit rien produire non plus ;
- 4 taux de TVA différents dont un à 0 % (exonéré) ;
- 2 caisses avec des soldes distincts (74 598 F et 59 904 F) → deux comptes 571 ;
- 14 paiements pour 137 502 F encaissés, sur 200 422 F facturés → **62 920 F
  d'impayés** qui doivent apparaître au compte 411 et nulle part ailleurs.

Chacun de ces cas est un test à écrire et un bug potentiel. C'est là que passe
le temps, et c'est là que se cachent les erreurs qui ne se voient pas.

---

## 4. Est-ce que ça vaut la peine ? Les arguments des deux côtés

### 4.1 Ce qui plaide POUR

**a) L'architecture est déjà prête, et c'est rare.**
Trois réflexes pris depuis le début rendent la greffe naturelle :
le journal de stock immuable (on n'efface jamais, on ajoute), la contre-passation
au lieu de la suppression (déjà codée pour les paiements), et la configuration
pilotée par la donnée. Ce sont exactement les principes de la comptabilité. On
ne se bat pas contre le code existant — c'est le meilleur signal du dossier.

**b) `nature_comptable` a déjà été posé.**
Le champ ajouté hier est précisément la clé de répartition des comptes. Le
travail conceptuel le plus délicat (« comment un logiciel devine-t-il si un
article va en 601 ou en 602 ? ») est **déjà fait et déjà testé sur tes données**.

**c) Un contrôle croisé gratuit.**
La balance par tiers doit retomber exactement sur les soldes clients que Djigui
calcule déjà. Si un jour les deux divergent, c'est qu'il y a un bug — et tu le
sauras. Aujourd'hui, personne ne le saurait.

**d) C'est un argument commercial fort sur un segment précis.**
Face à une PME, une ONG, une entreprise qui dépose ses comptes au greffe, « mon
logiciel sort la balance OHADA » n'est pas un détail : c'est ce qui fait qu'on
te choisit plutôt qu'un concurrent.

**e) C'est le passage obligé vers le bilan.**
Les états financiers (N4) se fabriquent **à partir de la balance**. Sans N2,
N4 est définitivement hors d'atteinte.

### 4.2 Ce qui plaide CONTRE

**a) Le commerçant a déjà un comptable — qui a déjà son logiciel.**
C'est l'argument le plus lourd, et il est écrit noir sur blanc dans ta propre
spec (`plan_comptable.md` §9). En pratique, la boutique confie ses pièces à un
comptable une fois par an. Ce comptable ne va pas abandonner son outil pour
saisir chez toi. **Ce dont il a besoin, c'est d'un export propre** — pas que tu
refasses son métier.

Un export comptable, c'est **une demi-session**. N2 complet, c'est 11.
Le même comptable est servi dans les deux cas.

**b) Ça contredit la règle que tu as toi-même posée.**
Tes mots exacts, enregistrés : **« il ne faut pas compliquer la tâche à un
commerçant »**. Une comptabilité complète, ce sont des comptes à paramétrer, des
écritures à contrôler, un vocabulaire à apprendre. On peut mettre tous les
garde-fous du monde (compte 471 par défaut, alertes jamais bloquantes) — ça
reste un écran de plus qu'il ne comprendra pas.

**c) Une compta fausse est pire que pas de compta.**
Aujourd'hui Djigui ne prétend rien sur le plan comptable, donc il ne ment pas.
Le jour où il sort une balance, elle **engage** : quelqu'un va s'en servir pour
une déclaration fiscale. Or 25 articles sur 25 sont sans prix d'achat. Il faut
donc soit livrer la valorisation de stock d'abord, soit accepter de produire des
chiffres faux — et il n'y a pas de troisième voie.

**d) 11 sessions, c'est le module Marchés qui attend.**
Et le module Marchés, lui, est **demandé pour un besoin identifié** — il existe
déjà une application de référence. Le rapport valeur / temps n'est pas comparable.

**e) Il reste des trous plus visibles dans le produit.**
`rapport.rs` ne contient que les bénéfices : pas de journal des ventes, pas de
marge par article, pas d'état du stock, pas d'encours clients. Il n'y a **aucun
écran `rapports.html`** (le lien du menu pointe dans le vide) et le tableau de
bord d'accueil n'est branché sur rien. Un client verra ces manques en trois
minutes ; il ne verra pas l'absence de grand livre.

---

## 5. Recommandation

**Ne fais pas N2 maintenant. Fais le socle (2 à 3 sessions), puis les Marchés.**

Le raisonnement tient en une ligne : **l'export comptable rend le même service
que N2 à la quasi-totalité de tes clients, pour un vingtième du temps.**

Ce que je propose de livrer à la place, dans l'ordre :

1. **Valorisation du stock (CUMP)** — *1,5 session*. À faire quoi qu'il arrive :
   sans elle, ta marge affichée est fausse aujourd'hui. Gain immédiat et visible,
   et prérequis de tout le reste si tu changes d'avis plus tard.
2. **Écran comptable séparé** — *0,5 session*. Déjà promis. Le comptable y
   reclasse les articles ; le commerçant n'y met jamais les pieds.
3. **Fin du N1** : montant en toutes lettres, droit de timbre, retenue à la
   source — *1 session*. Ce sont des **obligations légales sur la facture**, pas
   du confort. Elles comptent plus qu'un grand livre.
4. **Export comptable** (CSV reprenable par n'importe quel logiciel) —
   *0,5 session*. C'est ça, la vraie réponse au besoin « comptabilité ».

**Total : 3,5 sessions**, et le sujet comptable est clos pour la très grande
majorité des clients. Ensuite, Marchés.

### Quand faudra-t-il faire N2 pour de bon

Trois déclencheurs, un seul suffit :

- un client te demande explicitement la balance ou le grand livre ;
- tu vises les **PME, ONG ou entreprises structurées** plutôt que les boutiques ;
- tu veux sortir le **bilan** (les états financiers en dépendent).

Le jour où l'un se produit, le chiffrage de ce document reste valable — et il
sera même un peu plus court, parce que la valorisation du stock aura déjà été
faite au point 1.

---

## 6. Ce dont j'ai besoin de toi pour décider

Une seule question, et elle n'est pas technique :

> **Tes clients Djigui, ce sont des boutiques et des restaurants, ou des
> entreprises qui déposent des comptes ?**

- **Des boutiques** → recommandation ci-dessus, socle puis Marchés.
- **Des entreprises structurées** → alors N2 se justifie, et il faut le planifier
  franchement sur 8 à 11 sessions plutôt que de le grignoter.
- **Les deux** → socle maintenant, N2 plus tard sur demande client réelle.

---

*Sources : `claude_spec/plan_comptable.md`, code de `crates/core/src/modules/`,
et données réelles de `djigui.db` au 2026-07-27 (25 articles, 13 factures
validées, 14 paiements, 2 caisses).*
