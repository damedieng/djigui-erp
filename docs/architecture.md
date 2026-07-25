# Architecture — Djigui Desktop

Réf. spec : `claude_spec/djigui-desktop-spec.md`. Ce document décrit **comment**
le code est organisé pour respecter le **quoi** de la spec.

## Vue d'ensemble

```
┌─────────────────────────────────────────────┐
│  Coquille desktop (Tauri) — à venir          │
│  WebView ── charge frontend/ ── fetch ──┐    │
└─────────────────────────────────────────│────┘
                                          ▼
                         ┌────────────────────────────┐
   Autres postes  ─────► │  djigui-server (axum)       │  API HTTP/JSON
   (mode client)         │  Mutex<Connection> unique   │  §2.1 écrivain unique
                         │        │                    │
                         │        ▼                    │
                         │  djigui-core (métier + DB)  │
                         │        │                    │
                         │        ▼  SQLite (WAL)      │
                         └────────────────────────────┘
```

- **`crates/core`** — cœur métier + accès données. Aucune dépendance réseau.
  - `db.rs` : ouverture SQLite (WAL, FK), **migrations versionnées**.
  - `domain.rs` : enums reflétant les CHECK du schéma.
  - `authorization.rs` : **point de contrôle unique** (§3.4), renvoie OK en v1.
  - `modules/` : un module par frontière métier (§3.4) — `parametres`, `tiers`, …
- **`crates/server`** — transport HTTP/JSON (axum) + service des fichiers
  `frontend/`. Ne contient **aucune règle métier**, uniquement traduction
  appel↔cœur et erreur↔code HTTP.
- **`frontend/`** — UI (repris des maquettes). À brancher sur l'API.

## Décisions clés

- **Un seul écrivain** (§2.1) : `Mutex<Connection>`. Correct car l'architecture
  interdit tout accès concurrent au fichier. Voie PostgreSQL laissée ouverte via
  l'isolement de l'accès DODO dans `core` (aucun SQL hors de `core`).
- **Migrations** : fichiers `crates/core/migrations/NNNN_nom.sql`, listés dans
  `MIGRATIONS` de `db.rs`, suivis par la table `schema_migrations`. On **n'édite
  jamais** une migration publiée ; on en ajoute une nouvelle.
- **Piloté par la donnée** (§3.2/§6.1) : le comportement des types de documents
  (impact stock, transformations, plus tard numérotation) vit dans des tables
  `config_*`, pas dans des `if type == …`.
- **Soldes** (§6.4) : dérivés mais tenus à jour à l'écriture ; un utilitaire de
  recalcul depuis les journaux reste la source de vérité (à implémenter).

## Lancer en développement

```bash
# variables d'env optionnelles : DJIGUI_DB, DJIGUI_FRONTEND, DJIGUI_PORT (déf. 1704)
cargo run -p djigui-server
# puis ouvrir http://localhost:1704
```

## Conventions

- Code et identifiants métier en **français** (aligné sur la spec et le domaine).
- Tout horodatage via `djigui_core::now()` (ISO-8601 UTC).
- Voir `JOURNAL.md` pour l'état d'avancement des tâches.
