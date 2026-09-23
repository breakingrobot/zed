# ADR-003 — Fichiers et chemins sur l'hôte

- Statut : proposé (2026-09-23)
- Contexte : la config est lue par le `Fs` local (`01` H3) ; les fichiers générés vont dans `temp_dir()` du client
  (4 sites, `01` §3) ; `Worktree::load_file` refuse les worktrees distants (`worktree/src/worktree.rs:922-929`) ;
  `RemotePathBuf` existe mais sans `join`/`parent` (`util/src/paths.rs:359-380`).
- Décision :
  - `HostFiles` (sur `EngineHost`) : `read_to_string`, `write` (contenu passé par **stdin**, jamais dans l'argv),
    `create_build_dir` (un dossier `zed-devcontainer-build-<uuid>` par build, supprimé en fin de build), `exists`.
    Local : `Fs`. Distant v1 : hôte **POSIX** uniquement (`cat`, `mkdir -p`, `mktemp -d`, `rm -rf` limité au dossier créé).
  - Chemins de l'hôte : **étendre `RemotePathBuf`** (`join`, `parent`, `file_name`, `is_absolute`) plutôt que d'introduire `HostPathBuf`.
  - Features OCI : téléchargées par le client (identifiants et proxy HTTP du client), puis écrites dans le `BuildDir` de l'hôte.
- Alternatives :
  - upload de répertoire via `upload_directory` (#62680 : `~/.zed_devcontainer`, jamais nettoyé) ;
  - UNC `\\wsl.localhost` (pupeno-wsl, WSL seulement) ;
  - ouvrir des buffers via le projet (effets de bord dans l'UI).
- Conséquences : plus de collision de noms fixes pour les overrides compose (`03` C3) ; rien n'est persisté sur l'hôte.
