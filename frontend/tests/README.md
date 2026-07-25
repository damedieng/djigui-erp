# Tests d'interface (frontend)

Deux niveaux, complémentaires.

## 1. Tests de fumée (jsdom) — `npm test`

Rapides (quelques secondes). La page est chargée dans un DOM, le réseau est
remplacé par des données factices, et on vérifie que le script ne plante pas et
que les commandes sont réellement câblées.

```bash
cd frontend/tests
npm install       # une seule fois (jsdom 24 ; plus récent casse avec Node 20)
npm test
```

**Limite majeure, apprise à nos dépens :** jsdom **ne calcule pas la mise en
page**. `getBoundingClientRect()` renvoie toujours 0. Un bloc écrasé à 0 pixel
de haut passe donc inaperçu — c'est exactement le bug qui a fait « disparaître »
l'en-tête du projet sur 4 onglets sur 6, pendant que ces tests étaient au vert.

## 2. Test en vrai navigateur — `npm run test:navigateur`

Pilote **Chrome déjà installé** (aucun téléchargement) et mesure ce que
l'utilisateur voit vraiment : hauteurs réelles, position de défilement,
troncature des montants, erreurs de console.

```bash
# l'application doit tourner sur http://localhost:1704
npm run test:navigateur
```

Il vérifie notamment :

- l'en-tête du projet est visible **sur les six onglets** (hauteur > 0) ;
- on revient en haut à chaque changement d'onglet ;
- la barre verte collante reste en haut pendant le défilement ;
- aucun montant n'est coupé dans sa tuile ;
- aucune erreur ni ressource manquante dans la console.

Il produit aussi deux captures, `reel-haut.png` et `reel-gantt-defile.png`,
utiles pour juger le rendu d'un coup d'œil.

⚠️ L'identifiant de projet est codé en tête du fichier : adaptez-le si la base
de démonstration change.
