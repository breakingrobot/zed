# ADR-004 — Connexion au conteneur via le même hôte (`DockerHost`)

- Statut : proposé (2026-09-23)
- Contexte : aujourd'hui la connexion exécute `docker` sur le client (`remote/src/transport/docker.rs`). pupeno-wsl garde ce
  modèle et exige le « même moteur » ; #62680 ajoute `DockerHost` ; pupeno-remote ajoute `RemoteConnectionOptions::HostDocker`.
- Décision :
  - `DockerConnectionOptions.host: DockerHost { Local, Ssh(SshConnectionOptions), Wsl(WslConnectionOptions) }`,
    `#[serde(default)]`, **non récursif** ;
  - `ConnectionPool` réutilise ou recrée la connexion de l'hôte (`connect_docker_host`, #62680) ;
  - `DockerExecConnection` fait passer toutes ses commandes par un point unique `docker_command()` ;
  - upload du serveur en flux : `docker exec -i … sh -c 'cat > "$1"' zed-upload <dst>` ;
  - arrêt du proxy via le handle du processus, et non via le binaire `kill` (absent sous Windows, `01` §2).
- Alternatives : `HostDocker` (nouvelle variante dans tous les `match`, persiste le mot de passe SSH — `02-overlap-matrix.md` §5 C) ;
  garde « même moteur ».
- Conséquences : pas de port forwarding en v1 (PR séparée, cf. #63899).
- Révisions après revue adverse (`08` F-4) :
  - **Pas de `SshConnectionOptions` embarqué** : il sérialise `password` (`remote/src/transport/ssh.rs:136-141`) et
    `RemoteConnectionOptions` est persisté en JSON complet dans `sidebar_threads`/`sidebar_terminal_threads`
    (`agent_ui/src/thread_metadata_store.rs:1528`). Utiliser un type dédié `DockerHostSsh { host, username, port, args, nickname }`
    **sans secret**, reconverti en `SshConnectionOptions` à la connexion (le mot de passe éventuel est redemandé).
  - **Reconnexion** : l'hôte est reconnecté par le pool avant `DockerExecConnection` ; en cas d'échec de l'hôte, l'erreur
    affichée nomme l'hôte (pas le conteneur).
  - **Arrêt** : tuer le processus local (ssh/wsl.exe) ne garantit pas l'arrêt du `docker exec` distant ; prévoir un arrêt explicite
    du proxy (identifiant de processus dans le conteneur) — **non vérifié** : comportement actuel de `kill` sur les transports SSH/WSL.
