# Module Gestion de Projet — Djigui ERP

## Contexte
Module additionnel de l'ERP Djigui (Tauri 2 / Rust-axum / Vue.js 3 / SQLite).
Cible : PME sénégalaises. Gestion **par projet** (pas de vue transversale multi-projets pour cette v1 — chaque projet est cloisonné).

---

## ⚠️ INSTRUCTION IMPORTANTE POUR CLAUDE CODE

Les **jalons** (milestones) du projet seront branchés à terme sur l'**agenda général** de l'ERP (le même agenda utilisé par les autres modules).

**Avant d'implémenter ce branchement agenda, tu dois me consulter.**
Implémente d'abord le module Gestion de Projet de façon autonome (jalons stockés localement au projet, sans lien avec l'agenda). Le branchement agenda sera fait dans une étape séparée, après validation avec moi de :
- comment un jalon doit apparaître dans l'agenda (juste une entrée en lecture seule ? un événement modifiable ?)
- ce qui se passe si la date du jalon change côté projet (est-ce que l'agenda se met à jour automatiquement ?)
- ce qui se passe si l'agenda modifie l'événement (est-ce que ça remonte au projet ?)

Ne prends aucune décision de conception sur cette synchronisation sans validation préalable.

---

## Entités

### Projet
- id, nom, client_id (FK vers module commercial), chef_de_projet_id (FK user)
- date_debut_prevue, date_fin_prevue, date_debut_reelle, date_fin_reelle
- statut : planifié / en_cours / suspendu / clôturé
- budget_global

### Jalon (Milestone)
- id, projet_id, nom, date_prevue, date_reelle (nullable)
- statut : à_venir / atteint / manqué
- **Pas de lien agenda pour l'instant** (voir instruction ci-dessus)

### Tâche
- id, projet_id, tache_parente_id (nullable, 1 seul niveau de hiérarchie)
- nom, description
- date_debut_prevue, date_fin_prevue, date_debut_reelle, date_fin_reelle
- statut : à_faire / en_cours / bloquée / terminée
- pourcentage_avancement (saisie manuelle, pas de calcul automatique complexe)

### Dépendance
- tache_id, tache_predecesseur_id
- type : fin_debut uniquement pour cette v1 (le plus courant, suffisant en pratique)

### Assignation (Tâche ↔ Utilisateur)
- tache_id, user_id, heures_allouees

### Utilisateur (Ressource humaine)
- Existe déjà dans l'ERP (table users). Ajouter :
- taux_horaire ou taux_journalier (nullable, optionnel selon si la PME facture le temps)

### Ressource matérielle/financière
- id, projet_id, tache_id (nullable, peut être au niveau projet ou tâche)
- type : matériel / budget / sous-traitance
- cout_unitaire, quantite

### Action (journal d'activité)
- id, tache_id, user_id, date, description
- Sert de log concret de ce qui a été fait, sans multiplier les tâches

---

## Vues calculées (dérivées, pas stockées sauf besoin de perf)

### Gantt
Généré à partir des dates + dépendances des tâches du projet.

### Retard
- Par tâche : écart entre date_fin_prevue et date_fin_reelle (ou date du jour si pas encore terminée)
- Par projet : % de tâches en retard + retard cumulé en jours

### Coût
- Coût réel = Σ (ressources.cout_unitaire × quantite) + Σ (heures_allouees × taux_horaire des users assignés)
- Comparé au budget_global du projet
- Indicateur d'écart (positif/négatif, %)

---

## Point de conception à trancher avant l'implémentation du calcul de retard en cascade

Si une tâche prédécesseur prend du retard, deux options :
1. **Propagation automatique** : le système recalcule les dates prévues des tâches dépendantes (vrai moteur de planification, comme MS Project)
2. **Signalement seul** : le système affiche une alerte d'incohérence mais ne touche pas aux dates

→ **Ne pas trancher seul, me consulter avant d'implémenter cette logique.**

---

## Ce qui est explicitement hors scope pour cette v1
- Vue transversale multi-projets (allocation de ressources entre plusieurs projets simultanés)
- Dépendances autres que fin→début (début→début, fin→fin, etc.)
- Synchronisation bidirectionnelle temps réel avec l'agenda (voir instruction en haut du document)
