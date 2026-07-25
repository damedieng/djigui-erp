# Djigui — Seeder de catalogues métier

**Spécification technique à l'attention de Claude Code.**
Ce document décrit *comment* implémenter le pré-remplissage du catalogue (catégories + articles + images) selon le type de commerce, choisi à l'onboarding. Les données elles-mêmes sont dans `CATALOGUES-DONNEES.md`.

---

## 1. Objectif

Un commerçant qui installe Djigui et découvre « 0 article » doit saisir des dizaines de lignes avant sa première vente. C'est le point d'abandon principal du produit. Le seeder ramène le *time-to-first-sale* de plusieurs heures à quelques minutes.

Ce n'est pas un catalogue définitif : c'est une **amorce crédible**. 5 à 7 catégories et 15 à 25 articles par type de commerce suffisent. L'exhaustivité est un piège — elle multiplie la dette de maintenance sans améliorer l'adoption.

**Périmètre**
- ✅ Créer catégories + articles + images associées
- ✅ Rejouable à tout moment depuis Paramètres
- ❌ Ne crée jamais de tiers, de stock, de prix, ni de mouvement comptable

---

## 2. Arborescence

```
djigui/
├── src-tauri/
│   ├── src/
│   │   └── seeder/
│   │       ├── mod.rs           # API publique : appliquer_catalogues(&[&str])
│   │       ├── modele.rs        # structs serde
│   │       └── application.rs   # insertion SQL transactionnelle
│   ├── assets/
│   │   └── catalogue/
│   │       ├── types.json               # index des types de commerce
│   │       ├── types/
│   │       │   ├── alimentation_generale.json
│   │       │   ├── restaurant_fast_food.json
│   │       │   └── …
│   │       └── images/
│   │           ├── articles/            # pool partagé, 256×256 .webp
│   │           │   ├── eau_minerale_1_5l.webp
│   │           │   └── …
│   │           └── categories/          # optionnel, sinon icône Tabler
│   └── build.rs                 # validation des JSON à la compilation
```

**Règle structurante : les catalogues sont de la *donnée*, pas du code.** Aucune catégorie ni article ne doit apparaître en dur dans un `.rs`. On doit pouvoir corriger un libellé ou ajouter un type de commerce sans toucher à la logique Rust — et, plus tard, rafraîchir les catalogues depuis le service cloud sans redéployer le binaire.

---

## 3. Schéma des données

### `types.json` — index

```json
{
  "version": 1,
  "types": [
    {
      "code": "alimentation_generale",
      "libelle": "Alimentation générale / boutique",
      "description": "Boutique de quartier, épicerie, libre-service",
      "icone": "ti-building-store",
      "ordre": 10
    }
  ]
}
```

### `types/<code>.json` — un catalogue

```json
{
  "code": "alimentation_generale",
  "version": 1,
  "categories": [
    {
      "code": "boissons",
      "libelle": "Boissons",
      "icone": "ti-bottle",
      "couleur": "#2563eb",
      "ordre": 10,
      "articles": [
        {
          "code": "eau_minerale_1_5l",
          "libelle": "Eau minérale 1,5 L",
          "unite": "bouteille",
          "gere_stock": true,
          "tva": 18,
          "prix_vente": null,
          "image": "articles/eau_minerale_1_5l.webp"
        }
      ]
    }
  ]
}
```

### Contraintes de champs

| Champ | Type | Règle |
|---|---|---|
| `code` | string | **Identifiant stable.** `snake_case`, ASCII, sans accent. Ne jamais le modifier après publication — c'est la clé d'idempotence. |
| `libelle` | string | Affiché à l'utilisateur. Modifiable librement d'une version à l'autre. |
| `unite` | enum | Référence `unites.code`. Voir §4. |
| `gere_stock` | bool | `false` pour les prestations (coupe de cheveux, retouche). Évite les alertes de stock absurdes. |
| `tva` | int | Taux par défaut. Voir §5. |
| `prix_vente` | null | **Toujours `null`.** Voir §5. |
| `image` | string / null | Chemin relatif dans `assets/catalogue/images/`. Voir §7. |
| `ordre` | int | Pas de 10 (10, 20, 30…) pour insérer sans renuméroter. |

---

## 4. Table `unites` — à seeder en premier

Le seeder de catalogues dépend d'un référentiel d'unités déjà présent. À insérer au premier lancement, indépendamment du type de commerce :

`piece`, `paquet`, `sachet`, `boite`, `carton`, `bouteille`, `bidon`, `kg`, `g`, `litre`, `metre`, `paire`, `lot`, `heure`, `prestation`.

---

## 5. Prix et TVA — deux décisions à ne pas contourner

**`prix_vente` est systématiquement `null`.** Les prix varient d'un quartier à l'autre et d'un mois à l'autre. Un prix pré-rempli est une donnée fausse que personne ne corrige, et qui finira sur une facture client. L'article est créé « à compléter » :

- badge orange dans la liste Articles ;
- dans la Caisse, l'ajout au ticket ouvre une saisie de prix au lieu de bloquer ;
- un écran « Compléter mes prix » en fin d'onboarding, listant uniquement les articles sans prix, saisie au clavier numérique en enfilade.

**La TVA du seed est écrasée par le paramètre entreprise.** Beaucoup de commerçants ciblés ne sont pas assujettis. Si `parametres_entreprise.assujetti_tva = false`, tous les articles sont créés à `tva = 0`, quel que soit le contenu du JSON. Le champ `tva` du seed n'est donc qu'un *défaut applicable si assujetti*.

---

## 6. Idempotence — la règle la plus importante

Le seeder est rejouable : à l'onboarding, puis depuis *Paramètres → Articles → Ajouter un modèle de catalogue*, et potentiellement plusieurs fois si le commerce se diversifie. Il doit être **strictement additif**.

**Schéma requis**

```sql
ALTER TABLE categories ADD COLUMN code_seed TEXT;
ALTER TABLE articles   ADD COLUMN code_seed TEXT;
CREATE UNIQUE INDEX idx_categories_code_seed ON categories(code_seed) WHERE code_seed IS NOT NULL;
CREATE UNIQUE INDEX idx_articles_code_seed   ON articles(code_seed)   WHERE code_seed IS NOT NULL;

CREATE TABLE seed_applique (
  code_type    TEXT PRIMARY KEY,
  version      INTEGER NOT NULL,
  applique_le  TEXT NOT NULL
);
```

**Règles d'insertion**

1. `INSERT … ON CONFLICT(code_seed) DO NOTHING`. **Jamais d'`UPDATE`.** Si l'utilisateur a renommé « Eau minérale 1,5 L » en « Eau 1,5 », un re-seed ne doit pas écraser sa saisie.
2. Un article supprimé par l'utilisateur ne doit pas réapparaître : utiliser une suppression logique (`supprime_le`) et considérer le `code_seed` comme consommé.
3. **Dédoublonnage inter-types.** Le choix est multiple (§8) : « eau minérale » existe dans *alimentation* et dans *restaurant*. Le `code` article étant global, la seconde insertion est simplement ignorée. C'est pour cela que le pool d'images est partagé et non rangé par type.
4. **Une seule transaction** pour l'ensemble des types choisis. Un échec à mi-parcours laisse un catalogue à moitié rempli, incohérent et pénible à nettoyer.
5. Enregistrer dans `seed_applique` à la fin, pour savoir plus tard quels modèles ont été appliqués et proposer les autres.

---

## 7. Images — le point à traiter avec soin

### 7.1 Contrainte non négociable : hors-ligne

Aucune URL, aucun CDN. Les images sont **embarquées dans le binaire** (`rust-embed` ou `include_bytes!`). Une boutique de Kaolack sans connexion doit voir sa grille de caisse illustrée dès le premier lancement.

### 7.2 Format et budget

| Paramètre | Valeur |
|---|---|
| Format | WebP (qualité 80), fallback PNG si le décodage WebP pose problème sur la WebView cible |
| Dimensions | 256 × 256 px, carré, sujet centré, fond neutre ou détouré |
| Poids cible | 8–15 Ko par image |
| Volume total | ~250 images ≈ 3 Mo — acceptable pour un binaire desktop |

Le carré est imposé par la grille de caisse : toute autre proportion cassera la mise en page. Recadrer à la source, ne jamais compter sur le CSS.

### 7.3 Générique, jamais de marque

⚠️ **Point juridique et pratique à respecter.** Les visuels du seed doivent représenter le *produit*, pas un emballage de marque : une bouteille d'eau neutre, pas la bouteille Kirène ; un stick de café, pas un stick Nescafé.

Deux raisons, la seconde comptant autant que la première :

- Les packagings et logos sont protégés — on ne les redistribue pas dans un binaire commercial.
- Le commerçant ne vend pas forcément cette marque-là. Une image de marque concurrente est pire que pas d'image.

La même logique s'applique aux **libellés** : `Café instantané (stick)` et non `Nescafé`, `Lait concentré sucré 397 g` et non le nom commercial. Le commerçant précise sa marque lui-même s'il le souhaite.

Sourcing : illustrations produites en interne, ou photos sous licence permissive vérifiée (CC0 / domaine public), traçées dans `assets/catalogue/images/SOURCES.md` avec l'origine et la licence de chaque fichier.

### 7.4 Stockage : chemin en base, jamais de blob

```sql
ALTER TABLE articles ADD COLUMN image_chemin  TEXT;
ALTER TABLE articles ADD COLUMN image_origine TEXT CHECK(image_origine IN ('seed','utilisateur'));
```

Au moment du seed, l'image embarquée est **extraite du binaire et écrite** dans le répertoire de données applicatif :

```
{app_data_dir}/media/articles/{code_article}.webp
```

`image_chemin` stocke le chemin relatif (`articles/eau_minerale_1_5l.webp`), jamais un chemin absolu — il casserait au premier changement de machine ou de session Windows.

Pourquoi extraire plutôt que servir depuis le binaire :

- l'utilisateur peut **remplacer** l'image par la photo de son vrai produit — c'est même le comportement souhaitable à terme ;
- la sauvegarde et la restauration manipulent des fichiers ordinaires ;
- la base SQLite reste légère : des blobs d'images gonflent le fichier, ralentissent les sauvegardes et n'apportent rien.

Quand l'utilisateur remplace une image, passer `image_origine` à `'utilisateur'`. Un re-seed ne doit alors jamais réécrire ce fichier.

### 7.5 Repli obligatoire

Une image manquante ou corrompue ne doit pas produire un carré cassé dans la caisse. Repli déterministe : tuile de couleur dérivée du hash du `code` article, avec les deux premières initiales du libellé en blanc. C'est lisible, c'est stable dans le temps, et cela rend l'image facultative dans tout le reste du code.

En pratique, la majorité des articles créés par l'utilisateur n'auront pas d'image : ce repli est le cas nominal, pas l'exception.

---

## 8. Onboarding — comportement attendu

- **Sélection multiple, pas exclusive.** Beaucoup de boutiques sénégalaises sont hybrides : alimentation + recharge téléphonique + cosmétique. Forcer un choix unique produit un catalogue faux. Cases à cocher, union des catalogues, dédoublonnage par `code`.
- **Case « Pré-remplir aussi des articles types »**, cochée par défaut. Certains veulent la structure de catégories sans les articles.
- **Étape passable en un clic** (« Je crée mon catalogue moi-même »). Un onboarding bloquant fait plus de dégâts qu'un catalogue vide.
- **Écran de confirmation** avant insertion : « 6 catégories et 23 articles vont être créés ». Pas de surprise.
- **Rejouable** : *Paramètres → Articles → Ajouter un modèle de catalogue*, proposant les types non encore appliqués (lus dans `seed_applique`).

---

## 9. Validation à la compilation

Ajouter dans `build.rs` — ou à défaut dans un test d'intégration exécuté en CI — les contrôles suivants, qui doivent faire **échouer la compilation** :

1. Tous les JSON parsent sans erreur.
2. Unicité des `code` article et des `code` catégorie **sur l'ensemble des types** (pas seulement au sein d'un fichier).
3. Chaque `unite` référencée existe dans le référentiel.
4. Chaque `image` référencée correspond à un fichier réellement présent dans `assets/`.
5. Inversement : signaler les images orphelines (présentes mais référencées nulle part) — elles alourdissent le binaire pour rien.
6. Tous les `prix_vente` sont `null`.
7. Aucun `code` ne contient d'accent, d'espace ou de majuscule.

Ces règles paraissent tatillonnes ; elles évitent la classe de bugs la plus pénible ici, celle qui ne se manifeste que sur le poste du client, après installation.

---

## 10. Tests attendus

| Test | Attendu |
|---|---|
| Seed sur base vierge | Catégories, articles et fichiers image créés ; `seed_applique` renseigné |
| Seed rejoué à l'identique | 0 insertion, 0 modification, aucun doublon |
| Seed de deux types partageant un article | L'article n'existe qu'une fois |
| Article renommé puis re-seed | Le libellé utilisateur est conservé |
| Article supprimé puis re-seed | L'article ne réapparaît pas |
| Image utilisateur puis re-seed | Le fichier utilisateur n'est pas écrasé |
| Entreprise non assujettie TVA | Tous les articles créés à `tva = 0` |
| Échec en cours d'insertion | Rollback complet, base inchangée |

---

## 11. Ordre d'implémentation suggéré

1. Migrations SQL (`code_seed`, `image_chemin`, `image_origine`, `seed_applique`) et référentiel `unites`.
2. Structs serde + chargement `rust-embed` + validations `build.rs`.
3. Insertion transactionnelle idempotente, **sans images** — et ses tests.
4. Extraction des images vers `app_data_dir` + repli par initiales dans l'UI.
5. Écran d'onboarding (sélection multiple, confirmation, skip).
6. Écran « Compléter mes prix ».
7. Entrée rejouable dans Paramètres.

Les étapes 1 à 3 constituent un incrément livrable et testable seul. Les images peuvent arriver ensuite sans rien casser, puisque le champ est facultatif de bout en bout.
