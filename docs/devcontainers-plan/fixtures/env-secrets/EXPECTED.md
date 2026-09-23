# env-secrets — test de non-régression (ADR-008, **en suspens**)

Lancer avec `ZED_FIXTURE_TOKEN=zed-fixture-secret-value` dans l'env du client.
- La chaîne `zed-fixture-secret-value` ne doit apparaître ni dans la base Zed (`remote_connections.remote_env`), ni dans les
  logs Zed, ni dans l'argv des `docker exec` de connexion (seul `remoteEnv` résolu est autorisé, et il contient la valeur :
  la redaction doit s'appliquer aux logs).
Sur `main` : persistée en clair (`workspace/src/persistence.rs:1754`) et passée en `-e` (`devcontainer_manifest.rs:208-223`).
