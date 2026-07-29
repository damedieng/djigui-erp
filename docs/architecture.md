# Architecture — Djigui Desktop

État au **2026-07-26**. Réf. spec : `claude_spec/djigui-desktop-spec.md`.
Ce document décrit **comment** le code est organisé pour respecter le **quoi** de
la spec. Voir aussi `docs/dictionnaire-donnees.md` et `docs/manuel-utilisateur.md`.

## Vue d'ensemble

```
┌──────────────────────────────────────────────┐
│  djigui-desktop (Tauri)                      │
│  WebView2 ── charge frontend/ ── fetch ──┐   │
└──────────────────────────────────────────│───┘
                                           ▼
                          ┌────────────────────────────┐
   Autres postes  ──────► │  djigui-server (axum)      │  API HTTP/JSON, port 1704
   (mode client, à faire) │  Mutex<Connection> unique  │  §2.1 écrivain unique
                          │        │                   │
                          │        ▼                   │
                          │  djigui-core (métier + DB) │
                          │        │                   │
                          │        ▼  SQLite (WAL)     │
                          └────────────────────────────┘
```

- **`crates/core`** — cœur métier + accès données. Aucune dépendance réseau.
  - `db.rs` : ouverture SQLite (WAL, FK), **migrations versionnées** (0001→0031).
  - `domain.rs` : enums reflétant les CHECK du schéma, via la macro `enum_texte!`.
  - `authorization.rs` : point de contrôle unique (§3.4).
  - `modules/` : un module par frontière métier — 25 modules aujourd'hui.
- **`crates/server`** — transport HTTP/JSON (axum) + service des fichiers `frontend/`.
  **Aucune règle métier** : uniquement traduction appel↔cœur et erreur↔code HTTP.
  Contient aussi les exports fichiers (`export.rs`, `export_projet.rs`) et
  l'impression (`impression.rs`).
- **`crates/desktop`** — coquille Tauri. Décide de la fermeture de fenêtre
  (`on_window_event`) et déclare ses capacités dans `capabilities/default.json`.
- **`frontend/`** — JS **vanilla**, une page HTML par écran, pas de framework ni de
  build. (La spec du module projet mentionne Vue.js : c'est une erreur, on reste
  sur l'existant.)

## Décisions clés

### Un seul écrivain
`Mutex<Connection>` (§2.1). Correct parce que l'architecture interdit tout accès
concurrent au fichier. Tout le SQL vit dans `core` — aucune requête ailleurs —
ce qui laisse la porte ouverte à PostgreSQL plus tard.

### Migrations
Fichiers `crates/core/migrations/NNNN_nom.sql`, listés dans `MIGRATIONS` de `db.rs`,
suivis par `schema_migrations`, chacune dans sa transaction.

- On **n'édite jamais** une migration publiée ; on en ajoute une nouvelle.
- Pour changer une table existante, SQLite impose la **reconstruction** : créer la
  nouvelle table, `INSERT … SELECT` depuis l'ancienne, supprimer l'ancienne.
  ⚠️ **Toujours recopier les données** (exigence explicite de l'utilisateur : « ne
  supprime pas les données »). Exemples : 0026, 0031.
- Le test `db::migration_…` vérifie la version maximale : **le bumper à chaque ajout**.

### Piloté par la donnée
Le comportement des types de documents (impact stock, transformations autorisées,
préfixe de numérotation) vit dans des tables `config_*`, pas dans des `if type == …`.

### Le journal fait foi
Le stock n'est **jamais** stocké : il se recalcule depuis `mouvement_stock`. Les
soldes de caisse et de tiers sont stockés pour la performance, mais reconstructibles
depuis les journaux (`POST /api/recalcul-soldes`).

### Inaltérabilité des pièces
Une pièce validée ne se modifie pas et ne se supprime pas : elle s'**annule**, ce
qui produit des écritures inverses (stock réintégré, encaissements contre-passés).
Même principe pour un ordre de production terminé.

## Trois règles de conception qui reviennent partout

1. **Alerter, ne pas bloquer.** Stock insuffisant, prix d'achat manquant, écart de
   production, incohérence de dates, NINEA absent : tout cela s'affiche en jaune et
   laisse passer. Le logiciel ne doit empêcher ni de vendre, ni de produire.
   **Deux exceptions, et deux seulement** : une écriture comptable déséquilibrée
   est refusée (elle fausserait la balance sans que personne ne s'en aperçoive —
   voir `modules/comptabilite.rs`), et la clôture d'exercice le sera aussi le jour
   où elle existera.
2. **Rien d'automatique dans le dos de l'utilisateur.** Pas de propagation de dates,
   pas de repli mémorisé, pas d'en-tête qui se compacte au défilement. Quand un
   recalcul est possible, on propose un **aperçu** et un bouton explicite
   (« Harmoniser les dates », « Ajuster la fin »).
3. **Supprimer = détacher.** Supprimer un jalon ne détruit ni ses livrables ni ses
   documents ; supprimer une recette ne détruit pas les ordres de fabrication ;
   supprimer une règle comptable ne détruit pas les écritures qu'elle a produites.
4. **Corriger = contre-passer, jamais effacer.** Journal de stock, annulation
   d'une vente encaissée (mig 0019), écritures comptables (mig 0034) : on ajoute
   l'opération inverse, on ne réécrit pas le passé. Ce réflexe pris dès le début
   est ce qui a rendu la greffe comptable naturelle.

### Le procédé comptable, à part

La comptabilité (mig 0034) suit une logique **inverse** de celle qu'on trouve
d'ordinaire dans un ERP, et c'est une décision de l'utilisateur : Djigui ne
devine aucun compte. Le **comptable** crée ses comptes, écrit des **règles
multicritères**, et celles-ci s'appliquent à **tout l'historique déjà en base**
comme aux opérations futures. Le moteur connaît le schéma de chaque opération et
possède déjà tous les montants ; la règle ne fait que nommer les comptes.

Conséquence pratique : le module ne touche à **aucun flux existant**. Rien dans
la vente, la caisse ou le stock ne dépend de lui. On peut l'ignorer entièrement.

## Frontend

- **Menu centralisé** dans `assets/app.js` (liste `MENU`) : les pages n'ont qu'un
  `<aside class="sidebar">` vide. Ajouter un écran = **une ligne**. Auparavant le
  menu était recopié dans 14 pages, avec trois divergences constatées.
- `app.js` fournit aussi `Djigui.api`, `fmt`, `esc`, `toast`, `confirm`/`alert`
  thématisés, `selectRecherche` (select cherchable), la session utilisateur, la
  cloche de notifications et l'aide repliable.
- **Chaque écran a une section d'aide** (`.aide`) en langage simple : les
  utilisateurs visés ne sont pas tous à l'aise avec l'écrit.
- **Toute liste sait agir par lot** : cases à cocher + barre d'actions groupées.

### Deux pièges CSS à ne jamais réintroduire

1. `.content` est un flex colonne : un enfant peut être **écrasé à 0 px** quand le
   contenu déborde. D'où `.content > * { flex-shrink: 0 }`.
2. **`[hidden]` est battu par un `display` d'auteur.** Un élément masqué par
   `hidden` mais portant `display:flex` (surtout en style inline, imbattable) reste
   visible. Toute classe qui pose un `display` doit poser aussi
   `.maclasse[hidden] { display: none }` — voir `.bulk-bar`, `.modal-overlay`,
   `.notif-panneau`, `.tab-panel`.

### Contraintes du WebView2 (Tauri)
Le WebView ne sait pas télécharger : `<a download>` et les blobs sont bloqués.
Les exports (.xlsx, .ics) sont donc **écrits sur le disque par le serveur** dans
`%USERPROFILE%\Downloads`, puis ouverts, et le chemin réel est renvoyé à l'écran.
Même chose pour l'ouverture d'un document joint.

## Tests

| Portée | Où | Comment |
|---|---|---|
| Cœur métier | `cargo test -p djigui-core` | **66 tests**, base en mémoire. **Jamais sur la vraie base.** |
| Interface (logique) | `cd frontend/tests && npm test` | **138 tests** jsdom, rapides. |
| Interface (visuel) | `npm run test:navigateur` | Pilote le Chrome **déjà installé** (puppeteer-core, aucun téléchargement), mesure les hauteurs réelles, le défilement, la troncature, les erreurs console. |

⚠️ **jsdom ne calcule pas la mise en page** : `getBoundingClientRect` renvoie 0.
Pour tout doute visuel, passer directement par Chrome — c'est l'absence de ce
réflexe qui a laissé passer cinq bugs d'affichage le 2026-07-25.

Réflexe complémentaire : extraire le `<script>` inline d'une page et lancer
`node --check` avant de conclure qu'elle fonctionne.

## Lancer en développement

```bash
# Depuis la RACINE du dépôt, sinon le serveur ne trouve pas frontend/ (404).
cd /d/DJGUI_ERP && CARGO_HTTP_CHECK_REVOKE=false cargo run -p djigui-desktop

# Serveur seul (API + pages sur http://localhost:1704)
cargo run -p djigui-server
```

Variables d'environnement : `DJIGUI_DB`, `DJIGUI_FRONTEND`, `DJIGUI_PORT` (défaut 1704).

⚠️ Une seule instance à la fois : deux processus se disputent le port 1704
(« Address already in use »). Faire `taskkill /IM djigui-*.exe /F` avant de relancer.

## Conventions

- Code et identifiants métier en **français** (aligné sur la spec et le domaine).
- Tout horodatage via `djigui_core::now()`.
- `ApiError` est un **newtype** `ApiError(CoreError)`, pas un enum.
- Codes HTTP : `NotFound` → 404, `Rule` → **422**, `Forbidden` → 403,
  `Unauthorized` → 401, `Db` → 500.
