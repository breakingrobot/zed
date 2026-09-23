# 08 — Revue adverse du plan Dev Containers

Date : 2026-09-23. Relecteur : revue adverse (lecture seule du code et des autres documents).
Base de code relue : branche `plan/devcontainers` (le code sous `crates/` est identique à `main` = `16c9aa7ea6` :
`git diff 16c9aa7ea6 --stat -- crates` est vide). Les branches `review/*` ont été lues avec `git show` / `git grep`, sans checkout.

## Périmètre et méthode

- **Documents relus** : `README.md`, `01`, `02-overlap-matrix.md` (et les verdicts des `02-contrib-*`), `03-spec-compliance.md`,
  `04-topologies.md`, `04-wslc.md` (en partie), `05`, `06`, `07`, `adr/ADR-001…010`, `fixtures/devcontainer-id`,
  `fixtures/wslc-2.9.12/run.help.txt`, `raw/INDEX.md`, `raw/discussion-56252.md`, les commentaires bruts (`raw/*comments.json`).
- **Code relu** :
  - `crates/dev_container/src/*` ;
  - `crates/remote/src/{remote_client.rs, remote_identity.rs, transport/{docker,ssh,wsl,mock}.rs}` ;
  - `crates/workspace/src/persistence.rs`, `crates/agent_ui/src/{thread_metadata_store,terminal_thread_metadata_store}.rs`,
    `crates/git_ui_core/src/created_worktrees.rs` ;
  - `crates/util/src/{command.rs, command/darwin.rs, paths.rs}`, `crates/fs/src/fs.rs` ;
  - `crates/recent_projects/src/*`, `crates/settings_content/src/settings_content.rs` ;
  - `CONTRIBUTING.md`, `docs/src/development/feature-process.md`.
- **Méthode** :
  1. Sondage de 46 références `fichier:ligne` porteuses, relues dans le code ;
  2. recherche de contradictions entre documents ;
  3. vérification de faisabilité des ADR contre les signatures et le comportement réels des transports ;
  4. recherche de risques absents du plan ;
  5. confrontation de la pile de PRs avec `CONTRIBUTING.md` et les données de `raw/`.
- **Légende** : ✅ exacte · ⚠️ ligne décalée ou affirmation incomplète, mais vraie sur le fond · ❌ fausse.
- **Gravité** : **Haute** (décision ou sécurité fausse, pile bloquée) · **Moyenne** (à corriger avant d'écrire du code) · **Basse** (précision).

## 1. Vérification par sondage (46 références)

Bilan : **37 ✅ · 7 ⚠️ · 2 ❌**.

| # | Réf. (doc) | Affirmation | Verdict | Preuve (extrait court) |
|---|---|---|---|---|
| 1 | `dc/docker.rs:365-386` (01, 03 L1) | hooks : `sh -c` sur `join(" ")`, aucun quoting | ✅ | l.383 `command.arg("sh")` … l.391 `command.args(&["-c", &inner_program_script.join(" ")])` (fonction l.357-400) |
| 2 | `dc/docker.rs:193 / 488` (01) | `Docker` / trait `DockerClient` | ✅ | l.193 `pub(crate) struct Docker {` ; trait l.488-516 |
| 3 | `dc/docker.rs:218-221` (01) | `docker buildx version` lancé sur le client | ✅ | l.218-221 `Command::new(docker_cli).args(["buildx", "version"])` |
| 4 | `dc/docker.rs:270` (01) | `inspect` sans `--` | ✅ | l.270 `command.args(&["inspect", "--format={{json . }}", id]);` |
| 5 | `manifest:1699,1717` (01, 03 U2) | `id -u`/`id -g` locaux, hors `CommandRunner` | ✅ | l.1699 `let host_uid = Command::new("id").arg("-u")` ; l.1717 idem `-g` |
| 6 | `manifest:1916` (01, 03 B1, ADR-006) | `buildx build` codé en dur | ✅ (incomplet, voir S-1) | l.1916 `command.args(["buildx", "build"]);` ; mais `Docker::new` honore `dev_container_use_buildkit` (`docker.rs:205-229`) et le repli classique existe déjà pour compose (`docker.rs:323-331`) |
| 7 | `manifest:113-124` (01, 03 V1) | `devcontainerId` = `DefaultHasher`, 16 hex | ✅ | l.117 `DefaultHasher::new()` … l.123 `format!("{:016x}", hasher.finish())` |
| 8 | `manifest:126-138` (01, 03 I1) | labels `local_folder` / `config_file` | ✅ | l.126-138 `identifying_labels` |
| 9 | `manifest:208-223` (03 E2, ADR-008) | `remote_env` = tout l'env du conteneur (moins `HOME`) + `remoteEnv` | ✅ | l.212 `let mut merged_remote_env = container_env.clone();` l.214 `remove("HOME")` |
| 10 | `persistence.rs:1035` (03 E2, ADR-008) | colonne `remote_env` | ✅ | l.1035 `ALTER TABLE remote_connections ADD COLUMN remote_env TEXT;` |
| 11 | `persistence.rs:1754` (03 E2) | `remote_env` sérialisé en clair | ✅ | l.1754 `remote_env = serde_json::to_string(&options.remote_env).ok();` |
| 12 | `persistence.rs:1692` (01) | `get_or_create_remote_connection` | ✅ | l.1692 ; clé de recherche l.1790-1808 (inclut `container_id`) |
| 13 | `remote_identity.rs:20-24` (01) | `Docker { container_id, name, remote_user }` | ✅ | l.20-24 |
| 14 | `remote_identity.rs:49-53` (ADR-005, 03 I4) | `docker:{remote_user}@{name}:{container_id}` | ✅ | l.53 `format!("docker:{remote_user}@{name}:{container_id}")` |
| 15 | `manifest:3027-3035` (01) | `'` échappé en `\'` dans des quotes simples | ✅ | l.3028 `.replace('\'', "\\'")` puis `getent passwd '{shell}'` |
| 16 | `manifest:3365-3369 → 3396` (01) | `join(" ")` dans le script marqueur postStart | ✅ | l.3365-3369 `command_to_shell_string` … `command_parts.join(" ")` ; l.3396 `script.push_str(&command_to_shell_string(&command))` |
| 17 | `manifest:1835-1838` (01, 03 U3) | `ENV {key}={value}` non échappé | ✅ | l.1837 `dockerfile = format!("{dockerfile}ENV {key}={value}\n");` |
| 18 | `manifest:880-894` (01) | entrypoints de features concaténés | ✅ | l.886-888 `entrypoint_script_lines.push(entrypoint.clone())` puis `join("\n")` |
| 19 | `rt:646-649` (01, ADR-004) | `kill <pid>` externe → échec sous Windows | ✅ | l.646 `util::command::new_command("kill").arg(pid.to_string()).spawn()` ; appelé au redémarrage du proxy (l.679-683) |
| 20 | `rt:884-895` (01) | env filtré par nom et passé en `-e K=V` | ✅ | l.890-894 `is_valid_environment_name` … `args.push(format!("{name}={value}"))` |
| 21 | `wsl.rs:545-563` (05 §7) | quoting via `ShellKind` | ✅ | l.545 `String::from("exec env ")`, l.549 `shell_kind.try_quote(&assignment)` |
| 22 | `ssh.rs:1909-1928` (05 §7) | idem pour SSH | ✅ | l.1909 `write!(exec, "exec env ")` ; l.1913 `try_quote` ; l.1922 `try_quote_prefix_aware` |
| 23 | `remote_client.rs:1640-1648` (ADR-002) | signature de `build_command` | ✅ | `(program: Option<String>, args, env, working_dir, port_forward, interactive) -> Result<CommandTemplate>` |
| 24 | `worktree.rs:922-929` (ADR-003) | `load_file` refuse les worktrees distants | ✅ | l.926 `"remote worktrees can't yet load files"` |
| 25 | `util/src/paths.rs:359-380` (ADR-003) | `RemotePathBuf` sans `join`/`parent` | ✅ | seules méthodes : `new`, `from_str`, `path_style`, `to_proto` |
| 26 | `manifest:2186-2204` (03 B5, ADR-006) | options Podman ajoutées sur tout OS | ⚠️ | vrai, mais aux l.2194-2201 : `if &docker_cli == "podman" { run_if_missing("--security-opt", "--security-opt=label=disable" …` |
| 27 | `manifest:708-711` (ADR-007) | UID désactivé si **client** Windows | ✅ | l.708-711 `#[cfg(target_os = "windows")] let update_remote_user_uid = false;` |
| 28 | `manifest:3449-3465` (ADR-007, 03 I1) | normalisation des labels selon `cfg(windows)` | ✅ | l.3449-3465 `normalize_label_path` |
| 29 | `devcontainer_json.rs:366` (ADR-007, 03 L2) | `/bin/sh -c` même sous Windows | ✅ | l.366-367 `vec!["/bin/sh".to_string(), "-c".to_string(), …]` |
| 30 | `recent_projects.rs:508-521` (01, ADR-009) | refus « Cannot open Dev Container from remote project » | ✅ | l.507-512 |
| 31 | `recent_projects.rs:2191-2208` (01, 03 L10) | réouverture depuis les récents sans `docker start` ni hooks | ✅ | l.2191-2208 : `open_remote_project(connection.clone(), …)` direct |
| 32 | `devcontainer_json.rs:411-416` (03 L2) | `initializeCommand` : exit ≠ 0 seulement journalisé | ✅ | l.410-415 `if !output.status.success() { … log::error!(…) }` puis `Ok(())` |
| 33 | `features.rs:280-286` (01, 03 L5) | hooks de features recopiés dans le label, jamais exécutés | ✅ | l.279-289 : insertion dans `entry` ; aucune lecture de ces clés dans `manifest` hors tests |
| 34 | `manifest:2137-2157` (03 W1) | mount par défaut « **sans** `consistency` », W1 ✅ | ❌ | `MountDefinition::fmt` écrit **toujours** `consistency=cached` : `devcontainer_json.rs:95` `write!(f, ",target={},consistency=cached", self.target)` (y compris pour tmpfs, test l.1610) |
| 35 | `devcontainer_json.rs:308` (03 W2) | `workspaceMount`/`workspaceFolder` exigés ensemble | ⚠️ | fonction l.308, condition l.313-322 |
| 36 | `devcontainer_json.rs:214/216/243/244` (01 H4, 03) | `userEnvProbe`, `shutdownAction`, `waitFor`, `hostRequirements` parsés, ignorés | ✅ | champs privés aux l.214, 216, 243, 244 |
| 37 | `manifest:2265-2271` (03 R3) | `forwardPorts` numériques → `-p n:n` (toutes interfaces) | ✅ | l.2267-2269 `command.arg("-p"); command.arg(format!("{port_number}:{port_number}"))` |
| 38 | `manifest:444,1170,1269,1336` + `:4172` + `api:366` (01 §3) | 4 `temp_dir()` en prod + 1 test + 1 dans l'API | ✅ | `std::env::temp_dir().join("devcontainer-zed")` aux 4 lignes ; noms fixes `docker_compose_build.json` (l.1171), `docker_compose_runtime.json` (l.1337) |
| 39 | `util/src/command.rs:28-43` (01 §2) | « wrapper `smol::process::Command` ; sous Windows seulement `CREATE_NO_WINDOW` » | ⚠️ | vrai hors macOS ; sous macOS, `Command` est une implémentation `posix_spawn` propre (`command.rs:8-9` `pub use darwin::{Child, Command, Stdio}`, `command/darwin.rs`, ≈1 100 lignes) |
| 40 | 01 §0 « manifest 8 718 lignes, dc ≈ 15 070 » | taille | ⚠️ | ce sont des lignes **non vides** ; `wc -l` : manifest 9 787, `dc/` 16 726 |
| 41 | 06 §0 « KyleBarton, 23 commits » | auteur principal du crate | ⚠️ | `git shortlog -sn 16c9aa7ea6 -- crates/dev_container` → **22** ; le rôle est vérifiable (voir C-4) |
| 42 | ADR-002 « `FakeRemoteConnection` de #62680, `remote/src/transport/mock.rs` » | emplacement du faux transport | ⚠️ | `FakeRemoteConnection` est dans `dev_container` sur la branche (`crate::FakeRemoteConnection`, `review/pr-62680-remote:…/devcontainer_manifest.rs:3924`) ; `mock.rs` contient `MockRemoteConnection` (l.65, impl l.187) |
| 43 | 02 §8 « #59500 […] ouverte par un mainteneur » | statut de l'auteur | ❌ | `raw/discussion-56252.md` : « macraig (authorAssociation **CONTRIBUTOR** — staff status NOT verified) » |
| 44 | `rp/remote_connections.rs:96` (01, H5) | `upload_binary_over_docker_exec: false` codé en dur | ✅ | l.96 |
| 45 | `dc/devcontainer_api.rs:309-317` (01) | `docker|podman --version` local | ✅ | l.309-316 ; **seul l'échec de spawn** est testé (`Ok(_) => Ok(())`, code de sortie ignoré) |
| 46 | `02-overlap-matrix` « `ssh.rs:1999` env ignoré par `build_command_windows` » | env perdu vers un hôte SSH Windows | ⚠️ | vrai : paramètre `_input_env` l.1971, boucle commentée vers l.2003 |

Conclusion du sondage : les références de `01` sont **fiables**. Aucune erreur sur les affirmations de bug ou de sécurité.
Les deux ❌ portent sur une ligne de conformité (`03` W1) et sur le statut d'un tiers (`02` §8). Aucun des deux n'est anodin :
le premier impacte ADR-006 (Podman, wslc), le second la stratégie de coordination.

## 2. Contradictions internes

**C-1 — `consistency=cached` : 03 W1 vs code vs ADR-006 (Moyenne).**
- `03` W1 déclare le mount par défaut « sans `consistency` », conforme ✅.
- ADR-006 prévoit « Podman : pas de `consistency` », ce qui suppose qu'il y en a.
- Le code l'ajoute à **tous** les mounts (`devcontainer_json.rs:95`), tmpfs compris.
- `04-wslc.md:259` établit que wslc **rejette** `consistency=` avec une erreur « unsupported ».

*Correction* :
- passer W1 en ⚠️ (m) : `consistency` ajouté hors spec sous Linux/Podman et sur tmpfs/volumes ;
- ajouter à ADR-006 : « ne jamais émettre `consistency` pour Podman/Wslc ; ne l'émettre que pour les bind mounts » ;
- ajouter un test N1.

**C-2 — `HostPathBuf` vs `RemotePathBuf` (Basse).**
- `02-overlap-matrix.md` §4 et §7 (« `HostPathBuf` … **Reprendre** ») et `04-topologies.md` K3 retiennent `HostPathBuf`.
- ADR-003 et `05` §3 choisissent d'étendre `RemotePathBuf`, sans dire que la décision est renversée ni pourquoi.

*Correction* : ajouter à ADR-003 une ligne « remplace la recommandation de 02 §7 » avec la raison (pas de nouveau type public dans `util`), et mettre à jour K3.

**C-3 — WSLc et features (Moyenne).**
- `04-topologies.md` Q3, `04-wslc.md` §6 (« limitée aux configurations `image`/`dockerFile` ») et `07` §3 (T8 : « pas compose, pas features ») excluent les features.
- ADR-006 et `05` §5 (T8 : « features sans `--build-context` (copie dans le contexte) ») les **incluent**.
- Le mécanisme « copie dans le contexte » n'est pas spécifié (voir F-6).

*Correction* : aligner ADR-006 sur Q3 (« v1 wslc : pas de features ») ou spécifier le mécanisme et l'ajouter aux fixtures.

**C-4 — Statut des interlocuteurs (Moyenne).**
- `02` §8 affirme que #59500 a été ouverte par un mainteneur (❌, sondage #43).
- `06` §0 écrit « KyleBarton … rôle dans l'équipe non vérifié ». Or `raw/` le montre :
  - `raw/pr-56293.json` → `merged_by: KyleBarton` ;
  - `raw/pr-56293-comments.json` → KyleBarton (**COLLABORATOR**) poste `@zed-industries/approved` ;
  - il répond comme mainteneur dans #51664, #53635 et #54452.

*Correction* : dans `02` §8, remplacer par « #59500 : ouverte par un contributeur (macraig), sans validation staff ». Dans `06` §0, écrire « KyleBarton : a les droits de merge sur le crate (preuves ci-dessus) ; sa PR #58500 est elle-même sans revue depuis le 2026-06-04 ».

**C-5 — Vagues de `06` §3 incompatibles avec le graphe de dépendances et le plafond (Haute).**
- Vague 3 = C1 + C3, alors que C3 dépend de C1.
- Vague 4 = C2 + C4, alors que C4 dépend de C2 (et de C3).
- Vague 5 = C5 + C6, alors que C6 dépend de C5.
- Vague 2 = A3, A5, A6, mais A6 = « deux PRs S », donc **4 PRs** : le plafond de 3 est dépassé.

Des PRs empilées ouvertes en même temps comptent toutes dans le plafond, et leurs dépendances les laisseraient en attente.

*Correction* : recalculer les vagues par tri topologique, au plus 3 PRs ouvertes, aucune PR dans la même vague que sa dépendance. Voir aussi P-1.

**C-6 — Réouverture depuis les récents vs « `initializeCommand` à chaque ouverture » (Moyenne).**
- ADR-010 et A2 promettent « à chaque ouverture ».
- La réouverture depuis les récents (`recent_projects.rs:2191-2208`) court-circuite le manifest (ni `start`, ni hooks : `03` L10). Elle n'est traitée que par E1, vague 6.
- A2 ne couvre donc que le chemin `OpenDevContainer`.

*Correction* : réécrire A2 en « à chaque passage par `spawn_dev_container` », et lier explicitement L2 et L10 (E1) dans ADR-010.

**C-7 — `05`/ADR-003 : sujet de la suppression du `BuildDir` (Basse).**
- `05` §3 dit « supprimé au `Drop`/fin ».
- ADR-003 dit « supprimé en fin de build ».
- Un `Drop` ne peut pas lancer un `rm -rf` distant asynchrone de façon fiable (connexion possiblement tombée, pas d'exécuteur garanti).

*Correction* : supprimer « `Drop` » ; nettoyage explicite dans un `finally` asynchrone, plus le nettoyage opportuniste.
Au passage, ADR-003 mélange deux schémas de nommage (`zed-devcontainer-build-<uuid>` et `mktemp -d`) : garder
`mktemp -d -t zed-devcontainer-build.XXXXXXXX` (création atomique, mode 0700).

**C-8 — Lecture des fichiers de l'hôte : l'alternative RPC n'est jamais évaluée (Moyenne).**
- `02` §7 recommande « préférer le `Fs` RPC du `remote_client` quand disponible » ; `04-topologies.md` K2 dit « RPC `Fs` à évaluer ».
- ADR-003 retient `cat`/`mktemp`/`rm` sans citer cette option.
- Pour T2-T4, un serveur distant Zed tourne **déjà** sur l'hôte, avec un `Fs` complet et indépendant du shell.

*Correction* : ajouter l'alternative à ADR-003 et trancher par écrit (latence, disponibilité avant connexion, hôte Windows).

**C-9 — Gravités divergentes entre 01 §5 et 03 (Basse).**

| Écart | `01` §5 | `03` |
|---|---|---|
| Lifecycle des features | « moyenne » | **B** |
| `initializeCommand` | « moyenne » | **B** |
| E2 (secrets persistés) | absent | **B (sécurité)** |

*Correction* : aligner `01` §5 sur `03`, qui fait foi.

**C-10 — Éléments « Reprendre » de 02 §7 absents de 06 (Basse).**
Absents de la pile :
- `open_in_dev_container` pour projets distants (pupeno-wsl) ;
- découverte de `.devcontainer` gitignoré ;
- annulation du démarrage (pupeno-remote).

*Correction* : les ajouter à 06 (Piste A ou E) ou les déclasser explicitement.

**C-11 — README obsolète (Basse).**
- Les phases 5 à 7 sont marquées « — », alors que les livrables existent.
- `05` s'appuie sur les hypothèses du point d'arrêt 2, toujours « en attente ».

*Correction* : statut « brouillon, dépend du point d'arrêt 2 » sur 05/06/07 et les ADR.

## 3. Faisabilité technique des ADR

**F-1 — ADR-002 : `build_command` convient, mais le shell distant n'est pas un shell de login (Haute).**
- Ce qui est faisable :
  - SSH construit `cd <cwd> && exec env 'K=V' <prog> <args quotés>` (`ssh.rs:1866-1929`) ;
  - WSL construit `wsl.exe --distribution D --cd <cwd> -- <shell> -c "exec env …"` (`wsl.rs:541-592`) ;
  - `env` et `current_dir` **ne sont donc pas perdus** ; c'est déjà le modèle de `path_exists` (`rp/remote_connections.rs:479-496`).
- Ce qui manque :
  - (a) `exec env` dans un shell **non login, non interactif** : `PATH` et `DOCKER_HOST` peuvent différer de ceux d'un terminal. Cas typiques :
    - Docker/Podman via Homebrew sur un hôte macOS (`/opt/homebrew/bin`) ;
    - Docker rootless dont `DOCKER_HOST=unix:///run/user/<uid>/docker.sock` est posé dans un `.bashrc`, qui sort tôt en non-interactif (Ubuntu) → le CLI parle **silencieusement** à un autre démon, ou à aucun.
  - (b) Le Windows côté SSH ignore `env` (`ssh.rs:1971`) : la garde T4w doit précéder tout appel.
  - (c) `dev_container` ne dépend pas de `remote` (`dev_container/Cargo.toml`) : C1 ajoute cette dépendance, à signaler dans la PR.
  - (d) `RemoteClient::connection()` renvoie `Option<Arc<…>>` : `None` ou connexion périmée pendant une reconnexion.

*Correction* : dans ADR-002 ou ADR-007, ajouter une **sonde d'environnement de l'hôte** (une fois par hôte : `$SHELL -l -c 'command -v docker podman; printenv DOCKER_HOST DOCKER_CONTEXT'`), un réglage de chemin du binaire, et « résoudre la connexion à chaque commande, échouer proprement si `None` ».

**F-2 — Le filet de sécurité de C1 (« égalité d'argv avant/après ») ne peut pas être construit comme décrit (Haute).**
1. `util::command::Command` n'expose que `get_program` et `get_args` (`command.rs:59,125` ; `darwin.rs:107,225`) : ni `get_envs`, ni `get_current_dir`. L'env (`DOCKER_BUILDKIT`, `docker.rs:324-330`) et le cwd de `initializeCommand` (`devcontainer_json.rs:402`) sont donc **invisibles** dans l'enregistrement de `07` §4 (« `program + args + env + cwd` »).
2. Les sites qui contournent `CommandRunner` ne peuvent pas être interceptés **avant** le refactor sans modifier le code d'abord :
   - `manifest:1699, 1717` ;
   - `docker.rs:218` ;
   - `devcontainer_api.rs:309` ;
   - les `Command::new` de `Docker`.

*Correction* : C1 introduit d'abord `HostCommand` (données pures, `PartialEq`, `Debug`) et fait construire chaque site un `HostCommand` converti en `Command`. Les goldens se capturent sur `HostCommand`. L'égalité « avant/après » se vérifie par relecture et par les tests `FakeDocker` existants, pas par une capture sur `main`. Sinon, une PR préalable ajoute des getters à `util::command::Command` (petite, mais transverse).

**F-3 — ADR-003 : écrire par stdin fonctionne, mais le coût est mal estimé (Moyenne).**
- La faisabilité est acquise : `Interactive::No` donne `ssh -T` (`ssh.rs:1955`), et stdin est pipé sur le `Command` construit (comme `docker.rs:716-719`).
- Coût :
  - sous **Windows**, SSH n'a pas de ControlMaster (`ssh.rs:236-238`) : **chaque** commande ouvre une connexion SSH complète et rejoue le mot de passe via askpass (`ssh.rs:1315-1340`) ;
  - une feature compte plusieurs fichiers, et un build complet plusieurs dizaines de commandes (`ps`, `inspect`, `-v`, `buildx version`, `id` ×2, `cat` ×N, `mktemp`, écritures ×N, `build`, `run`, hooks, `rm`) ;
  - avec 2FA ou `MaxStartups`, cela échoue.

*Correction* :
- transférer le `BuildDir` en **un seul flux `tar -x -C <dir>`** ;
- regrouper les lectures ;
- valider le chemin avant `rm -rf -- <dir>` (préfixe attendu, pas de `..`) ;
- nettoyage opportuniste limité à `-maxdepth 1 -user "$(id -u)" -type d` ;
- mesurer le nombre d'allers-retours par topologie dans `07`.

**F-4 — ADR-004 : `DockerHost::Ssh(SshConnectionOptions)` contredit « persisté sans secret » (Haute).**
- (a) `SshConnectionOptions` sérialise `password` (`ssh.rs:136-141`).
  - #62680 ne le retire que dans la table `remote_connections` (`review/pr-62680-remote:…/persistence.rs:1085-1103`).
  - Mais `sidebar_threads.remote_connection` et `sidebar_terminal_threads.remote_connection` stockent le JSON **complet** de `RemoteConnectionOptions` (`agent_ui/src/thread_metadata_store.rs:1405, 1525-1530` ; `terminal_thread_metadata_store.rs:545`).
- (b) `DockerConnectionOptions` dérive `PartialOrd, Ord` (`rt:36-47`), `SshConnectionOptions` non : #62680 a dû supprimer `Ord`. Les usages sont à vérifier.
- (c) Le pool est indexé par l'égalité complète des options (`remote_client.rs:1262-1319`), y compris `remote_env` et l'hôte.
- (d) La reconnexion de l'hôte (qui relance le maître SSH ?) n'est pas décrite. #62680 la traite (`review/pr-62680-remote:…/remote_client.rs:1514-1554`), l'ADR non.
- (e) « Arrêt du proxy via le handle du processus » : le `Child` est **déplacé** dans `handle_rpc_messages_over_child_process_stdio` (`rt:731-738`), ce qui demande une restructuration. En distant, tuer le `ssh` local ne garantit pas la mort du `docker exec` distant (pas de TTY avec `-T`) : des proxys orphelins sont possibles.

*Correction* :
- type dédié `DockerSshHost { host, username, port, args, nickname }` sans `password`, avec `Ord` manuel ou sans `Ord` ;
- section « reconnexion » dans l'ADR ;
- test N4 : aucune occurrence de `password` ni de valeurs d'env dans `sidebar_threads`.

**F-5 — ADR-005 : la migration oublie les autres consommateurs de l'identité (Moyenne).**
- Les fils d'agent retrouvent leur connexion via `same_remote_connection_identity` appliqué aux **options stockées** (`thread_metadata_store.rs:358-362`). Les lignes existantes n'ont ni `project_root` ni `config_file` → repli `container:{id}` → **#56576 non résolu pour les fils existants** tant que le JSON de `sidebar_threads` n'est pas migré.
- `created_worktrees.rs:45` utilise `persistence_key()` comme clé d'enregistrement : changer le format rend ces enregistrements orphelins.
- Le séparateur `|` peut apparaître dans un chemin Linux (collision théorique).
- Windows : casse des chemins.
- WSL : `user = None` et utilisateur explicite donnent deux clés pour la même distro.

*Correction* :
- sérialiser la clé en JSON ou préfixer par la longueur ;
- migration de `sidebar_threads` et de `created_worktrees`, ou double lookup ;
- tests « fil créé avant migration retrouvé après rebuild ».

**F-6 — ADR-006 : `ContainerCli` est placé dans le mauvais crate ; plusieurs détails wslc sont faux ou manquants (Moyenne).**
- **Crate** : `DockerExecConnection` (crate `remote`) doit aussi connaître la variante (`cp` vs `container cp`, chemin `wslc.exe`, pas de `--format`). Or `remote` ne dépend pas de `dev_container`. L'enum doit vivre dans `remote` ou plus bas. Le booléen `use_podman` est persisté en base (`persistence.rs:1032`) et dans les réglages (`DevContainerConnection.use_podman`) : une migration est nécessaire.
- **wslc** :
  - `--mount` **existe** dans wslc 2.9.12 (`fixtures/wslc-2.9.12/run.help.txt:38`, `create.help.txt:37`). Convertir en `-v` est un choix (celui du CLI de référence), pas une nécessité ; le vrai bloquant est `consistency` (C-1) ;
  - `-v` **crée** la source manquante (`04-wslc.md:296`) : une faute de frappe produit un dossier vide, il faut vérifier l'existence avant ;
  - les labels d'image ne sont pas hérités (`04-wslc.md:360`) → `devcontainer.metadata` perdu (`remoteUser`, hooks). L'ADR ne le mentionne pas ;
  - « contenu des features copié dans le contexte » : le contexte est souvent le dossier du projet, qu'il **ne faut pas modifier**. Préférer le chemin classique existant (image `dev_container_feature_content_temp`, `manifest:1843-1879`), ou exclure les features (C-3).
- **Réglages** : `remote.dev_container_use_buildkit` (`settings_content.rs:1363-1372`) n'est pas mentionné. Il est **ignoré** pour image/Dockerfile (`manifest:648` `use_buildkit = … || !is_compose`). Son interaction avec le nouveau `remote.dev_container_cli` est à définir.

**F-7 — ADR-010 : trois décisions sur-spécifiées (Moyenne).**
- **Marqueurs** : la compatibilité est démontrée avec le **CLI** (`$HOME/.devcontainer`, `03-spec-reference.md:71` ; Zed l'utilise déjà pour postStart, `manifest:3383-3384`). Avec **VS Code**, elle n'est pas démontrée : l'extension peut passer un autre `--container-data-folder`. « Zed et VS Code se comportent pareil » est donc non sourcé (S-2).
- **postAttach à chaque (re)connexion** : la reconnexion du transport (`start_proxy(reconnect=true)`, `rt:665-707`) n'a pas accès à la config. Il faudrait persister le hook ou relire le label. Et une reconnexion réseau n'est pas un « attach » au sens de la spec : risque de relancer un script à chaque micro-coupure.
- **`initializeCommand` fatal** : c'est un changement de comportement pour des configurations qui fonctionnent aujourd'hui malgré un exit ≠ 0. À annoncer, voire à placer derrière une période d'avertissement.

**F-8 — ADR-009 : l'heuristique T5 bloquerait T1c (Haute).**
- « `DOCKER_HOST` ou contexte non-unix en local ⇒ avertissement bloquant » : sous Windows, le point d'accès **par défaut** de Docker Desktop et de Podman est `npipe://…`, donc « non-unix ». Tous les utilisateurs Windows natifs seraient bloqués.
- `tcp://localhost` est légitime.
- La détection T6a par `/.dockerenv`/cgroup donne des faux positifs sous toolbox/distrobox (`/run/.containerenv`, mêmes chemins que l'hôte : ça marche aujourd'hui).

*Correction* : bloquer seulement si l'endpoint est `ssh://` ou `tcp://` vers un hôte non local, ou si un contexte distant est actif. `npipe`/`unix` locaux = OK. T6a en avertissement non bloquant.

**F-9 — ADR-007 : faisable (Basse).**
- `remote_platform()` et `path_style()` existent sur le trait (`remote_client.rs:1654-1656`).
- Un point à préciser : en T1c (Windows + Docker Desktop), la « plateforme de l'hôte » est Windows alors que le démon est Linux. ADR-007 désactive donc `updateRemoteUserUID`, ce qui est conforme au CLI. À écrire explicitement pour éviter un « correctif » ultérieur.

## 4. Risques non couverts

**R-1 — Workspace Trust absent du plan (Haute).**
- `docs/src/development/feature-process.md` §3 demande explicitement : « How does this feature interact with Workspace Trust? ». Zed a un mécanisme de confiance (`project/src/trusted_worktrees.rs`).
- Aucun document du plan n'en parle ; aucune vérification de confiance dans `dev_container` ou `recent_projects/src/dev_container_suggest.rs`.
- Or ouvrir un dev container exécute du code du dépôt **sur l'hôte** :
  - `initializeCommand` ;
  - `runArgs` verbatim (`--privileged`, `-v /:/host`) ;
  - `build.options` ;
  - installation automatique des extensions de `customizations.zed` (`remote_servers.rs:2276-2283`) ;
  - le tout aussi via le flag `--dev-container`.
- A2 (« à chaque ouverture ») et C6/C7 (sur des serveurs partagés) **augmentent** cette surface.

*Correction* : ADR dédié « confiance » ; exiger un worktree de confiance avant tout `initializeCommand`/build ; rubrique obligatoire du message C0.

**R-2 — Fichiers temporaires à noms fixes dans un `/tmp` partagé (Linux) : c'est un problème de sécurité, pas une « collision » (Haute).**
- Mécanisme :
  - `temp_dir()` vaut `/tmp` sous Linux ;
  - `/tmp/devcontainer-zed/` est créé par `create_dir` s'il n'existe pas ;
  - les fichiers sont écrits par `std::fs::write` (`fs/src/fs.rs:1037-1051`), qui **suit les liens symboliques**.
- Scénario : un autre utilisateur local pré-crée `/tmp/devcontainer-zed/` (il en est alors propriétaire), puis :
  - (a) remplace `docker_compose_runtime.json` par un lien vers un fichier de la victime (écrasement) ;
  - (b) ou réécrit l'override entre l'écriture et `compose up` (`privileged: true`, `volumes: ["/:/host"]`) : conteneur privilégié lancé par la victime, qui a l'accès Docker (≈ root).
- Les options de features (parfois des jetons) sont aussi lisibles par tous.
- `01` §5 et `03` C3 classent cela « basse, collision ». macOS et Windows ne sont pas touchés (répertoire temporaire par utilisateur).

*Correction* : PR de Piste A immédiate (répertoire unique 0700 par build, via `tempfile`/`mktemp`, `O_EXCL`), indépendante de C2 ; décider du canal (pas de `SECURITY.md` à la racine du dépôt : canal à identifier avant de publier).

**R-3 — Secrets : le périmètre d'E2 et d'ADR-008 est incomplet (Haute).**
- (a) Au-delà de `remote_connections.remote_env`, le JSON complet des options (`remote_env` compris) est persisté dans `sidebar_threads` et `sidebar_terminal_threads` (F-4).
- (b) « Persister seulement `remoteEnv` résolu » ne protège pas le cas **le plus courant** (`"remoteEnv": {"GITHUB_TOKEN": "${localEnv:GITHUB_TOKEN}"}`) : le secret reste en base et reste dans l'argv `-e K=V` de chaque `docker exec` (visible par `ps`), localement comme sur l'hôte distant.
- (c) La valeur n'est écrite qu'à l'INSERT et jamais mise à jour (`persistence.rs:1768-1808`) : elle est périmée.

*Correction* :
- persister le **gabarit non résolu** (ou seulement les noms), résoudre au lancement ;
- passer les valeurs par `--env-file` (fichier 0600 dans le `BuildDir`/tmp, supprimé après) ou `-e NOM` + env du processus en local ;
- couvrir les trois tables dans les tests `fixtures/env-secrets`.

**R-4 — `forwardPorts` publiés sur toutes les interfaces d'un serveur SSH (Haute pour C6).**
- `-p n:n` (`manifest:2267-2269`) sur un hôte distant expose les services de dev sur l'IP publique du serveur.
- ADR-004 reporte le forwarding, mais pas la **publication**.

*Correction* : prérequis de C6 : #63899 (liaison loopback) mergée, ou `-p 127.0.0.1:n:n` imposé quand l'hôte ≠ client.

**R-5 — Performances et UX des builds distants (Moyenne).**
- Tous les appels utilisent `.output()` (sortie tamponnée) : pas de journal en continu ni de progression sur un build SSH de plusieurs minutes.
- L'annulation doit tuer le processus **distant** (voir F-4 e).
- Allers-retours : voir F-3.

*Correction* : exigence « flux de logs + annulation » dans C4/C6, et un test.

**R-6 — `${localEnv:…}` sur un hôte distant non spécifié (Moyenne).**
- `DevContainerContext::environment` utilise `local_directory_environment` du client (`dc/lib.rs:126-135`).
- `05` §4 ne dit pas d'où viennent les variables quand l'hôte est SSH/WSL.
- La référence prend l'env de l'hôte du CLI (`03` V2).

*Correction* : une étape explicite dans `05` §4 (env du shell de login de l'hôte, cf. F-1 a).

**R-7 — Issues de `raw/INDEX.md` jamais analysées alors qu'elles touchent des décisions (Moyenne).**

| Issue / PR | Décision concernée |
|---|---|
| #58794 (repli buildx cassé) | A5 / ADR-006 |
| #53529 (le PATH de l'image est modifié) et #49199 (`remoteEnv` absent des shells) | directement liées à la réinjection de l'env complet que ADR-008 veut supprimer |
| #57039 (chemins relatifs dans `runArgs`) | sémantique des chemins en distant |
| #54257 (permission lors de l'upload Podman) | ADR-004, upload |
| #55864 (id périmé) | ADR-005 |
| #47121 (agents SSH/GPG) | attendu en SSH |
| **#58500 (KyleBarton, relabel SELinux Podman)** | touche exactement les options Podman d'A6 / ADR-006 |

*Correction* : une ligne de recoupement par issue dans `03`/`06`.

**R-8 — Réglages et Settings UI (Basse).**
- `feature-process.md` §3 : « Don't forget to add new settings to the Settings UI ».
- Le nouveau `remote.dev_container_cli` et la cohabitation `use_podman` / `dev_container_use_buildkit` / `dev_container_cli` ne sont pas décrits (priorités, dépréciation).

**R-9 — Plateformes (Basse).**
- Hôte SSH macOS : chemins Homebrew (F-1), `id -u` = 501 sans alignement UID (hôte non Linux, conforme).
- Windows ARM : wslc et Docker Desktop arm64 non évalués.
- Podman rootless sur un hôte SSH : `XDG_RUNTIME_DIR`/socket selon la session.
- Ouvertures concurrentes du même projet dans deux fenêtres : deux builds, puis `MultipleMatchingContainers`.

## 5. Réalisme de la pile de PRs (06)

**P-1 — Débit de revue ignoré (Haute).**
- `raw/INDEX.md` recense 11 PRs ouvertes sous `area:dev containers`, **toutes** sans commentaire MEMBER/OWNER, y compris #58500 de KyleBarton (ouverte le 2026-06-04).
- La vague 2 dépend de #63034 (autre auteur, sans revue depuis le 2026-08-21).
- Six vagues séquentielles ⇒ au moins six cycles de merge : à ce débit, plusieurs trimestres.

*Correction* : réduire l'engagement initial à :
1. C0 (discussion) ;
2. une ou deux PRs de correction indiscutables (R-2 temp dir, A2 réduite) ;
3. un soutien actif aux PRs existantes (#63034, #63391, #63899, #64025) : relecture, test, repro.

Le reste de la pile est conditionné à une réponse du staff.

**P-2 — Mauvais canal pour C0 (Moyenne).**
- `CONTRIBUTING.md:33` : « The right place for the proposals is GitHub discussions (not GitHub issues) ». `CONTRIBUTING.md:53-55` et `feature-process.md` : sans issue **confirmée par le staff**, commencer par une Discussion.
- Le brouillon vise l'issue #59500 (ouverte par un contributeur, C-4).

*Correction* : poster dans la discussion **#56252** (l'originale, `raw/discussion-56252.md`) en suivant le gabarit de `feature-process.md` (« Why / What / What else does this affect » : Remote, Persistence, Platform, **Security/Workspace Trust**, Settings UI). Lier #59500 et #56576.

**P-3 — La « Piste A sans accord préalable » contient des changements de comportement (Moyenne).**
- A2 : `initializeCommand` fatal et exécuté à chaque ouverture.
- A3 : exécute du code de features qui n'était pas exécuté.
- A4 : renomme les volumes `…-${devcontainerId}` et perd, par exemple, le cache docker-in-docker.

Ce sont des corrections de conformité, mais elles sont **visibles**. `CONTRIBUTING.md:98-100` : une PR « not obviously great » est fermée.

*Correction* : les lister dans C0 comme « bugfix à comportement visible », avec note de version ; pour A4, garder l'ancien id tant que le conteneur existant est réutilisé.

**P-4 — Répartition nominative et politique IA (Moyenne).**
- `06` §3 attribue C4, C6 et E2 à alexdhill (#62680 « primarily AI generated ») et A1 et C7 à pupeno (« fully vibe coded… haven't read the code »).
- `CONTRIBUTING.md:88` exige un humain qui comprend le code : confier le cœur de la piste C à ces branches augmente le risque de fermeture.
- Attribuer publiquement des tâches à des tiers sans leur accord est maladroit.

*Correction* : ne pas nommer de répartition dans le premier message ; proposer ensuite, en privé ou dans le fil, et seulement à ceux qui confirment pouvoir expliquer leur code.

**P-5 — Brouillon de message : affirmations inexactes ou invérifiables (Moyenne).**

| Passage | Problème | Correction |
|---|---|---|
| « They overlap heavily: every pair of them conflicts in 3–16 files » | ✅ conforme à `02-overlap-matrix.md` §3, mais mesuré sur des merges d'essai de branches **locales**, dont deux ne sont pas des PRs | « in trial merges I ran locally » |
| « each introduces its own "run docker on the project host" abstraction » | ❌ faux pour #60975 (`02-overlap-matrix.md` §2 : « — » sur toutes les lignes d'exécution) | « the three remote-host efforts each introduce… » |
| « this fixes #56576 » | vrai seulement si `sidebar_threads` est migré (F-5) | « this should fix #56576 for new threads; existing threads need a migration » |
| « identical argv for local projects » | promesse non vérifiable en l'état (F-2) | « verified by recorded command tests » |
| citer « pupeno's … branches » | branches sans PR, publiques mais non proposées | demander d'abord à pupeno |
| — (absent) | rien sur la sécurité ni sur Workspace Trust, rubrique exigée par `feature-process.md` | ajouter une phrase |

Rappel (déjà dans `06` §4, à conserver) : réécriture humaine, citations IA signalées.

## 6. Sur-affirmations et affirmations non sourcées

**S-1 — « `buildx` codé en dur … même avec podman → échec » (`01` ligne 6, `03` B1) (Moyenne).**
- Sur la forme, c'est vrai (`manifest:1916`).
- L'échec avec Podman n'est **pas démontré** : `podman buildx build` est un alias de `podman build` dans les versions récentes (**à vérifier**, avec le support de `--load` et de `--build-context`).
- Le plan omet que le réglage `dev_container_use_buildkit` et le repli classique existent déjà (compose), et qu'une issue (#58794) porte justement sur ce repli.

*Correction* : reformuler en « non testé avec Podman ; réglage ignoré pour image/Dockerfile ».

**S-2 — « Zed et VS Code se comportent pareil sur un même conteneur (marqueurs partagés) » (ADR-010) (Moyenne).**
Seul le CLI de référence est sourcé (F-7). *Correction* : « compatibles avec `devcontainers/cli` ; VS Code : non vérifié ».

**S-3 — « W1 ✅ (`consistency` sans effet sur les moteurs récents) » (`03`) (Moyenne).**
Faux sur le fond (C-1) : wslc rejette l'option.

**S-4 — « #59500 ouverte par un mainteneur » (`02` §8) (Moyenne).**
Voir C-4.

**S-5 — `01` §2 : description de `util::command::Command` (Basse).**
Voir sondage #39.

**S-6 — `05` §7 « WSLc … GA visée à l'automne 2026 » et ADR-006 (Basse).**
Aucune source citée dans ADR-006 ; `04-wslc.md` doit porter la référence, ou « non sourcé ».

**S-7 — `06` §2, tailles S/M/L (Basse).**
Estimations sans méthode ; C4 « L » (500-1 000 lignes) risque d'être perçu comme le « giant refactoring » que `CONTRIBUTING.md:81` refuse. *Correction* : découper C4 (transport/pool, puis upload, puis identité hôte).

## 7. Top 10 des corrections à appliquer

1. **Ajouter un ADR « Workspace Trust »** (R-1) : aucune exécution d'`initializeCommand`, de build ni d'installation d'extensions sans worktree de confiance. En faire une rubrique de C0.
2. **Requalifier en sécurité (Haute) le `/tmp/devcontainer-zed` à noms fixes** (R-2). PR Piste A immédiate (répertoire 0700 unique par build), canal de signalement à décider.
3. **Étendre E2/ADR-008** à `sidebar_threads` et `sidebar_terminal_threads` ; persister des gabarits non résolus, pas des valeurs ; passer l'env par `--env-file` 0600 (R-3, F-4).
4. **ADR-004 : remplacer `DockerHost::Ssh(SshConnectionOptions)` par un type sans `password`**, décrire la reconnexion de l'hôte et l'arrêt du proxy distant (F-4).
5. **Refaire le filet de C1** : goldens sur `HostCommand` (pas sur `util::command::Command`, sans getters env/cwd) ; `07` §4 à corriger (F-2).
6. **Sonde d'environnement de l'hôte** (shell de login) pour `PATH`/`DOCKER_HOST`, plus un réglage du chemin du CLI, avant C6/C7 (F-1, R-6).
7. **Corriger ADR-009** : pas de blocage sur `npipe`/`unix` locaux ; T6a non bloquant (F-8).
8. **Recalculer les vagues de `06` §3** (dépendances hors vague, au plus 3 PRs, A6 compté double) et réduire l'engagement initial à C0 + 1-2 correctifs + soutien des PRs existantes (C-5, P-1).
9. **Poster C0 dans la discussion #56252** au format `feature-process.md`, sans répartition nominative, avec les corrections de formulation de P-5 ; corriger « #59500 ouverte par un mainteneur » et documenter le rôle de KyleBarton (P-2, P-4, C-4).
10. **Aligner WSLc et Podman** :
    - `consistency=cached` (C-1 / S-3) ;
    - pas de features en wslc v1, ou mécanisme spécifié sans toucher au dossier du projet (C-3, F-6) ;
    - `ContainerCli` dans le crate `remote`, avec migration de `use_podman` (F-6) ;
    - recouper #58500 et #58794 (R-7).

## 8. Suite donnée (2026-09-23, par l'auteur du plan)

Chaque constat repris ci-dessous a été **revérifié dans le code** avant correction.

| # Top 10 | Vérification | Correction appliquée |
|---|---|---|
| 1 Workspace Trust | `feature-process.md:49` ; `TrustedWorktrees` (`project/src/git_store.rs:11-12`) ; aucune référence dans `dev_container` | ADR-011 ajoutée ; T1 dans `06` ; étape 0 du flux `05` §4 |
| 2 `/tmp` prévisible | `RealFs::write` = `std::fs::write` (`fs/src/fs.rs:1037-1051`) ; noms fixes (`manifest:444, 1170, 1269, 1336`) | requalifié **haute (sécurité)** dans `01` §5 et `03` C3 ; ADR-008 ; A8 **en suspens** (décision utilisateur) |
| 3 secrets `sidebar_*` | `thread_metadata_store.rs:1528`, `terminal_thread_metadata_store.rs:540` (JSON complet de `RemoteConnectionOptions`) | `03` E2 et ADR-008 élargis (gabarits non résolus, `--env-file`) ; B1 inclut les tables `sidebar_*` |
| 4 `password` SSH | `ssh.rs:136-141` | ADR-004 : type `DockerHostSsh` sans secret |
| 5 filet de C1 | `util/src/command.rs:59, 125` (pas de getter env/cwd) | ADR-002 et `07` §4 : goldens sur `HostCommand` |
| 6 env de l'hôte | shell non-login de `build_command` (constat de la revue, non revérifié ligne à ligne) ; pas de ControlMaster Windows (`ssh.rs:236`, vérifié) | C2b dans `06` ; ADR-002 révisée |
| 7 ADR-009 `npipe` | endpoint par défaut de Docker Desktop Windows (connaissance générale, non vérifiée localement : pas de Docker ici) | ADR-009 et `05` §4 révisés |
| 8 vagues | relu `06` §3 | vagues recalculées, engagement initial réduit |
| 9 canal C0, KyleBarton | `CONTRIBUTING.md:33` ; `gh pr view 56293` → `mergedBy: KyleBarton`, `COLLABORATOR` | C0 → discussion #56252, gabarit `feature-process.md`, pas de répartition nominative ; `02` §8 corrigé (#59500) |
| 10 Wslc/Podman | `consistency=cached` à `devcontainer_json.rs:95` (vérifié) | ADR-006 révisée ; `03` W1 corrigé ; A6a ; #58794 **non vérifiée** |

Autres corrections : tailles de `01` §0 (`wc -l`), emplacement de `FakeRemoteConnection` (ADR-002).
Non repris : aucun constat rejeté ; les ⚠️ de décalage de lignes (#26, #35) sont laissés tels quels (affirmations vraies).
