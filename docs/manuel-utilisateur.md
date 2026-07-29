# Manuel utilisateur — Djigui

État au **2026-07-26**. Ce manuel décrit ce que fait chaque écran et **pourquoi il
se comporte ainsi**. Il est écrit en langage simple : chaque écran de l'application
contient déjà une section d'aide, ce document les rassemble et va un peu plus loin.

## Trois principes à connaître avant tout

1. **Les messages en jaune sont des avertissements, pas des interdictions.**
   Stock insuffisant, prix d'achat manquant, dates incohérentes, NINEA absent :
   Djigui vous prévient et vous laisse continuer. C'est vous qui décidez.
2. **Rien ne se fait tout seul dans votre dos.** Aucune date n'est recalculée sans
   que vous cliquiez, aucun affichage ne se replie de lui-même.
3. **Ce qui est validé ne se réécrit pas.** Une facture validée ou un ordre de
   fabrication terminé ne se modifie plus : on l'**annule**, ce qui laisse une trace.

---

## Démarrage et connexion

Au lancement, Djigui demande un identifiant. Le compte de départ est
**`djigui` / `djigui`** — changez-le dès que possible dans **Utilisateurs**.

Deux rôles :

| Rôle | Ce qu'il peut faire |
|---|---|
| **Caissier** | Vendre, encaisser, consulter. |
| **Administrateur** | Tout, plus : magasins, utilisateurs, journal d'audit, annulation de vente. |

La session se ferme quand vous quittez l'application : on se reconnecte à chaque
démarrage. Tout ce que vous faites est enregistré dans le **journal d'audit**.

---

## Accueil

Tableau de bord d'arrivée. La **cloche** en haut à droite regroupe les rappels du
jour : activités en retard, jalons manqués, articles sous le seuil, rendez-vous du
jour. Un clic sur un rappel ouvre l'écran concerné. « Tout marquer comme lu » vide
la pastille sans supprimer les alertes.

---

## Vendre

### Caisse
Trois zones : les **catégories**, les **articles**, le **panier**. Plusieurs
ventes peuvent être en cours en même temps, sous forme d'**onglets** — pratique
quand un client attend qu'on aille chercher un article.

À l'encaissement, choisissez le **moyen de paiement** (Orange Money, Wave,
espèces…). Le bloc « reçu / rendu » n'apparaît que pour les moyens qui rendent la
monnaie. Le ticket peut s'imprimer automatiquement ou à la demande (réglage dans
Paramètres).

La vente déduit le stock **du magasin rattaché à la caisse**.

### État de caisse
Trois onglets :

- **Journal** — tous les encaissements et décaissements, filtrables par période et
  par sens. Le bandeau « Période analysée » en haut pilote aussi les cartes de
  caisse, qui affichent alors le **net de la période** (encaissé − décaissé) en
  plus du solde courant.
- **Sessions** — ouverture et fermeture de caisse, avec l'**écart** entre le montant
  compté et le montant théorique.
- **Bénéfices** — chiffre d'affaires, coût et bénéfice par mois et par caisse.
  ⚠️ Le bénéfice n'est juste que si les **prix d'achat** de vos articles sont saisis.

Un administrateur peut **annuler une vente encaissée** : le stock est réintégré,
l'encaissement est contre-passé par un décaissement, le solde du client revient en
place. Le motif est obligatoire.

Bouton **Exporter (.xlsx)** : cinq feuilles (Ventes, Détail ventes, Mouvements,
Sessions, Bénéfices) pour la période affichée. Le fichier est déposé dans vos
**Téléchargements** puis ouvert. Si une période ne contient aucune vente, Djigui
vous le dit au lieu de produire un fichier vide.

### Factures et devis
Un même écran gère devis, factures, avoirs, commandes, livraisons et proformas,
en vente comme en achat.

- Une pièce naît en **brouillon** : modifiable, supprimable, sans numéro définitif.
- La **validation** attribue le numéro et applique les mouvements de stock.
- La **transformation** crée la pièce suivante (devis → facture) en gardant le lien.
- Seul un brouillon se supprime. Une pièce validée s'annule.

### Abonnements
Pour les clients facturés à intervalle régulier (mensuel, trimestriel, annuel).
Djigui prépare les factures à l'échéance à partir d'un modèle de pièce.

---

## Agenda

Vue **calendrier mensuel** (pastilles colorées par statut) et vue **liste** (avec
traitement par lot). Un rendez-vous peut être rattaché à un tiers, à un
responsable, à un lieu.

Bouton **Exporter (.ics)** : produit un fichier importable dans Google Agenda,
Outlook ou Apple Calendrier. Il n'y a **pas** de synchronisation automatique :
Djigui fonctionne hors ligne, l'export est un choix assumé.

---

## Catalogue

### Articles
Un article est soit un **bien** (avec stock), soit un **service** (jamais de stock).
Prix de vente, prix d'achat, code-barres, image, catégorie, taxes, seuil d'alerte.

Le **prix d'achat** sert à deux choses : calculer votre marge, et calculer le prix
de revient de vos fabrications. Prenez le temps de le renseigner.

Traitement par lot : changer la catégorie, désactiver, supprimer. La suppression
refuse les articles qui ont une histoire (ventes, mouvements) — on les **désactive**.

### Magasins *(administrateur)*
- **Magasins** : vos lieux de stockage. Le stock est suivi magasin par magasin.
- **Transfert** : déplacer des articles d'un magasin à un autre. Seuls les articles
  réellement présents dans la source sont proposés.
- **Inventaire** : comptage daté. À la validation il est **verrouillé** (c'est une
  preuve) et les écarts sont ajustés automatiquement.

### Production
Deux onglets.

**Recettes** — ce qu'il faut pour fabriquer un article : « pour **20 baguettes**,
10 kg de farine et 0,2 kg de sel ». Écrivez les quantités **pour le lot entier**,
comme vous les diriez à voix haute : quand vous lancerez un ordre de 40 baguettes,
tout sera doublé automatiquement. La **perte en %** prévoit ce qui se perd toujours
un peu (épluchures, chutes, sciure).

Une recette est un **modèle, pas une règle** : dans un ordre de fabrication vous
pouvez toujours ajouter, retirer ou corriger un composant sans toucher à la recette.

**Ordres de fabrication** — le déroulé d'une fabrication réelle :

1. **Brouillon** : vous préparez. Rien ne bouge dans le stock, vous pouvez tout
   corriger ou supprimer l'ordre.
2. **En cours** : la fabrication est lancée. Le stock ne bouge toujours pas.
3. **Clôture** : c'est **là** que le stock change. Vous saisissez ce qui a été
   *réellement* produit et *réellement* consommé — c'est rarement exactement le
   prévu, et c'est normal. Les composants sortent du magasin, l'article fabriqué y
   entre.

À la clôture, Djigui calcule le **prix de revient** : coût des composants consommés
+ frais (main-d'œuvre, gaz, électricité), divisé par la quantité obtenue. Une case
cochée par défaut le recopie sur la fiche de l'article — vos marges deviennent
justes. Décochez-la si vous préférez garder votre prix.

Ce qui est signalé sans bloquer : stock insuffisant (le stock passera en négatif,
c'est qu'il était faux), prix d'achat manquant (le prix de revient sera
sous-évalué), écart entre le prévu et le produit.

Un ordre terminé ne se modifie plus. Un ordre pas encore clôturé s'annule avec un
motif.

---

## Contacts

### Tiers
Clients, fournisseurs, ou les deux. Choisissez la **nature** :

- **Particulier** → prénom et CNI. *La CNI n'est jamais imprimée sur une facture.*
- **Entreprise** → NINEA et RCCM, mentions attendues sur une facture conforme.

Rien n'est obligatoire : Djigui affiche ce qui manque, vous enregistrez quand même.

Le bouton **fiche** ouvre le solde, l'historique des règlements et permet
d'enregistrer un encaissement ou un décaissement directement.

---

## Comptabilité *(réservé au comptable)*

> Cet écran n'apparaît que pour un compte **administrateur**. Un commerçant n'a
> jamais besoin d'y aller : Djigui vend, encaisse et suit le stock sans lui.

### L'idée en trois phrases

Djigui enregistre les **faits** : une vente, un encaissement, un achat. Il ne
décide **jamais** de comptabilité à votre place. C'est le comptable qui vient
par-dessus, avec ses propres comptes et ses propres règles.

### Mes comptes

Un **compte** est une boîte numérotée où l'on range les opérations de même
nature : 571 la caisse, 411 ce que les clients doivent, 701 les ventes.

Créez les comptes dont vous avez l'habitude. Si vous préférez partir d'une base,
le bouton **« Installer le plan OHADA de base »** en propose une trentaine — vos
comptes existants ne sont pas touchés.

Un compte qui porte des écritures ne se supprime plus : **désactivez-le**.

### Mes règles

Une règle dit une seule chose : **« dans cette situation, prends ce compte »**.
Par exemple : *les ventes de la catégorie Boissons vont au 701*.

Le **rôle** indique la place du compte. Pour une vente : le *produit* (ce que la
vente rapporte), le *tiers* (le client), la *taxe* (la TVA). Pour un règlement :
la *trésorerie* (caisse ou banque). Djigui connaît déjà tous les montants et sait
qui va au débit et qui va au crédit — votre règle ne fait que **nommer les
comptes**.

Les critères sont **facultatifs** : laissé vide, un critère veut dire « peu
importe ». Une règle sans aucun critère est votre règle par défaut.

Vous n'avez **pas à réfléchir à l'ordre** de vos règles : la plus **précise**
l'emporte toujours. Écrivez un défaut large, puis vos exceptions.

### À ranger

C'est la corbeille : tout ce qui n'est pas encore classé. Quand elle est vide,
votre historique est rangé.

1. La recherche du haut sert à **isoler** ce que vous voulez traiter : une
   période, un client, une catégorie, un moyen de paiement, une fourchette de
   montant… Les critères se combinent.
2. Vous cochez, puis **« Ranger la sélection »**. Ou **« Ranger tout le
   résultat »** d'un coup.
3. Vous pouvez aussi transformer votre recherche en **règle permanente** :
   « Faire une règle de cette recherche ».

⚠️ Point important : les règles s'appliquent à **tout l'historique déjà
enregistré**, y compris les pièces d'il y a six mois. Vous arrivez sur un dossier
en cours de route ? Vous rangez le passé aussi.

### Quand une règle manque

L'opération n'est **jamais perdue**. Elle part au **compte d'attente 471**, avec
un avertissement qui vous dit précisément ce qui manquait. Écrivez la règle,
puis cliquez sur **« Rejouer ce qui est en attente »**.

Vous pouvez aussi ouvrir l'écriture et choisir le bon compte **à la main**, ligne
par ligne. En cas de doute, **c'est vous qui tranchez**.

### Grand livre

Tout ce qui est passé par un compte, dans l'ordre du temps, avec le **solde qui
se met à jour ligne après ligne**. C'est différent des historiques du reste de
Djigui : ceux-ci sont classés par objet (ce stock, cette caisse), celui-ci est
classé **par compte**.

**Rapprocher (lettrer)**, c'est dire « ce règlement paie cette facture ». Cochez
les lignes, cliquez : elles reçoivent la même lettre. Ce qui n'a pas de lettre
est ce qui **reste dû**. Un rapprochement partiel est normal (un acompte, un
paiement en plusieurs fois).

### Balance

Le résumé : par compte, ce qui est entré, ce qui est sorti, ce qui reste.

Le total du débit doit être **exactement égal** au total du crédit. Sinon la page
l'affiche en rouge — c'est un vrai défaut, pas une souplesse.

Un solde marqué **« inhabituel »** n'est pas forcément une erreur : un client qui
a payé d'avance, par exemple. C'est un signal, pas un verdict.

### Corriger une erreur

Une écriture juste ne se modifie pas et ne s'efface pas : on la
**contre-passe**, c'est-à-dire qu'on en écrit une seconde, à l'envers, qui
l'annule. Les deux restent visibles. C'est la façon correcte de corriger, et
c'est déjà le principe retenu partout ailleurs dans Djigui (annulation d'une
vente encaissée, journal de stock).

---

## Projets

### Liste des projets
Cartes groupées par statut (planifié, en cours, suspendu, clôturé) avec la barre
d'avancement.

### Détail d'un projet
L'en-tête montre les chiffres clés : budget saisi, budget planifié, écart, coût de
la main-d'œuvre, coût des ressources, et deux jauges — **avancement physique**
(moyenne pondérée par le budget) et **avancement budgétaire** (dépenses ÷ budget).

Onglets :

- **Liste** — les activités, jusqu'à **4 niveaux** de sous-activités. Pour une
  activité qui a des enfants, budget, dates et avancement sont **calculés depuis
  les enfants** et donc grisés : on saisit au plus bas niveau.
- **Gantt** — colonnes configurables à gauche, frise à droite. L'échelle
  (jour / semaine / mois) s'adapte à la durée, ou se force. Couleur par niveau, la
  jauge dans la barre montre l'avancement. Les flèches relient les activités liées.
  Un **trait rouge marque aujourd'hui** ; tout ce qui est à sa gauche devait déjà
  être fait.
- **Par personne** — le planning d'un intervenant. Les **chevauchements de dates
  apparaissent en rouge** : c'est une surcharge.
- **Ressources** — matérielles ou humaines. Pour chaque personne : temps, coût sur
  le projet, part du budget.
- **Jalons & livrables** — les dates clés et ce que le projet doit produire. Un
  retard est **signalé**, aucune date n'est recalculée.
- **Documents** — pièces jointes (20 Mo maximum), rattachables à une activité, un
  jalon ou un livrable.

**Voir ce qui est en retard.** Une activité est en retard quand sa **fin prévue
est passée** et qu'elle **n'est pas terminée**. Elle se repère à trois signes :

- la ligne prend une **teinte jaune très légère** avec un **filet orange** à gauche ;
- sa barre du Gantt se couvre de **hachures rouges** sur la partie échue (les
  couleurs des barres servant à indiquer le niveau, c'est la hachure qui dit le retard) ;
- une **pastille rouge « i »** apparaît près du nom. **Passez la souris dessus** :
  une bulle vous dit depuis combien de jours, et sur une activité parente elle
  **nomme les sous-activités** concernées — utile quand la branche est repliée.

Un retard est **signalé, jamais corrigé tout seul** : Djigui ne déplace aucune
date à votre place.

**« Ne peut commencer qu'après »** : c'est ainsi qu'on déclare un prédécesseur.
Quand des dates ne respectent plus les liens, un bandeau propose **« Harmoniser les
dates »** — vous voyez d'abord un **aperçu** de ce qui changerait, rien n'est
modifié avant votre accord. Même principe pour « Ajuster la fin » quand les
activités dépassent la fin prévue du projet.

Bouton **Exporter (.xlsx)** : cinq feuilles, avec le Gantt dessiné en cellules
colorées, les jalons en losanges et les sous-totaux par personne.

---

## Marchés

Pour suivre un appel d'offres du début à la fin : préparer le dossier, recevoir
les offres, attribuer, suivre l'exécution, réceptionner.

### Liste des marchés
Chaque marché montre son numéro, son objet, son montant, son avancement et son
retard éventuel. Vous pouvez filtrer par statut, par type, par responsable, ou
chercher librement.

### Créer un marché
Choisissez le **type de marché** (Travaux, Fournitures, Services, Prestations
intellectuelles) : Djigui installe automatiquement **toutes les étapes de la
procédure** avec leurs dates, calculées d'après la durée de chacune. La modale
vous les montre à droite : **ajustez-les avant de créer**, c'est plus simple que
de les corriger une par une ensuite.

Si les procédures proposées ne correspondent pas à vos habitudes, le bouton
**Types de marché** vous laisse les modifier. Attention : vos modifications
valent pour les **prochains** marchés. Un marché déjà lancé garde ses étapes —
c'est voulu, on ne réécrit pas un dossier en cours.

### Suivre les étapes
Marquer une étape **terminée** enregistre la date du jour, votre nom et l'heure.
C'est ce qui donne au dossier sa valeur de preuve.

Une étape en retard apparaît **en jaune**. **Rien n'est bloqué** : la suivante
peut démarrer quand même. Le bouton **« Décaler les suivantes »** vous montre
d'abord ce que deviendraient les dates — rien n'est écrit avant votre accord.

### Les étapes se suivent dans l'ordre
Une procédure n'est pas une liste de cases à cocher : **chaque acte fonde le
suivant**. On ne peut pas évaluer des offres qu'on n'a pas ouvertes.

- L'**étape du moment** est mise en avant en vert. Les suivantes portent un
  **cadenas** qui dit ce qu'il faut terminer d'abord.
- Si vous reprenez un dossier **déjà commencé sur papier**, le bouton **cadenas
  ouvert** permet de **passer outre**. Djigui vous demande le motif et garde
  votre nom : l'étape est alors marquée d'un point d'exclamation.
- ⚠️ **Annuler une étape déjà franchie remet en cause tout ce qui en découle.**
  Les étapes suivantes repassent « à faire ». C'est normal : si l'ouverture des
  plis est annulée, l'évaluation et l'attribution qui en découlaient ne valent
  plus rien. Djigui vous prévient avant, vous dit ce qu'il a rouvert, et **garde
  la trace** de ce qui avait été validé dans les observations.

### Les dates doivent se suivre
Quand vous validez une étape, Djigui vous demande **la date réelle de l'acte**.
Cette date ne peut pas être **antérieure** à celle de l'étape précédente : on ne
publie pas un avis avant d'avoir préparé le dossier qui le rend possible. Elle ne
peut pas non plus être **postérieure** à une étape déjà faite après elle.

Le calendrier vous montre directement les dates possibles et vous dit pourquoi
(« Ne peut pas être avant le 28/07/2026 — Préparation du dossier »).

Si vous reprenez un dossier commencé sur papier avec des dates qui ne collent
pas, le bouton **cadenas ouvert** permet de **passer outre** avec un motif.

Les dossiers saisis avant cette vérification peuvent contenir des dates
impossibles : Djigui vous les signale dans les **points à vérifier**, dès la
liste des marchés.

### La colonne « Écart »
Dans le déroulé, elle indique la distance entre le **réalisé** et le **prévu** :

- **+8 j** en rouge : l'étape a été faite avec 8 jours de retard ;
- **−2 j** en vert : elle a été faite 2 jours en avance ;
- **0 j** en vert : elle a été faite le jour prévu ;
- **+3 j en italique** : l'étape **n'est pas encore faite** et son échéance est
  passée — le retard court en ce moment même.

### Quand la procédure s'arrête
- **Appel d'offres infructueux** — aucune offre valable reçue. La procédure
  repart après la publication, les offres sont **écartées mais conservées**
  (elles prouvent que la consultation a eu lieu), l'attribution est annulée et le
  marché passe en 2ᵉ tentative.
- **Recours** — une entreprise conteste. La procédure est **gelée** jusqu'à la
  décision : les étapes n'avancent plus. C'est un arrêt **subi**, pas un retard
  de votre part, et l'écran le dit. Quand la décision tombe, cliquez sur
  **Clore** et la procédure repart.

### Suis-je dans les temps ?
L'en-tête du marché affiche **deux jauges** : la **procédure** (étapes franchies)
et le **délai écoulé**. Si la seconde avance plus vite que la première, elle passe
en **orange** — c'est le signal qu'il faut accélérer.

### Soumissionnaires
Enregistrez chaque offre reçue : montant, délai annoncé, et si vous les utilisez
les notes technique et financière. L'**écart avec votre estimation** est calculé
tout seul.

Rattacher un soumissionnaire à une fiche contact est **facultatif** : recevoir
une offre ne doit pas vous obliger à créer un tiers.

**Attribuer** fait tout d'un coup : l'offre passe « retenue », les autres sont
écartées, et le marché reçoit son attributaire et son montant.

### Avenants
Un avenant sert quand le marché **change en cours de route** : des travaux en
plus, un délai rallongé, une quantité revue.

- Le **montant d'origine ne bouge jamais**. Djigui l'additionne avec les avenants
  pour afficher le **montant courant**. Vous gardez ainsi la trace de ce qui
  était prévu et de ce qui a été ajouté.
- Un avenant reste un **projet** tant qu'il n'est pas approuvé : il ne compte pas
  encore. C'est l'**approbation** qui l'engage, et elle enregistre qui a approuvé
  et quand.
- Une fois approuvé, un avenant ne se modifie plus et ne se supprime plus. Pour
  revenir dessus, prenez un **nouvel avenant en sens inverse** — comme un avoir
  sur une facture. Un montant **négatif** est accepté : c'est une diminution.
- Si un avenant allonge le délai, Djigui vous **propose** la nouvelle date de
  clôture dans les alertes. **Il ne la change pas tout seul.**
- Quand les avenants dépassent **30 %** du montant de départ, un avertissement
  s'affiche. C'est un repère habituel des marchés publics, **pas une
  interdiction**.

### Réception
La **réception provisoire** se prononce quand le travail est livré. La
**réception définitive** vient après, une fois la garantie écoulée.

- Si tout n'est pas conforme, choisissez **« avec réserves »** et **écrivez ce
  qui reste à faire**. Djigui l'exige : une réserve qu'on ne peut pas relire ne
  sert à rien.
- Tant que les réserves ne sont pas levées, l'écran vous rappelle que **la
  retenue de garantie reste due** à l'entreprise. Le bouton **« Lever les
  réserves »** date cette levée.
- La **retenue de garantie** est la part du montant que vous conservez le temps
  de la garantie. Notez-la ici pour ne pas l'oublier au moment de payer.
- Rien n'est bloqué : une réception définitive prononcée trop tôt sera
  **signalée**, jamais refusée.

### Supprimer ou annuler un marché
Un marché ne se supprime que s'il n'a **rien produit** : aucune étape franchie,
aucun avenant approuvé, aucune réception. Dès qu'il a une histoire, on
**l'annule avec un motif** — l'histoire ne s'efface pas.

---

## Réglages

### Paramètres
- **Société** — raison sociale, forme juridique, capital, régime fiscal, NINEA,
  RCCM, adresse, logo. Un bandeau signale les mentions légales manquantes.
- **Taxes** — catalogue des taxes (une vente peut en porter plusieurs), taux par
  défaut, case « assujetti à la TVA ».
- **Impression** — format du ticket, impression automatique ou à la demande.
- **Moyens de paiement** — créez vos moyens (Orange Money, Wave…) avec image et
  couleur, réordonnez-les par glisser-déposer, activez ou désactivez.
- **Catalogue** — assistant en deux étapes pour démarrer avec un catalogue métier
  tout prêt : choisissez le type de commerce, puis **cochez les articles** que vous
  voulez. Rien n'est ajouté sans votre validation, et relancer l'assistant ne crée
  pas de doublons.

### Modules *(administrateur)*
Djigui est fait de **modules**. Votre **formule** détermine ceux auxquels vous
avez droit : ce sont vos modules, ceux que vous payez.

- Parmi eux, l'**interrupteur** vous laisse en **masquer** : le module disparaît
  du menu, ce qui allège l'écran d'une fonction que vous n'utilisez pas encore.
- **Masquer n'efface rien et ne change pas votre facture.** Vos données restent
  en place et reviennent dès que vous réaffichez le module. Djigui vous rappelle
  ce qu'il contient avant de le masquer (« 8 marchés seront conservés »).
- Les modules **grisés** ne font pas partie de votre formule. Ils sont là pour
  vous montrer ce que le logiciel sait faire d'autre — contactez Djigui si l'un
  vous intéresse. **Ils n'encombrent jamais votre menu de travail.**
- Le module **Base** ne se masque pas : sans lui l'application ne fonctionne plus.
- Certains modules en demandent d'autres : les abonnements ont besoin de la
  facturation. Djigui vous en avertit plutôt que de vous laisser une fonction
  vide entre les mains.

Le bouton **Formule** (réservé à l'installateur) pose ce qui est vendu : on
choisit une formule, qui coche un ensemble de modules, puis on ajuste.

### Utilisateurs *(administrateur)*
Comptes, rôles, activation. Le dernier administrateur ne peut pas être désactivé.

### Journal d'audit *(administrateur)*
Qui a fait quoi et quand. Le nom de l'auteur est conservé même si le compte est
supprimé plus tard.

---

## Questions fréquentes

**Mon stock est négatif, est-ce normal ?**
C'est possible et volontaire : si vous avez vendu ou fabriqué sans que l'entrée
correspondante ait été saisie, Djigui préfère refléter la réalité plutôt que de
vous empêcher de travailler. Corrigez par un **inventaire**.

**J'ai supprimé une recette, mes fabrications sont-elles perdues ?**
Non. Les ordres de fabrication gardent leurs composants ; seul le lien vers la
recette disparaît.

**Mes bénéfices affichent zéro de coût.**
Les prix d'achat de vos articles ne sont pas renseignés. Complétez-les dans
Articles ; pour les articles fabriqués, la clôture d'un ordre de production peut le
faire à votre place.

**Mon export ne se télécharge pas.**
Il ne passe pas par le navigateur : le fichier est écrit dans vos
**Téléchargements** puis ouvert. Le chemin exact vous est affiché.

**Le nombre affiché sur la cloche ne baisse pas.**
Il compte les rappels **non lus**. « Tout marquer comme lu » les met de côté ; les
situations réelles (retards, stock bas) restent visibles dans la liste.
