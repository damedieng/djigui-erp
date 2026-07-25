# Journal de développement — Djigui Desktop

> Suivi des tâches : ce qui est **fait**, ce qui **reste**. Mettre à jour à
> chaque avancée. Réf. spec : `claude_spec/djigui-desktop-spec.md`.

## Légende
- [x] terminé · [~] en cours · [ ] à faire

## Standards (à respecter pour CHAQUE module) — « du solide »
- **CRUD complet** : créer / lire / lister / **modifier** / **désactiver** (soft delete `actif=0`).
- **Cohérence signalée, non bloquante** : incohérences métier (ex. `prix_achat >= prix_vente`)
  → **alerte jaune** informative, sans bloquer l'enregistrement. Bloquer seulement les vrais
  invariants (ex. service ⇒ pas de stock).
- Chaque module : core + API + UI branchée + tests.
- **Traitement par lot systématique** : chaque écran de liste prévoit sélection multiple +
  barre d'actions groupées (affecter catégorie, changer rôle/statut, désactiver, exporter…) +
  **messages de retour** (toast). À concevoir dès le départ, jamais après coup.

---

## Socle & infrastructure
- [x] Workspace Cargo (`core` + `server`)
- [x] Schéma SQLite initial conforme §5 (`migrations/0001_initial.sql`)
- [x] Système de migrations versionnées (table `schema_migrations`, idempotent, transactionnel)
- [x] Connexion SQLite : WAL + foreign_keys ON, écrivain unique sérialisé (`Mutex`)
- [x] Point d'autorisation unique (`authorization.rs`, renvoie toujours OK en v1) §3.4
- [x] Config pilotée par la donnée : `config_type_document`, `config_transformation`, `parametre_global` §6.1
- [x] Serveur axum : API HTTP/JSON + service des fichiers frontend §2.1
- [x] Contournement SSL cargo Windows (`.cargo/config.toml`)
- [x] Coquille **Tauri** (`crates/desktop`) : fenêtre native, démarre le serveur en interne, charge localhost §2.2
- [x] Icônes de l'app générées depuis le logo (ico + png multi-tailles)
- [ ] Mode client (pointer vers l'IP d'un autre poste) + découverte du serveur §2.1
- [ ] Empaquetage (`tauri build`) : bundler le frontend + installeur Windows
- [x] **Offline** : icônes Tabler + polices (woff2/woff/ttf) téléchargées en local (`assets/tabler/`), CDN retiré de toutes les pages — zéro dépendance externe

## Modules métier (cœur)
- [x] Paramètres entreprise (singleton) §5.9 — lire / enregistrer
- [x] Tiers unifié §3.1 — créer / lire / lister
- [x] **Tiers CRUD complet** : créer/lire/lister(filtre rôle)/modifier/désactiver + recherche
- [x] Tiers : **traitement par lot** (changer rôle en masse, désactiver en masse) + messages toast
- [x] Écran `tiers.html` créé (n'existait pas) : liste, chips rôles, sélection multiple, barre d'actions par lot, modale CRUD
- [ ] Tiers : historique des documents + détail du solde
- [x] Articles & services §5.2 — **CRUD complet** : créer/lire/lister/modifier/désactiver (soft delete)
- [x] Articles : stock dérivé du journal (Σ entrées − sorties) dans le SELECT §3.3
- [x] Articles UI : édition en modale, désactivation, **alerte jaune cohérence prix** (achat ≥ vente, non bloquante), case « gérer le stock » corrigée
- [x] Articles : **images produit** (migration 0004, data-URI base64) — upload modale + miniature liste
- [x] **Catégories** d'articles (migration 0002) : table `categorie` + FK `article.categorie_id`, seed 4 catégories
- [x] **Gérer les catégories** — CRUD complet (créer/lister avec nb_articles/renommer/supprimer) ; suppression déclasse les articles (categorie_id → NULL) ; modale « Gérer les catégories » sur l'écran Articles
- [ ] Filtre par catégorie sur la liste articles et la caisse
- [ ] **Listes de catégories potentiellement longues** : prévoir recherche/scroll (fait : scroll modale gestion) ;
      pour la caisse, chips catégories défilables + regroupement si beaucoup de catégories
- [ ] **Caisse (POS) avec images produits** : afficher la photo de l'article sur les tuiles pour
      améliorer l'expérience client (le champ `article.image` existe déjà, migration 0004)
- [ ] **Jeux de catégories réels par type de commerce** (boutique alimentaire, quincaillerie,
      pharmacie, restaurant, cosmétique…) : modèles pré-remplis proposés à
      l'initialisation pour que le commerçant démarre vite. Prévoir une table/asset
      de « modèles de catégories » sélectionnable à la première configuration.
- [x] Dépôts §5.3 — créer/lister + dépôt par défaut unique + `defaut()` auto-crée « Principal »
- [x] Documents §5.4 — création en-tête + lignes, **totaux dérivés** (remise/TVA/TTC), numérotation `PREFIXE-EXERCICE-NNNN` (migration 0003, pilotée par la donnée)
- [x] Documents — lire (avec lignes), lister (filtres sens/type/statut)
- [x] **Validation document → mouvements de stock** : règle des 3 conditions §6.1 (4 tests couvrant facture/devis/article sans stock), avoir = inverse
- [x] Mouvements de stock : module journal, stock = Σ(entrées)−Σ(sorties) §3.3/§5.5
- [x] Inventaire : `ajuster_inventaire` (comptage → mouvement d'écart) §6.3 (core ; UI à venir)
- [x] **Transformation de pièce** (devis/proforma/commande → facture…) via `config_transformation` §6.2 — source figée en `transforme`, lignes copiées, lien `document_source_id` (test dédié)
- [x] Documents **frontend** : écran `documents.html` (liste filtrée sens/type, recherche, éditeur de lignes live avec totaux, validation, transformation) + `facture.html` pilotée par `?id=` (impression)
- [~] **Caisse (POS)** §5.6 — écran 3 conteneurs (catégories / articles avec images / panier),
      recherche nom+code-barres, encaissement = facture vente validée (impacte stock)
- [x] Caisse : **tickets multiples en onglets** (un par client, restent ouverts tant que non encaissés/supprimés, +/suppr)
- [x] Caisse : **modale de règlement** — montant reçu + boutons coupures (500→10000) + **rendu de monnaie** bien visible (aide au rendu, non stocké), inspirée des POS éprouvés
- [x] Articles : **code-barres** (migration 0005) — champ + recherche/scan en caisse
- [ ] Caisse : **persister le paiement** (module `caisse`+`paiement`) et MAJ solde caisse + solde tiers §6.4
- [x] Caisse : **impression du ticket** — impression **isolée par iframe** (ne ferme plus l'appli),
      **à la demande** (bouton) ou **automatique** selon le réglage `impression_auto` (Paramètres → Impression)
- [x] Paramètres **en onglets** (Info société / Taxes / Impression) — tab panel
- [x] Paramètres globaux clé/valeur : API `/api/config` (GET), `/api/config/:cle` (PUT) ; réglage impression auto
- [ ] Caisse : sélection du client (autre que comptoir)
- [x] **Taux de TVA paramétrables** (migration 0006 + module + API) : gérés dans Paramètres, un seul « par défaut », proposés en liste à la création d'article
- [x] **Paramètres entreprise câblés** sur l'API (§5.9) : identité + logo (upload base64) chargés/enregistrés + gestion des taux de TVA
- [x] **Taxes multiples** (migration 0007) : catalogue `taxe` (nom, taux, type %/fixe), `article_taxe`
      (article→plusieurs taxes), `document_ligne_taxe` (snapshot figé). Totaux = somme de toutes les
      taxes par ligne, repli sur `taux_tva` si aucune. Transformation propage les taxes. Testé (14/14 + e2e 180+20=200).
- [x] Taxes UI : Paramètres gère le catalogue (nom/taux/type/défaut) ; modale Article = **multi-sélection** de taxes ;
      documents.html + caisse construisent les taxes de ligne depuis l'article.
- [x] Taxes **actif/inactif** (interrupteur dans Paramètres) : seules les taxes **actives** sont proposées et
      **comptabilisées** (caisse & documents utilisent `/api/taxes` = actives seulement). `/api/taxes?tous=true` pour le paramétrage.
- [x] **Caisse : ventilation par taxe** — une ligne par taxe dans le ticket écran + imprimé ; repli sur les
      taxes actives quand l'article n'a pas de taxes propres (même repli dans l'éditeur de documents)
- [ ] Facture imprimable (`facture.html`) : ventilation par taxe (données `ligne.taxes` déjà présentes)
- [ ] **Stress-test « supermarché »** : seed de nombreuses catégories + milliers de produits ;
      prévoir alors **pagination + recherche côté serveur** (aujourd'hui `/api/articles` renvoie tout,
      images base64 comprises → à alléger : endpoint léger sans image + recherche SQL + lazy images)
- [ ] Utilitaire de recalcul complet des soldes depuis les journaux §6.4
- [ ] Production §5.7 — ordre + composants, clôture = sorties composants + entrée produit
- [ ] Facturation cyclique §5.8 — génération à partir d'un modèle, avance échéance
- [ ] Numérotation des pièces (par type + exercice), pilotée par config

## API serveur
- [x] `/api/sante`
- [x] `/api/parametres` (GET/PUT), `/api/taux-tva` (GET/POST/DELETE), `/api/taxes` (GET/POST), `/api/taxes/:id` (PUT/DELETE)
- [x] `/api/tiers` (GET filtre+POST), `/api/tiers/:id` (GET/PUT/DELETE), `/api/tiers/lot/{role,desactiver}` (POST)
- [x] `/api/articles` (GET liste+filtre, POST), `/api/articles/:id` (GET)
- [x] `/api/categories` (GET, POST), `/api/categories/:id` (PUT, DELETE)
- [x] `/api/depots` (GET, POST)
- [x] `/api/documents` (GET liste+filtres, POST), `/api/documents/:id` (GET), `/api/documents/:id/valider` (POST), `/api/documents/:id/transformer` (POST)
- [ ] Endpoints mouvements/inventaire, caisse, paiements, production, rapports

## Rapports §7
- [ ] Journal des ventes / des achats
- [ ] Croisement ventes/achats par article (marge, stock restant)
- [ ] État du stock par dépôt + alertes de rupture
- [ ] État de caisse par période
- [ ] Encours clients / fournisseurs
- [ ] Export CSV (PDF en option)

## Frontend
- [x] Maquettes reprises dans `frontend/` (accueil, articles, caisse, facture, paramètres)
- [x] Logo produit Djigui installé dans la marque de la sidebar (toutes pages) ; sidebar éclaircie
- [x] Helper API partagé `assets/app.js` (fetch, format nombres, échappement)
- [x] Écran **Articles** branché sur l'API : liste, filtres (chips), recherche, modale de création
- [ ] Brancher les autres écrans (accueil, caisse, facture, paramètres) sur l'API
- [ ] Écrans manquants : Ventes, Achats, Production, Tiers, Rapports
- [ ] Modales : nouvel article, transformation de pièce, inventaire, encaissement

## Tests
- [x] Migration idempotente + config présente
- [x] Tiers créer/lister
- [ ] Couvrir : totaux document, règle stock des 3 conditions, transformation, soldes, recalcul

## Hors périmètre v1 (rappel §8) — NE PAS implémenter
- Licence gratuit/payant (seules les coutures §3.4 existent), sync cloud,
  compta SYSCOHADA, paie, migration PostgreSQL (garder l'abstraction propre).

---

## Notes de session
- 2026-07-21 : socle posé (workspace, schéma, migrations, autorisation, serveur
  axum, frontend copié). `cargo test -p djigui-core` : 2/2 OK. Build complet OK.
  Prochaine tranche suggérée : module **Articles** (vertical complet : core →
  API → écran branché), car il conditionne documents et stock.

- 2026-07-21 (fin de journée) — **grosse session, on reprend demain 2026-07-22**.
  État : **cœur 14/14 tests OK**, appli desktop Tauri fonctionnelle.
  Migrations appliquées : **0001→0007**.
  Modules faits (core+API+UI) : paramètres entreprise, **taxes multiples** (catalogue
  actif/inactif + défaut), tiers (CRUD+lot), articles (CRUD+image+code-barres+catégories+
  taxes), catégories (CRUD), dépôts, documents (création+totaux ventilés+validation stock
  §6.1+transformation §6.2), stock (journal+inventaire core), caisse POS (3 conteneurs,
  tickets en onglets, reçu/rendu, ventilation par taxe, impression iframe à la demande/auto),
  facture imprimable par `?id=`, paramètres en onglets (Société/Taxes/Impression).

  ### LANCER L'APPLI (important — depuis la racine, sinon frontend introuvable)
  ```bash
  cd /d/DJGUI_ERP && CARGO_HTTP_CHECK_REVOKE=false cargo run -p djigui-desktop
  # ou serveur seul : cargo run -p djigui-server  (http://localhost:1704)
  ```

  ### PROCHAINES ÉTAPES (par priorité)
  1. **Ventilation des taxes sur la facture imprimable** (`facture.html`) — données `ligne.taxes` déjà présentes, purement affichage.
  2. **Seed « supermarché » + perfs** : générer beaucoup de catégories + milliers de produits,
     puis passer `/api/articles` en **pagination + recherche côté serveur** + images à la demande
     (aujourd'hui renvoie tout, images base64 comprises → à alléger).
  3. **Persistance des paiements** (§5.6/§6.4) : modules `caisse`+`paiement`, l'encaissement écrit
     la ligne de paiement + met à jour solde caisse & solde tiers ; utilitaire de recalcul des soldes.
  4. Généraliser **CRUD + traitement par lot** aux écrans manquants ; brancher accueil (dashboard réel).
  5. **Rapports** §7 (ventes, achats, croisement, stock, caisse, encours) + export CSV.
  6. Écrans manquants : Achats (documents sens=achat déjà là), Production, Inventaire (UI), Rapports.
  7. Modèles de **catégories par type de commerce** à l'initialisation (alimentaire, pharmacie, resto…).
  8. Empaquetage Tauri (`tauri build`) : bundler le frontend + installeur Windows ; mode client (IP serveur).

  Réf. mémoires : `djigui-standards-modules`, `djigui-traitement-par-lot`, `djigui-taxes-multiples`.
