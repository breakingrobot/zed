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
- Conséquences : l'hôte est persisté **sans secret** (pas de `password`), mais avec les `args` SSH utiles, que #62680 perd ;
  pas de port forwarding en v1 (PR séparée, cf. #63899).
