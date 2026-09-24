# Plan d'ingénierie — support Dev Containers robuste (Zed)

Dossier de travail local (branche `plan/devcontainers`), **hors de l'arbre mdBook publié** (`docs/book.toml` → `src = "src"`).
Plan rédigé sur `main` = `16c9aa7ea6` (2026-09-22) ; branches rebasées sur `a405fb91d5` et poussées sur le fork
personnel `breakingrobot/zed` (aucune PR ouverte, rien posté sur `zed-industries/zed`).

## Résumé exécutif

1. **Aujourd'hui**, le support Dev Containers de Zed suppose que client, CLI docker et démon sont sur la même machine
   (30 lancements de processus locaux, aucun point d'exécution commun) et **refuse** les projets distants (`01`).
2. **Il a déjà des défauts sur `main`**, indépendants du distant (`03`) :
   - commandes lifecycle en chaîne cassées (régression v1.15.0, #62964) ;
   - `initializeCommand` sauté ou ignoré ;
   - hooks des features jamais exécutés ;
   - `devcontainerId` non conforme.
3. **Sécurité** : trois points sensibles vérifiés. Leur traitement est **en suspens** sur ta décision :
   - l'env complet du conteneur est persisté en clair (base `remote_connections` + tables `sidebar_*`) ;
   - un dossier `/tmp` prévisible suit les liens symboliques (Linux multi-utilisateur) ;
   - aucune porte Workspace Trust avant d'exécuter du contenu du dépôt.

   Le dossier `/tmp` est corrigé par A8a ; les deux autres points restent en suspens.
4. **Contributions existantes** (#60975, #62680, deux branches de pupeno) : utiles mais incompatibles entre elles (3 à 16 fichiers
   en conflit par paire) et largement générées par IA, sans aucune revue de mainteneur ; la tête de #62680 ne compile pas (`02`).
5. **Cible** (`05`, ADR-001…011) :
   - déplacer le CLI vers l'hôte des sources ;
   - une abstraction unique `EngineHost` au-dessus de `RemoteConnection::build_command` ;
   - la connexion passe par le même hôte ;
   - identité stable qualifiée par l'hôte ;
   - variantes docker/podman/wslc ;
   - porte Workspace Trust.
6. **Topologies** (`04`) :
   - P0 : local ;
   - P1 : SSH puis WSL ;
   - P2 : WSLc expérimental (préversion, pas d'API Docker, CLI `wslc` adaptable) ;
   - P3 : Zed dans un conteneur ;
   - refus explicites : hôte SSH Windows, `DOCKER_HOST` distant avec sources locales, dev container depuis un dev container.
7. **Livraison** (`06`) : engagement initial réduit (proposition dans la discussion #56252 + corrections indépendantes + soutien
   des PRs existantes), puis une pile de ~25 petites PRs en vagues de 3, conditionnée à une réponse du staff.
8. **Tests** (`07`) :
   - fixtures prêtes ;
   - vecteurs `devcontainerId` recoupés avec le code du CLI de référence ;
   - contrats `wslc` enregistrés ;
   - CLI de référence : 4/4 fixtures conformes ; e2e Zed : voir `07` §7.
9. **Revue adverse** (`08`) : 46 références relues (37 ✅ · 7 ⚠️ · 2 ❌) ; les 10 corrections prioritaires ont été revérifiées puis appliquées.

## Décisions et hypothèses

| Sujet | Statut |
|---|---|
| Priorités P0→P3, refus T4w/T5/T6b | ✅ **validé** (2026-09-24) |
| WSL : connexion via la distro | ✅ **validé** (2026-09-24) |
| WSLc : adaptateur CLI, sans compose ni features en v1, derrière un réglage expérimental | ✅ **validé** (2026-09-24) |
| Sécurité : dossier `/tmp` prévisible | ✅ **validé** → A8a faite (branche `devcontainers/a8-unique-build-dir`) |
| Sécurité : env persisté en clair | toujours en suspens (hors décision du 2026-09-24) |
| Installations pour les tests réels | ✅ **validé** → Docker Engine 29.1.3 + buildx 0.30.1 + compose 2.40.3 dans la distro Ubuntu 26.04 (WSL) |
| Publication de la proposition C0 | ❌ **refusé pour l'instant** : brouillon conservé, non posté |

## Implémentation

État branche par branche : `06-pr-stack.md` §3bis. Résultats de tests : `07-test-strategy.md` §7. Guide de test manuel :
`09-manual-test-guide.md`.

Toutes les branches sont empilées ; la dernière, **`devcontainers/e1-lifecycle-actions`**, contient tout :

| Étape | Branches | Apport |
|---|---|---|
| Correctifs | `integration` (A2–A8a, B1) | `initializeCommand`, hooks des features, `devcontainerId`, build sans BuildKit, Podman, arrêt du proxy, ordre des features, dossier de build privé, identité stable |
| WSL | `c1-engine-host` → `c4-connection-host` → `c7-wsl-projects` → `c7b-wsl-followups` | moteur dans la distro via `wsl.exe --exec`, fichiers via `\\wsl.localhost`, hôte persisté, suggestion, environnement de login, correctif `zed --dev-container` |
| SSH | `c6-ssh-projects` → `e2-ssh-port-forwarding` | moteur sur l'hôte via `ssh` (`BatchMode`), fichiers lus par commandes, dossier de build copié par tar, envoi par `docker exec -i`, tunnels `-L` pour les ports |
| Robustesse | `t1-workspace-trust` → `c2b-host-environment` → `c8-unsupported-setups` | porte de confiance, `PATH`/`DOCKER_HOST` de login, refus explicites |
| Cycle de vie | `e1-lifecycle-actions` | reconnect / restart / rebuild / stop / delete (commits d'Alex Berger), passage par l'hôte |

Installeurs locaux (Zed Dev, non signés) : `H:\Sources\zed-wt\artifacts\` (`…-c7.exe` WSL, `…-c6.exe` WSL + SSH,
`…-final.exe` tout).

Reste : e2e réels WSL / SSH / Podman (guide `09`), A8b (gelé par décision), D1/D2 (WSLc), reprise de la barre latérale et
de la vue agent de #60975, SSH sans ControlMaster sous Windows (une connexion par commande).

## Documents

| Phase | Livrable | État |
|---|---|---|
| 0 — Préparation | `raw/` (données GitHub : API publique puis `gh` authentifié ; `raw/INDEX.md` ; logs `raw/build-logs/`) | ✅ |
| 1 — Architecture actuelle | `01-current-architecture.md` | ✅ (révisé après 08) |
| 2 — Contributions | `02-contrib-*.md`, `02-overlap-matrix.md` | ✅ |
| 🛑 Point d'arrêt 1 | | ✅ validé |
| 3 — Conformité spec | `03-spec-reference.md`, `03-spec-compliance.md` | ✅ (révisé après 08) |
| 4 — Topologies | `04-reference-topologies.md`, `04-wslc.md`, `04-topologies.md` | ✅ |
| 🛑 Point d'arrêt 2 | | ✅ (sécurité : en suspens) |
| 5 — Architecture cible | `05-target-architecture.md`, `adr/ADR-001…011` | ✅ (révisé après 08) |
| 6 — Pile de PRs | `06-pr-stack.md` (+ brouillon C0 non posté) | ✅ (réécrit après 08) |
| 7 — Tests | `07-test-strategy.md`, `fixtures/` | ✅ (révisé après 08) |
| 8 — Revue adverse | `08-adversarial-review.md` (+ §8 suite donnée) | ✅ |
| 🛑 Point d'arrêt final | | ✅ validé (2026-09-24), C0 non publié |
| 9 — Guide de test manuel | `09-manual-test-guide.md` | ✅ |

## Sources analysées (branches locales, lecture seule)

| Branche locale | Origine | Rapport |
|---|---|---|
| `review/pr-60975-lifecycle` | PR #60975 (alex-berger) | `02-contrib-pr-60975.md` |
| `review/pr-62680-remote` | PR #62680 (alexdhill) | `02-contrib-pr-62680.md` |
| `review/pupeno-wsl` | pupeno/zed `wsl-devcontainers` | `02-contrib-pupeno-wsl.md` |
| `review/pupeno-remote` | pupeno/zed `remote-devcontainer` | `02-contrib-pupeno-remote.md` |

Les worktrees d'essai `H:\Sources\zed-wt\<nom>` et les branches `trial/*` citées dans les rapports ont été supprimées après
analyse ; les logs sont dans `raw/build-logs/`. Les caches de build `H:\Sources\zed-wt\target*` et le clone
`%TEMP%\dccli` (spec + CLI de référence) sont conservés et peuvent être supprimés.

## Conventions

- Chaque affirmation est sourcée : `chemin:ligne` (sur `main` sauf mention), SHA, ou fichier de `raw/`.
- Hypothèses : ✅ confirmée · ❌ infirmée · ⚠️ partielle ; « non vérifié » quand non lu directement.
- Contraintes amont (`CONTRIBUTING.md`) :
  - fonctionnalités proposées d'abord en **Discussion** ;
  - une seule chose par PR, tests obligatoires ;
  - **3 PRs ouvertes max par auteur** ;
  - pas de « giant refactorings » ;
  - politique IA : un humain comprend le code, et les messages aux mainteneurs sont écrits par un humain.
