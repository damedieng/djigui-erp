# Spécification — Comptabilité OHADA / SYSCOHADA révisé (AUDCIF)

> **Statut : spécification, non implémentée.** Aucune ligne de code comptable
> n'existe aujourd'hui dans Djigui (vérifié : aucune table `compte`, aucune
> écriture, aucun journal, aucun lettrage). La spec initiale
> (`djigui-desktop-spec.md` §8) plaçait SYSCOHADA hors périmètre v1. Ce document
> est le cadrage du chantier v2.
>
> Réf. : Acte uniforme relatif au droit comptable et à l'information financière
> (**AUDCIF**), applicable depuis le 1er janvier 2018 dans les 17 États OHADA.

---

## 0. Principe directeur — la règle qui prime sur tout le reste

**La comptabilité ne doit JAMAIS empêcher de vendre.**

Le commerçant ouvre sa boutique le matin ; il n'ouvre pas un cabinet
d'expertise comptable. Toute la mécanique décrite ici est un **sous-produit
automatique** des faits de gestion déjà saisis (vente, achat, encaissement,
mouvement de stock). Elle tourne en arrière-plan.

Conséquences fermes, à ne jamais transgresser :

- Aucun champ comptable **obligatoire** dans un écran de vente ou de caisse.
- Un compte non paramétré → **compte d'attente** (471) + **alerte jaune**,
  jamais un refus d'enregistrer. Cohérent avec la règle générale du projet
  (voir mémoire `djigui-standards-modules`).
- Une écriture déséquilibrée est **signalée**, jamais silencieusement corrigée.
- Le commerçant qui n'active pas le module ne doit **rien voir changer**.

---

## 1. Périmètre et niveaux

Le chantier se découpe en **4 niveaux**, livrables indépendamment. Chaque
niveau est utilisable seul ; on ne passe au suivant que sur validation.

| Niveau | Contenu | Pour qui |
|---|---|---|
| **N1 — Facture conforme** | Mentions légales, numérotation continue, inaltérabilité, montant en lettres, timbre, retenues | Tout le monde, obligatoire |
| **N2 — Écritures & journaux** | Plan comptable, génération automatique, journaux, grand livre, balance, lettrage | Commerçant suivi par un comptable |
| **N3 — Exercice** | Ouverture/clôture, à-nouveaux, verrouillage de période | Entreprise structurée |
| **N4 — États financiers** | Bilan, Compte de résultat, Tableau des flux, Notes annexes — ou **SMT** | Dépôt légal au greffe |

**N1 est partiellement fait** (voir §2).

---

## 2. N1 — Facture conforme

### 2.1 Déjà en place (2026-07-25)
- Mentions vendeur : NINEA, RCCM, forme juridique, capital, régime fiscal
  (`parametres_entreprise`, migration 0027) — toutes **facultatives + alerte**.
- Nature du tiers particulier/entreprise, NINEA/RCCM client (migration 0027).
- Numérotation `PREFIXE-EXERCICE-NNNN` par type et par exercice (migration 0003).
- **Inaltérabilité** : une facture validée n'est ni modifiable ni supprimable ;
  la correction passe par **annulation avec contre-passation** (migration 0019).
- Journal d'audit horodaté et auteur des pièces (migration 0012).

### 2.2 Reste à faire

**a) Montant total en toutes lettres**
Mention attendue sur la facture. Fonction pure côté cœur :
`montant_en_lettres(m: f64, devise: &str) -> String`
→ « cent vingt-cinq mille francs CFA ». Règles françaises (quatre-vingts,
cent invariable sauf multiple, « et un »). Testable unitairement, aucune
dépendance. À poser dans `crates/core/src/modules/lettres.rs`.

**b) Continuité de la numérotation**
Aujourd'hui rien n'interdit un trou dans la séquence (une facture brouillon
supprimée ne consomme pas de numéro — c'est correct — mais rien ne le vérifie).
Ajouter un **contrôle de cohérence** (rapport, pas blocage) listant les
ruptures de séquence par type et exercice.

**c) Droit de timbre**
Ce n'est **pas** une taxe de ligne : il se déclenche selon le **moyen de
règlement** (espèces) et se calcule sur le montant réglé, pas sur l'article.
Le moteur de taxes actuel ne sait pas l'exprimer.
- `moyen_paiement.soumis_timbre` (booléen) + `parametre_global` `timbre_taux`
  et `timbre_seuil` (paramétrable, jamais codé en dur — un pays OHADA n'est
  pas l'autre).
- Calculé à l'encaissement, affiché sur le ticket et la facture, ligne distincte.

**d) Retenue à la source / précompte**
S'**ajoute** au document mais se **soustrait** de ce que le client verse —
l'inverse d'une taxe. Impacte le solde tiers.
- `tiers.retenue_source_taux` (facultatif) + `document.montant_retenue`.
- Net à payer = TTC − retenue. La retenue devient une créance sur le Trésor.

**e) Facture normalisée / certifiée (e-facturation)**
Plusieurs administrations OHADA déploient des dispositifs de facture certifiée
(transmission ou signature). **À cadrer pays par pays** avant tout code : les
formats et obligations diffèrent, et le mode hors-ligne de Djigui est un
contrainte forte. Ne rien implémenter sans spécification officielle en main.

---

## 3. N2 — Plan comptable et écritures

### 3.1 Le plan comptable OHADA

Comptes à **au moins 8 chiffres** possibles, structurés par classe :

| Classe | Nature | Comptes utiles à Djigui |
|---|---|---|
| 1 | Ressources durables | 101 capital, 16 emprunts |
| 2 | Actif immobilisé | 24 matériel, 28 amortissements |
| 3 | Stocks | 31 marchandises, 32 matières |
| 4 | Tiers | **401** fournisseurs, **411** clients, **4431** TVA collectée, **4451** TVA déductible, **471** compte d'attente |
| 5 | Trésorerie | **521** banque, **571** caisse, 585 virements internes |
| 6 | Charges | **601** achats marchandises, **6031** variation de stocks, 62/63 services |
| 7 | Produits | **701** ventes marchandises, **706** services |
| 8 | Autres charges/produits | HAO |

**Table `compte`** : `numero` (PK texte), `libelle`, `classe`, `sens_normal`
(débit/crédit), `lettrable` (bool), `actif`. **Seedée** avec un plan de base
OHADA — le commerçant ne doit pas le saisir. Extensible (sous-comptes clients
411xxx si souhaité).

### 3.2 Paramétrage comptable — le point sensible

Chaque objet de gestion porte **facultativement** son compte :

| Objet | Compte | Défaut si non paramétré |
|---|---|---|
| `article` / `categorie` | vente (701), achat (601), stock (31) | compte général du paramétrage |
| `taxe` | TVA collectée (4431) / déductible (4451) | 4431 / 4451 |
| `tiers` | 411 (client) / 401 (fournisseur) | compte collectif |
| `caisse` | 571 | 571 |
| `moyen_paiement` | 571 espèces, 521 banque/mobile | selon famille |

**Le paramétrage se fait par défaut au niveau global**, affinable par catégorie,
puis par article. Le commerçant qui ne touche à rien a une compta correcte.

### 3.3 Génération des écritures

Un **fait de gestion → une écriture équilibrée**, produite automatiquement,
jamais saisie à la main dans le flux normal.

Tables :
- `ecriture` : id, `journal_code`, `date`, `libelle`, `piece_id` (→ document
  ou paiement), `exercice`, `validee`, `cree_par`, `cree_le`.
- `ecriture_ligne` : id, `ecriture_id`, `compte_numero`, `libelle`, `debit`,
  `credit`, `tiers_id?`, `lettrage?`.

**Invariant absolu : Σ débit = Σ crédit** par écriture. Contrôlé en base
(transaction) et testé.

Schémas d'écriture minimaux :

```
VENTE (facture validée)          ACHAT (facture fournisseur)
  411 Client        D  TTC         601 Achats         D  HT
  701 Ventes        C  HT          4451 TVA déduct.   D  TVA
  4431 TVA collect. C  TVA         401 Fournisseur    C  TTC

ENCAISSEMENT                     ANNULATION DE VENTE
  571 Caisse        D  montant      → contre-passation exacte
  411 Client        C  montant        de l'écriture d'origine
                                      (jamais de suppression)
```

**Règle d'or** : l'annulation ne supprime jamais une écriture, elle la
contre-passe — exactement le mécanisme déjà retenu pour les paiements
(migration 0019). La cohérence architecturale est déjà là.

### 3.4 Journaux, grand livre, balance
- **Journaux** : VT (ventes), AC (achats), CA (caisse), BQ (banque), OD
  (opérations diverses). Table `journal` (code, libellé, contrepartie par défaut).
- **Grand livre** : mouvements d'un compte sur une période, avec solde progressif.
- **Balance générale** : par compte — débit, crédit, solde. Doit **toujours**
  s'équilibrer ; sinon, alerte rouge (c'est un vrai invariant, pas une souplesse).
- **Balance auxiliaire** : par tiers (411/401), à rapprocher du `tiers.solde`
  déjà tenu par le module paiement — c'est un **contrôle croisé gratuit**.

### 3.5 Lettrage
Rapprocher facture ↔ règlement sur les comptes de tiers. Lettrage manuel
(sélection) **et** automatique (même tiers, même montant). Colonne `lettrage`
sur `ecriture_ligne`.

---

## 4. N3 — Exercice comptable

- Table `exercice` : année, date début, date fin, statut
  (`ouvert` | `cloture`), date de clôture, auteur.
- **Verrouillage de période** : aucune écriture sur un exercice clôturé.
  ⚠️ C'est l'un des rares endroits où **bloquer est légitime** — une écriture
  sur un exercice clos est une faute, pas une souplesse.
- **À-nouveaux** : à la clôture, report des soldes de bilan (classes 1 à 5)
  sur l'exercice suivant ; les comptes de gestion (6 et 7) sont soldés dans le
  résultat.
- **Résultat de l'exercice** = Σ classe 7 − Σ classe 6, viré au compte 13.

---

## 5. N4 — États financiers

Deux régimes selon la taille de l'entreprise :

**Système Minimal de Trésorerie (SMT)** — très petites entités. Comptabilité
de trésorerie : recettes/dépenses, état des créances et dettes, situation
simplifiée. **C'est très probablement le régime de la majorité des clients
Djigui** — à privilégier en premier livrable de N4.

**Système Normal** — états complets :
- **Bilan** (actif / passif)
- **Compte de résultat** (charges / produits, par nature)
- **Tableau des flux de trésorerie**
- **Notes annexes**

Tous alimentés par la balance, via une **table de correspondance
poste ↔ comptes** (data-driven, jamais en dur — même principe que
`config_type_document`).

---

## 6. Ce qui joue déjà en faveur du projet

L'architecture existante est saine pour poser la compta dessus :

- Journal de stock **immuable**, stock = Σ(entrées) − Σ(sorties) — la logique
  de journal est déjà le réflexe du projet.
- Contre-passation au lieu de suppression, déjà implémentée.
- Journal d'audit + `cree_par` sur les pièces.
- Numérotation par exercice déjà pilotée par la donnée.
- Configuration data-driven (`config_type_document`, `config_transformation`) —
  le plan comptable et la correspondance des états suivront le même modèle.
- Soldes tiers et caisses déjà tenus et **recalculables depuis les journaux**
  (`recalculer_soldes`) : contrôle croisé naturel avec la balance auxiliaire.

---

## 7. Points ouverts — à trancher avec l'utilisateur

1. **Quel niveau viser** (N1 → N4) et dans quel ordre ?
2. **SMT ou Système Normal** en premier ? (SMT est plus proche du terrain)
3. **Multi-pays OHADA** ou un seul pays ? Les taux et mentions sont déjà
   paramétrables (catalogue de taxes) ; restent le timbre, les retenues et
   les obligations d'e-facturation.
4. **Immobilisations et amortissements** : dans le périmètre ou non ?
5. **Valorisation des stocks** : CUMP ou FIFO ? Aujourd'hui le stock est
   journalisé en quantité mais **non valorisé** — prérequis à l'inventaire
   permanent (6031) et au calcul de marge fiable.
6. **Export vers un logiciel comptable** tiers (format FEC-like ou CSV) :
   souvent, le commerçant a déjà un comptable avec son propre outil. Cela
   peut rendre N3/N4 inutiles — **à évaluer avant de tout construire**.

---

## 8. Ordre d'attaque conseillé

1. **N1 reste** : montant en lettres, timbre, retenue à la source.
2. **Valorisation du stock** (CUMP) — prérequis technique de tout le reste,
   et gain immédiat sur la marge réelle (aujourd'hui `prix_achat` est souvent
   vide, la marge est donc fausse).
3. **N2 socle** : plan comptable seedé + paramétrage par défaut + génération
   des écritures de vente/encaissement uniquement.
4. **N2 restitutions** : journaux, grand livre, balance, lettrage.
5. **Export comptable** (point 6 ci-dessus) — possible point d'arrêt satisfaisant.
6. **N3 exercice**, puis **N4 SMT**, puis Système Normal si réellement demandé.

---

## 9. Estimation grossière

| Bloc | Poids |
|---|---|
| N1 reste (lettres, timbre, retenue) | petit |
| Valorisation stock CUMP | moyen |
| N2 socle (plan + écritures) | gros |
| N2 restitutions | moyen |
| Export comptable | petit |
| N3 exercice | moyen |
| N4 SMT | moyen |
| N4 Système Normal complet | très gros |

**Recommandation** : livrer N1 + valorisation stock + export comptable avant
de s'engager sur N2 complet. Beaucoup de commerçants seront servis par là,
et leur comptable fait le reste.
