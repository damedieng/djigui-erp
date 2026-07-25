# Djigui Desktop — Spécification & Modèle de données

> **Projet** : Djigui Desktop — ERP de gestion commerciale pour TPE/PME
> **Société** : SODEVITEL — Conseil & Ingénierie Informatique, Dakar
> **Positionnement** : proche de Sage, mais volontairement plus simple
> **Nature** : application desktop, architecture client-serveur en réseau local
> **Objectif de ce document** : fournir à Claude Code la spec fonctionnelle et le
> modèle de données. Ce document décrit **le quoi** (structure et règles métier),
> pas **le comment** (le code d'implémentation reste à écrire par Claude Code).

---

## 1. Vision

Djigui Desktop est un logiciel de gestion commerciale complet et simple :
fournisseurs/clients, articles et services, devis, factures, avoirs, caisse,
stock et inventaire, production, et rapports de croisement ventes/achats.

Le pari produit : **battre Sage sur la simplicité**. Là où Sage multiplie les
mécaniques spécifiques par type de pièce, Djigui unifie autour de quelques
concepts robustes. Moins de tables, moins de code, moins de friction utilisateur.

---

## 2. Architecture

### 2.1 Modèle client-serveur

Une seule application, deux rôles selon la configuration au lancement :

- **Mode serveur** : le poste principal fait tourner l'application en serveur.
  Il détient la base de données et expose une API locale (HTTP/JSON sur le
  réseau local). Ce même poste sert aussi de **client à lui-même** (l'interface
  se connecte à `localhost`).
- **Mode client** : les autres postes du réseau lancent la même application en
  mode client. Ils ne détiennent aucune donnée : ils dialoguent avec le serveur
  via l'API locale (découverte du serveur par IP/nom d'hôte, comme le projet
  DigiDoctor).

Conséquence importante pour le choix de base de données (§2.3) : **seul le
processus serveur écrit dans la base**. Les clients passent tous par l'API du
serveur, qui sérialise les écritures. Il n'y a donc jamais d'accès concurrent
direct au fichier de base.

### 2.2 Stack

- **Shell desktop** : Tauri (Rust).
- **Backend / cœur métier** : Rust (dans le processus serveur).
- **Frontend / UI** : HTML/JS (WebView Tauri).
- **Communication client ↔ serveur** : API HTTP/JSON sur le réseau local.

### 2.3 Base de données — décision recommandée

**Recommandation : SQLite (mode WAL) sur le poste serveur.**

Justification : puisque l'architecture fait transiter toutes les écritures par
le processus serveur (les clients n'accèdent jamais au fichier directement), la
limite « un seul écrivain » de SQLite n'est pas un problème — le serveur
sérialise naturellement. On garde les avantages : fichier unique, sauvegarde
triviale (copie de fichier), zéro administration, parfait pour le mode gratuit
mono-poste.

**Voie d'évolution** : si le volume ou le nombre de postes grandit fortement,
migrer le serveur vers PostgreSQL sans changer le modèle de données (les types
ci-dessous sont compatibles). À prévoir dans l'abstraction d'accès aux données,
mais **ne pas implémenter maintenant**.

---

## 3. Principes de conception (à respecter absolument)

Trois paris structurent tout le modèle. Toute évolution doit les préserver.

### 3.1 Un seul `tiers`
Pas de table `client` et de table `fournisseur` séparées. Un partenaire a un
rôle (`client`, `fournisseur`, ou `les_deux`). Un même tiers peut être client et
fournisseur sans doublon.

### 3.2 Un seul `document`
Toutes les pièces commerciales (devis, facture, avoir, bon de commande, bon de
livraison, proforma) vivent dans **une seule table `document`**, distinguées par :
- `type_document` : la nature de la pièce ;
- `sens` : `achat` ou `vente`.

Un devis de vente et une facture de vente sont **deux enregistrements distincts**
de la même table. Ventes et achats partagent le même code, seul `sens` change.

> **Devis → facture** : un devis accepté n'est **pas modifié**. On **crée** une
> nouvelle pièce de type `facture`, on **copie** ses lignes, et on relie la
> facture au devis via `document_source_id`. Le devis reste figé, statut
> `transforme`. La séparation logique et la traçabilité sont préservées, sans
> dupliquer le schéma. (voir §5.4)

### 3.3 Le stock est un **journal**, jamais une valeur stockée
Le stock d'un article n'est pas un champ qu'on incrémente/décrémente. C'est la
**somme des mouvements** de l'article dans `mouvement_stock` (par dépôt). Comme
un relevé bancaire. Bénéfices : pas de désynchronisation possible, traçabilité
totale, inventaire trivial (§5.5).

### 3.4 Coutures pour le futur gratuit/payant (à prévoir, pas à activer)
La frontière gratuit/payant sera une **couche ajoutée plus tard**. Pour qu'elle
s'ajoute sans réécriture, deux coutures doivent exister dès maintenant :
- **Modules à frontière nette** : caisse, stock, facturation, production… chacun
  isolé et activable/désactivable indépendamment.
- **Point de contrôle unique d'autorisation** : une seule fonction/service
  répond à « cette capacité est-elle autorisée ? ». Interdit d'éparpiller des
  `if payant` dans le code métier. Aujourd'hui elle renvoie toujours `true`.

---

## 4. Périmètre fonctionnel (v1)

- Gestion des tiers (clients/fournisseurs) : fiche, solde, historique.
- Articles & services : fiche, prix vente/achat, gestion de stock optionnelle.
- Documents de vente et d'achat : devis, facture, avoir, bon de commande, bon
  de livraison, proforma.
- Transformation de pièces (devis → facture, bon de commande → facture, etc.).
- Caisse & paiements : encaissements/décaissements, soldes de caisse.
- Stock & dépôts : mouvements, valorisation, alertes de rupture.
- Inventaire : comptage physique et ajustement automatique.
- Production : ordres de production consommant des composants pour produire un
  article.
- Rapports : ventes, achats, croisement ventes/achats, marges, état du stock,
  état de caisse. (voir §7)
- Facturation cyclique : abonnements générant des factures récurrentes. (§5.6)

---

## 5. Modèle de données

Types génériques utilisés ci-dessous : `uuid`, `text`, `integer`, `decimal`
(montants/quantités), `date`, `datetime`, `boolean`, `enum`. En SQLite : `uuid`
et `enum` → `TEXT`, `decimal` → `NUMERIC`, `boolean` → `INTEGER 0/1`.

### 5.1 `tiers`
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| code | text | référence lisible, unique |
| type_role | enum | `client` \| `fournisseur` \| `les_deux` |
| nom | text | |
| telephone | text | |
| adresse | text | nullable |
| ninea | text | identifiant fiscal, nullable |
| solde | decimal | solde courant (dérivé, tenu à jour à l'écriture) |
| actif | boolean | défaut `true` |
| cree_le | datetime | |

### 5.2 `article`
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| code | text | unique |
| type | enum | `bien` \| `service` |
| designation | text | |
| prix_vente | decimal | HT |
| prix_achat | decimal | HT, nullable |
| taux_tva | decimal | ex. 18.00 ; 0 si exonéré |
| gere_stock | boolean | **clé de la règle vente→stock** ; toujours `false` si `type = service` |
| stock_alerte | decimal | seuil de rupture, nullable |
| actif | boolean | défaut `true` |

### 5.3 `depot`
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| nom | text | |
| par_defaut | boolean | un seul dépôt par défaut |

### 5.4 `document` et `document_ligne`

`document` — l'en-tête unifié de toute pièce commerciale.
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| numero | text | numérotation par type + exercice |
| type_document | enum | `devis` \| `facture` \| `avoir` \| `commande` \| `livraison` \| `proforma` |
| sens | enum | `vente` \| `achat` |
| tiers_id | uuid | FK → tiers |
| depot_id | uuid | FK → depot (dépôt impacté si mouvement de stock) |
| date | date | |
| statut | enum | `brouillon` \| `valide` \| `accepte` \| `transforme` \| `annule` |
| document_source_id | uuid | FK → document, nullable (pièce d'origine d'une transformation) |
| total_ht | decimal | dérivé des lignes |
| total_tva | decimal | dérivé |
| total_ttc | decimal | dérivé |
| note | text | nullable |
| cree_le | datetime | |

`document_ligne` — les lignes d'une pièce.
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| document_id | uuid | FK → document |
| article_id | uuid | FK → article |
| designation | text | copiée de l'article, éditable |
| quantite | decimal | |
| prix_unitaire | decimal | HT |
| taux_tva | decimal | copié de l'article, éditable |
| remise | decimal | %, défaut 0 |
| total_ligne_ht | decimal | dérivé |

`facture_detail` — extension 1‑1, **uniquement les champs propres à la facture**.
À créer seulement pour les documents de type `facture`.
| champ | type | notes |
|---|---|---|
| document_id | uuid | PK, FK → document |
| date_echeance | date | |
| conditions_paiement | text | nullable |
| mentions_legales | text | nullable |

> N.B. Si demain d'autres types ont beaucoup de champs propres (ex. `devis` avec
> `date_validite`), créer une extension dédiée sur le même modèle. Ne pas
> polluer `document` avec des champs spécifiques à un seul type.

### 5.5 `mouvement_stock`
Le journal de stock. **Un mouvement n'est jamais modifié ni supprimé** ; une
erreur se corrige par un mouvement inverse.
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| article_id | uuid | FK → article |
| depot_id | uuid | FK → depot |
| document_id | uuid | FK → document, **nullable** (mouvement sans pièce : inventaire, casse, transfert) |
| sens | enum | `entree` \| `sortie` |
| quantite | decimal | toujours positive ; c'est `sens` qui donne le signe |
| motif | enum | `vente` \| `achat` \| `inventaire` \| `casse` \| `transfert` \| `production` |
| date | datetime | |

Stock d'un article dans un dépôt = Σ(entrées) − Σ(sorties).

### 5.6 Caisse & paiements

`caisse`
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| nom | text | |
| solde | decimal | dérivé, tenu à jour à l'écriture |

`paiement`
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| tiers_id | uuid | FK → tiers |
| caisse_id | uuid | FK → caisse |
| document_id | uuid | FK → document, nullable (paiement rattaché à une pièce) |
| sens | enum | `encaissement` \| `decaissement` |
| montant | decimal | |
| mode | enum | `espece` \| `mobile_money` \| `virement` \| `cheque` |
| date | datetime | |

> Un paiement met à jour en une écriture le solde de la `caisse` **et** le solde
> du `tiers`.

### 5.7 Production

`ordre_production`
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| numero | text | |
| article_produit_id | uuid | FK → article (l'article fabriqué) |
| quantite | decimal | quantité à produire |
| depot_id | uuid | FK → depot |
| statut | enum | `brouillon` \| `en_cours` \| `termine` \| `annule` |
| date | date | |

`production_composant`
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| ordre_id | uuid | FK → ordre_production |
| article_id | uuid | FK → article (composant consommé) |
| quantite | decimal | par unité produite |

> À la clôture d'un ordre (`termine`) : mouvements de **sortie** de stock pour
> chaque composant (motif `production`) et mouvement d'**entrée** pour l'article
> produit.

### 5.8 Facturation cyclique

`abonnement`
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| tiers_id | uuid | FK → tiers |
| document_modele_id | uuid | FK → document (une pièce modèle servant de gabarit) |
| frequence | enum | `mensuel` \| `trimestriel` \| `annuel` |
| prochaine_echeance | date | |
| actif | boolean | |

> Un traitement (au lancement du serveur, ou à la demande) parcourt les
> abonnements actifs dont `prochaine_echeance <= aujourd'hui`, **génère** une
> facture à partir du modèle, puis avance `prochaine_echeance` selon `frequence`.

### 5.9 `parametres_entreprise`
Table **singleton** (une seule ligne) : l'identité de l'entreprise, réutilisée
partout — en-tête de l'application et surtout sur chaque PDF de document.
**À ne jamais oublier** : sans ces informations, aucune facture n'est valable.
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| raison_sociale | text | nom commercial affiché |
| ninea | text | identifiant fiscal (obligatoire sur facture) |
| rccm | text | registre du commerce, nullable |
| adresse | text | |
| telephone | text | |
| email | text | nullable |
| logo | text | chemin fichier ou base64 de l'image |
| devise | text | ex. `FCFA` |
| taux_tva_defaut | decimal | ex. 18.00 |
| pied_facture | text | mentions légales par défaut, nullable |

> Le logo et ces informations alimentent l'en-tête de tous les documents
> imprimés (facture, devis, proforma…) et l'en-tête de l'application.

---

## 6. Règles métier clés

### 6.1 Validation d'un document et impact sur le stock
À la validation d'un document (`statut` → `valide`), pour **chaque ligne** :

> Créer un mouvement de stock **si et seulement si** les trois conditions sont
> réunies :
> 1. `article.gere_stock = true`, **et**
> 2. le `type_document` est configuré comme impactant le stock (voir table
>    ci-dessous), **et**
> 3. le paramètre global « gestion de stock active » est vrai.
>
> Sinon : aucun mouvement.

Sens du mouvement selon le `sens` du document :
- document `vente` → mouvement `sortie` ;
- document `achat` → mouvement `entree`.

Comportement par type de document (à stocker en configuration, pas en dur) :
| type_document | impacte le stock ? |
|---|---|
| devis | non |
| proforma | non |
| commande | non |
| livraison | oui |
| facture | oui |
| avoir | oui (mouvement inverse) |

> Un `service` (donc `gere_stock = false`) traverse tout le circuit sans jamais
> générer de mouvement. C'est ainsi que « la vente agit sur le stock si le
> paramétrage l'exige » se résout **par la donnée, pas par du code métier**.

### 6.2 Transformation de pièce (ex. devis → facture)
1. Vérifier que la pièce source est dans un statut transformable
   (`accepte` pour un devis).
2. Créer un nouveau `document` du type cible (`facture`), même `tiers_id`, même
   `sens`, `document_source_id` = id de la source.
3. Copier les `document_ligne` de la source vers la cible.
4. Passer le statut de la source à `transforme`.
5. La source reste **inchangée** par ailleurs (jamais supprimée ni modifiée sur
   le fond).

### 6.3 Inventaire
1. L'utilisateur compte le stock physique d'un article dans un dépôt.
2. Le système calcule le stock théorique (Σ mouvements).
3. Pour l'écart, on écrit **un mouvement d'ajustement** (motif `inventaire`,
   sens selon le signe de l'écart). Aucune table spéciale : l'inventaire n'est
   qu'une source de mouvements.

### 6.4 Soldes
- Solde tiers et solde caisse sont **dérivés** mais tenus à jour à chaque
  écriture de paiement/document, pour éviter un recalcul coûteux à l'affichage.
- Un utilitaire de **recalcul complet** (depuis les journaux) doit exister pour
  réparer un solde en cas de doute. La vérité reste toujours dans les journaux.

---

## 7. Rapports (v1)

- Journal des ventes (par période, par tiers, par article).
- Journal des achats (mêmes axes).
- **Croisement ventes/achats** par article : quantités achetées vs vendues,
  marge (prix de vente − coût), stock restant.
- État du stock par dépôt, avec alertes de rupture (`stock <= stock_alerte`).
- État de caisse (entrées, sorties, solde) par période.
- Encours clients (créances) et encours fournisseurs (dettes) depuis les soldes.

Tous les rapports exportables en CSV (et PDF en option).

---

## 8. Hors périmètre v1 (à ne pas implémenter maintenant)

- La couche de licence gratuit/payant (seules les **coutures** du §3.4 sont
  requises maintenant ; la logique de gating vient plus tard).
- La synchronisation multi-sites / cloud.
- La comptabilité générale (plan comptable, écritures, bilan). Djigui reste de
  la gestion commerciale, pas de la compta au sens SYSCOHADA — à discuter en v2.
- La paie.
- Migration PostgreSQL (garder l'abstraction d'accès aux données propre pour la
  rendre possible, sans l'implémenter).

---

## 9. Consignes pour Claude Code

- Respecter strictement les trois paris du §3 : tiers unifié, document unifié,
  stock en journal. Ne jamais créer de tables `client`/`fournisseur` séparées,
  ni de tables `devis`/`facture` séparées.
- Le comportement des types de documents (impact stock, transformations
  autorisées, numérotation) doit être **piloté par configuration/données**, pas
  codé en dur dans des `if type == ...` dispersés.
- Isoler chaque module (caisse, stock, facturation, production) derrière une
  frontière nette, et faire transiter toute vérification de droit par le point
  de contrôle unique d'autorisation (§3.4), qui renvoie `true` pour l'instant.
- Toutes les écritures passent par le processus serveur ; les clients ne
  touchent jamais la base directement.
- Prévoir dès le départ l'utilitaire de recalcul des soldes depuis les journaux.
