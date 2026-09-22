# 02 — Contribution : PR #60975 « Add lifecycle management for dev containers »

- Source : branche locale `review/pr-60975-lifecycle` (tête `5a0d2930cd`), PR https://github.com/zed-industries/zed/pull/60975 (alex-berger, ouverte, non draft, `mergeable_state: dirty` — `raw/pr-60975.json`).
- Worktree d'essai : `H:\Sources\zed-wt\pr-60975`, branche `trial/pr-60975` (rebasée sur `origin/main` = `16c9aa7ea6`).
- Sauf mention, les `fichier:ligne` renvoient à la branche `review/pr-60975-lifecycle`.

## Résumé

1. La PR fait trois choses distinctes : (a) une **identité stable** des dev containers (clé `local_folder` + `config_file` au lieu de `container_id`) jusque dans la DB, (b) des **opérations de cycle de vie** Docker (stop/start/restart/rm/rebuild), (c) beaucoup d'**UX** (5 actions, menu barre de titre, invites d'erreur, callout de l'agent, sidebar).
2. L'identité stable est le morceau le plus précieux et le plus petit ; il résout #56576 (doublons après rebuild), mais il ignore l'**hôte** qui porte le démon (SSH/WSL) et n'est pas normalisé sur Windows.
3. Les opérations Docker passent l'argv sans shell (pas d'injection), mais ne gèrent pas Docker Compose (on n'arrête/supprime que le conteneur principal), et le rebuild supprime l'ancien conteneur **avant** de savoir si le build réussira.
4. Aucune revue de mainteneur, aucun check CI exécuté (`raw/pr-60975-checks.json` : `total_count: 0`) ; seuls des retours d'utilisateurs positifs (woopstar, pupeno).
5. Rebase sur `origin/main` : **1 conflit textuel trivial** (migration SQL, `crates/workspace/src/persistence.rs`) + **1 conflit sémantique** (test de #63606 dans `transport/docker.rs`) ; `dev_container` 122/122, `workspace persistence` 51/51, `remote` ne compile pas en test (voir §Essai). Verdict : reprendre l'identité (adaptée), adapter les ops Docker, redécouper/réduire l'UX.

## Commits & périmètre

Merge-base avec `origin/main` : `01acd0ee8e` (2026-08-28). `origin/main` a 353 commits de plus depuis. 25 fichiers, +2509/−204 (`git diff --stat 01acd0ee8e review/pr-60975-lifecycle`).

| SHA | Titre | Fichiers principaux |
|---|---|---|
| `792ac171dd` | Give dev containers a stable identity across rebuilds | `remote_identity.rs`, `transport/docker.rs`, `persistence.rs`, `project.rs`, `settings_content.rs`, `devcontainer_api.rs` (+416/−25) |
| `69d6b8d835` | zed_actions: Add dev container lifecycle actions | `zed_actions/src/lib.rs` (+32) |
| `03af5123c8` | dev_container: Add container lifecycle Docker operations | `devcontainer_api.rs`, `devcontainer_manifest.rs`, `docker.rs`, `lib.rs`, `remote_servers.rs` (+422/−33) |
| `5de5bef927` | Add dev container lifecycle management and reconnect flow | nouveau `recent_projects/src/dev_container_lifecycle.rs` (645 l.), `remote_connections.rs`, `disconnected_overlay.rs` (+968/−85) |
| `0043031140` | title_bar: Add dev container menu | `title_bar.rs` (+96/−28) |
| `8884a5a578` | Surface lost dev container connections in error prompts | `workspace/src/notifications.rs`, `project_panel.rs` |
| `7616c76e1f` | agent_ui: Handle lost dev container connections | `agent_ui/.../thread_view.rs` (+119) |
| `bc12a7652c` | sidebar: Disambiguate and delete dev containers | `sidebar.rs`, `sidebar_tests.rs` (+400/−30) |
| `6bca6567bc` | fix rebase/merge conflicts leftovers | corrige notamment une erreur de syntaxe introduite par `792ac171dd` dans `multi_workspace_tests.rs` (fonction de test insérée avant l'accolade fermante de `setup_multi_workspace`) → `792ac171dd` seul **ne compile pas** |
| `a99f4e3e8a`, `752d8ebd7e`, `5a0d2930cd` | Merge branch 'main' (×3, 27–28 août) | merges de synchronisation, pas de contenu propre significatif |

Les 9 commits non-merge portent tous la même date d'auteur (2026-08-20 12:45:35), signe d'un historique réécrit/rebasé ; la PR déclare être « implemented with AI assistance, supervised by me » (`raw/pr-60975.json`, body).

## Abstractions introduites

### Identité / persistance (`792ac171dd`)
- `pub enum DockerIdentityKey { DevContainer { local_folder: String, config_file: String }, ContainerId(String) }` — `crates/remote/src/remote_identity.rs:31`. Remplace `container_id` + `name` dans `RemoteConnectionIdentity::Docker { remote_user, key }`. Construction : `(Some, Some)` → `DevContainer`, sinon repli `ContainerId` (`remote_identity.rs:84` et suiv.). `persistence_key()` → `docker:{user}@devcontainer:{folder}:{config}` ou `docker:{user}@container:{id}` (`remote_identity.rs:52`). **Le `name` ne fait plus partie de l'identité.**
- `DockerConnectionOptions.local_folder: Option<String>`, `.config_file: Option<String>` (`#[serde(default)]`) — `crates/remote/src/transport/docker.rs:56,60`.
- `DevContainerConnection.local_folder: String`, `.config_file: String` (`#[serde(default)]`, chaîne vide par défaut) — `crates/settings_content/src/settings_content.rs:1353,1355` ; converties en `Some(..)` sans filtrer la chaîne vide (`recent_projects/src/remote_connections.rs`, `From<Connection>`), donc une entrée de settings ancienne donne une identité `DevContainer{"",""}` partagée par tous les anciens conteneurs sans labels.
- Valeurs calculées dans `start_dev_container_with_config` : `context.project_directory.display()` et `project_directory.join(config_path).display()` — `crates/dev_container/src/devcontainer_api.rs:326-336` (branche). **Sans** `normalize_label_path` (`devcontainer_manifest.rs:3467`) utilisé pour les vrais labels Docker (`identifying_labels`, `devcontainer_manifest.rs:126`).
- `ProjectGroupKey` : `PartialEq`/`Hash` manuels basés sur `matches()` → `paths` + `remote_connection_identity(host)` — `crates/project/src/project.rs:6511,6519`. Changement sémantique global (SSH/WSL aussi).
- `Project::is_dev_container(&self, cx) -> bool` (= remote Docker) — `crates/project/src/project.rs:3080`.
- DB : migration `ALTER TABLE remote_connections ADD COLUMN local_folder TEXT; … config_file TEXT;` (`crates/workspace/src/persistence.rs:1066`) ; `get_or_create_dev_container_connection_query(this, docker, local_folder, config_file) -> Result<RemoteConnectionId>` (`persistence.rs:1797`) : SELECT sur `(kind, user, local_folder, config_file)` puis **UPDATE** des champs runtime (`container_id`, `name`, `use_podman`, `remote_env`) ou INSERT. Aiguillage depuis `get_or_create_remote_connection_internal` (`persistence.rs:1701`). Pas de backfill des lignes existantes.

### Opérations Docker (`03af5123c8`)
- Trait `DockerClient` : `async fn stop_container(&self, id: &str)`, `async fn remove_container(&self, id: &str)` — `crates/dev_container/src/docker.rs:564,569` ; impl `docker stop <id>` / `docker rm -f <id>` via `Command::args` — `docker.rs:433,454`.
- `DockerConfigLabels.local_folder/config_file` (labels `devcontainer.local_folder` / `devcontainer.config_file`) — `docker.rs:50` et suiv.
- `DevContainerConfig::from_recovered_paths(local_folder: &Path, config_file: &Path) -> Self` — `devcontainer_api.rs:58`.
- `pub struct DevContainerOrigin { local_folder: PathBuf, config: DevContainerConfig }` + `pub async fn dev_container_origin(container_id: &str, use_podman: bool) -> Result<DevContainerOrigin, DevContainerError>` (via `docker inspect`) — `devcontainer_api.rs:383,388`.
- `pub async fn stop_dev_container / start_dev_container / restart_dev_container / remove_dev_container(container_id: &str, use_podman: bool) -> Result<(), DevContainerError>` — `devcontainer_api.rs:423,441,460,479` ; `pub async fn rebuild_dev_container(context, config, environment) -> Result<(DevContainerConnection, String), _>` — `devcontainer_api.rs:499`. Chacune recrée un `Docker::new("docker"|"podman", None)` (code dupliqué ×4).
- `start_dev_container_with_config(..., force_rebuild: bool)` — `devcontainer_api.rs:287` ; `spawn_dev_container(..., force_rebuild)` → `remove_existing_container_if_present()` puis `build_and_run()` — `devcontainer_manifest.rs:2512,2731,2750`.
- `DevContainerContext::for_local_directory(project_directory: Arc<Path>, workspace, cx) -> Self` — `crates/dev_container/src/lib.rs:120`.
- `DevContainerError` devient public (`lib.rs`, re-export).

### Flux applicatifs (`5de5bef927` et suiv.)
- Actions `projects::{StopDevContainer, DeleteDevContainer, RebuildDevContainer, ReconnectDevContainer, RestartDevContainer}` — `crates/zed_actions/src/lib.rs:724-750`, enregistrées dans `recent_projects::init`.
- Module `crates/recent_projects/src/dev_container_lifecycle.rs` : `stop_dev_container` (l.65), `delete_dev_container` (l.146), `pub async fn delete_dev_container_with_options(options, connected_workspaces, reopen, cx)` (l.181, exportée pour la sidebar), `rebuild_dev_container` (l.268), `reconnect_dev_container` (l.295), `restart_dev_container_and_reconnect` (l.319), `open_dev_container_modal(workspace, force_rebuild, ..)` (l.339), `enum ReconnectMode { Resume, Restart, Rebuild }` (l.388), `reconnect_connected_dev_container` (l.428), `rebuild_dev_container_connection(workspace, &options, cx) -> Result<RemoteConnectionOptions>` (l.593), `shutdown_remote_connection` (l.37, via `RemoteClient::shutdown_processes`).
- `remote_connections.rs` : `enum ConnectionFailureChoice { Retry, RetryWith(RemoteConnectionOptions), Cancel }` (l.132), `enum DevContainerRecoveryOp` (l.145), `prompt_connection_failure` (l.195), `run_dev_container_recovery` (l.259), `show/dismiss_connection_status` (l.320/351) ; `open_remote_project(mut connection_options, ..)` (l.369) peut désormais changer d'options en cours de boucle. `show_connection_status` duplique `show_lifecycle_status` (`dev_container_lifecycle.rs:544`).
- `RemoteServerProjects.force_rebuild: bool` — `crates/recent_projects/src/remote_servers.rs:73,1436,2252`.
- `DisconnectedOverlay` : pour `Docker`, route vers `reconnect_dev_container` — `disconnected_overlay.rs:99`.
- `workspace::notifications::dev_container_reconnect_available` + boutons Reconnect/Restart/Rebuild dans **tout** `detach_and_prompt_err` sur `ErrorCode::Disconnected` — `crates/workspace/src/notifications.rs:1655`.
- Barre de titre : `PopoverMenu "dev-container-menu"` qui **remplace** le menu distant standard pour Docker — `crates/title_bar/src/title_bar.rs:599,660`.
- Agent : `ThreadView.remote_input_disabled`, `is_remote_disconnected`, `render_disconnected_callout` — `thread_view.rs:643,11857,11864`.
- Sidebar : `ListEntry::ProjectHeader.subtitle`, `enum RemoteGroupConnection` (l.491), `dev_container_config_subtitle` (l.572), `delete_dev_container_for_group` (l.1358), `remote_group_connection_state` (l.2468), entrée « Delete Dev Container » (l.3373) — `crates/sidebar/src/sidebar.rs`.

## Qualité

### Tests ajoutés
| Test | Emplacement | Couvre |
|---|---|---|
| `dev_container_identity_is_stable_across_rebuilds`, `dev_container_identity_distinguishes_config_file`, `docker_identity_without_labels_falls_back_to_container_id` | `crates/remote/src/remote_identity.rs` (tests) | égalité d'identité selon labels / repli container_id |
| `dev_container_project_group_key_is_stable_across_rebuilds` | `crates/workspace/src/multi_workspace_tests.rs` | Eq **et** Hash de `ProjectGroupKey` |
| `test_dev_container_connection_is_stable_across_rebuilds` | `crates/workspace/src/persistence.rs:4346` | réutilisation de ligne DB + rafraîchissement `container_id`/`name` + distinction par config |
| `test_from_recovered_paths_{default,root,subfolder}_config`, `..._falls_back_when_prefix_mismatched` | `crates/dev_container/src/devcontainer_api.rs` (tests) | inversion labels → `DevContainerConfig` |
| `should_remove_existing_container_when_present`, `should_not_remove_container_when_none_present` | `devcontainer_manifest.rs` (tests, `FakeDocker`) | `remove_existing_container_if_present` |
| `test_dev_container_config_subtitle`, `test_dev_container_tooltip_descriptor` | `crates/sidebar/src/sidebar_tests.rs` | formatage des sous-titres/tooltips |

Non testé : aucun test des flux `dev_container_lifecycle.rs` (stop/delete/rebuild/reconnect), de `prompt_connection_failure`/`RetryWith`, du menu de la barre de titre, du callout agent, ni de `delete_dev_container_for_group`. `FakeDocker::stop_container` renvoie `Ok(())` sans enregistrer (`devcontainer_manifest.rs`, impl de test). Le chemin `spawn_dev_container(force_rebuild=true)` complet n'est pas testé (seulement la sous-fonction).

### Cas non gérés / bugs probables
- **Docker Compose** : stop/restart/delete opèrent sur `options.container_id` seul (`dev_container_lifecycle.rs:97,224,489`) ; les services annexes du projet compose restent en marche/présents ; le rebuild ne supprime que le conteneur trouvé par labels (`devcontainer_manifest.rs:2512`), pas `docker compose down`.
- **Rebuild destructif avant build** : `remove_existing_container_if_present` précède `build_and_run` (`devcontainer_manifest.rs:2750`) ; un build qui échoue laisse l'utilisateur sans conteneur.
- **Rebuild depuis la fenêtre connectée** exige que le conteneur existe encore (`dev_container_origin` fait `docker inspect`, `dev_container_lifecycle.rs:443`) alors que les labels sont déjà dans `options.local_folder/config_file` ; seul le chemin « échec de connexion » les utilise (`dev_container_lifecycle.rs:593`). Incohérent : si le conteneur a été supprimé hors de Zed, « Rebuild » depuis la barre de titre échoue.
- **`container_id` périmé dans la sidebar** : comme `ProjectGroupKey` compare par identité, la clé conservée dans les `HashMap` peut porter l'ancien `container_id` ; `delete_dev_container_for_group` utilise `project_group_key.host()` (`sidebar.rs:1358`) → `docker rm -f <ancien id>` échoue après un rebuild non vu par cette clé.
- **Environnement** : `for_local_directory` prend `workspace.project().environment()` du workspace courant, qui est le projet *distant* quand on rebuild depuis le conteneur (`dev_container/src/lib.rs:120`, appelé en `dev_container_lifecycle.rs:455`) — l'environnement « local » utilisé pour `${localEnv:…}` peut ne pas être celui de l'hôte.
- **Lignes DB héritées** : pas de backfill ; les anciennes lignes (sans labels) restent comme doublons (`persistence.rs:1797` ne matche que les lignes avec labels).
- **Chaîne vide** : un `DevContainerConnection` ancien (`local_folder: ""`) produit `Some("")` → identité `DevContainer{"",""}` commune (voir §Abstractions).
- **Changement global de `ProjectGroupKey`** : l'égalité ignore désormais tout champ hors identité pour SSH/WSL aussi (`project.rs:6511`) ; cohérent avec `matches()`, mais modifie le comportement de toutes les `HashMap<ProjectGroupKey, _>` (le premier `host` inséré gagne).
- **UX** : `detach_and_prompt_err` propose Reconnect/Restart/**Rebuild** (destructif) sur n'importe quelle erreur `Disconnected` sans confirmation (`notifications.rs:1655` et suiv.) alors que Delete, lui, est confirmé ; le menu dev container de la barre de titre remplace, pour Docker, le popover standard `RemoteServerProjects::popover` (`title_bar.rs:660` vs `title_bar.rs:746-757` dans la branche d'essai), l'utilisateur perd donc l'accès aux autres projets distants depuis ce bouton ; `set_read_only` dans `render()` (`thread_view.rs`, `impl Render`) — mutation en rendu, tolérée par un cache booléen.
- Duplication : `show_connection_status`/`show_lifecycle_status`, `DevContainerRecoveryOp`/`ReconnectMode` (mêmes libellés), 4 × `Docker::new(...)`.

### Quoting / injection
- `docker stop` / `docker rm -f` : `Command::new(docker_cli).args(["stop", id])` — argv direct, **aucun shell** (`docker.rs:433-475`). `container_id` provient de `docker ps`/DB. Pas de risque d'injection.
- Filtres de labels : `format!("label={k}={v}")` passé comme argument unique (préexistant, `devcontainer_manifest.rs:2498`) — pas de shell.
- Aucune concaténation de commande shell ajoutée par la PR.

### Secrets
- `remote_env` (issu de `remoteEnv`, qui peut contenir des `${localEnv:TOKEN}` résolus) était déjà persisté en clair dans `remote_connections.remote_env` (migration préexistante, `persistence.rs:1035`). La PR **rafraîchit** cette colonne à chaque connexion (UPDATE, `persistence.rs:1797` et suiv.) : pas de nouvelle classe de fuite, mais la persistance en clair est pérennisée et mise à jour. `main` a entre-temps ajouté la rédaction des secrets dans les logs `docker exec` (`1057c2cf3d`, #63606), sans rapport avec la DB.
- Les chemins hôte (`local_folder`, `config_file`) sont désormais stockés en DB et dans l'identité (données peu sensibles).

### Windows
- Non testé (« Not covered: Windows and Linux hosts were not tested », body de la PR).
- L'identité utilise `display().to_string()` brut (`devcontainer_api.rs:326-336`) alors que les labels passent par `normalize_label_path` (drive letter minuscule, `\`, `devcontainer_manifest.rs:3467`) : sur Windows, `C:\p` vs `c:\p` selon l'ouverture ⇒ deux identités/ lignes DB pour le même conteneur. `from_recovered_paths` repose sur `strip_prefix` entre chaînes issues des labels (cohérent entre elles), mais `rebuild_dev_container_connection` mélange valeur d'identité (non normalisée) et labels.
- Pas de notion d'hôte : un dev container lancé depuis WSL ou via SSH aurait un `local_folder` qui est un chemin **de cet hôte**, indistinguable d'un chemin local homonyme.

## Retours mainteneurs & CI

- **Aucune revue** : `raw/pr-60975-reviews.json` et `raw/pr-60975-review-comments.json` vides ; `requested_reviewers: []` (`raw/pr-60975.json`).
- Commentaires (`raw/pr-60975-comments.json`, 8) : cla-bot ×3 puis CLA signé (https://github.com/zed-industries/zed/pull/60975#issuecomment-4977477347) ; woopstar demande un rebase (…#issuecomment-5233275014) ; pupeno a testé la branche, « it works », et anticipe des conflits avec ses propres travaux (…#issuecomment-5385872086), puis cherche à contacter l'auteur (…#issuecomment-5431525148). Aucun membre de l'équipe Zed n'a commenté.
- **CI** : `raw/pr-60975-checks.json` = `{"total_count": 0, "check_runs": []}` → aucun check exécuté sur la tête `5a0d2930cd` (probablement workflows non approuvés pour un premier contributeur ; label `first contribution`). `mergeable: false`, `mergeable_state: dirty`.
- Labels : `cla-signed`, `area:dev containers`, `first contribution`. PR « Fixes #56576 ».

## Essai de rebase/merge

- `git rebase origin/main` dans la worktree : les 3 merges sont aplatis, 9 commits rejoués. **1 conflit** : `crates/workspace/src/persistence.rs` (tableau des migrations : `main` a ajouté `recent_navigation_history` et `native_window_state`). Résolution triviale : migration `local_folder/config_file` ajoutée **en dernière position**. Rebase terminé : `trial/pr-60975` = `8b1c7dbf8c … d2c4028933` (9 commits).
- Leçon : la migration SQL de la PR devra être re-positionnée à chaque rebase (append-only).
- Compilation / tests : voir ci-dessous.

Résultats (exécutés par moi après interruption de l'agent ; Windows natif, `CARGO_TARGET_DIR=H:\Sources\zed-wt\target`,
cmake de `C:\Program Files\CMake\bin` ajouté au PATH ; log : `raw/build-logs/pr-60975-build.log`) sur `trial/pr-60975` = `d2c4028933` :

- `cargo check -p dev_container -p remote -p recent_projects` : **OK** (exit 0, 1 min 59 s, aucune erreur).
- `cargo test -p dev_container` : **122 passed, 0 failed**.
- `cargo test -p workspace persistence` : **51 passed, 0 failed** (la migration repositionnée s'applique ; log `raw/build-logs/pr-60975-extra.log`).
- `cargo test -p remote` : **ne compile pas** — `E0063 missing fields config_file and local_folder` à `crates/remote/src/transport/docker.rs:1071`.
  `git blame` : ce test vient de `main`, `1057c2cf3d` (#63606 « Redact environment secrets from dev container `docker exec` logs »,
  2026-09-04), postérieur au merge-base de la PR. **Conflit sémantique** (non textuel, invisible pour `git merge-tree`) ;
  correction triviale (`local_folder: None, config_file: None`), non appliquée. Conséquence : les tests `remote_identity` de la PR
  n'ont pas pu être exécutés ici.

## Recouvrements probables

- **PR #62680 (SSH, alexdhill, `review/pr-62680-remote`)** : ajoute `DockerConnectionOptions.host: DockerHost { Local, Ssh(..) }` et un champ `host: Option<String>` dans `RemoteConnectionIdentity::Docker`, en gardant `name` + `container_id` comme clé (diff de `remote_identity.rs` sur cette branche). Conflit frontal avec `DockerIdentityKey` : les deux redéfinissent la même variante et `persistence_key()`, et tous deux touchent `persistence.rs`, `transport/docker.rs`, `remote_connections.rs`, `devcontainer_api.rs`. Sémantiquement complémentaires : la clé cible devrait être `(hôte, local_folder, config_file, remote_user)`.
- **pupeno/remote-devcontainer (`review/pupeno-remote`)** : introduit `RemoteConnectionIdentity::HostDocker { project_host, project_root, devcontainer_config, container_id, name, remote_user }` — même idée de clé projet+config, mais **garde `container_id`** dans la clé (donc pas de stabilité au rebuild) et ajoute l'hôte. Touche aussi `sidebar.rs`, `title_bar.rs`, `settings_content.rs`, `persistence.rs`. Recouvrement fort sur l'identité et la DB.
- **pupeno/wsl-devcontainers (`review/pupeno-wsl`)** : touche `devcontainer_api.rs`, `devcontainer_manifest.rs`, `docker.rs` (trait `DockerClient`) — conflits mécaniques probables avec les nouvelles méthodes `stop_container`/`remove_container` et `force_rebuild`.
- `main` récent : rédaction des secrets dans les logs `docker exec` (`1057c2cf3d`), réorganisations de la sidebar (`36726df0c3`, `aef893e276`, `9785475caa`) — n'ont pas conflicté au rebase mais la sidebar bouge vite.

## Verdict par morceau

| Morceau | Verdict | Justification |
|---|---|---|
| Identité stable (`DockerIdentityKey`, colonnes DB, UPDATE runtime) — `792ac171dd` | **Adapter / reprendre** | Bon concept, testé, résout #56576. À adapter : inclure l'hôte (SSH/WSL, cf. #62680/pupeno), normaliser les chemins comme `normalize_label_path`, traiter `""` comme `None`, backfill/nettoyage des lignes héritées, migration en dernière position. Isoler le changement d'`Eq/Hash` de `ProjectGroupKey` dans sa propre PR avec justification. |
| Ops Docker (`stop_container`, `remove_container`, `force_rebuild`, `from_recovered_paths`, `dev_container_origin`) — `03af5123c8` | **Adapter** | Argv sûr et petit. Manque : support Compose (`docker compose stop/down` sur le projet), factoriser `Docker::new`, rebuild non destructif (build puis bascule, ou au moins confirmation), faire passer par l'abstraction d'hôte si #62680 atterrit. |
| Actions `zed_actions` — `69d6b8d835` | **Reprendre** | Trivial ; à fusionner avec l'implémentation qui les utilise (une PR « une seule chose »). |
| Module `dev_container_lifecycle.rs` + `prompt_connection_failure` — `5de5bef927` | **Adapter (réduire)** | Utile (Reconnect qui fait `docker start`, overlay déconnecté), mais 645 l. sans test, deux machineries dupliquées (`ReconnectMode`/`DevContainerRecoveryOp`), rebuild qui ignore les labels persistés. Garder Reconnect/Stop/Rebuild dans un seul chemin basé sur les options persistées. |
| Menu barre de titre — `0043031140` | **Adapter** | Ajouter les entrées au menu distant existant plutôt que de remplacer le popover `RemoteServerProjects`. |
| Invites d'erreur génériques (`notifications.rs`) — `8884a5a578` | **Remplacer / abandonner** | Couple `workspace` à une sémantique dev container dans un trait générique et propose un Rebuild destructif sans confirmation ; l'overlay déconnecté suffit. |
| Callout agent — `7616c76e1f` | **Adapter / reporter** | Idée pertinente mais devrait valoir pour tout remote déconnecté (pas seulement Docker) ; relève de l'équipe agent. |
| Sidebar (sous-titres, Delete, indicateur) — `bc12a7652c` | **Adapter / reporter** | Sous-titres utiles ; Delete depuis la sidebar dépend d'un `container_id` potentiellement périmé ; zone très mouvante sur `main`. À faire après l'identité. |
| Commits de merge / fix — `6bca6567bc`, merges | **Abandonner** | Artefacts d'historique ; `792ac171dd` isolé ne compile pas sans `6bca6567bc`. |
