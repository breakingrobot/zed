# 05 — Architecture cible

Base : `main` = `16c9aa7ea6`. Entrées : `01` (existant), `02-overlap-matrix.md` (contributions), `03-spec-compliance.md`
(écarts), `04-topologies.md` (capacités K1–K10). Décisions détaillées : `adr/ADR-001` … `ADR-010`.

**Hypothèses de travail (point d'arrêt 2, non validées explicitement — à confirmer) :**
H-a priorités P0 local → P1 SSH puis WSL → P2 WSLc expérimental → P3 Zed-dans-conteneur ; refus explicite de T4w, T5, T6b.
H-b pour WSL, la connexion passe par la distro (pas de garde « même moteur »).
H-c WSLc via le CLI `wslc`, première version sans compose ni `--build-context`.
Sécurité (`03` E2) : **en suspens sur décision utilisateur** — l'architecture prévoit le correctif (ADR-008), mais aucun signalement ni PR n'est planifié tant que ce n'est pas décidé.

**Révisions après la revue adverse (`08`)** : ajout d'ADR-011 (Workspace Trust, porte avant toute exécution) ; ADR-002
(goldens sur `HostCommand`, sonde d'environnement de l'hôte en shell de login, coût SSH sans ControlMaster sous Windows) ;
ADR-004 (type d'hôte SSH **sans** `password`, reconnexion, arrêt du proxy distant) ; ADR-006 (`consistency`, pas de features
en Wslc v1, `ContainerCli` dans le crate `remote`) ; ADR-008 (tables `sidebar_*`, gabarits non résolus, `--env-file`,
`BuildDir` 0700 unique) ; ADR-009 (pas de blocage sur `npipe`, T6a non bloquant). Le §4 ci-dessous intègre ces révisions.

## 1. Principes

1. **Trois lieux explicites** : client, hôte moteur (= hôte CLI = hôte démon), conteneur. On **déplace le CLI** vers l'hôte
   des sources ; on ne traduit pas de chemins (sauf T6a, plus tard) — ADR-001.
2. **Une seule abstraction d'exécution**, qui délègue à `RemoteConnection::build_command` : aucun nouveau transport,
   aucun quoting maison ; en local, argv **strictement identique** à aujourd'hui — ADR-002.
3. **Même hôte pour la création et la connexion** ; l'hôte est persisté avec la connexion — ADR-004.
4. **Décisions selon la plateforme de l'hôte**, jamais selon `cfg(windows)` du client — ADR-007.
5. **Zéro comportement implicite dangereux** : pas de secret persisté, redaction sur tous les chemins, refus explicites — ADR-008, ADR-009.
6. **Petites PRs, comportement local inchangé à chaque étape** (contrainte `CONTRIBUTING.md`).

## 2. Vue d'ensemble

```mermaid
flowchart LR
  subgraph Client["Client Zed"]
    UI[recent_projects<br/>OpenDevContainer / picker / lifecycle]
    DC[dev_container<br/>manifest + lifecycle runner]
    EH[EngineHost<br/>Local | Remote(conn)]
    CLI[ContainerCli<br/>Docker | Podman | Wslc]
    HF[HostFiles<br/>read / write / temp dir]
    RC[remote::DockerExecConnection<br/>host: DockerHost]
  end
  subgraph Host["Hôte moteur (local, distro WSL, hôte SSH)"]
    BIN[docker / podman / wslc]
    TMP[(dossier de build temporaire<br/>1 par build, nettoyé)]
    SRC[(sources + .devcontainer)]
  end
  subgraph Ctr["Conteneur"]
    SRV[zed-remote-server proxy]
    HOOKS[hooks lifecycle]
  end
  UI --> DC
  DC --> CLI --> EH
  DC --> HF --> EH
  EH -- "Local: Command direct<br/>Remote: RemoteConnection::build_command" --> BIN
  HF -.-> TMP
  BIN -- "run / exec" --> Ctr
  RC -- "même EngineHost<br/>docker exec -i … proxy" --> SRV
  BIN -. bind-mount .-> SRC
```

## 3. Composants

| Composant | Crate / fichier cible | Responsabilité | Origine | ADR |
|---|---|---|---|---|
| `EngineHost` | `dev_container/src/engine_host.rs` (nouveau, 1 fichier) | `enum { Local, Remote(Arc<dyn RemoteConnection>) }` ; `command(HostCommand) -> Result<util::command::Command>` ; `platform()`, `path_style()` | fusion `DevContainerHost` (#62680) / `ProjectCommandBuilder` (pupeno-wsl) | 002 |
| `HostCommand` | idem | builder `program, args, env, current_dir, stdin` ; aucune chaîne shell | `ProjectCommand` + `current_dir` (#62680) | 002 |
| `HostFiles` | idem | `read_to_string(path)`, `write(path, bytes)` (stdin), `create_build_dir() -> BuildDir` (supprimé au `Drop`/fin), `exists` ; local = `Fs` ; distant = commandes POSIX (`cat`, `mkdir -p`, `mktemp -d`, `rm -rf` borné au dossier créé) | `ProjectHost` (pupeno-remote), réduit | 003 |
| Chemins hôte | `util::paths::RemotePathBuf` **étendu** (`join`, `parent`, `file_name`, `is_absolute`) | chemins typés par `PathStyle` de l'hôte | `HostPathBuf` (pupeno-remote), sans nouveau type | 003 |
| `ContainerCli` | crate `remote` (révisé : la connexion en a besoin — `cp` vs `container cp`), utilisé par `dev_container/src/docker.rs` | `enum { Docker, Podman, Wslc }` détecté par `<cli> -v` (surchargé par réglage) ; filtre/traduit les options par variante ; remplace le booléen `use_podman` (réglage conservé comme alias) | `CLIVariant` du CLI de référence | 006 |
| `Docker` (client) | `dev_container/src/docker.rs` | **toutes** les commandes moteur passent par `ContainerCli` + `EngineHost` (y compris celles que le manifest fabrique aujourd'hui via `docker_cli()`) ; fonctions `*_args` pures testables | `Docker::run` + `*_args` (#62680) | 002, 006 |
| Lifecycle runner | `dev_container/src/devcontainer_manifest.rs` (extraction éventuelle `lifecycle.rs`) | argv préservés ; marqueurs spec ; hooks features/metadata ; forme objet ; `initializeCommand` sur l'hôte à chaque ouverture | #63034, spec | 010 |
| `DockerHost` | `remote/src/transport/docker.rs` | `enum { Local, Ssh(SshConnectionOptions), Wsl(WslConnectionOptions) }` plat, `#[serde(default)]` dans `DockerConnectionOptions` | #62680 | 004 |
| `DockerExecConnection` | idem | toutes ses commandes via `docker_command()` → hôte ; upload serveur par flux `exec -i` ; redaction conservée | #62680 + #63606 | 004, 008 |
| Identité | `remote/src/remote_identity.rs`, `workspace/src/persistence.rs` | clé `(engine_host_key, project_root, config_file, remote_user)` ; `container_id`/`name` runtime | #60975 + #62680 | 005 |
| Garde de topologie | `dev_container/src/lib.rs` `unsupported_reason(project)` | point d'entrée unique des refus, message actionnable | pupeno-wsl | 009 |

## 4. Flux cible (ouverture)

0. **Porte Workspace Trust** (ADR-011) : rien d'exécutable tant que le worktree (local ou distant) n'est pas de confiance.
1. `unsupported_reason(project)` : collab → refus ; projet déjà en conteneur → refus (T6b) ; hôte SSH Windows → refus (T4w) ;
   endpoint **distant** (`ssh://`, `tcp://` non-loopback) en local → avertissement bloquant (T5) — `npipe://`/`unix://` locaux
   acceptés ; Zed dans un conteneur → avertissement non bloquant (T6a) ; sinon `EngineHost` = connexion du projet.
2. Sonde d'environnement de l'hôte (shell de login, cache : `PATH`, `DOCKER_HOST`) puis `ContainerCli::detect(engine_host)`
   (`<cli> -v`, cache par hôte).
3. Lecture de devcontainer.json / Dockerfile / compose via `HostFiles` (local : `Fs` ; distant : `cat`).
4. `initializeCommand` via `EngineHost` (hôte POSIX : `/bin/sh -c` ; hôte Windows : `cmd /c`), **à chaque ouverture**, échec fatal.
5. Recherche du conteneur par labels (chemins hôte, normalisés selon la plateforme **de l'hôte**).
6. Si build : `BuildDir` sur l'hôte ; features OCI téléchargées par le client puis écrites dans `BuildDir` (`HostFiles::write`) ;
   `build` via `ContainerCli` (repli sans BuildKit) ; `id -u/-g` via `EngineHost` si hôte Linux ; `BuildDir` supprimé.
7. `run` / `compose up` via `ContainerCli` ; hooks conteneur (marqueurs, features, argv).
8. `DevContainerConnection { …, host: DockerHost }` → `DockerConnectionOptions { host }` → pool → `DockerExecConnection`
   qui exécute `docker exec -i … proxy` **sur l'hôte**.
9. Persistance : identité stable ; seulement les **gabarits** `remoteEnv` non résolus (ni l'env du conteneur, ni les valeurs
   résolues), y compris dans `sidebar_threads`/`sidebar_terminal_threads` ; env transmis aux `docker exec` par `--env-file`
   0600 (ADR-008, en suspens).

## 5. Correspondance topologies ↔ composants

| Topologie | `EngineHost` | `ContainerCli` | `DockerHost` | Spécifique |
|---|---|---|---|---|
| T1a/b/c, T7 | Local | Docker/Podman (options Linux seulement si hôte Linux) | Local | `cmd /c` sur Windows ; `kill` portable |
| T2, T3 | Remote(WSL) | Docker/Podman dans la distro | Wsl | message si intégration Docker Desktop absente et pas de moteur dans la distro |
| T4 | Remote(SSH) | Docker/Podman sur l'hôte | Ssh | hôte POSIX requis |
| T8 | Local (Windows) | **Wslc** | Local | `-v` au lieu de `--mount` ; `list --format json` ; `container cp` ; refus compose ; features sans `--build-context` (copie dans le contexte) |
| T6a (P3) | Local | Docker | Local | traduction `local_workspace_folder` → source hôte par `inspect` du conteneur courant (ADR-009 §futur) |
| T4w, T5, T6b | — | — | — | refus explicite (ADR-009) |

## 6. Compatibilité et migration

| Élément | Stratégie |
|---|---|
| `DockerConnectionOptions` sérialisé (settings, DB) | nouveaux champs `#[serde(default)]` (`host = Local`, origine `None`) → anciennes entrées = local |
| Table `remote_connections` | migrations **append-only** (colonnes `docker_host`, `project_root`, `config_file`) ; lookup par nouvelle clé puis repli sur l'ancienne ; mise à jour des champs runtime (`container_id`, `name`) |
| Colonne `remote_env` existante | cesser d'y écrire l'env complet ; purge des valeurs existantes à la migration (ADR-008) — **en suspens** avec la décision sécurité |
| Réglage `remote.use_podman` | conservé ; mappé sur `ContainerCli::Podman` ; nouveau réglage `remote.dev_container_cli` (`auto`/`docker`/`podman`/`wslc`) |
| `devcontainerId` | nouvel algorithme conforme ; les volumes nommés avec l'ancien id ne sont plus réutilisés → mention dans les notes de version (ADR-010) |
| Télémétrie `connection_type` | `docker`/`podman` inchangés en local ; nouvelles valeurs (`docker-ssh`…) **à valider avec l'équipe** |

## 7. Risques

| Risque | Mitigation |
|---|---|
| Taille/refus de revue (« giant refactorings ») | pile de petites PRs, chacune sans changement de comportement local (06) |
| Régression locale en introduisant `EngineHost` | PR dédiée « no-op » avec test d'égalité d'argv avant/après sur tous les sites (07) |
| Divergence avec les auteurs actifs (alexdhill, pupeno, alex-berger) | message de coordination proposant **ce** modèle avant d'écrire du code (06) |
| Hôtes non POSIX / shells exotiques (fish, nu) côté WSL/SSH | quoting délégué à `ShellKind` du transport (`remote/src/transport/wsl.rs:545-563`, `ssh.rs:1909-1928`) ; tests avec faux transport |
| WSLc en préversion (surface qui bouge) | derrière un réglage expérimental ; tests de contrat sur les sorties `--help`/JSON enregistrées (fixtures) |
| Nettoyage du `BuildDir` distant interrompu | nom préfixé `zed-devcontainer-build-<uuid>` ; nettoyage opportuniste des dossiers de plus de 24 h au build suivant |
