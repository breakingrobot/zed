# ADR-008 — Secrets et environnement

- Statut : proposé.
  - Dossier temporaire : **accepté** (décision du 2026-09-24), mis en œuvre par A8a (`devcontainers/a8-unique-build-dir`).
  - Env persisté : **en suspens** (décision utilisateur du 2026-09-23).
- Contexte :
  - `remote_env` = env complet du conteneur + `remoteEnv` (`devcontainer_manifest.rs:208-223`) ;
  - il est passé en `-e` sur chaque `docker exec` et **persisté en clair** (`workspace/src/persistence.rs:1035, 1754`) ;
  - pupeno-remote persisterait le `password` SSH, et son chemin distant contourne la redaction de #63606.
- Décision :
  - persister et réinjecter **seulement** `remoteEnv` résolu : l'env du conteneur y est déjà ;
  - ne jamais persister de `password` ;
  - redaction (`util::redact`) sur **tous** les chemins de log et d'erreur, locaux comme distants ;
  - `HostFiles::write` transmet le contenu par stdin ;
  - `BuildDir` supprimé en fin de build, donc les options de features ne restent pas sur l'hôte.
- Conséquences : purge de la colonne `remote_env` existante à la migration ; tests de non-régression (07).
- Révisions après revue adverse (`08` R-2, R-3) :
  - **Périmètre élargi** : `RemoteConnectionOptions` (donc `DockerConnectionOptions.remote_env`) est aussi persisté en JSON
    complet dans `sidebar_threads` (`agent_ui/src/thread_metadata_store.rs:1528`) et `sidebar_terminal_threads`
    (`agent_ui/src/terminal_thread_metadata_store.rs:540`) — vérifié.
  - « Seulement `remoteEnv` » ne suffit pas : `remoteEnv: {"TOKEN": "${localEnv:GITHUB_TOKEN}"}` reste un secret une fois résolu.
    Persister les **gabarits non résolus** et résoudre à la connexion ; transmettre l'env aux `docker exec` par `--env-file`
    (fichier 0600 sur l'hôte, supprimé ensuite) plutôt que par `-e K=V` dans l'argv.
  - **Dossier temporaire** : `temp_dir()/devcontainer-zed/` prévisible + `std::fs::write` qui suit les liens
    (`fs/src/fs.rs:1047`) → sous Linux multi-utilisateur, créer un dossier **unique 0700** par build (`mkdtemp`), refuser
    s'il existe, jamais de noms fixes (lien avec ADR-003).
  - Dossier temporaire : fait dans A8a (`tempfile::TempDir` 0700 par build, supprimé en fin de build).
