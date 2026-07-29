# Spécification — Passation et suivi des marchés

> **Source** : module existant de l'application **OLAC**
> (`C:\xampp82.12\htdocs\olac\app\controller\MarcheController.php`, base `db_olac`,
> tables `marche_*`). Structure et données lues le 2026-07-27.
>
> Ce document traduit ce module dans les conventions de Djigui. Il ne recopie pas
> le code : il retient le **modèle métier**, qui est bon, et l'aligne sur des
> mécaniques que Djigui possède déjà.

---

## 1. Ce que fait le module d'origine

Un **marché** est une commande publique (ou une commande importante) qui ne
s'exécute pas d'un coup : elle traverse une **suite d'étapes obligatoires**
(publication de l'avis, ouverture des plis, attribution, signature du contrat…),
chacune avec une date prévue, une date effective, et des pièces justificatives.

Le module sert à **ne pas rater une étape** et à **prouver qu'on ne l'a pas
ratée**. C'est un outil de conformité autant que de suivi.

### Le modèle d'origine, en cinq tables

| Table | Rôle |
|---|---|
| `marche_type_marche` | Les **familles** de marché : Travaux, Fournitures, Services, Prestations intellectuelles. Configurables. |
| `marche_etape_type` | Le **modèle de procédure** de chaque famille : les étapes, leur ordre, leur durée prévue en jours, leur caractère obligatoire. |
| `marche_marche` | Le marché lui-même : numéro, objet, montant estimé, statut, dates, responsable, lieu d'exécution. |
| `marche_etape_suivi` | Les étapes **réellement parcourues** par un marché : date prévue, date effective, statut, observations, qui a validé et quand. |
| `marche_document` | Les pièces jointes, rattachées au marché **et éventuellement à une étape précise**. Fichiers sur disque. |

### L'idée forte à conserver

**Le type de marché porte sa procédure.** On ne ressaisit pas les étapes à chaque
fois : on choisit « Travaux », et les étapes se déroulent toutes seules avec leurs
dates prévues calculées par cumul des durées.

C'est exactement le rapport **modèle → instance** que Djigui emploie déjà entre
une **recette** (`nomenclature`) et un **ordre de fabrication** : le modèle sert
d'amorce, l'instance vit ensuite sa vie et reste modifiable au cas par cas.

### Données réelles observées

4 familles utilisées (Travaux, Fournitures, Services, Prestations
intellectuelles) plus des types d'essai. Exemple de procédure « Travaux » :
publication de l'avis (3 j) → ouverture des plis (1 j) → attribution provisoire
(7 j) → signature du contrat (10 j). Les autres familles suivent le même schéma
avec des durées différentes.

### Règles métier repérées dans le contrôleur

1. **Numéro de marché généré automatiquement** (`generateNumeroMarche`).
2. **Marchés en retard** : une étape `en_cours` dont la date prévue est dépassée,
   avec le **nombre de jours de retard**. C'est la requête centrale du tableau de
   bord, triée du plus en retard au moins en retard.
3. **Fenêtre de modification d'une étape validée** : une étape validée ne reste
   modifiable que **30 jours** (`DELAI_MODIFICATION_SUIVI_JOURS`,
   `getSuiviEditDeadline`, `suivi_editable`). Au-delà, elle est figée.
4. **Traçabilité de la validation** : qui a validé (`user_validation`) et quand
   (`date_validation`).
5. Statuts du marché : `en_cours`, `realise`, `annule`, `suspendu`.
   Statuts d'étape : `en_attente`, `en_cours`, `termine`, `annule`, `reporte`.
6. **Motif d'annulation obligatoire** à l'annulation d'un marché.
7. Recherche multicritère : numéro, objet, type, statut, période.

---

## 2. Transposition dans Djigui

### 2.1 Ce qu'on garde tel quel

Le modèle en cinq tables est sain et se transpose directement. On conserve la
séparation **type → étapes modèles** et **marché → étapes suivies**.

### 2.2 Ce qu'on branche sur l'existant plutôt que de le refaire

| Besoin | Ce que Djigui a déjà |
|---|---|
| Numérotation automatique | `sequence_numero` + `config_prefixe_document` (préfixe `MA` → `MA-2026-0001`), comme les factures et les ordres de fabrication |
| Pièces jointes | `document_joint` (migration 0028) : fichiers **sur disque**, nom régénéré en UUID, 20 Mo max, ouverture via `ouvrir_fichier`. Il suffit d'ajouter les colonnes de rattachement `marche_id` / `etape_suivi_id` |
| Responsable, traçabilité | `utilisateur`, `cree_par`, `journal_audit` |
| Fournisseur attributaire | `tiers` (nature entreprise, NINEA, RCCM — migration 0027) |
| Retards et alertes | Le module `notification` (migration 0030), qui calcule déjà les retards du jour |
| Traitement par lot | Le motif `lot/statut` et `lot/supprimer` présent dans tous les modules |

### 2.3 Ce qu'on adapte aux règles de Djigui

**Le retard est un signalement, jamais un blocage.** Comme partout ailleurs :
bandeau jaune, jamais un refus d'enregistrer. Une étape en retard ne bloque pas
la suivante — le terrain ne s'arrête pas parce qu'un logiciel n'est pas content.

**Pas de recalcul automatique en cascade.** Si une étape glisse, on **signale**
que les suivantes sont décalées et on propose un bouton explicite
« Replanifier les étapes suivantes » avec un **aperçu avant écriture**. C'est
exactement le mécanisme « Harmoniser les dates » du module Projet, et il
respecte la barrière posée dans la spec Gestion de projet.

**La fenêtre de 30 jours devient un paramètre.** Codée en dur dans OLAC, elle sera
un réglage (`parametre_global`), avec 30 jours par défaut. Un pays, une
administration, un client n'ont pas les mêmes usages.

**Suppression = détachement.** Supprimer une étape ne détruit pas ses documents ;
supprimer un type de marché ne détruit pas les marchés qui l'utilisent
(`type_marche_id` passe à `NULL`, le marché garde ses étapes déjà instanciées).

### 2.4 Ce qu'on ajoute, parce que Djigui peut le faire

- **Rattacher un fournisseur attributaire** (`tiers`) au marché, une fois
  l'attribution prononcée. OLAC ne le fait pas ; Djigui a déjà les tiers avec
  leur identité légale.
- **Rattacher le marché à un projet** (`projet`, migration 0021), facultatif :
  un marché finance souvent une activité de projet. Simple lien, pas de
  propagation de dates.
- **Montant attribué** en plus du montant estimé, et l'**écart** entre les deux —
  c'est le chiffre que tout le monde regarde.
- **Section d'aide** en langage simple sur chaque écran, comme partout.

---

## 3. Modèle de données proposé (migration 0037)

### `marche_type` — les familles et leur procédure
`id`, `code` (unique), `libelle`, `description`, `actif`, `cree_par`, `cree_le`.
Seedé avec Travaux, Fournitures, Services, Prestations intellectuelles.

### `marche_etape_modele` — la procédure d'une famille
`id`, `type_id` → `marche_type`, `libelle`, `description`, `ordre`,
`duree_prevue_jours`, `obligatoire`, `actif`.

### `marche` — le marché
`id`, `numero` (unique, `MA-AAAA-NNNN`), `objet`, `type_id`,
`montant_estime`, `montant_attribue`, `monnaie` (défaut FCFA),
`statut` (`en_cours` | `realise` | `annule` | `suspendu`),
`date_lancement`, `date_cloture_prevue`, `date_cloture_effective`,
`attributaire_id` → `tiers` (facultatif), `projet_id` → `projet` (facultatif),
`responsable_id` → `utilisateur`, `lieu_execution`, `observations`,
`motif_annulation`, `annule_par`, `annule_le`, `cree_par`, `cree_le`.

### `marche_etape` — les étapes réellement suivies
`id`, `marche_id`, `etape_modele_id` (traçabilité ; `NULL` si étape ajoutée à la
main), `libelle` (**recopié**, l'étape est autonome ensuite), `ordre`,
`date_prevue`, `date_effective`,
`statut` (`en_attente` | `en_cours` | `termine` | `annule` | `reporte`),
`obligatoire`, `observations`, `valide_par`, `valide_le`, `cree_le`.

⚠️ Le libellé est **recopié**, pas joint : modifier une procédure ne doit pas
réécrire l'histoire des marchés déjà lancés. Même règle que la recette de
production.

### Documents
Extension de `document_joint` : ajout de `marche_id` et `marche_etape_id`.
Pas de nouvelle table — les fichiers restent sur disque, avec le même garde-fou
(nom régénéré en UUID, jamais le nom fourni par le client).

---

## 4. Comportements attendus

### Création d'un marché
Choisir un type → les étapes du modèle sont **instanciées automatiquement**, avec
les dates prévues calculées par **cumul des durées** à partir de la date de
lancement. Ensuite, l'utilisateur peut ajouter, retirer ou décaler une étape :
le modèle est une amorce, pas une contrainte.

### Avancement
Une étape passe `en_attente` → `en_cours` → `termine`. La terminer **horodate**
`date_effective` et enregistre `valide_par` / `valide_le`, comme le changement de
statut en lot des jalons.

L'avancement du marché = part des étapes obligatoires terminées.

### Retards — signalement uniquement
Une étape `en_cours` dont `date_prevue` est dépassée est **en retard**, avec son
nombre de jours. Elle remonte : en jaune dans la liste des étapes, dans le
bandeau du marché, dans le tableau de bord, et dans les **notifications du jour**.

### Clôture, annulation, suspension
- `realise` : renseigne `date_cloture_effective`.
- `annule` : **motif obligatoire**, auteur et date tracés (même mécanique que
  l'annulation d'une facture, migration 0019).
- `suspendu` : réversible, sans effet sur les dates.

### Alertes non bloquantes
Date de clôture prévue antérieure à la dernière étape ; montant attribué
supérieur au montant estimé ; étape obligatoire non terminée alors que le marché
passe en « réalisé » ; attributaire sans NINEA. **Tout en jaune, rien ne bloque.**

---

## 5. Écrans

**`marches.html`** — liste et tableau de bord. Tuiles (en cours, réalisés, en
retard, montant total), recherche multicritère (numéro, objet, type, statut,
période, responsable), traitement par lot sur le statut, section d'aide.

**`marche-detail.html?id=`** — en-tête du marché (objet, montant estimé vs
attribué et écart, avancement, bandeau de retard), puis onglets :
- **Étapes** : la procédure, chronologique, avec dates prévue/effective, statut
  modifiable en place, retards surlignés, bouton « Replanifier les suivantes »
  avec aperçu.
- **Documents** : joindre / ouvrir / supprimer, rattachement à une étape.
- **Informations** : attributaire, projet, lieu, responsable, observations.

**`parametres.html`** — nouvel onglet « Types de marchés » : les familles et
leurs procédures (étapes, ordre, durées), avec réordonnancement.

---

## 6. Points à trancher avec l'utilisateur

1. **Les soumissionnaires** : OLAC ne les stocke pas (pas de table d'offres, pas
   de montants proposés, pas de commission d'évaluation). On garde ce périmètre —
   seul l'**attributaire** est enregistré — ou on ajoute le dépouillement des
   offres ? Cela changerait sensiblement la taille du module.
2. **Les avenants** : absents d'OLAC. Un marché qui augmente en cours
   d'exécution, ça arrive constamment. À prévoir ou non ?
3. **La réception des travaux** (provisoire, définitive, garanties) : absente
   d'OLAC également. Étape du modèle, ou objet à part entière ?
4. **Le lien avec la comptabilité** : un marché attribué débouche sur des
   factures fournisseur. Faut-il rattacher les factures d'achat au marché pour
   suivre la consommation du budget ? C'est peu coûteux et très parlant.
5. **La fenêtre de modification** : 30 jours par défaut, confirmé ?

---

## 7. Estimation

| Bloc | Poids |
|---|---|
| Migration 0037 + module cœur (types, étapes modèles, marchés, suivi) | moyen |
| API | petit |
| Écran liste + tableau de bord | moyen |
| Écran détail (étapes, documents, replanification) | moyen |
| Onglet paramétrage des types | petit |

**Environ 2 sessions** pour le périmètre d'OLAC transposé. Les points ouverts §6
(soumissionnaires, avenants, réception) ajouteraient à peu près autant.
