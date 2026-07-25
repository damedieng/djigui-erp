# Djigui — Sourcing des images du seeder

Complément à `SEEDER-CATALOGUES.md` §7. Décrit d'où viennent les images embarquées dans le binaire, sous quelle licence, et comment les produire de façon reproductible.

---

## 1. Stratégie à deux niveaux

Ne pas chercher 265 photos. Deux niveaux, dans cet ordre :

### Niveau 1 — Pictogramme (couverture 100 %, immédiat)

Chaque catégorie et chaque article a une **icône Tabler** (déjà utilisée dans toute l'UI, licence MIT, redistribution commerciale sans condition). Rendue en tuile colorée 256×256, c'est propre, lisible à distance sur un écran de caisse, et cela pèse ~1 Ko en SVG.

C'est le socle. Il rend le produit livrable **sans une seule photo**.

### Niveau 2 — Photo (couverture partielle, progressif)

Une photo n'apporte un vrai gain que là où l'utilisateur reconnaît le produit plus vite qu'il ne lit son nom : alimentation et restauration essentiellement. Sur une prestation (« Réparation fermeture éclair »), une photo n'apporte rien qu'un pictogramme ne dise mieux.

Cible réaliste : **60 à 80 photos**, pas 265. Le reste reste en pictogramme, définitivement.

---

## 2. Sources et licences

| Source | Licence | Clé API | Verdict |
|---|---|---|---|
| **Openverse** (api.openverse.org) | Agrège CC0, domaine public, CC-BY… **filtrable par licence** | Non requise (quotas bas) ou gratuite | ✅ Recommandé pour l'automatisation |
| **Wikimedia Commons** | Variable, beaucoup de CC0/PD | Non | ✅ Bon pour les produits agricoles et locaux |
| **Pexels / Unsplash** | Licence propriétaire permissive : usage commercial et intégration produit autorisés | Gratuite, requise | ✅ Meilleure qualité photo, en secours |
| **Pixabay** | Content License, usage commercial OK | Gratuite | ⚠️ Vérifier au cas par cas |
| Résultats Google Images | Inconnue | — | ❌ Jamais |

**Filtre retenu pour l'automatisation : `cc0` et `pdm` (domaine public) uniquement.** Ce sont les seules licences qui n'imposent aucune obligation d'attribution — donc aucune contrainte à porter dans un binaire distribué à des commerçants.

Si tu veux élargir à `by` / `by-sa` pour la qualité, il faut alors un **écran « Crédits » dans l'application** listant auteurs et licences. C'est faisable, mais c'est une dette : commence en CC0 strict.

---

## 3. Revue visuelle — étape non négociable

Le script génère une planche-contact HTML. Passer chaque image en revue et rejeter :

- ❌ tout **logo ou emballage de marque visible**, même partiellement — la licence de la photo ne couvre pas la marque photographiée ;
- ❌ tout **visage identifiable** — droit à l'image, distinct du droit d'auteur ;
- ❌ les images hors sujet (les moteurs de recherche d'images libres remontent régulièrement n'importe quoi) ;
- ❌ les photos trop sombres, floues, ou dont le sujet ne survit pas à la réduction à 256 px.

Compter un taux de rejet de **40 à 60 %**. C'est normal. Le script est conçu pour être relancé sur les codes rejetés avec une requête différente.

---

## 4. Chaîne de traitement

```
requetes_images.json   →  telecharger_images.py  →  assets/catalogue/images/articles/*.webp
   (code → requête)          (Openverse API)           SOURCES.md   (traçabilité licences)
                                                       planche.html (revue visuelle)
```

Caractéristiques du script :

- **Idempotent** : un fichier déjà présent n'est pas retéléchargé. On relance sans crainte, seuls les manquants sont traités.
- **Traçabilité** : `SOURCES.md` consigne pour chaque image son titre, son auteur, sa licence et son URL d'origine. Ce fichier est ta preuve en cas de contestation — il se versionne avec le code.
- **Normalisation** : recadrage carré centré, 256×256, WebP qualité 80. Voir §7.2 de la spec.
- **Tolérant aux pannes** : un lien mort ou une API indisponible n'interrompt pas le lot, l'échec est journalisé.

### Utilisation

```bash
pip install requests pillow

# Tout le lot prioritaire
python telecharger_images.py --sortie assets/catalogue/images/articles

# Un seul article, après rejet en revue
python telecharger_images.py --seulement riz_brise_parfume_kg --requete "broken rice grains bowl"

# Voir ce qui manque sans rien télécharger
python telecharger_images.py --manquants
```

Puis ouvrir `planche.html`, supprimer les fichiers rejetés, relancer.

---

## 5. Ce que le script ne fait pas

- Il **ne vérifie pas** que l'image correspond au produit. Un moteur d'images libres ne comprend pas « nététou ».
- Il **ne détecte pas** les marques ni les visages.
- Il **ne juge pas** la qualité esthétique.

Ces trois points restent humains, et c'est précisément pour ça que la cible est 60–80 photos et non 265 : c'est une demi-journée de revue, pas une semaine.

---

## 6. Recommandation de séquencement

1. Implémenter le seeder **sans images** (spec §11, étapes 1–3) — livrable et testable seul.
2. Ajouter le repli pictogramme (§7.5) → l'application est déjà présentable.
3. Lancer le script sur `alimentation_generale`, revue, intégration.
4. Idem `restaurant_fast_food`.
5. S'arrêter là et voir si les utilisateurs réclament davantage. Probablement pas.
