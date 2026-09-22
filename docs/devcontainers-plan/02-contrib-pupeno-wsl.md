# 02 — Contribution : pupeno/zed `wsl-devcontainers`

- Source : branche locale `review/pupeno-wsl` (tête `245901b02a`), fork https://github.com/pupeno/zed/tree/wsl-devcontainers.
  **Aucune PR** ouverte pour cette branche. Annoncée dans #59500 (https://github.com/zed-industries/zed/issues/59500#issuecomment-5302708928)
  avec l'avertissement de l'auteur : « fully vibe coded, I don't know Rust, and I haven't read the code ».
- Merge-base avec `main` : `bc538def45` (2026-08-15), 566 commits de retard. Worktree d'essai : `H:\Sources\zed-wt\pupeno-wsl` (`trial/pupeno-wsl`).
- Indépendante de `review/pupeno-remote` : `652cb44b40` n'est contenu que dans `review/pupeno-wsl` (`git branch --contains`).
- Les `fichier:ligne` renvoient à `review/pupeno-wsl` sauf mention. Analyse faite par moi (l'agent dédié a été interrompu).

## Résumé

1. Rend « Ouvrir dans un Dev Container » possible depuis un **projet WSL** : les commandes docker de **création**
   sont lancées **dans la distro** via `RemoteConnection::build_command`, tandis que la **connexion** (transport
   `DockerExecConnection`) reste sur le CLI docker **local** de Windows.
2. Ce découpage n'est valable que si les deux CLI pilotent **le même moteur** ; la branche le vérifie en comparant
   `docker info --format {{.ID}}` des deux côtés (Docker Desktop + intégration WSL) et refuse sinon.
3. Deux abstractions réutilisables : `ProjectCommandBuilder` (miroir exact de `build_command`) et `ContainerEngine`
   (commande + style de chemin + projection du FS de la distro en UNC `\\wsl.localhost\…`).
4. Points non traités : `initializeCommand` toujours local (cwd Linux inexistant sous Windows), `join(" ")` non quoté
   des hooks conservé, SSH/WSLc/imbriqué non couverts, dossiers temporaires distants jamais nettoyés, 3 tests seulement.
5. Fusion avec `main` : 2 conflits textuels (`git merge-tree`) ; build et tests : voir §Essai. Verdict : **reprendre l'idée
   `ProjectCommandBuilder`** (généralisée), **remplacer** le garde « même moteur » par un vrai transport vers l'hôte.

## Commits & périmètre

10 commits linéaires (aucun merge), tous datés du 2026-08-15/16 :

| SHA | Sujet |
|---|---|
| `652cb44b40` | Support Dev Containers in WSL projects (commit principal) |
| `8d69fb1983` | Better gates for deciding on open devcontainer |
| `c8366cf747`, `44042c4998`, `59eb068153`, `764dc86b98` | refactorings / formatage |
| `19b821c815` | Clarify Container Engine Context Naming |
| `96e209a082` | Unify local and remote command construction (`ProjectCommandBuilder`) |
| `9ef99a5665` | Handle temporary directories correctly (`mktemp -d` côté hôte) |
| `245901b02a` | Fix bug of attempting to open devcontainer in devcontainer |

`git diff --stat bc538def45 review/pupeno-wsl` : 15 fichiers, +1068/−342. Gros morceaux : `devcontainer_manifest.rs` (+/−681),
`project_command.rs` (+188, nouveau), `container_engine.rs` (+174, nouveau), `docker.rs` (158), `devcontainer_api.rs` (82),
`lib.rs` (60) ; retouches `recent_projects` (3 fichiers), `workspace.rs`, `zed/src/main.rs`, `docs/src/dev-containers.md`.

## Abstractions introduites

| Élément | Où | Rôle |
|---|---|---|
| `ProjectCommandBuilder { connection: Option<Arc<dyn RemoteConnection>> }` | `crates/dev_container/src/project_command.rs:18-20` | `local()` / `for_remote_connection()` ; `build_command(program, args, env, working_dir, port_forward, interactive) -> Result<Command>` **signature identique** à `RemoteConnection::build_command` (l.103-137) : local → `Command` direct ; distant → `CommandTemplate` du transport |
| `ProjectCommandBuilder::project_temporary_directory()` | `project_command.rs:47-86` | local : `std::env::temp_dir()` ; distant POSIX : `mktemp -d` ; distant Windows : PowerShell `New-Item … devcontainer-zed-<guid>` |
| `ProjectCommand<'a>` | `project_command.rs:144-186` | builder `arg/args/env/build()` (pas de `current_dir`) |
| `ContainerEngine { command_builder, path_style, wsl_distro_name }` | `crates/dev_container/src/container_engine.rs:13-17` | `command()`, `join_path()`/`normalize_path()` selon le `PathStyle` **de l'hôte**, `filesystem_path()` (l.96-114) : chemin Linux → `\\wsl.localhost\<distro>\…` pour que le `Fs` **local** de Windows lise/écrive dans la distro |
| `requires_local_engine_match_verification()` | `container_engine.rs:53-55` | vrai si distant |
| `container_engine_identity()` / `check_for_docker(&context)` | `crates/dev_container/src/devcontainer_api.rs` (diff l.304-371) | `docker info --format {{.ID}}` (podman : `{{.Host.Hostname}}:{{.Store.GraphRoot}}:{{.Store.RunRoot}}`) sur l'hôte **et** en local ; différent → `ContainerEngineNotReachableLocally` |
| `dev_container::unsupported_reason(project, cx)` | `crates/dev_container/src/lib.rs` (diff l.97-142) | remplace le garde `is_local()` : refuse projets collab, projets déjà dans un conteneur, et remotes sans connexion |
| `DevContainerContext.engine` | `lib.rs` | `fs` devient `project.fs()`, env via `directory_environment` (non plus `local_directory_environment` + `Shell::System`) |
| `normalize_label_path(path, posix_host)` | `devcontainer_manifest.rs` (diff) | forme du label choisie selon l'OS **de l'hôte** et non `cfg(windows)` |
| `OpenOptions.open_in_dev_container` propagé aux projets distants | `recent_projects/src/remote_connections.rs`, `workspace/src/workspace.rs`, `zed/src/main.rs` | `zed --dev-container` fonctionne aussi pour une URL/projet distant |

## Modèle d'exécution (WSL)

| Opération | Où / comment | Réf. |
|---|---|---|
| sondes `docker info`, `buildx version`, `ps`, `inspect`, `pull`, `compose *`, `build`, `run`, `start` | **distro** via `wsl.exe --distribution D --cd ~ -- <shell> -c "exec env K=V 'docker' 'arg'…"` (argv quoté par `shell_kind.try_quote`) | `container_engine.rs`, `docker.rs` (diff), `crates/remote/src/transport/wsl.rs:527-575` (main) |
| `id -u` / `id -g` | **distro** (via `engine_command("id")`) ; alignement UID activé si l'hôte est POSIX (`engine.is_posix()`) au lieu de `cfg(not(windows))` | `devcontainer_manifest.rs` (diff l.60-90, 263-268) |
| dossiers temporaires | **distro** (`mktemp -d`), écrits depuis Windows via UNC `\\wsl.localhost\…` ; un nouveau dossier **à chaque appel** (4 sites), jamais nettoyé | `project_command.rs:47-86`, `container_engine.rs:96-114` |
| lecture devcontainer.json / Dockerfile / features locales | `Fs` local de Windows sur chemin UNC | `devcontainer_manifest.rs` (diff l.16-26, 114-155) |
| `initializeCommand` | **client Windows**, `current_dir` = chemin Linux de la distro → échec de spawn (`CommandFailed`) | `devcontainer_manifest.rs:2527-2542`, `devcontainer_json.rs:372-399` |
| source du bind-mount workspace | chemin Linux de la distro (`/home/…`), résolu par Docker Desktop via l'intégration WSL | `devcontainer_manifest.rs` `remote_workspace_mount` / `local_workspace_folder` l.2189 |
| hooks onCreate…postAttach | `docker exec … sh -c "<join(' ')>"` lancé **dans la distro** — bug de quoting de `main` conservé | `docker.rs` (diff, `run_docker_exec`) |
| upload remote_server, proxy, terminaux | **CLI docker local Windows** (transport `rt/` inchangé) — d'où l'exigence « même moteur » | `rp/remote_connections.rs:86-103` (main, inchangé) |
| identité / persistance | inchangées : `docker:{user}@{name}:{container_id}`, pas d'hôte | `remote/src/remote_identity.rs` (non modifié) |

## Qualité

### Tests
- 3 nouveaux tests, tous dans `container_engine.rs` : commande locale = argv direct ; pas de vérification « même moteur » en local ;
  (Windows) projection UNC. **Aucun** test de `ProjectCommandBuilder` distant, de `project_temporary_directory`, de
  `container_engine_identity`, ni des chemins POSIX construits depuis un client Windows.
- Les tests existants du manifest continuent d'utiliser `DefaultCommandRunner`/fakes locaux ; pas de `RemoteConnection` factice.

### Cas non gérés / bugs probables
- `initializeCommand` : cf. tableau — cassé dans le scénario visé.
- `join(" ")` non quoté des hooks (`dc/docker.rs:377-386` sur main) : conservé ; désormais interprété par **deux** shells
  (shell de login WSL puis `sh -c` dans le conteneur) — le premier est correctement quoté, le second non.
- Garde « même moteur » : faux positif possible si `docker info` échoue côté Windows (Docker Desktop arrêté) → message
  « Docker or Podman CLI/engine is not available » alors que la distro a un moteur ; et **exclut par construction** Docker CE
  dans WSL sans Docker Desktop, SSH, WSLc, et Zed-dans-conteneur.
- `unsupported_reason` accepte **toute** connexion distante (SSH compris) ; la création échouera ensuite au garde moteur
  (sauf si `DOCKER_HOST` local pointe vers le même démon) — message tardif.
- `filesystem_path` n'est défini que pour WSL : en SSH, `Fs` local lit un chemin distant → échec (comme sur main).
- `project.fs()` : pour un projet distant, c'est bien le `Fs` du client (RealFs) — l'accès à la distro repose sur le partage UNC, donc sur `\\wsl.localhost` (Windows 10 1903+/11).
- Dossiers temporaires `mktemp -d` : un par appel, non nettoyés ; les JSON compose y gagnent en unicité (corrige la collision de noms fixes de main).
- `ProjectCommand` n'expose pas `current_dir` : ne peut pas remplacer `LifecycleScript::run` en l'état.

### Quoting / injection
- Positif : plus de concaténation côté hôte ; chaque argument est quoté par le transport (WSL : `try_quote`, `try_quote_prefix_aware`).
- Les injections de `main` (hooks, `runArgs`, `build.options`, `--mount`, `ENV`, `getent`) ne sont **pas** traitées.

### Secrets
- Rien de nouveau persisté. `remote_env` continue d'être passé en `-e K=V` sur la ligne de commande (visible via `ps` dans la distro, désormais aussi dans la ligne `wsl.exe … exec env …`).

### Windows / non-Windows
- `cfg(windows)` remplacé par l'OS de l'hôte pour UID et labels : **correct** conceptuellement et réutilisable.
- Hors Windows, `filesystem_path` est l'identité ; aucun changement de comportement pour les projets locaux (argv identique, test `local_engine_runs_the_program_directly`).

## Retours mainteneurs & CI

Pas de PR → ni revue ni CI. L'auteur a ouvert séparément #63034 (« Preserve Docker exec command arguments », ouverte,
`raw/open-area-devcontainers.json`), qui vise vraisemblablement le bug `join(" ")` — à analyser en P6.
Sur #62680, l'auteur annonce vouloir « break it into smaller PRs » (au sujet de `pupeno-remote`) (https://github.com/zed-industries/zed/pull/62680#issuecomment-5431544679).

## Essai de rebase/merge

- `git merge-tree --write-tree origin/main review/pupeno-wsl` : **2 fichiers en conflit** — `crates/dev_container/src/devcontainer_manifest.rs`,
  `crates/recent_projects/src/dev_container_suggest.rs`.
- Dans la worktree, merge `--no-commit` d'`origin/main` résolu (best-effort) par l'agent interrompu ; non commité.
- Compilation / tests : voir ci-dessous.

Résultats (Windows natif, `CARGO_TARGET_DIR=H:\Sources\zed-wt\target`, cmake ajouté au PATH ; log `raw/build-logs/pupeno-wsl-build.log`),
sur l'arbre fusionné non commité (aucun marqueur de conflit restant dans `crates/dev_container`, `crates/recent_projects`) :

- `cargo check -p dev_container -p remote -p recent_projects` : **OK** (exit 0, 4 min 03 s).
- `cargo test -p dev_container` : **120 passed, 0 failed** (dont les 3 nouveaux tests ; le test Windows UNC s'exécute ici).
- Réserve : la résolution des 2 conflits a été faite par un agent interrompu, sans relecture ligne à ligne ; le résultat
  compile et passe les tests, mais n'a pas été exercé sur un vrai projet WSL (prévu en P4/P7).

## Recouvrements

`git merge-tree` entre branches (fichiers en conflit) : × #62680 = 7, × pupeno-remote = 8, × #60975 = 3.

- **#62680 (SSH)** : même problème, réponse différente — #62680 fait passer **aussi la connexion** par l'hôte (`DockerHost::Ssh`
  dans `DockerConnectionOptions`), ce qui évite le garde « même moteur ». `ProjectCommandBuilder` et le `DockerHost` de #62680
  sont deux formes du même besoin : « exécuter une commande sur l'hôte du moteur via la connexion du projet ».
- **pupeno-remote** : branche indépendante du même auteur, plus large (SSH + WSL + identité `HostDocker`) ; à comparer dans la matrice.
- **#60975** : conflits mécaniques dans `devcontainer_manifest.rs`/`lib.rs`/`recent_projects.rs` ; aucun recouvrement fonctionnel (identité non touchée ici).
- **#63034** : même zone (`run_docker_exec`).

## Verdict par morceau

| Morceau | Verdict | Justification |
|---|---|---|
| `ProjectCommandBuilder` (signature miroir de `build_command`) | **Reprendre / généraliser** | Bonne idée centrale : réutilise le quoting et l'authentification de chaque transport ; argv local strictement identique. À étendre : `current_dir`, stdin, sortie en flux, et un faux `RemoteConnection` pour les tests. |
| `ContainerEngine::join_path/normalize_path` (style de chemin de l'hôte) | **Reprendre** | Nécessaire pour tout hôte ≠ client ; à fusionner avec `RemotePathBuf`/`PathStyle` existants. |
| `filesystem_path` → UNC `\\wsl.localhost` | **Adapter** | Pragmatique pour WSL, mais spécifique ; en cible, lire/écrire via la connexion (upload/commande) plutôt que via un chemin magique, pour couvrir SSH. |
| UID/labels selon l'OS de l'hôte (au lieu de `cfg(windows)`) | **Reprendre** | Correct et petit. |
| `project_temporary_directory` (`mktemp -d` sur l'hôte) | **Adapter** | Bon principe ; créer **un** dossier par session de build et le nettoyer. |
| Garde « même moteur » (`docker info` ID) | **Remplacer** | Contournement : la cible est de faire passer la connexion elle aussi par l'hôte (approche #62680), ce qui rend le garde inutile. Conserver l'idée d'identité moteur pour **l'identité de connexion**. |
| `unsupported_reason` | **Adapter** | Bon point d'entrée unique ; les motifs doivent refléter les topologies réellement supportées. |
| `open_in_dev_container` pour projets distants | **Reprendre** (petite PR séparée) | Indépendant, utile. |
| Messages d'erreur par étape (`dev_container_stage_error`) | **Reprendre** | Petit gain UX. |
| `initializeCommand` inchangé | **À corriger** | Doit tourner sur l'hôte des sources (P3). |
