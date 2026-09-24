# 07 — Stratégie de test

Base : `main` = `16c9aa7ea6`. Fixtures : `fixtures/` (décrites en §5). Aucune installation n'a été faite sur cette machine
(décision : rien installer sans accord) ; les tests e2e ci-dessous sont **planifiés, non exécutés**.

## 1. Niveaux

| Niveau | Outil | Ce qu'il prouve | Existant réutilisé |
|---|---|---|---|
| N1 Unitaire pur | `#[test]` | fonctions `*_args` pures (argv docker par variante), `devcontainerId`, normalisation des labels, parsing `wslc list --format json` | tests `devcontainer_json.rs`, `command_json.rs` |
| N2 Manifest avec faux moteur | `#[gpui::test]` + `FakeDocker` (`devcontainer_manifest.rs:7308-7339`) + `TestCommandRunner` (`:7708-7726`) + `FakeFs` | ordre des étapes, hooks, marqueurs, `initializeCommand`, UID selon plateforme d'hôte | 122 tests `dev_container` actuels |
| N3 Hôte distant simulé | faux `RemoteConnection` (base : `MockRemoteConnection`, `remote/src/transport/mock.rs:187` ; modèle : `FakeRemoteConnection` de #62680) qui **enregistre** les `CommandTemplate` et simule le quoting SSH/WSL | K1–K4 : argv préservé à travers `build_command`, `HostFiles` (stdin), `BuildDir` nettoyé, connexion via l'hôte | — |
| N4 Persistance | tests `workspace` (`cargo test -p workspace persistence`, 51 tests aujourd'hui) | migrations append-only, lookup par nouvelle clé + repli, UPDATE runtime, pas de secret | tests de #60975 (identité) |
| N5 Contrat CLI | sorties enregistrées (`fixtures/wslc-2.9.12/`, à compléter par `docker`/`podman`) | options réellement disponibles par variante ; détection `-v` | — |
| N6 E2E manuel/CI | matrice §3 | bout en bout par topologie | — |

## 2. Tests obligatoires par PR (`06-pr-stack.md`)

| PR | Tests exigés (niveau) |
|---|---|
| A1 | N1 : argv de `run_docker_exec` pour chaîne/tableau/objet = attendus de `fixtures/lifecycle-forms/EXPECTED.md` ; N2 : régression #62964 |
| A2 | N2 : conteneur existant ⇒ `initializeCommand` exécuté ; `exit 3` ⇒ erreur ; plateforme Windows ⇒ `cmd /c` |
| A3 | N2 : ordre hooks image → features → json ; marqueurs (recréé vs redémarré) ; forme objet : tous exécutés, échec ⇒ erreur |
| A4 | N1 : `fixtures/devcontainer-id/vectors.json` (3 vecteurs **recoupés avec le code du CLI de référence sous Node : MATCH ×3**) |
| A5 | N1 : commande de build sans buildx ; N2 : image temporaire de features |
| A6 | N1 : options Podman selon plateforme d'hôte ; arrêt du proxy sans `kill` externe |
| A7 | N1 : tri topologique, cycles ; options tableau/nombre ; N6 : `fixtures/features-order` (après vérification CLI réf.) |
| B1 | N4 : rebuild ⇒ même `RemoteConnectionId` ; `""` = absent ; migration ; N2 : threads retrouvés (#56576) |
| C1 | **N1/N2 : égalité d'argv avant/après** — capturer l'argv de *chaque* site (§4) sur `main`, puis exiger l'identité après refactor |
| C2 | N3 : `cat`/`mktemp -d`/`rm -rf <BuildDir>` ; contenu par stdin (absent de l'argv) ; nettoyage même en cas d'échec |
| C3 | N2 paramétré : client {Linux, Windows} × hôte {Linux, Windows} ⇒ UID, labels, shell |
| C4 | N3 : proxy, upload par flux, terminal, `serde(default)` ⇒ anciennes options = `Local` |
| C5 | N4 : deux hôtes, même chemin ⇒ identités distinctes ; aucune donnée sensible dans la clé |
| C6/C7 | N3 + N6 (T4, T2/T3) |
| C8 | N1 : chaque motif de refus |
| D1/D2 | N5 : détection par `-v` (`fixtures/wslc-2.9.12/version.txt`), options filtrées ; parsing `list --format json` |
| A8 (en suspens) | N4 + N6 : `fixtures/env-secrets` — la valeur du secret absente de la DB et des logs |

## 3. Matrice e2e (N6)

| Topologie | Fixture(s) | Prérequis sur cette machine | Ailleurs |
|---|---|---|---|
| T1a Linux | toutes | — | CI Linux (Docker) |
| T1c Windows natif | lifecycle-forms, initialize-command, compose-basic | Podman machine **ou** Docker Desktop (non installés) | CI Windows : **à vérifier** si Docker est disponible sur les runners Windows de Zed |
| T2 WSL + Docker Desktop | toutes | Docker Desktop (non installé) | — |
| T3 WSL + Docker CE | toutes | **installé le 2026-09-24** : paquets Ubuntu `docker.io` 29.1.3, `docker-buildx` 0.30.1, `docker-compose-v2` 2.40.3 dans `Ubuntu` 26.04 ; `robot` dans le groupe `docker` | — |
| T4 SSH | toutes | Docker installé dans `Ubuntu` ; sshd et SSH vers `localhost` non configurés | VM Linux distante |
| T7 Podman | lifecycle-forms, compose-basic | Podman machine (non créée) | CI Linux rootless |
| T8 WSLc | lifecycle-forms, initialize-command (pas compose, pas features) | **disponible** (`wslc` 2.9.12) — exécuter des conteneurs demande ton accord | — |

## 4. Capture d'argv de référence (préalable à C1)

Révisé après la revue adverse (`08` F-2) : `util::command::Command` n'expose que `get_program`/`get_args`
(`util/src/command.rs:59, 125`), **ni env ni cwd**, et plusieurs sites contournent `CommandRunner` (`id -u/-g`,
`command_json.rs:35, 52`, `check_for_docker`, `Docker::new`). Méthode en deux temps :

1. **PR préparatoire (dans C1)** : introduire `HostCommand { program, args, env, cwd, stdin }` et faire passer **tous** les
   sites de `01` §2 par une fonction unique `EngineHost::command(&HostCommand)` ; en test, un enregistreur capture le
   `HostCommand` complet **avant** conversion.
2. Goldens `*.golden` (dans le crate, pas dans ce dossier) produits sur les fixtures `lifecycle-forms`, `initialize-command`,
   `compose-basic`, puis comparés après chaque PR de la piste C. Pour la variante `Local`, un test vérifie en plus que la
   conversion `HostCommand → Command` conserve `get_program`/`get_args`.

Limite assumée : l'égalité « avant/après » ne peut pas être prouvée contre `main` pour env/cwd (non observables) ; elle l'est
à partir de la PR préparatoire.

## 5. Fixtures fournies

| Dossier | Couvre | Statut |
|---|---|---|
| `fixtures/lifecycle-forms/` | L1, L4, L6, L7 (formes chaîne/tableau/objet, espaces, `$HOME`) | prêt |
| `fixtures/features-order/` | F1, L5 (`installsAfter`, `dependsOn`, hooks de features) | ⚠️ à valider contre le CLI de référence (features locales) |
| `fixtures/initialize-command/` | L2 (à chaque ouverture, échec fatal, shell selon l'hôte) | prêt |
| `fixtures/env-secrets/` | E2 / ADR-008 | prêt — **mise en œuvre en suspens** |
| `fixtures/compose-basic/` | C1–C3, refus compose+wslc | prêt |
| `fixtures/devcontainer-id/vectors.json` | V1 / A4 | prêt, recoupé avec le CLI (MATCH ×3) |
| `fixtures/wslc-2.9.12/` | N5 : `--help` de `wslc` 2.9.12 (lecture seule) | prêt — sorties **en français** (locale système) : parser les options, pas les descriptions ; ré-enregistrer en `en-US` si possible |

## 6. Hygiène de build (leçon de la Phase 2)

- Un `CARGO_TARGET_DIR` **par worktree** : un target partagé entre branches a produit une fausse erreur E0277
  (`02-overlap-matrix.md` §1).
- cmake requis sous Windows (`C:\Program Files\CMake\bin` hors `PATH` par défaut ici).
- Les conflits **sémantiques** échappent à `git merge-tree` (cas #60975 × #63606) : toujours compiler **les tests** de chaque
  crate touché (`cargo test -p <crate> --no-run`), pas seulement `cargo check`.
- Exécuter les tests `dev_container` sous Windows **et** Linux : le test rouge de #62680 n'apparaît que sous Windows.
