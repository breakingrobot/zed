# ADR-002 — Une seule abstraction d'exécution : `EngineHost`

- Statut : proposé (2026-09-23)
- Contexte : 30 sites de lancement sans point commun (`01` §2, H6 ❌). Trois contributions proposent chacune une abstraction
  qui délègue à `RemoteConnection::build_command` (`02-overlap-matrix.md` §4).
- Décision : `enum EngineHost { Local, Remote(Arc<dyn RemoteConnection>) }` + `HostCommand { program, args, env, current_dir, stdin }`
  dans `dev_container` ; `EngineHost::command(&HostCommand) -> Result<util::command::Command>`.
  - Local : `util::command::Command` construit **exactement** comme aujourd'hui (même argv, même env, `current_dir`).
  - Distant : `build_command(Some(program), args, env, cwd, None, Interactive::No)` (signature `remote/src/remote_client.rs:1640-1648`),
    puis `Command::new(template.program).args(template.args).envs(template.env)`.
  - Tous les sites de `dev_container` passent par là, **y compris** `id -u`, `initializeCommand`, `check_for_docker`,
    `buildx version` et les commandes que le manifest fabrique via `docker_cli()`.
- Alternatives :
  - `ProjectHost` complet (pupeno-remote : crate `remote`, trait `?Send`, scanner de source) — trop gros pour une première PR ;
  - `deploy(Command)` qui ré-enveloppe un `Command` déjà construit (#62680) — perd `current_dir`, fragile.
- Conséquences : PR 1 = introduction sans changement de comportement (seule la variante `Local` est utilisée) + test
  d'égalité d'argv. Tests distants via un faux `RemoteConnection` (modèle : `FakeRemoteConnection` de #62680, défini dans
  `dev_container` sur cette branche ; base existante : `MockRemoteConnection`, `remote/src/transport/mock.rs:65, 187`).
- Révisions après revue adverse (`08`) :
  - **Filet de sécurité de C1** : `util::command::Command` n'expose que `get_program`/`get_args` (`util/src/command.rs:59, 125`),
    ni env ni cwd → les goldens se font sur `HostCommand` (program, args, env, cwd, stdin), **avant** conversion ; les sites qui
    contournent `CommandRunner` (`id -u/-g`, `command_json.rs:35, 52`, `check_for_docker`, `Docker::new`) sont d'abord routés.
  - **Environnement de l'hôte** : `build_command` distant lance un shell **non-login** → `PATH`/`DOCKER_HOST` peuvent manquer
    (Homebrew, Docker rootless). Ajouter une sonde d'environnement de l'hôte (shell de login, mise en cache) et un réglage du
    chemin du CLI, **avant** d'activer SSH/WSL (nouvelle PR C2b).
  - **Performance** : sous Windows, SSH n'a pas de ControlMaster (`remote/src/transport/ssh.rs:236`) → chaque commande ouvre
    une connexion complète ; regrouper les commandes (script unique pour les lectures) et mesurer avant C6.
