# Dictionnaire de données — Djigui Desktop

État au **2026-07-28**, migration **0041**. **62 tables**.

Ce document décrit *ce que chaque table signifie*. Le schéma exact (types, index,
contraintes CHECK) fait foi dans `crates/core/migrations/`. Quand les deux
divergent, c'est ce document qui a tort — mettez-le à jour.

## Conventions valables partout

| Convention | Détail |
|---|---|
| Clés primaires | `TEXT`, un **UUID v4** généré côté Rust (`Uuid::new_v4()`). Jamais d'auto-incrément. |
| Dates | `TEXT`. Jour seul = `AAAA-MM-JJ`, horodatage = ISO-8601 UTC via `djigui_core::now()`. Format choisi pour que la **comparaison lexicale** (`date <= ?`) soit exacte. |
| Booléens | `INTEGER` 0/1 (SQLite n'a pas de type booléen). |
| Montants | `NUMERIC`. Devise unique, paramétrée dans `parametres_entreprise.devise` (défaut FCFA). |
| Énumérations | `TEXT` + contrainte `CHECK`, doublées par un enum Rust dans `crates/core/src/domain.rs`. **Les deux doivent rester alignés.** |
| `cree_le` / `cree_par` | Horodatage de création et id de l'utilisateur auteur. `cree_par` n'a **pas** de clé étrangère : un utilisateur supprimé ne doit pas effacer l'historique. |
| Soldes | `caisse.solde` et `tiers.solde` sont **dérivés mais stockés** (tenus à jour à l'écriture, §6.4). Le journal reste la source de vérité : `POST /api/recalcul-soldes` les reconstruit. |
| Suppression | La règle maison est le **détachement**, pas la cascade : supprimer un jalon met `livrable.jalon_id = NULL`, il ne détruit pas les livrables. |

---

## 1. Référentiel commercial

### `parametres_entreprise` — l'entreprise elle-même (une seule ligne)
Singleton garanti par `singleton INTEGER UNIQUE CHECK (singleton = 1)`.
Porte l'identité légale imprimée sur les factures : `raison_sociale`, `ninea`,
`rccm`, `forme_juridique`, `capital`, `regime_fiscal`, `adresse`, `telephone`,
`email`, `logo`. Réglages : `devise`, `taux_tva_defaut`, `assujetti_tva`
(si 0, le seeder force les TVA à 0), `pied_facture`.

### `tiers` — clients et fournisseurs
`type_role` ∈ `client` | `fournisseur` | `les_deux`.
`nature` ∈ `particulier` | `entreprise` — **commande les mentions attendues** :
NINEA/RCCM pour une entreprise, `prenom`/`cni` pour un particulier. Aucune n'est
obligatoire : l'absence produit une alerte informative (`tiers::alertes_identite`),
jamais un blocage. `solde` > 0 = le tiers doit de l'argent. `exonere_tva` retire
la TVA de ses documents. ⚠️ **La CNI n'est jamais imprimée sur une facture.**

### `article` — biens et services
`nature_comptable` (migration 0032) ∈ `marchandise` | `matiere_premiere` |
`produit_fini` | `service`. **Un seul champ, deux usages** : il range les écrans
*et* décidera des comptes.

| Nature | Sens | Comptes OHADA | Où l'article apparaît |
|---|---|---|---|
| `marchandise` | achetée pour être revendue **en l'état** | 601 / 701 / 31 | caisse **et** recettes |
| `matiere_premiere` | achetée pour être **transformée** | 602 / — / 32 | recettes seulement |
| `produit_fini` | **fabriquée** par l'entreprise | — / 702 / 36 (+ 73 production stockée) | caisse **et** recettes |
| `service` | ni stock ni transformation | — / 706 / — | caisse seulement |

Pourquoi c'est structurant : un négociant se lit en **marge commerciale**, un
fabricant en **coût de production**. De la farine comptabilisée en 601 fausse la
marge commerciale et fait disparaître la production stockée.

⚠️ **Le commerçant ne renseigne jamais ce champ** (décision du 2026-07-26 :
« il ne faut pas compliquer la tâche à un commerçant »). Djigui le déduit :
mettre un article dans une recette ou clôturer un ordre suffit. Deux garde-fous
essentiels, appliqués **à l'identique** dans la migration et à l'exécution
(`production::classer_matiere_premiere`) :

- on ne déclasse jamais un article qui a un **prix de vente** ou qui a **déjà été
  vendu** — sinon il disparaîtrait de la caisse ;
- `modifier` un article **sans** envoyer le champ ne l'écrase pas
  (`COALESCE(?, nature_comptable)`) : changer un prix ne doit pas défaire le
  classement.

Le reclassement manuel appartiendra à l'**écran comptable**, réservé au comptable.

`type` ∈ `bien` | `service`, avec le garde-fou `CHECK (type <> 'service' OR gere_stock = 0)` :
un service ne gère jamais de stock. `prix_achat` sert au calcul de marge et au
prix de revient de production. `stock_alerte` déclenche la notification « stock bas ».
Colonnes du seeder : `code_seed`, `unite`, `image_chemin`, `image_origine`,
`prix_a_completer` (article importé dont le prix reste à saisir).

### `categorie`, `unite`, `taxe`, `taux_tva`, `article_taxe`
`categorie` : `icone` + `couleur` alimentent le repli pictogramme de la caisse
quand l'article n'a pas d'image. `taxe` : catalogue de taxes (`type` ∈ `pourcentage` | `fixe`,
`actif`, `par_defaut`) — **une vente peut porter plusieurs taxes**, pas seulement la TVA.
`article_taxe` fait le lien N-N. `taux_tva` est l'ancien catalogue de taux simples.

---

## 2. Pièces commerciales

### `document` — la pièce unifiée (pari §3.2)
**Une seule table** pour devis, facture, avoir, commande, livraison, proforma.
Ce qui change d'un type à l'autre vit dans les tables `config_*`, pas dans des `if`.

- `type_document` ∈ `devis` | `facture` | `avoir` | `commande` | `livraison` | `proforma`
- `sens` ∈ `vente` | `achat`
- `statut` ∈ `brouillon` | `valide` | `accepte` | `transforme` | `annule`
- `document_source_id` : la pièce dont celle-ci est issue (devis → facture).
- `numero` : `PREFIXE-EXERCICE-NNNN`, attribué **à la validation** depuis `sequence_numero`.
- Annulation : `motif_annulation`, `annule_par`, `annule_le`.

**Inaltérabilité** : seul un `brouillon` se supprime ou se modifie. Une pièce validée
se corrige par **annulation** (contre-passation), jamais par réécriture.

### `document_ligne`, `document_ligne_taxe`, `facture_detail`
Une ligne par article vendu. `document_ligne_taxe` **fige** le détail de chaque taxe
appliquée à la ligne (nom, type, taux, montant) : si le taux change demain, la
facture d'hier ne bouge pas. `facture_detail` porte les mentions propres à la
facture (échéance, conditions, mentions légales).

### `config_type_document`, `config_transformation`, `config_prefixe_document`, `sequence_numero`
Le **comportement piloté par la donnée** : quel type impacte le stock
(`impacte_stock`, `mouvement_inverse`), quelle transformation est permise depuis
quel statut, quel préfixe et quel compteur par exercice.

---

## 3. Stock

### `mouvement_stock` — le journal, jamais modifié
`sens` ∈ `entree` | `sortie` ; `motif` ∈ `vente` | `achat` | `inventaire` | `casse` | `transfert` | `production`.
**Le stock n'est jamais stocké** : il vaut toujours Σ(entrées) − Σ(sorties) pour un
couple (article, dépôt), calculé à la demande. Une erreur se corrige par un
mouvement inverse, jamais par un `UPDATE`.

### `depot` — les magasins
`par_defaut` désigne celui utilisé quand rien n'est précisé. `caisse.depot_id`
rattache une caisse à un magasin : **la vente déduit le stock du magasin de sa caisse**.

### `inventaire`, `inventaire_ligne`
Comptage **daté et verrouillé** (`statut = valide`) : c'est une preuve. Chaque ligne
garde `stock_theorique`, `stock_compte` et l'`ecart` ajusté.

---

## 4. Production *(migration 0031 — nouveau)*

### `nomenclature` — la recette
Modèle réutilisable rattaché à l'article fabriqué (`article_id`).
`quantite_produite` = ce que rend **un lot** de la recette (« 20 baguettes »).
Les composants sont donc exprimés **pour le lot**, pas à l'unité — c'est ainsi
qu'une recette s'écrit dans la vraie vie.

### `nomenclature_composant`
`quantite` pour le lot entier, `perte_pct` = perte technique attendue
(épluchures, chutes, sciure). `UNIQUE (nomenclature_id, article_id)`.

### `ordre_production` — l'ordre de fabrication
`numero` = `OF-EXERCICE-NNNN`. `statut` ∈ `brouillon` | `en_cours` | `termine` | `annule`.

- `quantite` = **prévu**, `quantite_produite` = **réel** (saisi à la clôture, NULL avant).
- `frais` : main-d'œuvre, énergie… incorporés au prix de revient.
- `cout_total` / `cout_unitaire` : renseignés **à la clôture**, jamais avant.
- `nomenclature_id` est conservé pour la traçabilité mais **l'ordre est autonome** :
  supprimer la recette le met à `NULL` sans toucher aux composants de l'ordre.

**Le stock ne bouge qu'au passage à `termine`** (via la clôture) : une sortie par
composant + une entrée pour le produit fini, motif `production`. Un ordre terminé
ne se modifie ni ne se supprime : il a bougé le stock.

### `production_composant`
`quantite_prevue` vs `quantite_reelle` (NULL = « comme prévu »).
`cout_unitaire` est **figé à la clôture** : le coût d'une fabrication passée ne
doit pas se revaloriser quand le prix d'achat du composant changera.

---

## 5. Caisse et règlements

### `caisse`, `session_caisse`
`session_caisse` : `fond_ouverture`, `montant_compte`, `ecart` (compté − théorique),
`statut` ∈ `ouverte` | `fermee`. Une caisse avec session ouverte ne se modifie pas.

### `paiement`
`sens` ∈ `encaissement` | `decaissement` ; `mode` ∈ `espece` | `mobile_money` | `virement` | `cheque`.
Un paiement met à jour **en une écriture** le solde de la caisse **et** celui du tiers.
`annulation_de` pointe le paiement contre-passé lors d'une annulation de vente.

### `moyen_paiement`
Moyens concrets configurables (Orange Money, Wave…) : `nom`, `image`, `couleur`,
`ordre`, `actif`. `famille` reprend les valeurs de `paiement.mode` — **c'est la
famille qui pilote le comportement**, le nom n'est que l'étiquette affichée.
`rendu_monnaie` décide si l'écran de caisse propose le bloc reçu/rendu.

### `abonnement`, `abonnement_ligne`
Facturation cyclique : `frequence` ∈ `mensuel` | `trimestriel` | `annuel`,
`prochaine_echeance`, `nb_echeances` / `echeances_faites`.

---

## 6. Agenda

### `rendez_vous`
`debut` / `fin` en `AAAA-MM-JJ HH:MM`, `journee_entiere`,
`statut` ∈ `planifie` | `confirme` | `honore` | `annule` | `reporte`.
Rattachements : `tiers_id`, `responsable_id`, `lieu`, `note`.
⚠️ **Aucun lien avec les jalons de projet** — barrière décidée avec l'utilisateur.

---

## 7. Gestion de projet

### `projet`
`statut` ∈ `planifie` | `en_cours` | `suspendu` | `cloture`. `budget_global` est le
budget **saisi** ; le budget *planifié* est calculé (voir ci-dessous) et l'écart
entre les deux est affiché, jamais imposé.

### Retard d'une activité *(champ calculé, non stocké)*
`Tache.retard_jours` et `Tache.nb_en_retard` sont posés par `enrichir()`.
Définition : **activité feuille**, `statut <> terminee`, `date_fin_prevue`
dépassée. Une **activité parente** porte le **plus grand retard de sa
descendance** — sans quoi replier une branche escamoterait le retard du
planning.

⚠️ Cette définition est **strictement alignée** sur
`notification::activites_en_retard` : la cloche et le planning ne doivent jamais
se contredire. **Signalement uniquement** — aucune date n'est recalculée
(barrière « cascade »).

### `tache` — les activités
Hiérarchie **jusqu'à 4 niveaux** via `tache_parente_id` (anti-cycle vérifié).
`statut` ∈ `a_faire` | `en_cours` | `bloquee` | `terminee`, `avancement` 0..100.

**Calcul bas → haut** : pour une tâche parente, budget, dates et avancement sont
**agrégés depuis les enfants** et affichés grisés (non saisissables). Au niveau du
projet : `budget_planifie` = budget des tâches + coût main-d'œuvre + coût ressources.

### `dependance` — les prédécesseurs (flèches du Gantt)
`type` ∈ `fin_debut` | `debut_debut` | `fin_fin` | `debut_fin` (**seul `fin_debut` est
exploité en v1**), `decalage` en jours (peut être négatif). `UNIQUE (tache_id, predecesseur_id)`.

⚠️ **Barrière** : la propagation en cascade existe mais n'est **jamais automatique**.
On signale l'incohérence, l'utilisateur clique « Harmoniser les dates » et voit un
**aperçu avant application**.

### `intervenant`, `assignation`
`intervenant.type` ∈ `interne` | `externe` ; `type_taux` ∈ `horaire` | `journalier` | `forfait`.
`assignation.heures_allouees` est une **quantité générique** : des jours si le taux
est journalier, des heures s'il est horaire, ignorée si forfait.
Coût = forfait ? `taux` : `quantité × taux`.

### `ressource`
Moyens non humains d'un projet : `type` ∈ `materiel` | `budget` | `sous_traitance`,
coût = `cout_unitaire × quantite`.

### `jalon`, `livrable`, `document_joint`
`jalon.statut` ∈ `a_venir` | `atteint` | `manque` ; `livrable.statut` ∈ `a_produire` |
`en_cours` | `livre` | `accepte` | `refuse`. Le retard est **signalé uniquement**,
aucune date n'est recalculée.

`document_joint` : les fichiers sont **sur le disque**, jamais en base
(`chemin` pointe dans `documents/<projet_id>/`). Le nom de fichier est **régénéré
en UUID** — on ne fait jamais confiance au nom fourni par le client (`..`, `/`
permettraient d'écrire hors du dossier). Limite 20 Mo.

### `tache_action`
Journal d'avancement : une ligne par point de situation (`avancement`, `observation`, `date`).

---

## 8. Utilisateurs, traçabilité, réglages

### `utilisateur`
`role` ∈ `admin` | `caissier`. `mot_de_passe_hash` : **jamais le mot de passe en clair**
(un test le vérifie). Utilisateur par défaut `djigui` / `djigui`. Le dernier admin
ne peut pas être désactivé.

### `journal_audit`
Qui a fait quoi, quand : `utilisateur_nom` est **recopié** (pas joint), pour que
l'historique survive à la suppression du compte.

### `notification_lue`
Les notifications sont **calculées à la volée** (retards, stock bas, rendez-vous du
jour…) ; seule la *lecture* est persistée, par `cle` métier stable.

### `parametre_global`, `seed_applique`, `schema_migrations`
Réglages clé/valeur ; catalogues métier déjà appliqués (idempotence du seeder) ;
migrations déjà jouées.

---

## 9. Comptabilité *(migration 0034 — nouveau)*

Ces tables servent **l'écran du comptable**, et lui seul. Le commerçant n'en voit
jamais la couleur, et **aucune vente ne dépend d'elles** : la comptabilité
n'empêche jamais de vendre (`claude_spec/plan_comptable.md` §0).

Procédé validé avec l'utilisateur le 2026-07-27, qui inverse l'approche
classique : Djigui ne devine rien, le comptable crée ses comptes, écrit ses
règles, et celles-ci s'appliquent à **tout l'historique déjà en base**.

### `compte`
`numero` (clé, texte — un compte peut valoir `4011` ou `411CLI001`, et `06` ne
doit pas devenir `6`), `libelle`, `classe` (1 à 8, déduite du premier chiffre),
`sens_normal` (`debit`|`credit`, **indicatif** : signale un solde anormal dans la
balance, ne refuse jamais une écriture), `lettrable`, `actif`, `note`.

Aucun plan n'est imposé. Un seul compte est seedé, le **471 compte d'attente**,
et pour une raison technique : il faut toujours pouvoir écrire quelque part
plutôt que de perdre une opération. Un plan OHADA de base d'une trentaine de
comptes est **proposé** en un clic (`installer_plan_ohada`).

### `journal_comptable`
`code` (clé), `libelle`, `ordre`, `actif`. Seedé : VT ventes, AC achats,
CA caisse, BQ banque, ST stocks, OD opérations diverses.

### `regle_comptable`
Le cœur du procédé. Une règle dit : **« pour ce rôle, quand ces critères sont
réunis, prends ce compte »**.

- `role` — la place du compte dans l'écriture : `produit` (ce que la vente
  rapporte), `charge` (ce que l'achat coûte), `tiers` (client ou fournisseur),
  `taxe`, `tresorerie`, `stock`. Le moteur connaît le schéma de chaque opération
  et possède déjà tous les montants ; la règle ne fait que **nommer les comptes**.
- `compte_numero` → `compte`.
- **Critères, tous facultatifs** (`NULL` = « peu importe »), combinables :
  `domaine`, `categorie_id`, `article_id`, `nature_comptable`, `tiers_id`,
  `nature_tiers`, `caisse_id`, `moyen_paiement_id`, `famille_paiement`,
  `depot_id`, `taux_taxe`, `montant_min`, `montant_max`, `libelle_contient`.
  Ce sont **exactement** les critères de la recherche multicritère de l'écran :
  une recherche se transforme en règle d'un seul geste.
- `journal_code` (forçage), `ordre`, `actif`.

La règle **la plus spécifique gagne** (le plus grand nombre de critères
renseignés), `ordre` départageant les ex æquo. Le comptable écrit donc un défaut
large puis des exceptions étroites **sans avoir à réfléchir à leur ordre**.

### `ecriture`
`journal_code`, `date`, `libelle`, `exercice` (année, dénormalisée pour filtrer
vite), `origine_type` (`document`|`paiement`|`mouvement`|`manuel`|
`contrepassation`), `origine_id`, `complete` (faux tant qu'une ligne pointe sur
le 471), `contrepasse_de`, `cree_par`, `cree_le`.

⚠️ **Index unique sur `(origine_type, origine_id)`** hors contre-passation : une
pièce ne produit qu'une écriture. C'est le garde-fou contre le double comptage
si le comptable relance le rattachement.

### `ecriture_ligne`
`ecriture_id`, `compte_numero`, `libelle`, `debit`, `credit` (contrainte de
table : l'un des deux est nul), `tiers_id` (balance auxiliaire — contrôle croisé
avec `tiers.solde` que le module paiement tient de son côté), `lettrage`
(code A, B… par compte ; `NULL` = non lettré), `role`, `ordre`.

**Invariant absolu : Σ débit = Σ crédit par écriture.** C'est le seul endroit de
Djigui où l'on refuse d'écrire une donnée incohérente, et c'est délibéré : une
écriture déséquilibrée fausserait la balance sans que personne ne s'en aperçoive.

**On ne modifie ni ne supprime jamais une écriture complète** : on la
contre-passe (débit et crédit échangés). Seule exception, documentée : une
écriture **incomplète** (en 471) peut être *rejouée* après correction d'une
règle — ce n'est pas un enregistrement comptable, c'est un brouillon inachevé.

---

## 10. Passation et suivi des marchés *(migration 0037 — nouveau)*

Transposé du module « Passation de Marché » de l'application OLAC, étendu aux
soumissionnaires, avenants et réceptions. Spec : `claude_spec/SPEC_MODULE_MARCHES.md`.
**Aucun lien avec la comptabilité** — décision arrêtée avec l'utilisateur.

L'idée structurante : **le type de marché porte sa procédure**. C'est le même
rapport modèle → instance qu'entre `nomenclature` et `ordre_production` : le
modèle amorce, l'instance vit ensuite sa vie.

### `marche_type`, `marche_etape_modele` — les familles et leur procédure
Quatre familles seedées (Travaux, Fournitures, Services, Prestations
intellectuelles) avec leurs étapes et `duree_prevue_jours`. Corriger une
procédure ne vaut que **pour les prochains marchés**.

### `marche` — le marché
`numero` = `MA-EXERCICE-NNNN`. `statut` ∈ `en_cours` | `realise` | `annule` | `suspendu`.

- `montant_estime` **ne bouge jamais** ; `montant_attribue` est posé à l'attribution.
- `attributaire_id` → `tiers`, `projet_id` → `projet` (**simple lien, aucune
  propagation de dates** : barrière « cascade » de la spec Gestion de projet).
- Le passage à `annule` exige un motif ; `annule_par`/`annule_le` sont tracés.

### `marche_etape` — les étapes réellement suivies
⚠️ `libelle` est **recopié** du modèle, pas joint : corriger une procédure ne
doit jamais réécrire l'histoire des marchés déjà lancés. `etape_modele_id` n'est
là que pour la traçabilité. Dates calculées **par cumul des durées** à la
création, puis librement modifiables. `valide_par`/`valide_le` sont horodatés au
passage à `termine` — c'est ce qui donne au dossier sa valeur de preuve.
Une étape validée reste modifiable pendant `marche_delai_modification_suivi_jours`
(30 jours par défaut, réglable dans `parametre_global`).

### Enchaînement des étapes *(migration 0038)*
La 0037 traitait les étapes comme une **liste plate** : on pouvait annuler
l'ouverture des plis et valider l'attribution. Défaut relevé par l'utilisateur le
2026-07-28 — et il avait raison : une procédure de passation est une **chaîne
d'actes**, chacun fondant le suivant.

**Trois règles**, portées par `changer_statut_etape_avec` :
1. Une étape est **verrouillée** tant qu'une étape **obligatoire antérieure**
   n'est pas terminée (`Etape.verrouillee` + `raison_verrou`, calculés). Les
   étapes **facultatives** et **annulées** ne bloquent pas : les sauter est prévu.
2. **Dérogation motivée** : `derogation` + `motif_derogation` + `derogation_par`
   + `derogation_le`. Sans cette porte, impossible de saisir un dossier déjà
   commencé sur papier — cas réel constaté chez l'utilisateur.
3. **Rouvrir une étape franchie rouvre tout ce qui en découle** (cascade, choix
   utilisateur). Les validations effacées sont **consignées dans les
   observations** de chaque étape touchée : rien ne disparaît en silence.

Une seule étape porte `est_courante` : la première non franchie.

⚠️ `modifier_etape` **route tout changement de statut** vers cette même règle :
sinon le formulaire d'édition serait une porte dérobée contournant le verrou.

**Contrôle chronologique** (même esprit, ajouté après constat sur les données
réelles) : la date de réalisation d'une étape ne peut être **ni antérieure** à
celle de l'étape franchie qui la précède, **ni postérieure** à celle d'une étape
déjà franchie qui la suit. Une « publication de l'avis » datée du 04/11/2025
alors que la préparation du dossier l'a été le 28/07/2026 n'est pas un retard,
c'est une impossibilité. Même porte de sortie : la dérogation motivée.

`Marche.nb_dates_incoherentes` compte les étapes qui violent cette règle dans
les dossiers **saisis avant l'existence du contrôle** : elles alimentent une
alerte, visible dès la liste.

Ce qui **ne change pas** : le module ne bloque pas sur les **retards**. Une étape
en retard n'empêche rien. Le verrou porte sur l'**ordre** et la **cohérence
chronologique** des actes, qui sont d'une autre nature qu'un délai dépassé.

### `marche_incident` — infructueux et recours *(migration 0038)*
Deux évènements qui **interrompent** la chaîne, dans une seule table parce qu'ils
se datent, se motivent et se closent de la même façon :
- **`infructueux`** : aucune offre conforme. `relancer_apres_infructueux` remet en
  attente tout ce qui suit la **publication**, **écarte** les offres (jamais ne
  les efface : elles prouvent que la consultation a eu lieu), annule l'attribution
  et incrémente `marche.tentative`.
- **`recours`** : un candidat conteste. Tant que `statut = 'ouvert'`, aucune étape
  ne peut être franchie — c'est un arrêt **subi**, distinct d'un retard, et les
  alertes le disent.

### `marche_soumissionnaire` — le dépouillement
`tiers_id` est **facultatif** : recevoir une offre ne doit pas obliger à créer
une fiche contact. `statut` ∈ `recu` | `conforme` | `non_conforme` | `retenu` | `ecarte`.
L'attribution est **un seul geste** : l'offre passe `retenu`, les autres
`ecarte`, et le marché reçoit son attributaire et son montant.

### `marche_avenant` — les modifications du contrat
`UNIQUE (marche_id, numero)` : la numérotation est **par marché** (avenant n° 1,
n° 2…), comme on les désigne dans les actes.
`statut` ∈ `projet` | `approuve` | `rejete`.

- **Seuls les avenants approuvés comptent.** Un avenant en projet est une
  intention, pas un engagement : il n'entre ni dans le montant ni dans le délai.
- `montant_variation` peut être **négatif** (diminution), `delai_jours` aussi.
- Le montant du marché d'origine reste **intact** ; le *montant courant* se
  déduit (`montant_attribue` sinon `montant_estime`, plus les variations approuvées).
- Un avenant approuvé est **figé** : ni modification, ni suppression, ni retour
  en arrière. Pour revenir dessus on prend un avenant en sens inverse — même
  logique qu'un avoir sur une facture.
- Le report de clôture (`date_cloture_revisee`) est **calculé et affiché**,
  jamais écrit : aucune date n'est recalculée sans un geste explicite.
- Au-delà de **30 %** du montant initial, une alerte s'affiche (repère usuel des
  marchés publics UEMOA). C'est un signalement, pas une interdiction.

### `marche_reception` — les procès-verbaux
`type_reception` ∈ `provisoire` | `definitive` | `partielle`,
`resultat` ∈ `prononcee` | `avec_reserves` | `refusee`.

- **Seule exigence dure du module** : une réception `avec_reserves` doit dire
  lesquelles. Une réserve qu'on ne peut pas relire laisse le marché dans un état
  invérifiable.
- `date_levee_reserves` NULL = réserves encore ouvertes → **la retenue de
  garantie reste due**. C'est ce lien qui justifie de tracer la levée à part.
- `garantie_mois` sert à afficher une fin de garantie indicative (30 j/mois).
- Une définitive sans provisoire préalable, ou prononcée avec des réserves
  ouvertes : **acceptée et signalée**, jamais refusée.

### Pièces jointes
`document_joint` (migration 0028) est **étendu** avec `marche_id` et
`marche_etape_id` plutôt que dupliqué : fichiers sur disque, nom régénéré en
UUID, 20 Mo max — toute la mécanique éprouvée est réutilisée.

### Suppression
Un marché ne se supprime que s'il est **vierge de tout acte** : aucune étape
franchie, aucun avenant approuvé, aucune réception. Sinon il s'annule avec un
motif. Les pièces jointes sont **détachées** avant purge, jamais orphelines.

---

## 11. Activation des modules *(migrations 0040 et 0041)*

⚠️ **Ce n'est pas un filtre d'affichage.** À l'installation, la **formule vendue**
détermine les modules auxquels le client a droit : c'est une **donnée de
facturation**. On doit pouvoir dire, des mois plus tard, ce qui a été souscrit,
quand et par qui.

### `module`
| Colonne | Décidé par | Nature |
|---|---|---|
| `souscrit` (+ `souscrit_le`, `souscrit_par`) | l'installateur | **facturation** — le client n'y touche pas |
| `actif` | le client | **confort** — il masque ce qu'il n'utilise pas |

`visible = socle OR (souscrit AND actif)` : c'est la seule chose dont le menu a
besoin. `socle = 1` marque les modules sans lesquels l'application n'existe plus
(articles, tiers, paramètres, utilisateurs) — jamais désactivables.
`requiert` porte les dépendances (les abonnements exigent la facturation).

Les **formules** vivent dans le code (`activation::FORMULES`), pas en base : ce
sont des offres commerciales, elles suivent le catalogue de vente et non les
données d'un client. Elles ne font que **pré-cocher** — c'est la liste ajustée
qui est enregistrée.

### Trois refus, tous structurels
- le **socle** ne se masque pas ;
- un module **non souscrit** ne s'active pas (c'est toute la règle de facturation) ;
- un module dont **un autre dépend** ne se masque pas tant que celui-là est affiché.

### Ce que la désactivation ne fait pas
Elle ne touche à **aucune donnée**. `activation::contenu()` compte ce que le
module renferme (« 8 marchés, 3 avenants ») pour **prévenir avant de masquer** :
sans ce message, l'utilisateur croit qu'il a effacé son travail. Tout revient
intact à la réactivation.

### Migration 0041 — ne jamais couper l'accès à l'existant
La 0040 crée le catalogue avec `souscrit = 0`, ce qui est juste pour une
installation neuve et **désastreux sur une base en service** : le menu d'un
client qui travaillait depuis des mois se réduirait au socle au premier
démarrage après mise à jour. La 0041 ouvre donc tout, et c'est l'écran
« Modules » qui restreint ensuite. **Le sens de l'erreur compte** : un module
ouvert par excès se referme d'un clic, un module fermé par erreur fait croire à
une perte de données.

La même prudence existe côté écran : une liste de modules vide ou inexploitable
est traitée comme « pas d'information », et le menu reste entier.

---

## Ce qui n'existe pas encore

Le niveau 2 de `claude_spec/plan_comptable.md` (écritures et journaux) est en
place depuis la migration 0034, mais **dans la variante « pilotée par le
comptable »** décrite ci-dessus, pas dans la variante automatique de la spec.

Restent hors périmètre : l'**exercice comptable** (N3 — clôture, à-nouveaux,
verrouillage de période) et les **états financiers** (N4 — SMT, bilan, compte de
résultat). Reste aussi du niveau 1 : montant en toutes lettres, droit de timbre,
retenue à la source. Et la **valorisation du stock** (CUMP) : le stock est
journalisé en quantité mais pas en valeur, ce qui rend la marge affichée fausse
tant que `article.prix_achat` n'est pas renseigné.
