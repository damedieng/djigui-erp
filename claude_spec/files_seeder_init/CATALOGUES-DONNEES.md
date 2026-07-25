# Djigui — Données des catalogues métier

Contenu à convertir en `assets/catalogue/types/<code>.json`. Le format, les règles d'idempotence et la gestion des images sont décrits dans `SEEDER-CATALOGUES.md`.

**Conventions appliquées ici**

- Libellés **génériques**, jamais de marque commerciale (voir §7.3 de la spec).
- `code` = clé stable, `snake_case` sans accent. **Ne jamais modifier après publication.**
- Toutes les images pointent vers `articles/<code>.webp` — le `code` de l'article *est* le nom du fichier. Pas de table de correspondance à maintenir.
- `gere_stock = false` sur les prestations.
- `tva = 18` partout, écrasé à 0 si l'entreprise n'est pas assujettie.
- `prix_vente` toujours `null`.

---

## Index des types de commerce (`types.json`)

| ordre | code | libellé | icône |
|---|---|---|---|
| 10 | `alimentation_generale` | Alimentation générale / boutique | `ti-building-store` |
| 20 | `restaurant_fast_food` | Restaurant, fast-food & gargote | `ti-tools-kitchen-2` |
| 30 | `quincaillerie` | Quincaillerie & matériaux | `ti-hammer` |
| 40 | `vetements_chaussures` | Vêtements & chaussures | `ti-shirt` |
| 50 | `telephonie_accessoires` | Téléphonie & accessoires | `ti-device-mobile` |
| 60 | `cosmetique_beaute` | Cosmétique, coiffure & beauté | `ti-scissors` |
| 70 | `papeterie_librairie` | Papeterie, librairie & imprimerie | `ti-notebook` |
| 80 | `pharmacie_para` | Pharmacie / parapharmacie | `ti-vaccine` |
| 90 | `services_atelier` | Atelier & prestations de services | `ti-settings-cog` |
| 999 | `aucun` | Je crée mon catalogue moi-même | `ti-plus` |

`aucun` n'a pas de fichier catalogue : il court-circuite le seeder.

---

## 1. `alimentation_generale`

### `boissons` — Boissons · `ti-bottle` · `#2563eb`

| code | libellé | unité |
|---|---|---|
| `eau_minerale_1_5l` | Eau minérale 1,5 L | bouteille |
| `eau_minerale_50cl` | Eau minérale 50 cl | bouteille |
| `soda_33cl` | Soda en canette 33 cl | piece |
| `soda_1_5l` | Soda 1,5 L | bouteille |
| `jus_bissap_1l` | Jus de bissap 1 L | bouteille |
| `jus_bouye_1l` | Jus de bouye 1 L | bouteille |
| `cafe_touba_sachet` | Café Touba (sachet) | sachet |
| `cafe_instantane_stick` | Café instantané (stick) | piece |
| `the_vert_paquet` | Thé vert en paquet | paquet |

### `epicerie_seche` — Épicerie sèche · `ti-shopping-bag` · `#d97706`

| code | libellé | unité |
|---|---|---|
| `huile_vegetale_1l` | Huile végétale 1 L | bouteille |
| `huile_vegetale_5l` | Huile végétale 5 L | bidon |
| `sucre_morceaux_1kg` | Sucre en morceaux 1 kg | paquet |
| `sucre_poudre_1kg` | Sucre en poudre 1 kg | paquet |
| `tomate_concentree_70g` | Tomate concentrée 70 g | boite |
| `pates_alimentaires_500g` | Pâtes alimentaires 500 g | paquet |
| `sardine_boite` | Sardines à l'huile (boîte) | boite |
| `farine_ble_1kg` | Farine de blé 1 kg | paquet |

### `cereales_legumineuses` — Céréales & légumineuses · `ti-wheat` · `#65a30d`

| code | libellé | unité |
|---|---|---|
| `riz_brise_parfume_kg` | Riz brisé parfumé | kg |
| `riz_brise_ordinaire_kg` | Riz brisé ordinaire | kg |
| `mil_kg` | Mil | kg |
| `mais_kg` | Maïs | kg |
| `niebe_kg` | Niébé (haricot) | kg |
| `arachide_decortiquee_kg` | Arachide décortiquée | kg |

### `produits_laitiers` — Produits laitiers · `ti-milk` · `#0891b2`

| code | libellé | unité |
|---|---|---|
| `lait_poudre_sachet` | Lait en poudre (sachet) | sachet |
| `lait_concentre_sucre` | Lait concentré sucré | boite |
| `lait_uht_1l` | Lait UHT 1 L | bouteille |
| `yaourt_pot` | Yaourt (pot) | piece |
| `beurre_plaquette` | Beurre (plaquette) | piece |

### `condiments_epices` — Condiments & épices · `ti-pepper` · `#dc2626`

| code | libellé | unité |
|---|---|---|
| `bouillon_cube` | Bouillon cube | piece |
| `sel_fin_1kg` | Sel fin 1 kg | paquet |
| `poivre_moulu_sachet` | Poivre moulu (sachet) | sachet |
| `nete_sachet` | Nététou (sachet) | sachet |
| `vinaigre_bouteille` | Vinaigre (bouteille) | bouteille |
| `moutarde_pot` | Moutarde (pot) | piece |

### `hygiene_entretien` — Hygiène & entretien · `ti-spray` · `#7c3aed`

| code | libellé | unité |
|---|---|---|
| `savon_menage_barre` | Savon de ménage (barre) | piece |
| `savon_toilette` | Savon de toilette | piece |
| `lessive_poudre_sachet` | Lessive en poudre (sachet) | sachet |
| `eau_javel_1l` | Eau de javel 1 L | bouteille |
| `papier_hygienique_rouleau` | Papier hygiénique (rouleau) | piece |
| `dentifrice_tube` | Dentifrice (tube) | piece |
| `allumettes_boite` | Allumettes (boîte) | boite |
| `bougie_piece` | Bougie | piece |

---

## 2. `restaurant_fast_food`

### `plats_locaux` — Plats locaux · `ti-bowl` · `#b45309`

| code | libellé | unité | stock |
|---|---|---|---|
| `thieboudienne` | Thiéboudienne | prestation | non |
| `yassa_poulet` | Yassa poulet | prestation | non |
| `mafe` | Mafé | prestation | non |
| `domoda` | Domoda | prestation | non |
| `soupe_kandia` | Soupe kandia | prestation | non |
| `thiou_viande` | Thiou viande | prestation | non |

### `grillades` — Grillades · `ti-flame` · `#dc2626`

| code | libellé | unité | stock |
|---|---|---|---|
| `poulet_braise_entier` | Poulet braisé entier | prestation | non |
| `demi_poulet_braise` | Demi-poulet braisé | prestation | non |
| `dibi_mouton_kg` | Dibi mouton | kg | non |
| `poisson_braise` | Poisson braisé | prestation | non |
| `brochette_viande` | Brochette de viande | piece | non |

### `sandwichs` — Sandwichs & fast-food · `ti-bread` · `#ea580c`

| code | libellé | unité | stock |
|---|---|---|---|
| `sandwich_viande` | Sandwich viande | prestation | non |
| `sandwich_poulet` | Sandwich poulet | prestation | non |
| `sandwich_omelette` | Sandwich omelette | prestation | non |
| `chawarma` | Chawarma | prestation | non |
| `burger_simple` | Burger simple | prestation | non |
| `frites_portion` | Portion de frites | prestation | non |

### `petit_dejeuner` — Petit-déjeuner · `ti-cup` · `#a16207`

| code | libellé | unité | stock |
|---|---|---|---|
| `cafe_touba_tasse` | Café Touba (tasse) | prestation | non |
| `cafe_au_lait` | Café au lait | prestation | non |
| `the_tasse` | Thé (tasse) | prestation | non |
| `pain_beurre` | Pain beurre | prestation | non |
| `bouillie_mil` | Bouillie de mil (laakh) | prestation | non |

### `boissons_service` — Boissons · `ti-glass` · `#2563eb`

| code | libellé | unité | stock |
|---|---|---|---|
| `eau_minerale_50cl` | Eau minérale 50 cl | bouteille | oui |
| `soda_33cl` | Soda en canette 33 cl | piece | oui |
| `jus_bissap_verre` | Jus de bissap (verre) | prestation | non |
| `jus_gingembre_verre` | Jus de gingembre (verre) | prestation | non |

### `accompagnements` — Suppléments · `ti-plus` · `#65a30d`

| code | libellé | unité | stock |
|---|---|---|---|
| `supplement_riz` | Supplément riz | prestation | non |
| `supplement_sauce` | Supplément sauce | prestation | non |
| `salade_portion` | Portion de salade | prestation | non |
| `emballage_a_emporter` | Emballage à emporter | piece | oui |

> `eau_minerale_50cl` et `soda_33cl` sont partagés avec `alimentation_generale` : le dédoublonnage par `code` s'applique.

---

## 3. `quincaillerie`

### `outillage_main` — Outillage à main · `ti-hammer` · `#57534e`

| code | libellé | unité |
|---|---|---|
| `marteau` | Marteau | piece |
| `tournevis_plat` | Tournevis plat | piece |
| `tournevis_cruciforme` | Tournevis cruciforme | piece |
| `pince_universelle` | Pince universelle | piece |
| `cle_molette` | Clé à molette | piece |
| `scie_a_metaux` | Scie à métaux | piece |
| `metre_ruban` | Mètre ruban | piece |
| `niveau_a_bulle` | Niveau à bulle | piece |

### `visserie` — Visserie & fixation · `ti-screw` · `#78716c`

| code | libellé | unité |
|---|---|---|
| `clou_kg` | Clous | kg |
| `vis_bois_lot` | Vis à bois (lot) | lot |
| `cheville_lot` | Chevilles (lot) | lot |
| `boulon_lot` | Boulons (lot) | lot |
| `fil_de_fer_kg` | Fil de fer | kg |

### `plomberie` — Plomberie · `ti-droplet` · `#0284c7`

| code | libellé | unité |
|---|---|---|
| `tuyau_pvc_metre` | Tuyau PVC | metre |
| `coude_pvc` | Coude PVC | piece |
| `raccord_pvc` | Raccord PVC | piece |
| `robinet_simple` | Robinet simple | piece |
| `joint_teflon` | Ruban téflon | piece |
| `siphon_evier` | Siphon d'évier | piece |

### `electricite` — Électricité · `ti-bolt` · `#eab308`

| code | libellé | unité |
|---|---|---|
| `cable_electrique_metre` | Câble électrique | metre |
| `interrupteur_simple` | Interrupteur simple | piece |
| `prise_murale` | Prise murale | piece |
| `douille_ampoule` | Douille d'ampoule | piece |
| `ampoule_led` | Ampoule LED | piece |
| `rallonge_multiprise` | Rallonge multiprise | piece |
| `disjoncteur` | Disjoncteur | piece |

### `peinture` — Peinture & finition · `ti-brush` · `#c026d3`

| code | libellé | unité |
|---|---|---|
| `peinture_blanche_pot` | Peinture blanche (pot) | piece |
| `peinture_couleur_pot` | Peinture couleur (pot) | piece |
| `pinceau` | Pinceau | piece |
| `rouleau_peinture` | Rouleau à peinture | piece |
| `papier_verre` | Papier de verre | piece |
| `diluant_litre` | Diluant | litre |

### `materiaux` — Matériaux de construction · `ti-wall` · `#a16207`

| code | libellé | unité |
|---|---|---|
| `ciment_sac_50kg` | Ciment (sac 50 kg) | piece |
| `fer_a_beton_barre` | Fer à béton (barre) | piece |
| `brique_ciment` | Brique en ciment | piece |
| `tole_ondulee` | Tôle ondulée | piece |

---

## 4. `vetements_chaussures`

### `homme` — Homme · `ti-shirt` · `#1d4ed8`

| code | libellé | unité |
|---|---|---|
| `chemise_homme` | Chemise homme | piece |
| `tee_shirt_homme` | Tee-shirt homme | piece |
| `pantalon_homme` | Pantalon homme | piece |
| `jean_homme` | Jean homme | piece |
| `grand_boubou_homme` | Grand boubou homme | piece |
| `kaftan_homme` | Kaftan homme | piece |

### `femme` — Femme · `ti-dress` · `#db2777`

| code | libellé | unité |
|---|---|---|
| `robe_femme` | Robe femme | piece |
| `chemisier_femme` | Chemisier femme | piece |
| `jupe_femme` | Jupe femme | piece |
| `pantalon_femme` | Pantalon femme | piece |
| `grand_boubou_femme` | Grand boubou femme | piece |
| `taille_basse` | Taille basse | piece |

### `enfant` — Enfant · `ti-mood-kid` · `#f59e0b`

| code | libellé | unité |
|---|---|---|
| `tee_shirt_enfant` | Tee-shirt enfant | piece |
| `robe_enfant` | Robe enfant | piece |
| `pantalon_enfant` | Pantalon enfant | piece |
| `ensemble_bebe` | Ensemble bébé | piece |

### `chaussures` — Chaussures · `ti-shoe` · `#7c2d12`

| code | libellé | unité |
|---|---|---|
| `sandale_homme` | Sandale homme | paire |
| `sandale_femme` | Sandale femme | paire |
| `basket` | Basket | paire |
| `chaussure_ville` | Chaussure de ville | paire |
| `babouche` | Babouche | paire |
| `tong` | Tong | paire |

### `tissus` — Tissus & pagnes · `ti-scissors` · `#059669`

| code | libellé | unité |
|---|---|---|
| `pagne_wax_metre` | Pagne wax | metre |
| `bazin_riche_metre` | Bazin riche | metre |
| `basin_simple_metre` | Basin simple | metre |
| `voile_metre` | Voile | metre |
| `doublure_metre` | Doublure | metre |

### `accessoires_mode` — Accessoires · `ti-bag` · `#9333ea`

| code | libellé | unité |
|---|---|---|
| `sac_a_main` | Sac à main | piece |
| `ceinture` | Ceinture | piece |
| `foulard` | Foulard | piece |
| `casquette` | Casquette | piece |
| `montre` | Montre | piece |

---

## 5. `telephonie_accessoires`

### `telephones` — Téléphones · `ti-device-mobile` · `#1e40af`

| code | libellé | unité |
|---|---|---|
| `smartphone_entree_gamme` | Smartphone entrée de gamme | piece |
| `smartphone_milieu_gamme` | Smartphone milieu de gamme | piece |
| `telephone_touches` | Téléphone à touches | piece |
| `telephone_occasion` | Téléphone d'occasion | piece |

### `accessoires_tel` — Accessoires · `ti-plug` · `#0d9488`

| code | libellé | unité |
|---|---|---|
| `chargeur_secteur` | Chargeur secteur | piece |
| `cable_usb_c` | Câble USB-C | piece |
| `cable_micro_usb` | Câble micro-USB | piece |
| `ecouteurs_filaires` | Écouteurs filaires | piece |
| `ecouteurs_bluetooth` | Écouteurs Bluetooth | piece |
| `batterie_externe` | Batterie externe | piece |
| `coque_telephone` | Coque de téléphone | piece |
| `verre_trempe` | Verre trempé | piece |
| `carte_memoire` | Carte mémoire | piece |
| `support_voiture` | Support voiture | piece |

### `recharges` — Recharges & SIM · `ti-signal-4g` · `#f97316`

| code | libellé | unité | stock |
|---|---|---|---|
| `credit_appel` | Crédit d'appel | prestation | non |
| `forfait_internet` | Forfait internet | prestation | non |
| `carte_sim` | Carte SIM | piece | oui |
| `puce_data` | Puce data | piece | oui |

### `services_tel` — Services · `ti-tool` · `#6366f1`

| code | libellé | unité | stock |
|---|---|---|---|
| `deblocage_telephone` | Déblocage de téléphone | prestation | non |
| `installation_logiciel` | Installation de logiciel | prestation | non |
| `transfert_donnees` | Transfert de données | prestation | non |
| `remplacement_ecran` | Remplacement d'écran | prestation | non |
| `remplacement_batterie` | Remplacement de batterie | prestation | non |

---

## 6. `cosmetique_beaute`

### `soins_cheveux` — Soins cheveux · `ti-wash` · `#be185d`

| code | libellé | unité |
|---|---|---|
| `shampoing_flacon` | Shampoing (flacon) | piece |
| `apres_shampoing` | Après-shampoing | piece |
| `huile_capillaire` | Huile capillaire | piece |
| `defrisant` | Défrisant | piece |
| `meche_synthetique` | Mèche synthétique | piece |
| `perruque` | Perruque | piece |
| `gel_coiffant` | Gel coiffant | piece |

### `soins_corps` — Soins du corps · `ti-droplet-half` · `#c026d3`

| code | libellé | unité |
|---|---|---|
| `lait_corporel` | Lait corporel | piece |
| `beurre_karite` | Beurre de karité | piece |
| `savon_gommant` | Savon gommant | piece |
| `huile_corporelle` | Huile corporelle | piece |
| `deodorant` | Déodorant | piece |

### `maquillage` — Maquillage · `ti-palette` · `#e11d48`

| code | libellé | unité |
|---|---|---|
| `fond_de_teint` | Fond de teint | piece |
| `poudre_compacte` | Poudre compacte | piece |
| `rouge_a_levres` | Rouge à lèvres | piece |
| `mascara` | Mascara | piece |
| `crayon_yeux` | Crayon à yeux | piece |
| `vernis_ongles` | Vernis à ongles | piece |

### `parfums` — Parfums & encens · `ti-flame` · `#7e22ce`

| code | libellé | unité |
|---|---|---|
| `parfum_flacon` | Parfum (flacon) | piece |
| `eau_de_toilette` | Eau de toilette | piece |
| `thiouraye_sachet` | Thiouraye (sachet) | sachet |
| `encens_baton` | Encens (bâton) | piece |

### `prestations_salon` — Prestations salon · `ti-scissors` · `#0f766e`

| code | libellé | unité | stock |
|---|---|---|---|
| `coupe_homme` | Coupe homme | prestation | non |
| `coupe_femme` | Coupe femme | prestation | non |
| `tresses` | Tresses | prestation | non |
| `tissage` | Tissage | prestation | non |
| `defrisage_prestation` | Défrisage | prestation | non |
| `coloration` | Coloration | prestation | non |
| `manucure` | Manucure | prestation | non |
| `pedicure` | Pédicure | prestation | non |
| `maquillage_evenement` | Maquillage événement | prestation | non |
| `soin_visage` | Soin du visage | prestation | non |

---

## 7. `papeterie_librairie`

### `fournitures_scolaires` — Fournitures scolaires · `ti-school` · `#2563eb`

| code | libellé | unité |
|---|---|---|
| `cahier_100p` | Cahier 100 pages | piece |
| `cahier_200p` | Cahier 200 pages | piece |
| `cahier_travaux_pratiques` | Cahier de travaux pratiques | piece |
| `stylo_bille` | Stylo à bille | piece |
| `crayon_papier` | Crayon à papier | piece |
| `gomme` | Gomme | piece |
| `regle_30cm` | Règle 30 cm | piece |
| `trousse` | Trousse | piece |
| `cartable` | Cartable | piece |
| `ardoise` | Ardoise | piece |

### `bureau` — Fournitures de bureau · `ti-briefcase` · `#475569`

| code | libellé | unité |
|---|---|---|
| `rame_papier_a4` | Rame de papier A4 | piece |
| `chemise_cartonnee` | Chemise cartonnée | piece |
| `classeur` | Classeur | piece |
| `agrafeuse` | Agrafeuse | piece |
| `agrafes_boite` | Agrafes (boîte) | boite |
| `perforateur` | Perforateur | piece |
| `ruban_adhesif` | Ruban adhésif | piece |
| `marqueur` | Marqueur | piece |
| `surligneur` | Surligneur | piece |

### `livres` — Livres · `ti-book` · `#b45309`

| code | libellé | unité |
|---|---|---|
| `livre_scolaire` | Livre scolaire | piece |
| `dictionnaire` | Dictionnaire | piece |
| `roman` | Roman | piece |
| `livre_religieux` | Livre religieux | piece |

### `impression` — Impression & reprographie · `ti-printer` · `#0891b2`

| code | libellé | unité | stock |
|---|---|---|---|
| `photocopie_nb` | Photocopie noir & blanc | piece | non |
| `photocopie_couleur` | Photocopie couleur | piece | non |
| `impression_nb` | Impression noir & blanc | piece | non |
| `impression_couleur` | Impression couleur | piece | non |
| `scan_document` | Numérisation de document | piece | non |
| `reliure` | Reliure | prestation | non |
| `plastification` | Plastification | prestation | non |
| `saisie_document` | Saisie de document | heure | non |

---

## 8. `pharmacie_para`

> ⚠️ Ce catalogue ne contient **aucun médicament** : ni dénomination commerciale, ni molécule, ni dosage. Seulement de la parapharmacie et des prestations. Le médicament relève d'une réglementation stricte (liste, ordonnance, traçabilité) et d'un modèle de données propre — hors périmètre de ce seeder. Le pharmacien saisit son stock de médicaments lui-même ou via un import dédié.

### `hygiene_soins` — Hygiène & soins · `ti-first-aid-kit` · `#0d9488`

| code | libellé | unité |
|---|---|---|
| `compresse_sterile` | Compresse stérile | boite |
| `bande_gaze` | Bande de gaze | piece |
| `pansement_adhesif` | Pansement adhésif | boite |
| `coton_hydrophile` | Coton hydrophile | paquet |
| `sparadrap` | Sparadrap | piece |
| `gant_jetable_boite` | Gants jetables (boîte) | boite |
| `masque_chirurgical` | Masque chirurgical | piece |
| `alcool_a_90` | Alcool à 90° | bouteille |
| `gel_hydroalcoolique` | Gel hydroalcoolique | piece |

### `materiel_medical` — Matériel médical · `ti-stethoscope` · `#1d4ed8`

| code | libellé | unité |
|---|---|---|
| `thermometre` | Thermomètre | piece |
| `tensiometre` | Tensiomètre | piece |
| `glucometre` | Glucomètre | piece |
| `bandelette_glycemie` | Bandelettes glycémie | boite |
| `seringue_jetable` | Seringue jetable | piece |
| `bassin_lit` | Bassin de lit | piece |

### `maman_bebe` — Maman & bébé · `ti-baby-carriage` · `#f472b6`

| code | libellé | unité |
|---|---|---|
| `couche_bebe_paquet` | Couches bébé (paquet) | paquet |
| `lingette_bebe` | Lingettes bébé | paquet |
| `biberon` | Biberon | piece |
| `tetine` | Tétine | piece |
| `savon_bebe` | Savon bébé | piece |
| `lait_infantile_boite` | Lait infantile (boîte) | boite |

### `para_soins` — Parapharmacie · `ti-leaf` · `#65a30d`

| code | libellé | unité |
|---|---|---|
| `creme_solaire` | Crème solaire | piece |
| `creme_hydratante` | Crème hydratante | piece |
| `complement_vitamine` | Complément vitaminé | boite |
| `tisane_sachet` | Tisane (sachet) | sachet |
| `spray_nasal_eau_mer` | Spray nasal eau de mer | piece |

### `prestations_officine` — Prestations · `ti-clipboard-heart` · `#dc2626`

| code | libellé | unité | stock |
|---|---|---|---|
| `prise_tension` | Prise de tension | prestation | non |
| `test_glycemie` | Test de glycémie | prestation | non |
| `injection` | Injection | prestation | non |
| `pansement_soin` | Réfection de pansement | prestation | non |

---

## 9. `services_atelier`

> Catalogue transverse pour couture, cordonnerie, réparation, transport. **Aucun article ne gère le stock**, sauf les fournitures.

### `couture` — Couture & retouche · `ti-needle-thread` · `#059669`

| code | libellé | unité | stock |
|---|---|---|---|
| `confection_boubou` | Confection grand boubou | prestation | non |
| `confection_chemise` | Confection chemise | prestation | non |
| `confection_pantalon` | Confection pantalon | prestation | non |
| `confection_robe` | Confection robe | prestation | non |
| `retouche_ourlet` | Retouche ourlet | prestation | non |
| `reparation_fermeture` | Réparation fermeture éclair | prestation | non |
| `broderie` | Broderie | prestation | non |

### `reparation` — Réparation · `ti-tool` · `#57534e`

| code | libellé | unité | stock |
|---|---|---|---|
| `diagnostic` | Diagnostic | prestation | non |
| `reparation_electromenager` | Réparation électroménager | prestation | non |
| `reparation_electronique` | Réparation électronique | prestation | non |
| `soudure` | Soudure | prestation | non |
| `main_oeuvre_heure` | Main-d'œuvre | heure | non |
| `deplacement` | Déplacement | prestation | non |

### `cordonnerie` — Cordonnerie · `ti-shoe` · `#7c2d12`

| code | libellé | unité | stock |
|---|---|---|---|
| `ressemelage` | Ressemelage | paire | non |
| `recollage_chaussure` | Recollage de chaussure | paire | non |
| `teinture_cuir` | Teinture de cuir | paire | non |
| `reparation_sac` | Réparation de sac | prestation | non |

### `fournitures_atelier` — Fournitures · `ti-package` · `#a16207`

| code | libellé | unité | stock |
|---|---|---|---|
| `fil_bobine` | Fil (bobine) | piece | oui |
| `fermeture_eclair` | Fermeture éclair | piece | oui |
| `bouton_lot` | Boutons (lot) | lot | oui |
| `elastique_metre` | Élastique | metre | oui |
| `colle_forte` | Colle forte | piece | oui |

---

## Récapitulatif

| Type | Catégories | Articles |
|---|---|---|
| `alimentation_generale` | 6 | 42 |
| `restaurant_fast_food` | 6 | 30 |
| `quincaillerie` | 6 | 37 |
| `vetements_chaussures` | 6 | 32 |
| `telephonie_accessoires` | 4 | 23 |
| `cosmetique_beaute` | 5 | 32 |
| `papeterie_librairie` | 4 | 31 |
| `pharmacie_para` | 5 | 30 |
| `services_atelier` | 4 | 22 |

**Total avant dédoublonnage : 279 articles.** Après dédoublonnage inter-types (eau, sodas, café, savon…), compter environ **265 codes distincts**, donc autant d'images à produire dans le pool partagé.

---

## Priorisation

Ne pas produire les 265 images d'un coup. Le seeder fonctionne sans image (repli par initiales, §7.5 de la spec) : c'est un enrichissement progressif.

1. **`alimentation_generale`** — la majorité des utilisateurs cibles. Catalogue et images complets.
2. **`restaurant_fast_food`** — second usage le plus courant, et celui où la grille de caisse illustrée apporte le plus (service rapide, personnel tournant).
3. **`quincaillerie`, `telephonie_accessoires`, `cosmetique_beaute`** — catalogues complets, images ensuite.
4. Le reste — libellés seuls dans un premier temps.

Un excellent catalogue « alimentation » vaut mieux que neuf catalogues médiocres. Les types suivants s'affineront quand tu verras ce que les premiers utilisateurs déclarent réellement à l'onboarding — pense d'ailleurs à journaliser ce choix : c'est ta première donnée produit gratuite.
