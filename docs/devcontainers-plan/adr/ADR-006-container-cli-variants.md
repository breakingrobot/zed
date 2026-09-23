# ADR-006 — Variantes de CLI : Docker, Podman, Wslc

- Statut : proposé (2026-09-23)
- Contexte : Zed n'a qu'un booléen `use_podman` ; les options Podman sont appliquées sur tout OS (`devcontainer_manifest.rs:2186-2204`) ;
  `buildx` est codé en dur (`:1916`). La référence détecte `docker|podman|wslc` via `-v`
  (`devcontainers/cli` `spec-shutdown/dockerUtils.ts:301-318` @`5dc7533`, vérifié).
- Décision : `enum ContainerCli { Docker, Podman, Wslc }`, détecté par `<binaire> -v` sur l'hôte et surchargeable par un
  réglage `remote.dev_container_cli` (`auto` par défaut ; `use_podman = true` ⇒ `podman`). Table d'options par variante :
  - **Podman** : `--userns=keep-id` et `label=disable` **seulement si l'hôte est Linux**, l'utilisateur non-root et `runArgs` sans `--uidmap` ; pas de `consistency`.
  - **Wslc** :
    - `--mount` → `-v` ;
    - retrait de `--init/--privileged/--cap-add/--security-opt/--sig-proxy`, avec avertissement ;
    - `list --format json` au lieu de `ps --format {{json .}}` ; `container cp` ; `inspect` sans template ;
    - compose ⇒ refus ;
    - binaire `wslc` avec repli sur `%ProgramFiles%\WSL\wslc.exe` (`04-wslc.md`).
  - **Toutes** : repli sans BuildKit pour image/Dockerfile (`03` B1).
- Conséquences : WSLc reste expérimental (préversion, GA visée à l'automne 2026). Les features via `--build-context` étant
  indisponibles sur Wslc, leur contenu est copié dans le contexte de build.
