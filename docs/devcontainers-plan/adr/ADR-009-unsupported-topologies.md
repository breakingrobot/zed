# ADR-009 — Topologies non supportées : refus explicites

- Statut : proposé (2026-09-23)
- Contexte : aujourd'hui, refus générique « Cannot open Dev Container from remote project »
  (`recent_projects/src/recent_projects.rs:508-521`), et échecs silencieux en T5 (mounts vides) et T6a (chemins faux).
- Décision : une fonction unique `unsupported_reason(project) -> Option<Reason>` (idée de pupeno-wsl), utilisée par l'action,
  la suggestion et le flag CLI `--dev-container`.
  - Refus v1 : projet collab ; projet déjà dans un conteneur (T6b) ; hôte SSH Windows (T4w) ; compose avec Wslc.
  - Avertissement bloquant :
    - `DOCKER_HOST` ou contexte non-unix en local (T5) ;
    - Zed lui-même dans un conteneur (T6a, détecté par `/.dockerenv` ou le cgroup), tant que la traduction n'existe pas.
- Révision après revue adverse (`08` F-8) : **ne pas** bloquer sur un endpoint « non-unix » — Docker Desktop/Podman sous
  Windows utilisent `npipe://` en local. Critère T5 = endpoint **distant** (`ssh://`, `tcp://` non-loopback, contexte dont
  l'hôte n'est pas local) ; T6a = **avertissement non bloquant** (les chemins peuvent coïncider avec le motif « même chemin »).
- Futur (P3) : T6a par `inspect` du conteneur courant → `Mounts[].Source` de la destination qui contient le projet.
- Conséquences : messages actionnables (« installez Docker dans la distro ou activez l'intégration WSL de Docker Desktop »).
