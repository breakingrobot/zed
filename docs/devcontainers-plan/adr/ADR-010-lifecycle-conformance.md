# ADR-010 — Exécution des hooks conforme à la spec

- Statut : proposé (2026-09-23)
- Contexte : `03-spec-compliance.md` L1–L10 et V1 ; issue #62964 ; PRs #63034, #62271, #63391.
- Décision :
  - **argv préservé** : `docker exec … <argv>` ; la forme chaîne `["/bin/sh","-c",s]` est passée telle quelle, sans `join` (reprendre #63034) ;
  - **`initializeCommand`** : à chaque ouverture, sur l'hôte, échec fatal, `cmd /c` si l'hôte est Windows (#63391) ;
  - **ordre** : hooks des métadonnées d'image, puis des features (ordre d'installation), puis de devcontainer.json ;
    la forme objet s'exécute en parallèle, avec un ordre de journalisation déterministe ;
  - **marqueurs** `~/.devcontainer/.<hook>Marker` (hooks de création : `Created` ; postStart : `StartedAt`), compatibles avec le CLI de référence ;
  - **`postAttachCommand`** à chaque (re)connexion ;
  - **`waitFor`** : bloquant en v1 (comme le CLI) ; en v2, arrière-plan après l'étape `waitFor` ;
  - **`devcontainerId`** = sha256 du JSON des labels triés → base32 sur 52 caractères (algorithme de la spec).
- Conséquences : Zed et VS Code se comportent pareil sur un même conteneur (marqueurs partagés) ; les volumes
  `…-${devcontainerId}` changent de nom une fois (à mentionner dans les notes de version).
