# Contribution — pupeno/zed `remote-devcontainer` (branche locale `review/pupeno-remote`)

> Analyse du 2026-09-22/23. Base `origin/main` = `16c9aa7ea6`. Worktree d'essai : `H:\Sources\zed-wt\pupeno-remote` (branche `trial/pupeno-remote`, merge non commité ; worktree supprimée après analyse, logs dans `raw/build-logs/`).
> Sauf mention, les références `fichier:ligne` pointent sur la pointe de `review/pupeno-remote` (`d1f6b3c58b`).

## Résumé

- Introduit une abstraction **`ProjectHost`** (crate `remote`) : la machine qui possède le projet (local, SSH ou WSL). Toutes les commandes docker, E/S de fichiers, `id -u/-g` et fichiers temporaires du flux Dev Container passent par elle, via `RemoteConnection::build_command` (le transport existant).
- Ajoute une variante de connexion **`RemoteConnectionOptions::HostDocker`** : `docker exec` sur le daemon de l'hôte distant, pour que Zed se connecte au conteneur. L'identité et la persistance sont qualifiées par l'hôte.
- Côté exécution distante, le design est propre : argv préservé jusqu'au transport, qui quote chaque argument. Il reste une concaténation `sh -c` pour les hooks du cycle de vie (déjà présente sur main). Pas de redirection de ports. Les fichiers temporaires distants ne sont jamais nettoyés.
- Branche volumineuse (≈ +6 100/−1 000, 34 fichiers, 15 commits + 1 merge) et « majoritairement IA » selon l'auteur (`raw/pr-62680-comments.json`, 2026-08-26). Il faudra la découper, comme l'auteur le dit lui-même.
- Essai d'intégration : le **merge** avec `origin/main` donne 3 fichiers en conflit, résolus en ~15 min, plus 2 corrections de compilation d'une ligne. `cargo check` passe. Tests : `dev_container` 161/161 OK, `remote --lib` 56/56 OK. Le **rebase** n'est pas praticable.

## Commits & périmètre

`git log origin/main..review/pupeno-remote` : 16 commits de Pablo Fernandez, du 2026-08-16 au 2026-08-26. Le dernier, `d1f6b3c58b`, est un merge de `main` (parent `fc9258b463`, qui est aussi le merge-base avec `origin/main`). `origin/main` a **369 commits** de plus depuis ce merge-base.

| SHA | Sujet | Zone |
|---|---|---|
| 285d2c2bcc | Add project host capability | `remote/src/project_host.rs` (+588) |
| 114a17633c | Route Dev Container orchestration through project host | `dev_container/*` (+1670/−581), nouveau `dev_container/src/project_host.rs` |
| dbee975f03 | Add host-backed container connections | `transport/docker.rs`, `remote_identity.rs`, `persistence.rs`, settings, UI |
| 3c90445b79 | Allow remote projects to reopen in Dev Containers | `devcontainer_api.rs`, `recent_projects.rs` |
| ae58491b4c | Cancel abandoned Dev Container startup | `remote_servers.rs` |
| 497bbb6292 | Document ProjectHost APIs | doc |
| e88510c3bb | Allow more cases to open the devcontainer | `dev_container_suggest.rs` |
| aa45b919e1 | Add host-semantic project paths (`HostPathBuf`) | `project_host.rs` (+1098/−542 net) et consommateurs |
| e902c1a087 | Use project host paths for Dev Container startup | manifest |
| 56e4ef41af | Route Dev Container environment through project | `lib.rs`, `project/src/environment.rs` |
| ae5e994e42 | Decide the container user remap on the project host | manifest (remap UID) |
| 4296cf12e7 | Guard the desktop boundary | nouveau `desktop_boundary.rs` (test-only, 398 l.) |
| e3b757eff9 | Find Dev Container configs under an ignoring repository | `worktree.rs`, `paths.rs`, api |
| e4187ccee8 | Ask the project, not an event, about its Dev Container | `dev_container_suggest.rs` (+569) |
| dc4ff3c367 | Let the modal go once the container is created | `remote_servers.rs`, suggest |
| d1f6b3c58b | Merge branch 'main' | — |

Le diff total (`git diff fc9258b463 review/pupeno-remote`) compte 34 fichiers, +6 123/−1 016. Les plus gros : `devcontainer_manifest.rs` (1 884 l. modifiées), `remote/src/project_host.rs` (1 539, dont ~650 de tests), `dev_container_suggest.rs` (757), `dev_container/src/project_host.rs` (553).

**Branche WSL (`review/pupeno-wsl`)** : `git merge-base review/pupeno-remote review/pupeno-wsl` = `bc538def45`, un commit de `main` (#62602). Il n'y a donc **aucun commit en commun**. La branche WSL introduit ses propres abstractions (`dev_container/src/container_engine.rs`, `project_command.rs`) et ne contient ni `ProjectHost`, ni `HostPathBuf`, ni `HostDocker`. Ce sont deux conceptions parallèles, concurrentes, du même problème (« exécuter le moteur là où vit le projet »). `remote-devcontainer` est la plus générale : elle couvre SSH et WSL.

## Abstractions introduites

| Abstraction | Signature | Emplacement |
|---|---|---|
| `ProjectHostKind` | `enum { Local, Ssh, Wsl }` | `crates/remote/src/project_host.rs:26` |
| `ProjectHostPlatform` | `enum { Linux, MacOs, Windows }`, `From<RemoteOs>` | `project_host.rs:40`, `:62` |
| `HostPathBuf` | `struct { text, PathStyle }`, avec `new/from_path/join/parent/root/is_absolute/starts_with/to_remote_path` et un Serde qui porte le style | `project_host.rs:84`–`320` |
| `HostProcessRequest` | `new(program, working_directory: HostPathBuf)`, `.arguments(..)`, `.environment(..)` | `project_host.rs:323` |
| `HostProcess` / `HostProcessOutcome` | `collect_output()`, `cancel()`, `into_child()`, `kill_on_drop` | `project_host.rs:367`–`411` |
| `ProjectHost` | `local(root)`, `from_remote_connection(root, Arc<dyn RemoteConnection>)`, `from_remote_client(..)` ; `temporary_root`, `read_file`, `is_file/is_dir`, `copy_dir`, `create_dir_all`, `write_file`, `start_process`, `build_command`, `stage_assets` | `project_host.rs:435`–`832` |
| `HostCommand` (dev_container) | builder `new/arg/args/path_arg/path_arg_with_prefix/env/current_dir`, `to_shell_words()` | `crates/dev_container/src/project_host.rs:11`, `:107` |
| `ProjectHostCapability` (trait, `?Send`) | `source_root, platform, temporary_root, run, read_file, write_file, create_dir_all, is_file, is_dir, copy_dir, stage_assets` | `dev_container/src/project_host.rs:117` |
| `RemoteProjectHost` | implémente le trait au-dessus de `Arc<ProjectHost>` + `AsyncApp` | `dev_container/src/project_host.rs:163` |
| `RecordingProjectHost` (tests) | fausse implémentation qui enregistre les commandes, adossée à `FakeFs` | `dev_container/src/project_host.rs` (`mod test_support`) |
| `ProjectHostConnectionOptions` | `enum { Ssh(SshConnectionOptions), Wsl(WslConnectionOptions) }` | `crates/remote/src/transport/docker.rs:46` |
| `HostDockerConnectionOptions` | `{ project_host, project_root: HostPathBuf, devcontainer_config: HostPathBuf, container: DockerConnectionOptions }` | `transport/docker.rs:61` |
| `RemoteConnectionOptions::HostDocker` | nouvelle variante | `crates/remote/src/remote_client.rs` (~1329) |
| `RemoteConnectionIdentity::HostDocker` | `{ project_host: Box<Identity>, project_root, devcontainer_config, container_id, name, remote_user }` | `crates/remote/src/remote_identity.rs:25` |
| `DevContainerProjectHost` / `DevContainerProjectPathStyle` (settings) | `Ssh{host,username,port}` / `Wsl{distro_name,user}` | `crates/settings_content/src/settings_content.rs:1344` |
| `is_supported_dev_container_source_connection` | `fn(&RemoteConnectionOptions) -> bool` (Ssh ou Wsl) | `crates/dev_container/src/devcontainer_api.rs:67` |
| `desktop_boundary` (test-only) | scanner du code source de `dev_container/src` qui interdit `cfg(target_os)`, `std::env::temp_dir`, `Path`/`PathBuf`… sauf si un commentaire `desktop-local` le justifie | `crates/dev_container/src/desktop_boundary.rs:193` |

`Docker` (`dev_container/src/docker.rs`) reçoit un `host: Rc<dyn ProjectHostCapability>`. Les commandes passent de `util::command::Command` à `HostCommand`, et `DockerClient` devient `#[async_trait(?Send)]`.

## Modèle d'exécution distante

Principe : `ProjectHost::start_process` (`project_host.rs:757`). Pour un hôte local, c'est un `Command` local (`Command::new`, qui ajoute `CREATE_NO_WINDOW` sous Windows). Pour un hôte distant, c'est `connection.build_command(program, args, env, cwd, None, Interactive::No)` (`:785`), puis un spawn local du `CommandTemplate` (en pratique `ssh … '<cd … && exec env … prog 'arg'…>'` ou `wsl.exe --distribution … --cd … -- <shell> -c …`).

| Opération | Où | Comment | Réf. |
|---|---|---|---|
| Sonde `docker/podman --version`, `buildx version` | hôte projet | `HostCommand` → `host.run` | `devcontainer_api.rs` (`check_for_docker`), `docker.rs:224` |
| `docker ps/inspect/pull/start/compose config/compose build/build/run` | hôte projet | argv (`Vec<String>`) → `build_command` ; SSH quote **chaque argument** avec le `ShellKind` distant (`ssh.rs:1909-1928`) ; WSL idem (`wsl.rs:547-564`) | `docker.rs`, manifest |
| Hooks du cycle de vie (`onCreate`, `postCreate`, …) | conteneur, via `docker exec` sur l'hôte | `sh -c <to_shell_words().join(" ")>` : **concaténation sans quoting**, comportement identique à main (#63034 vise à le corriger) | `crates/dev_container/src/docker.rs:389` |
| `initializeCommand` | hôte projet | `HostCommand` via `host.run` | `devcontainer_json.rs` (~402) |
| Lecture de `devcontainer.json`, `.env`, compose | hôte projet | `cat` (Unix) ou PowerShell `ReadAllBytes` (Windows) | `project_host.rs:560` |
| Écriture des Dockerfile générés et de `devcontainer.json` fusionné | hôte projet | `sh -c 'cat > "$1"'` + stdin (Unix) ; PowerShell + stdin (Windows) | `project_host.rs:697` |
| Répertoire temporaire | hôte projet | `sh -c 'printf %s "${TMPDIR:-/tmp}"'` ou `cmd /C echo %TEMP%`, puis `…/devcontainer-zed/container-features-<ts>` | `project_host.rs:526`, `devcontainer_manifest.rs:388`, `:437-444` |
| Contexte de build / workspace | **déjà sur l'hôte** (bind mount natif) ; aucun transfert | `calculate_context_dir` = `config_directory.join(context)` en sémantique hôte | `devcontainer_manifest.rs` (~2646) |
| Features OCI | téléchargées **sur le poste** (HTTP + identifiants du poste), décompressées dans `std::env::temp_dir()`, puis `stage_assets` → `RemoteConnection::upload_directory` vers l'hôte | | `devcontainer_manifest.rs:450`, `:600` |
| Features locales (`./feature`) | copie hôte → hôte (`cp -R`) | | `devcontainer_manifest.rs:397-420`, `project_host.rs:633` |
| `id -u` / `id -g` (remap UID) | hôte projet | `HostCommand::new("id").arg("-u")` ; remap désactivé si **l'hôte** est Windows (et non plus via `cfg(windows)` du poste) | `devcontainer_manifest.rs:1666`, `:1696`, `:1739` |
| Environnement shell (`${localEnv}`) | hôte projet | `ProjectEnvironment::directory_environment` (RPC `GetDirectoryEnvironment` si distant) | `dev_container/src/lib.rs` (`environment()`), test `dev_container_environment_uses_the_remote_project_route` |
| Upload de `remote_server` dans le conteneur | poste → hôte → conteneur | lecture locale du binaire, `project_host.write_file(<tmp>/zed-<pid>-<nom>)` (flux stdin via ssh), puis `docker cp` et `docker exec chown` sur l'hôte | `crates/remote/src/transport/docker.rs:543-583` |
| `upload_directory` (extensions…) | poste → hôte → conteneur | `stage_assets` vers `<tmp>/zed-<pid>-directory-upload`, puis `docker cp` et `chown` | `transport/docker.rs:864-917` |
| Proxy RPC (`zed-remote-server proxy`) | hôte projet | `start_process(docker exec -i …)` puis `into_child()` ; le proxy RPC reste un enfant local (ssh) | `transport/docker.rs` (~812-840) |
| Terminal / commandes interactives dans le conteneur | hôte projet | `project_host.build_command(docker exec -it …, interactive)`, donc `ssh -t` | `transport/docker.rs:1003-1012` |
| Redirection de ports | **non supportée** | `build_forward_ports_command` → `Err("Not currently supported for docker_exec")` | `transport/docker.rs:1026` |
| Identité / persistance | qualifiées par l'hôte | clé `host-docker:<identité hôte>:<root>:<config>:<user>@<name>:<id>` ; colonne SQL `transition_provenance` ; options SSH/WSL sérialisées en JSON dans la colonne `host` | `remote_identity.rs:62`, `workspace/src/persistence.rs:1056`, `:2058` |
| Réouverture depuis les paramètres | settings `dev_container` + `project_host` | reconstruit `HostDocker` si les trois champs (`project_host`, `project_root`, `devcontainer_config`) sont présents | `recent_projects/src/remote_connections.rs` (~93-140) |

**Généralisation :**
- **WSL** : oui, en tant qu'hôte projet (`ProjectHostKind::Wsl`, test `a_wsl_project_host_runs_commands_at_its_linux_project_root`). Pas de test de bout en bout.
- **Hôte Windows via SSH** : les chemins Windows et PowerShell sont prévus. Mais `build_command_windows` ignore l'environnement (`ssh.rs:1999`, limite de 8K), donc `DOCKER_BUILDKIT`/`COMPOSE_DOCKER_CLI_BUILD` ne sont pas transmis dans ce cas.
- **WSLc** (Docker Desktop, moteur partagé) : non traité explicitement.
- **Zed dans un conteneur** et conteneur imbriqué : explicitement refusés (`HostDocker`/`Docker` ne sont pas des sources valides, `devcontainer_api.rs:67`).

## Qualité

**Points forts**
- Une seule couture (`ProjectHostCapability`) que tous les appels traversent. La fausse implémentation `RecordingProjectHost` permet de tester un hôte Linux depuis un poste Windows et inversement.
- L'argv reste structuré jusqu'au transport. Aucun chemin « chaîne shell » n'est ajouté pour les commandes docker. Les scripts inline (`sh -c 'cat > "$1"' project-host <path>`) passent les chemins en `$1`/`$2`, sans interpolation. PowerShell utilise `$args[0]`.
- Sémantique des chemins de l'hôte (`HostPathBuf`) : cela corrige une vraie classe de bugs (un chemin Linux lu avec les règles Windows du poste).
- Remap UID décidé selon la plateforme de l'**hôte** (`devcontainer_manifest.rs:1666`). La PR #62680 garde au contraire `cfg(not(windows))`.
- Correctifs annexes utiles :
  - `.devcontainer` découvert même si gitignoré (`worktree.rs:6268`) ;
  - suggestion calculée à partir de l'état du projet et non d'un événement déjà passé (`dev_container_suggest.rs`) ;
  - annulation de la tâche de démarrage quand le modal est fermé (`remote_servers.rs`).
- Tests nombreux et nommés par comportement : 22 tests `project_host` dans `remote`, identité, persistance aller-retour, suggestion distante via `HeadlessProject`.

**Faiblesses et cas non gérés**
- **Nettoyage** : aucun `remove` des artefacts sur l'hôte (`<tmp>/devcontainer-zed/container-features-<ts>`, `zed-<pid>-*` du binaire serveur). Ils s'accumulent sur l'hôte distant. `grep remove` ne trouve rien dans `project_host.rs`, `transport/docker.rs` ni le manifest.
- **Concurrence** : noms temporaires `zed-<pid>-directory-upload` fixes par processus (`transport/docker.rs:877`), donc deux uploads parallèles peuvent se marcher dessus.
- **Hooks** : toujours `join(" ")` (`docker.rs:389`). Conflit direct avec #63034.
- **Pas de redirection de ports** (`forwardPorts`, débogueur) pour `HostDocker`.
- **Settings lossy** : `DevContainerProjectHost::Ssh{host,username,port}` perd `args`, `nickname`, `port_forwards`, `connection_timeout` (`devcontainer_api.rs:294`, `remote_connections.rs`). Une réouverture depuis les paramètres échoue si la connexion SSH dépend de `-i`/`ProxyJump` passés en `args`.
- **Secrets** : la persistance SQL sérialise `SshConnectionOptions` complet, y compris le champ `password`, en JSON (`persistence.rs`, bloc `HostDocker`). La PR #62680 annonce au contraire un « password stripping ». À vérifier et corriger.
- **Seconde connexion** : `new_host_backed` appelle `crate::connect(project_host_options, …)` (`transport/docker.rs:93`). Elle réutilise le pool, mais il n'y a pas de test d'arrêt ou de partage du master SSH.
- **UI** : `HostDocker` est mappé en `ProjectPickerData::Ssh { connection_string: "" }` (`remote_servers.rs:417`). C'est un bricolage.
- **Taille et provenance** : 6 000 lignes, un gros refactoring du manifest, et un code que l'auteur décrit comme « mostly AI, I was starting to self review ». Cela contredit la règle amont « pas de refactorings géants » et la politique IA. Il faut découper.
- `desktop_boundary.rs` : garde-fou original, mais c'est un scanner de code par motifs textuels (398 lignes). Il sera discuté par les mainteneurs et restera fragile.
- **Windows (poste)** : les tests `dev_container` (161) et `remote` (56) passent sous Windows 11. Aucun essai réel SSH ni WSL n'a été fait ici.

## Essai rebase / merge

1. `git rebase origin/main` : le merge `d1f6b3c58b` est linéarisé en 15 commits. Le rebase s'arrête dès le **commit 2/15** (`114a17633c`) sur un conflit dans `crates/dev_container/src/devcontainer_json.rs`. Les commits d'origine sont basés avant `fc9258b463`, donc chaque conflit déjà résolu dans le merge reviendrait. Rebase abandonné (`--abort`).
2. `git merge --no-commit --no-ff origin/main` : **3 fichiers en conflit**.
   - `crates/dev_container/src/devcontainer_json.rs` (1 hunk, imports). Garder `use util::redact::is_valid_environment_name;` (#63606) et supprimer `use util::command::Command;`.
   - `crates/recent_projects/src/dev_container_suggest.rs` (7 hunks) et `crates/recent_projects/src/recent_projects.rs` (1 hunk). `main` a ajouté `open_dev_container_from_cli` (via `6f73c7d0a4`, #63898) pour le même problème d'ouverture `--dev-container` avant les événements. La branche a sa propre version (`open_in_dev_container_from_cli` + `suggest_for_project_state`). Résolution : prise de la version de la branche (`--ours`) pour les deux fichiers.
3. Corrections de compilation après le merge (dues à des évolutions de `main`) :
   - `crates/remote/src/remote_client.rs` : le nouveau `RemoteConnectionOptions::host()` (#54379, `ba6b83f89d`) n'a pas de bras `HostDocker`. Ajout de `HostDocker(opts) => opts.container.name.clone()`.
   - `crates/remote/src/transport/docker.rs` (tests) : le helper `connection()` ajouté par #63606 ne connaît pas `project_host`/`host_backed_options`. Ajout de deux champs `None`.
   - Une erreur `paths::local_dev_container_folder_path` introuvable venait d'une empreinte obsolète dans le `target` partagé, et non du code. Un `touch crates/paths/src/paths.rs` a suffi.
4. Résultats (`CARGO_TARGET_DIR=/h/Sources/zed-wt/target`) :
   - `cargo check -p dev_container -p remote -p recent_projects` → **OK** (3 min 10 s, aucun warning signalé) ;
   - `cargo test -p dev_container` → **161 passed; 0 failed** ;
   - `cargo test -p remote --lib` → **56 passed; 0 failed** (dont 22 `project_host::tests`) ;
   - `recent_projects` : tests non lancés.

La worktree reste dans l'état « merge en cours », non commité, avec les 2 correctifs non indexés. Rien n'a été poussé.

## Recouvrements

| Contribution | Recouvrement | Commentaire |
|---|---|---|
| **#62680** (alexdhill, SSH) | **Fort, concurrent.** Même objectif (moteur sur l'hôte distant). #62680 : `DockerHost { Local, Ssh }` dans `DockerConnectionOptions`, argv identique « byte identical », upload par flux tar via `docker exec`, redirection de ports composée avec `ssh -L`, suppression du mot de passe à la persistance, `connection_type` `docker-ssh`. | pupeno couvre en plus WSL, les chemins sémantiques de l'hôte et le remap UID selon la plateforme de l'hôte. #62680 couvre en plus les ports et le nettoyage du mot de passe. Il faut choisir **un** modèle de connexion : variante `HostDocker` (pupeno) ou champ `host` (#62680). |
| **#60975** (alex-berger, cycle de vie) | Moyen. Identité de conteneur liée au projet, reconnect/rebuild. pupeno touche aussi `remote_identity`, `persistence` et `remote_servers` (modal). | Conflits probables dans `persistence.rs`, `remote_identity.rs` et `remote_servers.rs`. `transition_provenance` (root + config) recoupe l'identité « par projet » de #60975. |
| **#63034** (pupeno, argv docker exec) | Direct. `run_docker_exec` et `postStartCommand` sont modifiés des deux côtés. La branche garde `to_shell_words().join(" ")` (`docker.rs:389`). | À fusionner avant, ou à rebaser sur `HostCommand`. |
| **#63606** (mergé sur main, redaction des secrets) | Conflit résolu. `run_docker_command` redacté sur main ; la branche ajoute un chemin `project_host` dans `run_docker_command` (`transport/docker.rs:596-616`). | Confirmé après merge : le chemin distant **contourne la redaction** (worktree `crates/remote/src/transport/docker.rs:603-617`, `stderr` brut dans l'erreur, pas de `redact_arguments`). Le chemin local, lui, redacte (`:629`, `:637`). **Régression de sécurité** à corriger avant toute reprise. |
| **`review/pupeno-wsl`** | Aucun commit commun (merge-base `bc538def45` sur main). Abstraction concurrente (`ContainerEngine`, `ProjectCommand`) pour le seul cas WSL. | `remote-devcontainer` englobe fonctionnellement la branche WSL ; il ne faut pas garder les deux. |

## Verdict par morceau

| Morceau | Verdict | Justification |
|---|---|---|
| `ProjectHost` + `HostProcessRequest`/`HostProcess` (`remote/src/project_host.rs`) | **Reprendre / adapter** | Bonne couture unique au-dessus de `build_command`, qui préserve l'argv. Réduire la surface (Windows/PowerShell seulement si testé), ajouter un nettoyage et des noms temporaires uniques. |
| `HostPathBuf` (chemins sémantiques de l'hôte) | **Reprendre** | Corrige une vraie classe de bugs poste Windows vers hôte Linux. Petite PR autonome, bien testée. |
| `ProjectHostCapability` + `HostCommand` + `RecordingProjectHost` (dev_container) | **Adapter** | Bonne idée de testabilité. Fusionner avec `CommandRunner` de main et intégrer l'argv préservé de #63034 pour les hooks. |
| Réécriture du manifest pour passer par l'hôte (temp, Dockerfiles, features, `id -u`) | **Adapter** | Logique correcte (contexte de build sur l'hôte, features téléchargées localement puis déposées sur l'hôte, UID de l'hôte), mais diff trop gros : à redécouper en PRs mécaniques. |
| `HostDocker` (connexion, identité, persistance, settings) | **Adapter / arbitrer avec #62680** | Identité qualifiée par l'hôte : à garder. Corriger la persistance du `password` et les settings SSH lossy. Ajouter la redirection de ports (reprendre l'approche `ssh -L` de #62680). |
| Upload du `remote_server` et des répertoires via `write_file` + `docker cp` | **Adapter** | Fonctionne, mais laisse des fichiers sur l'hôte et collisionne par PID. L'alternative de #62680 (flux via `docker exec`, sans passage par le disque de l'hôte) est plus propre. |
| `desktop_boundary.rs` (scanner de source) | **Abandonner** (ou garder en local) | Peu idiomatique pour l'amont, fragile. Les tests `RecordingProjectHost` avec plateforme croisée couvrent l'essentiel. |
| `.devcontainer` sous un dépôt ignorant (`worktree.rs`, `paths.rs`) | **Reprendre** | Correctif indépendant, petit et testé : PR séparée. |
| Suggestion à partir de l'état du projet + CLI `--dev-container` | **Remplacer** | `main` a désormais `open_dev_container_from_cli` (#63898). Ne garder que les tests ou cas manquants (suggestion pour un projet distant déjà chargé). |
| Annulation de la tâche de démarrage (`remote_servers.rs`) | **Reprendre** | Petit correctif UX testé. À coordonner avec #60975. |
| Bras `HostDocker` → `ProjectPickerData::Ssh{""}` | **Remplacer** | Bricolage d'UI. |
