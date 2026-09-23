# ADR-008 — Secrets et environnement

- Statut : proposé — **mise en œuvre en suspens** (décision utilisateur du 2026-09-23 : ni PR ni signalement pour l'instant)
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
