# Contribution PR #62680 — « Add support for dev containers on remote host over SSH »

Auteur : alexdhill (première contribution, `author_association: NONE`), ouverte le 2026-08-15, MAJ 2026-09-11. Head `5ee9d2d010`. Labels : `cla-signed`, `area:dev containers`, `first contribution`, `no self-review` (raw/pr-62680.json).
Branche analysée : `review/pr-62680-remote` ; worktree d'essai `H:\Sources\zed-wt\pr-62680` (branche `trial/pr-62680`). Les `fichier:ligne` ci-dessous renvoient au head de la PR `5ee9d2d010`.

## Résumé

1. La PR ajoute un « hôte » aux dev containers : `DevContainerHost::{Local, Remote(Arc<dyn RemoteConnection>)}` côté provisionnement et `DockerHost::{Local, Ssh, Wsl}` côté connexion persistée. Toutes les commandes docker passent par le `RemoteConnection::build_command` existant (SSH ControlMaster / wsl.exe) : aucun nouveau transport.
2. L'argv est globalement préservé : c'est le transport qui applique le quoting (`build_command_posix`). Il reste des chaînes `sh -c` construites à la main, dont le `exec_args` hérité, qui joint les arguments avec des espaces.
3. Les entrées de build générées (Dockerfile étendu, features, override compose) sont préparées localement puis uploadées dans `~/.zed_devcontainer` sur l'hôte. Le binaire serveur et les extensions passent par `docker exec -i` (cat/tar). Rien n'est jamais nettoyé côté hôte.
4. Aucun retour de mainteneur. La CI n'a jamais lancé les tests : seul `route-pr` a tourné, les 10 autres jobs sont `skipped`. L'auteur signale que le code est « primarily AI generated ». Chevauchements forts avec pupeno (`ProjectHost`) et #60975 (`stop`/`rm` via `Command::new`).
5. Essai d'intégration :
   - le rebase est impraticable (conflit dès le commit 10/29) ; le merge de `origin/main` est propre (0 conflit) ;
   - **le head de la PR ne compile pas** : `cli_auto_open` a été ressuscité par le merge `5ee9d2d010`. Après suppression de 26 lignes, `cargo check` passe ;
   - `cargo test -p dev_container` : 131 tests OK, 1 KO sous Windows (`the_container_user_takes_its_id_from_the_host`).

## Commits & périmètre

- `origin/main..review/pr-62680-remote` : 36 commits, dont **7 merges** (`5ee9d2d010`, `830f297f8f`, `d6f9d41742`, `36e391245f`, `624f49d3f9`, `7c9ea350c6`, `45770fe2e5`). Le dernier merge (`5ee9d2d010`) intègre `a936ce01c1`.
- Merge-base : `a936ce01c1` (2026-09-11). `origin/main` a 159 commits d'avance, dont un seul touche les fichiers de la PR : `aec7395e30`, qui modifie les `Cargo.toml`.
- `git diff --stat a936ce01c1 5ee9d2d010` : 27 fichiers, +4241/−662. Principaux : `dev_container/src/devcontainer_manifest.rs` (+1723 lignes brutes, en majorité des tests), `remote/src/transport/docker.rs` (+1214), `dev_container/src/docker.rs` (+533), `dev_container/src/lib.rs` (+425), `remote/src/remote_client.rs` (+221), `workspace/src/persistence.rs` (+166), `recent_projects/src/remote_connections.rs` (+143). Petites retouches : `util/src/command.rs` (+`get_envs`), `sqlez/src/bindable.rs` (tuple à 11 colonnes), docs.
- Commits hors sujet mélangés à la PR. `4e42f009d7` « fixing bad parse on Dockerfile builds » (+269 lignes) apporte :
  - l'alias de stage Dockerfile ;
  - `head -n 1` sur `getent` (`devcontainer_manifest.rs:3320`) ;
  - `SCRIPT_DIR` dans le wrapper de features (`:3405`) ;
  - le quoting de `devcontainer-features.env` (`features.rs:79`) et de `_CONTAINER_USER` (`devcontainer_manifest.rs:520`) ;
  - le repli sur `root` quand l'image a un `USER` vide.

  Ces changements recouvrent d'autres PR ouvertes (#62196 « Preserve Dockerfile USER », #62271, #62964 ; voir raw/INDEX.md).
- Le support WSL a été ajouté après la description de la PR (`d81c349b1c`, `cea9cf10c8`, `95acdaa170`), qui ne mentionne encore que `Local | Ssh` (raw/pr-62680.json, body).

## Abstractions introduites

| Abstraction | Signature | Emplacement |
|---|---|---|
| `DevContainerHost` | `pub enum DevContainerHost { #[default] Local, Remote(Arc<dyn RemoteConnection>) }` | `crates/dev_container/src/lib.rs:106` |
| `DevContainerHost::docker_host` | `pub fn docker_host(&self) -> Result<remote::DockerHost, DevContainerError>` : Ssh→Ssh, Wsl→Wsl, sinon `UnsupportedHost` | `lib.rs:121` |
| `DevContainerHost::path_style / join / parent` | helpers de chemins dans le style de l'hôte | `lib.rs:141`, `:152`, `:164` |
| `DevContainerHost::command` | `pub(crate) fn command(&self, program:&str, args:&[String], env:&HashMap<String,String>, working_dir:Option<&Path>) -> Result<util::command::Command, DevContainerError>` : en local c'est un `Command` direct, en distant `connection.build_command(Some(program), args, env, wd, None, Interactive::No)` | `lib.rs:182` |
| `EnvironmentSource` / `environment_source` | `{Local, Host, Unavailable}` ; l'environnement shell vient de l'hôte, par RPC `remote_directory_environment` | `lib.rs:369`, `:383`, `:440` |
| `DevContainerContext.host` / `.remote_client` | nouveaux champs, dérivés de `project.remote_client().remote_connection()` | `lib.rs:391` et suivantes |
| `FakeRemoteConnection` (tests) | mock de `RemoteConnection` qui simule le quoting SSH et enregistre les uploads | `lib.rs` (après `:230`) |
| `Docker { host, .. }` + `Docker::run` | point unique d'exécution des commandes docker | `crates/dev_container/src/docker.rs:249` |
| `DockerClient::new_command / deploy / is_podman` | `fn new_command(&self) -> Command;` `fn deploy(&self, command: Command) -> Result<Command, DevContainerError>;` : « escape hatch » qui réécrit un `Command` local en commande enveloppée par le transport | `docker.rs:610`, `:614`, `:615` (impl. `:515`, `:519`) |
| `DevContainerError::UnsupportedHost(String)` | nouvelle erreur | `crates/dev_container/src/devcontainer_api.rs:92` |
| `LifecycleScript::scripts_for_host` / `run(host, ..)` | exécution de `initializeCommand` sur l'hôte | `crates/dev_container/src/devcontainer_json.rs:400`, `:432` |
| `DockerHost` | `pub enum DockerHost { #[default] Local, Ssh(SshConnectionOptions), Wsl(WslConnectionOptions), #[cfg(test)] Mock(..) }`, volontairement plat (non récursif) | `crates/remote/src/transport/docker.rs:44` |
| `DockerConnectionOptions.host` | `#[serde(default)] pub host: DockerHost` | `transport/docker.rs:76` |
| `DockerExecConnection.host` / `docker_command` | `host: Option<Arc<dyn RemoteConnection>>` ; `fn docker_command(&self, args: Vec<String>, interactive: Interactive) -> Result<CommandTemplate>` est le point unique de wrapping | `transport/docker.rs:90`, `:178` |
| `CommandTemplate::output` | `pub async fn output(&self) -> Result<std::process::Output>` | `crates/remote/src/remote_client.rs:134` |
| `ConnectionPool::connect_docker_host` | reconstruit (ou réutilise via le pool) la connexion vers l'hôte avant `DockerExecConnection::new` | `remote_client.rs:1255` |
| `RemoteConnectionIdentity::Docker { host: Option<String> }` | identité qualifiée par la `persistence_key` de l'hôte ; clé `docker:{user}@{name}:{id}@{host}` | `crates/remote/src/remote_identity.rs:27`, `:59` |
| `Connection::DevContainer(DevContainerConnection, DockerHost)` | l'hôte accompagne l'entrée de settings, qui ne le stocke pas | `crates/recent_projects/src/remote_connections.rs:86` |
| Colonne SQL `docker_host TEXT` | migration, `serialize_docker_host` / `deserialize_docker_host` | `crates/workspace/src/persistence.rs:1069`, `:1087`, `:1103` |
| `header_labels` | libellé « conteneur (hôte) » dans la modale | `crates/remote_connection/src/remote_connection.rs:243` |
| `is_dev_container_project` | refuse l'imbrication d'un conteneur dans un conteneur ; la clé de dismiss est qualifiée par l'hôte | `crates/recent_projects/src/dev_container_suggest.rs:87`, `:79` |

## Modèle d'exécution distante

Ce modèle repose entièrement sur le transport existant : `DevContainerHost::command` → `RemoteConnection::build_command` (`lib.rs:182`) pour le provisionnement, et `DockerExecConnection::docker_command` → `host.build_command` (`transport/docker.rs:178`) pour la connexion. Côté SSH, `build_command_posix` quote chaque argument avec `try_quote` et fait `cd` vers le home quand aucun working dir n'est fourni (`crates/remote/src/transport/ssh.rs:1850-1930`). Côté WSL, `wsl.exe --cd ~ -- sh -c 'exec env … quoted args'` (`transport/wsl.rs:527`).

| Opération | Où / comment |
|---|---|
| Détection du moteur (`docker --version`) | hôte, `context.host.command` (`devcontainer_api.rs:324`) |
| `docker buildx version`, `ps`, `inspect`, `pull`, `start`, `compose config/build` | hôte, via `Docker::run` → `host.command` (`docker.rs:249`) ; argv sous forme de `Vec<String>` (fonctions `*_args`) |
| `docker build` / `buildx build` / `run` / `compose up` (commandes construites par le manifest) | `docker_client.new_command()` puis `.deploy(cmd)`, qui rejoue args et env via `host.command` (`docker.rs:519` ; appels dans `devcontainer_manifest.rs` build/run) |
| `docker exec … sh -c "<inner>"` (scripts de cycle de vie dans le conteneur) | hôte, mais `exec_args` **concatène** programme et arguments avec des espaces (`docker.rs:364-392`, `inner_script.join(" ")`). Comportement hérité, sans quoting (cf. #62964 / #63034 de pupeno) |
| Lecture de `devcontainer.json`, `Dockerfile`, fragments compose, `.env` | hôte : `test -f <p>` puis `cat <p>` (`read_file_from_host`, `devcontainer_manifest.rs:2900`). En local, c'est `Fs::load` |
| `initializeCommand` | hôte, `LifecycleScript::run(host, ..)` avec `working_dir` = dossier projet de l'hôte (`devcontainer_json.rs:432`) ; la doc l'assume (`docs/src/dev-containers.md:44`) |
| `${localEnv:…}`, environnement shell | hôte, via RPC `remote_directory_environment`. Sans `remote_client`, la map est vide, sans repli local (`lib.rs:440`) |
| `id -u` / `id -g` pour `updateRemoteUserUID` | hôte, `host_id` (`devcontainer_manifest.rs:1015`). **Ignoré sur un client Windows** : le `cfg(target_os="windows")` force `false` (`devcontainer_manifest.rs:754-757`, `:1808-1826`), même avec un hôte Linux (doc `dev-containers.md:94`) |
| Génération du Dockerfile étendu, features OCI, override compose | **localement**, dans `std::env::temp_dir()/devcontainer-zed/container-features-<ts>/` (`devcontainer_manifest.rs:79`, `:474-496`, `:980`) |
| Transfert du contexte de build | `stage_build_context` (`devcontainer_manifest.rs:1072`) : `mkdir -p .zed_devcontainer <empty>` sur l'hôte, puis `connection.upload_directory(features_content_dir, ".zed_devcontainer")` (SFTP `put -r` ou `scp -r`, sinon `cp -r` sous WSL). Les chemins sont réécrits par `host_build_path` (`:1052`) en `.zed_devcontainer/container-features-<ts>/…`, relatifs au home. Ré-upload avant chaque build. **Jamais nettoyé.** |
| Workspace folder / bind mount | pas de transfert : le projet est déjà sur l'hôte. La source du mount est le chemin hôte, le nom de base est calculé selon le style de chemin de l'hôte (`devcontainer_manifest.rs:2270`). Les labels suivent le style de l'hôte (`normalize_label_path`, `:3751`) |
| Upload du `remote_server` dans le conteneur | hôte distant → `stream_file_into_container` : `docker exec -i -u <user> <id> sh -c 'cat > "$1"' zed-upload <dst>` alimenté par le stdin local, via SSH (`transport/docker.rs:789`, `:865`). Repli : `copy_server_binary_from_host`, qui fait `sh -c "docker cp \"$HOME/<bin>\" '<id>:<dst>'"` sur l'hôte, après une garde qui refuse `" $ \` \\` dans le nom (`:486-526`) |
| Upload de répertoires (extensions) | hôte distant : tar construit **localement** dans un `tempfile` (`archive_directory`, `:768`), puis `docker exec -i … sh -c 'mkdir -p "$1" && tar -xf - -C "$1"'` (`:748`). Nécessite `tar` dans le conteneur. En local, `docker cp -a` puis `chown`, comme avant |
| Proxy RPC | un seul processus local : `ssh … docker exec -i … <server> proxy`, sans TTY (`transport/docker.rs:1035`) |
| `kill` | tue seulement le processus proxy local ; le master SSH partagé reste en place (`:1084`). Utilise `kill <pid>`, ce qui ne fonctionne pas sous Windows (comportement hérité) |
| Port forwarding | `host.build_forward_ports_command`, dirigé vers l'IP du conteneur sondée par `docker inspect` (`:225`, `:1282`). Limité aux destinations loopback ; non supporté pour un hôte local (message explicite ; ce n'était pas supporté avant non plus) |
| Reconnexion | `ConnectionPool::connect_docker_host` reconstruit la connexion à l'hôte depuis les options persistées (`remote_client.rs:1255`) |

### Identité et sérialisation

- L'identité Docker inclut `host: Option<persistence_key>` (`remote_identity.rs:27`). Les tests vérifient qu'un mot de passe ou un nickname n'altère pas l'identité, et qu'un port différent la change.
- En base, `docker_host` est sérialisé en JSON. Pour SSH, seuls `host`, `username` et `port` sont conservés (`persistence.rs:1087-1100`) : mot de passe et nickname sont retirés, **mais aussi `args` (`-i key`, `-J`…) et `port_forwards`**. Un hôte qui dépend d'arguments ssh personnalisés dans les settings risque d'échouer à la reconnexion depuis l'historique ; les entrées de `~/.ssh/config` s'appliquent toujours. Le filtre `docker_host IS ?` sur `(container_id, docker_host)` évite les collisions entre hôtes (`:1828`).
- `connection_type()` renvoie de nouvelles valeurs `docker-ssh` / `podman-ssh` / `docker-wsl` / `podman-wsl` (`remote_client.rs:1422`), avec un impact sur la télémétrie (signalé dans la description).
- Les settings `DevContainerConnection` ne stockent pas l'hôte (`remote_connections.rs:86`, commentaire).

### Généralisation

- **WSL** : déjà câblé (`DockerHost::Wsl`, `lib.rs:121`) et passe par le `build_command` de `wsl.rs`. Non testé en réel (l'auteur propose de l'ajouter si quelqu'un teste, commentaire du 2026-08-26). Le port forwarding via WSL échoue (`wsl.rs` refuse `port_forward`), et le modèle « Docker Desktop + intégration WSL » n'est pas traité.
- **WSLc (WSL Containers)** : non abordé.
- **Zed dans un conteneur / imbrication** : refusé explicitement (`UnsupportedHost` pour `Docker`, `is_dev_container_project`). `DockerHost` est volontairement plat (`transport/docker.rs:36-41`).
- **Point fort** : l'abstraction repose sur `Arc<dyn RemoteConnection>`, ce qui la rend généralisable à tout transport qui implémente `build_command` et `upload_directory`.
- **Point faible** : les lectures de fichiers et le staging supposent un hôte POSIX (`test -f`, `cat`, `mkdir -p`, `id`). Un hôte SSH Windows n'est pas géré, et `path_style` n'est pris en compte que pour les chemins.

## Qualité

**Tests**

- 39 nouveaux `#[test]` / `#[gpui::test]` dans le diff.
- Ils couvrent :
  - les argv des commandes ;
  - le routage par `FakeRemoteConnection`, avec la sortie SSH quotée attendue (`docker.rs`, tests `should_route_commands_through_the_host_connection`, `should_deploy_escape_hatch_commands_to_the_host`) ;
  - la conversion `DockerHost` ;
  - le pool et la reconstruction de l'hôte (`remote_client.rs`, `docker_host_is_pooled_and_rebuilt_after_it_dies`) ;
  - l'identité et la persistance, y compris la suppression du mot de passe (`persistence.rs`, `test_docker_host_round_trip`) ;
  - le staging, `host_build_path` et `id` sur l'hôte (`devcontainer_manifest.rs`, `build_context_is_staged_on_a_remote_host`, `the_container_user_takes_its_id_from_the_host`) ;
  - l'action `OpenDevContainer` sur un projet distant (`remote_connections.rs`).
- Limites :
  - le mock SSH ne vérifie pas la sémantique réelle de `upload_directory` (arborescence de destination `dest/basename` avec SFTP/scp/cp) ;
  - plusieurs tests sont `#[cfg(unix)]` ou `#[cfg(not(windows))]` (6 gardes ajoutées) ;
  - l'auteur n'a testé qu'un client macOS vers un serveur Linux, sans compose réel ni podman (body de la PR).

**Quoting / injection**

- Bon point : l'argv est transmis non quoté au transport, qui quote chaque argument (`lib.rs:182`, doc-comment).
- Exceptions :
  - `exec_args` (`docker.rs:392`) joint les arguments avec des espaces dans `sh -c` : injection et perte de frontières d'arguments. C'est hérité, mais désormais exécuté aussi à distance.
  - `copy_from_host_command` interpole `$HOME/<bin>` dans une chaîne shell, protégée par une garde de caractères (`transport/docker.rs:486`).
  - `FakeRemoteConnection` quote naïvement avec `'{arg}'` : le test ne prouve pas la robustesse du vrai quoting.

**Secrets**

- `remote_env` et l'env utilisateur passent en `-e KEY=VALUE` sur la ligne de commande `docker exec`, donc visibles dans `ps` sur l'hôte distant. Les logs les masquent (`redact_arguments` / `redact_environment`, `transport/docker.rs:1355`).
- `RUST_LOG` et variables voisines du client sont propagées au proxy (`:1044`).
- Le contexte de build uploadé (`devcontainer-features.env`, qui contient les options de features et peut inclure des tokens) reste indéfiniment dans `~/.zed_devcontainer` sur l'hôte.
- Le mot de passe SSH n'est pas persisté (`persistence.rs:1087`).

**Fichiers temporaires**

- Staging local dans `temp_dir/devcontainer-zed`, comme avant.
- Archive tar dans un `tempfile` local, supprimée au drop.
- Côté hôte : `~/.zed_devcontainer/container-features-<ts>` s'accumule à chaque build, sans nettoyage.

**Windows**

- `updateRemoteUserUID` est désactivé pour un client Windows, même vers un hôte Linux (`devcontainer_manifest.rs:754`).
- `kill_inner` utilise `kill`.
- `normalize_label_path` suit maintenant le style de l'hôte, ce qui est correct.
- Aucun test réel sous Windows ; la doc le signale (`docs/src/dev-containers.md:91-94`).

**Autres**

- Refus des projets collab (`recent_projects.rs:512`).
- `check_for_docker` vérifie maintenant le code de sortie (`devcontainer_api.rs:324`).
- `read_file_from_host` distingue un fichier absent d'une erreur via `test -f`.
- Chaque lecture de fichier coûte 2 allers-retours SSH.

## Retours mainteneurs & CI

**Commentaires** (raw/pr-62680-comments.json, 7 au total) : aucune review ni commentaire de mainteneur (`pr-62680-reviews.json` et `-review-comments.json` sont vides).

- alexdhill, 2026-08-15 : « the implementation is primarily AI generated ».
- pupeno (CONTRIBUTOR), 2026-08-26 : propose de se synchroniser ; sa branche `remote-devcontainer` couvre SSH et WSL et recouvre #60975.
- alexdhill : peu de recouvrement avec le cycle de vie ; il pourrait ajouter WSL si quelqu'un teste.
- Data-Adventure : +1.

**Contexte**

- Issue #59500 (créée par macraig, mainteneur, pour suivre la discussion #56252) : forte demande (SSH, WSL2, GPU). Aucune indication de design de la part des mainteneurs.
- Mentions de WSL Containers (samphonic) et d'un contournement par wrapper SSH (th0rgall) (raw/issue-59500-comments.json).

**CI** (raw/pr-62680-checks.json, head `5ee9d2d010`) : seul `route-pr` a réussi. Les 10 jobs `bundle_*` / `build_*` sont `skipped`. **Aucun job de tests, clippy ou fmt n'a tourné**, probablement faute d'approbation du workflow pour une première contribution. `mergeable_state: unstable`, `rebaseable: false`.

## Essai rebase/merge

- `git rebase origin/main` : 29 commits rejoués (les merges sont linéarisés). **Conflit dès le 10/29** (`eca6a0eef6` « added DockerHost ») dans `crates/remote/src/transport/docker.rs`, parce que les résolutions de conflits faites par l'auteur dans ses merges seraient à refaire. Rebase abandonné (`git rebase --abort`).
- `git merge --no-commit --no-ff origin/main` : **« Automatic merge went well », 0 fichier en conflit**. `main` n'a presque pas touché ces fichiers depuis le merge-base, seulement `aec7395e30` sur les `Cargo.toml`. Worktree laissée dans l'état « merge non commité ».
- Build : voir la section suivante.

### Résultats de build (branche mergée, sous Windows 11)

**Environnement.** Le target partagé `/h/Sources/zed-wt/target` ne donne pas de résultat fiable. Cargo hash les dépendances de chemin relativement à la racine du workspace, donc les artefacts `sqlez` et `util` d'une autre worktree étaient considérés « frais » : on obtenait une fausse erreur E0277 sur le tuple `Column` à 11 colonnes, alors que la PR l'ajoute (`sqlez/src/bindable.rs`). Les résultats ci-dessous utilisent donc `CARGO_TARGET_DIR=/h/Sources/zed-wt/target-pr-62680`, avec `C:\Program Files\CMake\bin` ajouté au PATH (`wasmtime-c-api-impl` a besoin de cmake).

**`cargo check -p dev_container -p remote -p recent_projects` : échec d'abord (exit 101).**

```
error[E0425]: cannot find value `cli_auto_open` in this scope
   --> crates\recent_projects\src\dev_container_suggest.rs:138:8
```

- **Le bug est dans le head de la PR lui-même**, pas dans le merge d'essai. Le merge de l'auteur `5ee9d2d010` « Merging in main » a ressuscité un ancien bloc `if cli_auto_open { … }` (issu de `dc3f5b9972`, #51175), alors que `main` gère désormais le cas CLI via `open_dev_container_from_cli` en tête de fonction (`dev_container_suggest.rs:104`).
- `git grep cli_auto_open` ne trouve la variable ni dans `a936ce01c1` ni dans `origin/main`, seulement dans `5ee9d2d010`.
- **La PR ne compile donc pas en l'état.** La CI ne l'a pas vu, faute d'avoir exécuté les tests.
- **Résolution minimale appliquée dans la worktree** (non commitée) : suppression des 26 lignes du bloc mort. Ensuite, `cargo check` renvoie **exit 0, 0 warning, 0 erreur**.

**`cargo test -p dev_container` : exit 101, `131 passed; 1 failed`.**

```
devcontainer_manifest::test::the_container_user_takes_its_id_from_the_host ... FAILED
panicked at crates\dev_container\src\devcontainer_manifest.rs:4217:9:
the id must be read from the host, got [TestCommand { program: "ssh", args: ["host", "--", "'mkdir'", "'-p'", "'.zed_devcontainer'", "'.zed_devcontainer/empty-folder'"] }]
```

- Cause : sur un client Windows, `update_remote_user_uid` est forcé à `false` par `cfg(target_os="windows")` (`devcontainer_manifest.rs:754-757`), donc `id -u` n'est jamais exécuté. Or le test n'est pas conditionné à la plateforme.
- C'est un test rouge sous Windows, et il reflète la limitation fonctionnelle documentée (`docs/src/dev-containers.md:94`).
- Les autres tests ajoutés par la PR passent sous Windows.

**État laissé** : worktree `H:\Sources\zed-wt\pr-62680`, branche `trial/pr-62680`, merge `origin/main` non commité avec la correction `dev_container_suggest.rs` non indexée. Target `H:\Sources\zed-wt\target-pr-62680`.

## Recouvrements

- **#60975 (alex-berger, lifecycle)** (`review/pr-60975-lifecycle`)
  - Modifie les mêmes fichiers : `dev_container/src/docker.rs`, `devcontainer_manifest.rs`, `lib.rs`, `devcontainer_api.rs`, `remote_identity.rs`, `transport/docker.rs`, `persistence.rs`, `remote_connections.rs`, `remote_servers.rs`.
  - Ajoute `DockerClient::stop_container` / `remove_container` avec `Command::new(&self.docker_cli)` direct : c'est exactement le motif que #62680 remplace par `Docker::run`. Conflit textuel certain dans `docker.rs`.
  - Sémantique : stop/rebuild devront passer par l'hôte. #60975 récupère `devcontainer.local_folder` et `devcontainer.config_file` depuis les labels, dont #62680 change le style de chemin (`normalize_label_path`).
- **pupeno** (`review/pupeno-remote`, 34 fichiers, +6123/−1016 ; `review/pupeno-wsl`, 15 fichiers)
  - Design concurrent : `ProjectHost` / `HostProcessRequest` / `HostPathBuf` (nouveaux `crates/remote/src/project_host.rs` et `crates/dev_container/src/project_host.rs`).
  - Nouvelle variante `RemoteConnectionOptions::HostDocker(HostDockerConnectionOptions { project_host, project_root, devcontainer_config })` (dans `transport/docker.rs` de la branche).
  - `container_engine.rs` / `project_command.rs` pour WSL.
  - #63034 « Preserve Docker exec command arguments » corrige exactement la concaténation `exec_args` restée dans #62680.
  - Les deux PR résolvent le même problème par deux abstractions différentes et touchent quasiment les mêmes fichiers : il faut **choisir un seul modèle**.
- **Autres PR** : le contenu de `4e42f009d7` recouvre #62196 (USER du Dockerfile), #62271 et #62964. #63899 (bind loopback des ports) touche aussi le port forwarding.

## Verdict par morceau

| Morceau | Verdict | Justification |
|---|---|---|
| `DevContainerHost` + `command()` qui délègue à `RemoteConnection::build_command` (`lib.rs:106-230`) | **Reprendre** | Réutilise le transport et le quoting existants, sans nouveau protocole ; forme minimale et généralisable (SSH, WSL). |
| `Docker::run` + fonctions `*_args` pures + `new_command` / `deploy` (`docker.rs`) | **Adapter** | Bonne centralisation. `deploy` (reconstruire un `Command` à partir de ses args et de son env) est fragile : `current_dir` est perdu et les suppressions d'env sont ignorées. Préférer que le manifest produise directement des argv. `exec_args` doit être corrigé (reprendre #63034). |
| `DockerHost` plat + `DockerConnectionOptions.host` + `connect_docker_host` (`transport/docker.rs:44`, `remote_client.rs:1255`) | **Reprendre** | Simple, `#[serde(default)]` rétrocompatible, réutilise le pool. À arbitrer face à `HostDocker` de pupeno : il faut une seule forme. |
| `DockerExecConnection::docker_command` + upload par stream `exec -i` + tar local | **Adapter** | Point unique propre. Deux sémantiques d'upload de répertoire coexistent (`docker cp` crée `dest/basename` alors que tar extrait dans `dest`) : à unifier et tester. Dépendance à `tar` dans le conteneur. |
| Staging `~/.zed_devcontainer` + `host_build_path` (`devcontainer_manifest.rs:79-90`, `:1052-1110`) | **Adapter** | Fonctionnel, mais : pas de nettoyage, secrets de features persistés, sémantique `upload_directory` (SFTP / scp / `cp -r`) non vérifiée en réel, ré-upload complet avant chaque build. |
| `read_file_from_host` via `test -f` + `cat` | **Adapter** | Hôte POSIX uniquement, 2 allers-retours par fichier. Lire par RPC `Fs` du `remote_client` serait préférable quand il existe. |
| `host_id` (`id -u` / `id -g` sur l'hôte) | **Reprendre**, en retirant la désactivation sous Windows | La désactivation `cfg(windows)` est liée au client alors qu'elle devrait dépendre de l'hôte. |
| Identité, persistance `docker_host`, `header_labels`, `connection_type` | **Adapter** | Principe juste. La persistance SSH perd `args` / `port_forwards` ; les nouvelles valeurs de télémétrie doivent être validées par l'équipe. |
| Port forwarding via l'hôte (`:1282`) | **Adapter**, en le séparant dans une PR à part | Ne marche qu'en réseau bridge Linux ; interaction avec #63899. |
| Commit `4e42f009d7` (correctifs Dockerfile, getent, `SCRIPT_DIR`, quoting env) | **Remplacer** | Hors périmètre ; à fusionner avec les PR dédiées (#62196 etc.) ou à soumettre séparément. |
| Support WSL (`d81c349b1c`, `cea9cf10c8`, `95acdaa170`) | **Adapter** | Câblage correct, mais jamais testé ; à confronter à `review/pupeno-wsl`. |
| La PR telle quelle (36 commits, 7 merges, +4241 lignes, IA, CI jamais exécutée, head qui ne compile pas, 1 test rouge sous Windows) | **Abandonner en l'état** | À découper en PR indépendantes : (1) `DevContainerHost` + routage des commandes, (2) staging du contexte de build, (3) upload par stream, (4) identité/persistance, (5) port forwarding. À coordonner avec pupeno et #60975. |
