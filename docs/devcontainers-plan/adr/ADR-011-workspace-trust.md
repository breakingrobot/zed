# ADR-011 — Workspace Trust avant toute exécution issue du dépôt

- Statut : proposé (2026-09-23) — ajouté après la revue adverse (`08` R-1, Haute)
- Contexte :
  - `docs/src/development/feature-process.md:49` exige de traiter « How does this feature interact with Workspace Trust? ».
  - Ouvrir un dev container exécute du contenu du dépôt : `initializeCommand` **sur l'hôte**, Dockerfile/features, `runArgs`
    verbatim (`--privileged`, `-v /:/host`…), installation des extensions `customizations.zed.extensions`
    (`recent_projects/src/remote_servers.rs:2276-2284`). Aucune de ces étapes ne consulte la confiance.
  - Zed a déjà un mécanisme de confiance (`TrustedWorktrees`, utilisé par `project/src/git_store.rs:11-12, 1000-1001`).
  - A2 (`initializeCommand` à chaque ouverture) et l'ouverture depuis SSH/WSL **augmentent** cette surface.
- Décision :
  - aucune étape exécutable (initializeCommand, build, run, installation d'extensions) tant que le worktree n'est pas de confiance ;
  - le toast de suggestion et `--dev-container` passent par la même porte ;
  - en distant, la confiance porte sur le worktree **distant** (hôte + chemin), pas sur le client ;
  - afficher, avant la première exécution, un résumé des éléments sensibles détectés (`initializeCommand`, `privileged`, `runArgs`, mounts de l'hôte).
- Conséquences : question explicite à poser dans C0 ; A2 dépend de cette ADR (ne pas élargir l'exécution avant la porte de confiance).
- Non vérifié : sémantique exacte et API publique de `TrustedWorktrees` (à lire avant C0).
