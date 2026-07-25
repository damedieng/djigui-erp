-- ---------------------------------------------------------------------------
-- Utilisateurs & authentification (§ gestion des accès)
-- ---------------------------------------------------------------------------
-- Chaque utilisateur se connecte avec un login + mot de passe (stocké HACHÉ,
-- jamais en clair). Deux rôles : 'admin' (tout) et 'caissier' (caisse/ventes).
-- L'utilisateur par défaut « djigui / djigui » est créé côté Rust après la
-- migration (le hachage se fait en Rust), voir utilisateur::assurer_defaut.
CREATE TABLE utilisateur (
    id                TEXT PRIMARY KEY,
    login             TEXT NOT NULL UNIQUE,
    mot_de_passe_hash TEXT NOT NULL,
    nom               TEXT NOT NULL,
    role              TEXT NOT NULL DEFAULT 'caissier'
                          CHECK (role IN ('admin','caissier')),
    actif             INTEGER NOT NULL DEFAULT 1,
    cree_le           TEXT NOT NULL
);
