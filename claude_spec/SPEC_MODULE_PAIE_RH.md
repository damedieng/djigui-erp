# Module Paie / RH — Djigui ERP (Sénégal)

> **Projet** : Djigui Desktop — ERP de gestion pour TPE/PME (SODEVITEL, Dakar)
> **Périmètre de ce document** : module Paie (bulletins de salaire) conforme au
> droit social et fiscal sénégalais.
> **Nature du document** : spécification fonctionnelle et modèle de données pour
> Claude Code. Décrit **le quoi** (structure, règles), pas **le comment**.
> **Stack** : Tauri / Rust (axum) / SQLite (WAL) — toutes les écritures passent
> par le processus serveur, comme le reste de l'ERP.

---

## ⚠️ AVERTISSEMENT CAPITAL — Paramètres légaux

**Aucun taux, plafond, tranche ou montant légal ne doit être codé en dur.**
Tous vivent dans les tables de configuration `ref_*` (Groupe A). Le code de
calcul lit ces tables ; il ne connaît aucun chiffre.

Raison : les paramètres sociaux et fiscaux sénégalais (barème IR, plafonds
IPRES/CSS, montants TRIMF, plafond d'abattement, exonération transport) **changent
à chaque loi de finances** et **divergent d'une source publique à l'autre**. Les
valeurs de seed fournies en **Annexe A** sont **indicatives et non certifiées**.

> **CHECKPOINT — à me consulter avant mise en production.**
> Les valeurs de l'Annexe A doivent être confrontées au **CGI en vigueur** et au
> **simulateur officiel DGID** (dgid.sn/simulateur-part, impotsetdomaines.gouv.sn)
> et à l'IPRES/CSS. Ne pas considérer les valeurs de seed comme validées.
> Points de divergence connus à trancher : plafond IPRES régime général
> (360 000 vs 432 000 F/mois), plafond de l'abattement 30 % (900 000 vs
> 1 800 000 F/an), montants exacts de la TRIMF (annuels vs mensuels selon les
> sources), et si l'abattement 30 % se cumule ou non avec la déduction des
> cotisations IPRES du revenu imposable.

---

## 1. Principes de conception (cohérence avec l'architecture Djigui)

1. **Paramètres légaux en données, pas en code** (voir ci-dessus). Un
   administrateur met à jour les taux sans toucher au code, ni recompiler.
2. **Immutabilité du bulletin.** Un bulletin validé est une **photo figée** du
   calcul du mois. On ne le modifie jamais : une erreur se corrige par un
   bulletin de **régularisation** sur le mois suivant, ou par annulation +
   recalcul avant validation. Même philosophie que `mouvement_stock` et les
   écritures comptables ailleurs dans l'ERP.
3. **Module isolé, activable/désactivable** derrière une frontière nette, comme
   caisse/stock/production. Toute vérification de droit passe par le **point de
   contrôle unique d'autorisation** de l'ERP (renvoie `true` pour l'instant).
4. **La paie génère la comptabilité, elle ne la ressaisit pas.** À la clôture,
   chaque bulletin (ou le journal de paie agrégé du mois) produit
   automatiquement son écriture en partie double (§7). L'administrateur mappe
   les comptes une fois ; le moteur fait le reste.
5. **Tout passe par le serveur Rust.** Les clients n'accèdent jamais à la base.

---

## 2. Périmètre v1

**Inclus** : fiche salarié, contrat, saisie des variables mensuelles, calcul
complet d'un bulletin (brut → cotisations → fiscal → net), clôture mensuelle,
génération du bulletin PDF, du journal de paie, des fichiers de déclaration
(IPRES, CSS, DGID/NDAMLI), et des écritures comptables.

**Hors scope v1 (ne pas implémenter maintenant)** :
- Gestion des congés payés et soldes de congés (juste un champ d'absences pour
  l'instant ; le moteur de congés viendra plus tard).
- Prêts/échéanciers de remboursement complexes (on gère l'acompte simple).
- Paie rétroactive multi-mois automatisée.
- Déclarations annuelles récapitulatives (DTS) — v2.
- Multi-conventions collectives avec grilles automatiques — v2.

---

## 3. Modèle de données

Types : `uuid`, `text`, `integer`, `decimal`, `date`, `datetime`, `boolean`,
`enum`. En SQLite : `uuid`/`enum` → `TEXT`, `decimal` → `NUMERIC`, `boolean` →
`INTEGER 0/1`.

### Groupe A — Paramètres légaux (configuration)

Ces tables sont **versionnées par période d'application** : chaque ligne porte
une `date_debut` / `date_fin` (nullable) pour qu'un recalcul d'un mois passé
utilise les taux d'alors. Ne jamais écraser un ancien taux : on ferme sa période
et on en crée un nouveau.

**`ref_parametres_sociaux`** — taux et plafonds des cotisations.
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| organisme | enum | `ipres_rg` \| `ipres_rcc` \| `css_pf` \| `css_at` \| `ipm` |
| taux_salarial | decimal | %, 0 si à charge exclusive employeur |
| taux_patronal | decimal | % |
| plafond_mensuel | decimal | assiette plafonnée ; nullable si non plafonné |
| reserve_cadre | boolean | `ipres_rcc` = true (ne s'applique qu'aux cadres) |
| date_debut | date | |
| date_fin | date | nullable |

> `css_at` (accident du travail) a un **taux patronal variable selon le secteur
> de risque de l'entreprise** (§ paramètres_entreprise_paie). La ligne `ref`
> porte la fourchette ; la valeur retenue est celle configurée pour l'entreprise.

**`ref_bareme_irpp`** — tranches du barème progressif (**montants annuels**).
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| borne_inf | decimal | annuel |
| borne_sup | decimal | annuel ; nullable pour la dernière tranche |
| taux | decimal | % |
| date_debut / date_fin | date | |

**`ref_reductions_famille`** — réduction pour charge de famille par nombre de
parts. **Un taux SEUL ne suffit pas** : le CGI borne la réduction par un
plancher et un plafond.
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| nb_parts | decimal | 1 ; 1,5 ; 2 ; 2,5 ; … ; 5 |
| taux_reduction | decimal | % appliqué à l'IR **brut** |
| reduction_min | decimal | plancher annuel |
| reduction_max | decimal | plafond annuel |
| date_debut / date_fin | date | |

> ⚠️ **Ne PAS implémenter le quotient familial « à la française »** (diviser le
> revenu par les parts). Le Sénégal calcule l'IR sur le revenu **entier**, puis
> **soustrait** une réduction (taux borné min/max) à l'IR brut. Voir §5, étape 4.

**`ref_trimf`** — barème forfaitaire de la TRIMF par tranche de rémunération.
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| borne_inf | decimal | tranche de (brut + avantages en nature) |
| borne_sup | decimal | nullable |
| montant | decimal | forfait **par part TRIMF** |
| periodicite | enum | `mensuel` \| `annuel` — **à fixer selon le CGI** |
| date_debut / date_fin | date | |

**`ref_primes_reglementaires`** — plafonds d'exonération et barèmes d'avantages.
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| code | text | ex. `transport`, `av_logement`, `av_vehicule`, `av_nourriture` |
| plafond_exoneration | decimal | ex. exonération transport ; nullable |
| mode_evaluation | text | forfait / % du brut / barème ; selon l'avantage |
| valeur | decimal | selon `mode_evaluation` |
| date_debut / date_fin | date | |

**`ref_abattement_frais_pro`** — l'abattement forfaitaire de 30 %.
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| taux | decimal | ex. 30.00 |
| plafond_annuel | decimal | **À VALIDER** (voir avertissement) |
| date_debut / date_fin | date | |

### Groupe A bis — Paramètres de l'entreprise pour la paie

**`parametres_entreprise_paie`** — singleton, complète `parametres_entreprise`.
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| secteur_risque_at | text | catégorie de risque → taux `css_at` retenu |
| taux_at_retenu | decimal | % accident du travail applicable à l'entreprise |
| numero_ipres | text | n° employeur IPRES |
| numero_css | text | n° employeur CSS |
| ipm_id | text | IPM de rattachement, nullable |
| jours_ouvrables_mois | integer | base de proratisation des absences (ex. 26) |

### Groupe B — Core RH

**`employes`**
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| matricule | text | unique |
| nom / prenom | text | |
| date_naissance | date | |
| situation_matrimoniale | enum | `celibataire` \| `marie` \| `veuf` \| `divorce` |
| nb_conjoints_a_charge | integer | épouses **non salariées** (parts TRIMF) ; défaut 0 |
| nb_enfants_charge | integer | enfants à charge au sens fiscal |
| est_cadre | boolean | déclenche l'IPRES RCC |
| date_embauche | date | |
| actif | boolean | défaut true |

> Le **nombre de parts fiscales** n'est pas stocké : il est **calculé** depuis la
> situation matrimoniale + enfants (règle en §5, étape 4), plafonné à 5. La règle
> exacte d'attribution des parts (cas veuf, femme salariée) doit suivre le
> simulateur DGID — la coder de façon isolée et testable.

**`contrats`**
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| employe_id | uuid | FK → employes |
| type_contrat | enum | `cdi` \| `cdd` \| `stage` |
| date_debut / date_fin | date | date_fin nullable (CDI) |
| salaire_base | decimal | mensuel |
| sursalaire | decimal | complément contractuel, défaut 0 |
| actif | boolean | un seul contrat actif par employé à la fois |

**`contrat_avantages`** — avantages en nature attribués par contrat.
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| contrat_id | uuid | FK |
| code_avantage | text | FK logique → `ref_primes_reglementaires.code` |
| valeur_declaree | decimal | nullable si évalué par barème |

### Groupe C — Variables du mois

**`elements_variables`** — une ligne par (employé, mois, type d'élément).
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| employe_id | uuid | FK |
| periode | text | `AAAA-MM` |
| type_element | enum | `heures_supp` \| `prime_except` \| `prime_transport_reelle` \| `absence_non_payee` \| `acompte` \| `avance` \| `autre_gain` \| `autre_retenue` |
| quantite | decimal | ex. nb d'heures, nb de jours d'absence |
| montant | decimal | montant en F ; ou calculé depuis `quantite` selon le type |
| libelle | text | nullable |

### Groupe D — Résultats

**`bulletins_paie`** — la photo figée du calcul (immutable après validation).
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| employe_id | uuid | FK |
| periode | text | `AAAA-MM` |
| statut | enum | `brouillon` \| `valide` \| `regularise` \| `annule` |
| brut_global | decimal | |
| assiette_sociale | decimal | brut − remboursements de frais (base des plafonnements) |
| base_trimf | decimal | brut + avantages en nature |
| net_imposable | decimal | |
| abattement_frais_pro | decimal | montant retenu (plafonné) |
| ir_brut | decimal | |
| reduction_famille | decimal | |
| ir_net | decimal | après réduction |
| trimf | decimal | |
| nb_parts_ir | decimal | figé au moment du calcul |
| ipres_sal / ipres_pat | decimal | |
| css_pat | decimal | prestations familiales + AT (patronal) |
| ipm_sal / ipm_pat | decimal | |
| cfce_pat | decimal | 3 % assiette (charge employeur) |
| total_retenues_sal | decimal | social salarial + IR net + TRIMF + acomptes |
| net_a_payer | decimal | |
| cout_total_employeur | decimal | brut + toutes charges patronales |
| genere_le | datetime | |
| ecriture_id | uuid | FK → écriture comptable générée, nullable |

**`bulletin_lignes`** — détail des rubriques imprimées sur le bulletin (gains,
retenues, cotisations), pour réimpression à l'identique et transparence.
| champ | type | notes |
|---|---|---|
| id | uuid | PK |
| bulletin_id | uuid | FK |
| code_rubrique | text | ex. `salaire_base`, `ipres_rg_sal`, `ir_net`, `trimf` |
| libelle | text | |
| base | decimal | nullable |
| taux | decimal | nullable |
| montant_gain | decimal | 0 si non applicable |
| montant_retenue_sal | decimal | 0 si non applicable |
| montant_charge_pat | decimal | 0 si non applicable |
| ordre | integer | ordre d'affichage |

---

## 4. Constante importante : distinguer les bases de calcul

Le module manipule **plusieurs bases distinctes** — ne pas les confondre :

- **Brut global** = salaire de base + sursalaire + primes + heures supp +
  avantages en nature évalués − absences non payées.
- **Assiette sociale** = brut − remboursements de frais réels/forfaitaires
  (dont la prime de transport). C'est **elle** qui subit les **plafonds** IPRES /
  CSS / IPM, et qui sert de base à la **CFCE (3 %)**.
- **Base TRIMF** = brut + avantages en nature.
- **Net imposable** = base après abattement 30 % (plafonné) et retenues sociales
  salariales déductibles → base du barème IR.

---

## 5. Pipeline de calcul (6 étapes, ordre impératif)

```
[1 Collecte] → [2 Brut & assiette] → [3 Cotisations] → [4 Fiscal] → [5 Net] → [6 Clôture]
```

Chaque étape est une fonction pure et testable, lisant les `ref_*` de la période.

### Étape 1 — Collecte
Charger : contrat actif (base, sursalaire), avantages en nature du contrat,
situation de famille de l'employé, et toutes les `elements_variables` du mois.

### Étape 2 — Brut global & assiette sociale
- Évaluer les avantages en nature via `ref_primes_reglementaires`.
- Proratiser les absences non payées (base `jours_ouvrables_mois`).
- **Brut global** = somme de tout (voir §4).
- **Assiette sociale** = brut − remboursements de frais (transport, etc.).
- **Base TRIMF** = brut + avantages en nature.

### Étape 3 — Cotisations sociales (IPRES, CSS, IPM)
Pour chaque organisme dans `ref_parametres_sociaux` :
- Base = min(assiette sociale, `plafond_mensuel`) si plafonné, sinon assiette.
- Part salariale = base × `taux_salarial` ; part patronale = base × `taux_patronal`.
- **IPRES RG** : pour tous. **IPRES RCC** : **uniquement si `est_cadre`**, sur la
  fraction de l'assiette comprise entre le plafond RG et le plafond RCC.
- **CSS** (prestations familiales + accident du travail) : **patronal
  exclusivement**, sur l'assiette plafonnée CSS ; le taux AT est celui de
  l'entreprise (`taux_at_retenu`).
- **IPM** : selon la convention de l'IPM de rattachement (souvent partagé).

### Étape 4 — Fiscal (IR + réduction famille + TRIMF)
**4.1 Net imposable** (base du barème) :
```
net_imposable = assiette_imposable
              − abattement_frais_pro            (= min(30% × base, plafond ref))
              − retenues_sociales_sal_déductibles (IPRES sal + IPM sal)
```
> ⚠️ **À valider** : selon la lecture du CGI, l'abattement 30 % et la déduction
> des cotisations IPRES peuvent NE PAS se cumuler (l'abattement représentant déjà
> la part retraite). Coder les deux comme options paramétrables et **me consulter**
> avant de figer la règle. (voir avertissement en tête)
> La prime de transport est retranchée dans la limite de `plafond_exoneration`.

**4.2 IR brut** : appliquer `ref_bareme_irpp` au net imposable **annualisé**
(× 12), tranche par tranche, puis ramener au mois (÷ 12). Les allocations
familiales versées par la CSS et les remboursements de frais justifiés sont
**exonérés** (hors base).

**4.3 Parts fiscales** : `1` (célibataire/divorcé/veuf sans enfant) ; `1,5`
(marié) ; **+0,5 par enfant à charge** ; **plafond 5 parts**. Règle exacte à
caler sur le simulateur DGID (cas particuliers) et à isoler dans une fonction.

**4.4 Réduction pour charge de famille** (mécanisme sénégalais, **pas** le
quotient familial) :
```
taux, min, max = ligne de ref_reductions_famille correspondant à nb_parts
reduction = borne( IR_brut × taux , min_annuel/12 , max_annuel/12 )
IR_net = IR_brut − reduction        (jamais négatif)
```

**4.5 TRIMF** :
```
montant_part = montant de ref_trimf pour la tranche de base_trimf
parts_trimf  = 1 (salarié) + nb_conjoints_a_charge (épouses non salariées)
TRIMF = montant_part × parts_trimf   (ramené au mois si le barème est annuel)
```

### Étape 5 — Net à payer & charges patronales
```
net_a_payer = brut_global
            − (IPRES sal + IPM sal)
            − IR_net − TRIMF
            − acomptes/avances du mois
```
Charges patronales :
```
CFCE = 3% × assiette_sociale        (charge exclusive employeur)
charges_pat = IPRES pat + CSS pat + IPM pat + CFCE
cout_total_employeur = brut_global + charges_pat
```

### Étape 6 — Clôture du mois
1. Insérer `bulletins_paie` (statut `valide`) + `bulletin_lignes` (rubriques).
2. **Figer** : le bulletin devient immutable (§6).
3. Générer les livrables (§8) : bulletin PDF, journal de paie, fichiers de
   déclaration.
4. Générer l'**écriture comptable** (§7) et renseigner `ecriture_id`.

---

## 6. Immutabilité & régularisation
- Un bulletin `valide` n'est **jamais** modifié ni supprimé. Le PDF doit être
  réimprimable à l'identique depuis `bulletins_paie` + `bulletin_lignes`.
- Correction d'une erreur découverte après validation : créer un bulletin de
  **régularisation** (statut `regularise`) sur la période suivante, portant le
  différentiel, relié au bulletin d'origine.
- Avant validation, un `brouillon` peut être recalculé librement.

---

## 7. Schéma comptable de la paie (intégration compta)

À la clôture, générer une écriture équilibrée (par bulletin, ou agrégée par mois
via le journal de paie — **choix à me confirmer**). Schéma type :

| Compte (SYSCOHADA) | Libellé | Débit | Crédit |
|---|---|---|---|
| 661 | Rémunérations directes (brut) | brut_global | |
| 664 | Charges sociales patronales | charges sociales pat | |
| 63/64 (CFCE) | Impôts et taxes / charge patronale | CFCE | |
| 422 | Personnel, rémunérations dues | | net_a_payer |
| 431 | Sécurité sociale (CSS) | | CSS (pat) |
| 432 | Caisses de retraite (IPRES) | | IPRES (sal + pat) |
| 438 | Autres organismes sociaux (IPM) | | IPM (sal + pat) |
| 447 | État, impôts retenus à la source | | IR_net + TRIMF |
| 421 | Personnel, avances et acomptes | | acomptes retenus |

> ⚠️ **Les numéros de comptes ci-dessus sont indicatifs.** Les aligner
> exactement sur le **plan comptable SYSCOHADA seedé par le module compta** déjà
> livré. Le mapping compte ↔ rubrique de paie doit être **configurable**
> (une table `ref_mapping_comptable_paie`), pas codé en dur.

---

## 8. Livrables obligatoires (Sénégal)
1. **Bulletin de paie** (PDF, en-tête depuis `parametres_entreprise`) — pour le
   salarié.
2. **Journal de paie** du mois (récap tous salariés) — pour le comptable.
3. **Fichiers de déclaration** pour le télépaiement/télédéclaration :
   - IPRES (cotisations retraite),
   - CSS (prestations familiales + AT),
   - DGID : IR + TRIMF retenus à la source (plateforme **NDAMLI**),
   mensuel ou trimestriel selon l'obligation de l'entreprise.

> Le **format exact** des fichiers de déclaration (gabarit NDAMLI, fichiers
> IPRES/CSS) doit être vérifié auprès des organismes. **CHECKPOINT — me consulter
> avant de figer les formats d'export.**

---

## 9. Points à trancher avec moi avant implémentation
1. Validation des **valeurs légales** (Annexe A) contre le CGI/DGID en vigueur.
2. Règle du cumul **abattement 30 % ↔ déduction IPRES** du revenu imposable (§5.4.1).
3. Écriture comptable **par bulletin** ou **agrégée par mois** (§7).
4. Formats exacts des **fichiers de déclaration** (§8).
5. Attribution des **parts fiscales** dans les cas particuliers (§5.4.3).

Ne prendre aucune décision de conception sur ces cinq points sans validation.

---

## 10. Consignes Claude Code
- **Zéro paramètre légal en dur.** Tout vient des `ref_*`, versionnées par période.
- Chaque étape du pipeline = fonction pure, isolée, testable unitairement, avec
  jeux de tests couvrant : non-cadre célibataire, cadre marié avec enfants,
  salaire sous le seuil d'IR, salaire au-dessus du plafond IPRES RCC.
- Le module Paie est isolé derrière une frontière nette ; droits via le point de
  contrôle unique d'autorisation de l'ERP.
- Toutes les écritures passent par le serveur ; jamais d'accès base côté client.
- Bulletin validé = immutable. Prévoir l'utilitaire de recalcul d'un brouillon.
- Mapping comptable configurable, aligné sur le plan SYSCOHADA du module compta.

---

## Annexe A — Valeurs de seed INDICATIVES (⚠️ à valider, non certifiées)

> Sources : agrégateurs publics et sources de paie sénégalaises 2026. **Divergences
> constatées signalées.** Confronter au CGI/DGID/IPRES/CSS avant production.

**Barème IR (annuel)** : 0 % ≤ 630 000 ; 20 % de 630 001 à 1 500 000 ; 30 % de
1 500 001 à 4 000 000 ; 35 % de 4 000 001 à 8 000 000 ; 37 % de 8 000 001 à
13 500 000 ; 40 % au-delà. *(Une tranche à 43 % au-delà de 50 000 000 est
mentionnée par certaines sources — à vérifier.)*

**Réduction charge de famille** (taux sur IR brut ; min/max **annuels**) —
*table reconstituée, à vérifier ligne par ligne* :

| parts | taux | min | max |
|---|---|---|---|
| 1 | 0 % | 0 | 0 |
| 1,5 | 10 % | 100 000 | 300 000 |
| 2 | 15 % | 200 000 | 650 000 |
| 2,5 | 20 % | 300 000 | 1 100 000 |
| 3 | 25 % | 400 000 | 1 650 000 |
| 3,5 | 30 % | 500 000 | 2 030 000 |
| 4 | 35 % | 600 000 | 2 490 000 |
| 4,5 | 40 % | 700 000 | 2 755 000 |
| 5 | 45 % | 800 000 | 3 180 000 |

**Abattement frais pro** : 30 % ; plafond **900 000 F/an** *(alternative citée :
1 800 000 F/an — à trancher)*.

**IPRES** : RG 5,6 % salarial + 8,4 % patronal ; plafond **360 000 ou
432 000 F/mois selon la source — À VALIDER**. RCC (cadres) 2,4 % + 3,6 % ;
plafond ~1 080 000 à 1 296 000 F/mois — à valider.

**CSS** (patronal exclusif, plafond ~63 000 F/mois) : prestations familiales
~7 % ; accident du travail 1 % à 5 % selon le secteur de risque.

**IPM** : ~3 % salarial + 3 % patronal, plafond selon convention (~250 000 F/mois
typique) — variable selon l'IPM.

**CFCE** : 3 % patronal sur l'assiette (masse salariale). 

**Exonération transport** : ~20 800 F/mois (à confirmer).

**TRIMF** : forfait par tranche de (brut + avantages), × parts TRIMF (1 + épouses
non salariées). **Montants et périodicité (annuel vs mensuel) à saisir depuis le
CGI en vigueur** — les sources publiques divergent (fourchette citée 900 →
18 000 / 36 000 F). Ne pas figer sans vérification.
