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
- Conséquences : WSLc reste expérimental (préversion, GA visée à l'automne 2026).
- Révisions après revue adverse (`08` C-1, C-3, F-6, R-7) :
  - `consistency=cached` est ajouté par Zed à **tous** les mounts (`dev_container/src/devcontainer_json.rs:95`) : le retirer pour
    Wslc (refusé, `04-wslc.md` Q3) et sous Linux/Podman (le CLI de référence ne l'ajoute que hors Linux).
  - **Pas de features en Wslc v1** (refus explicite) : copier leur contenu dans le contexte de build toucherait le dossier du
    projet ; un mécanisme sans effet sur le projet reste à spécifier.
  - `ContainerCli` doit vivre dans le crate **`remote`** (la connexion en a besoin : `cp` vs `container cp`, `exec`), avec migration
    de `use_podman` dans `DockerConnectionOptions` et les settings.
  - Recouper avec les PRs Podman/SELinux ouvertes (#58500 de KyleBarton ; #58794 citée par la revue — **non vérifiée**).
