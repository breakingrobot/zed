# ADR-007 — Décisions selon la plateforme de l'hôte

- Statut : proposé (2026-09-23)
- Contexte :
  - `cfg(windows)` côté client décide de l'UID (`devcontainer_manifest.rs:708-711, 1644-1687`) et des labels (`:3449-3465`) ;
  - `initializeCommand` passe par `/bin/sh` même sous Windows (`devcontainer_json.rs:366`) ;
  - le test rouge de #62680 sous Windows vient de là.
- Décision : `EngineHost::platform()` (local : OS courant ; distant : `RemoteConnection::remote_platform()`/`path_style()`) pilote :
  - `updateRemoteUserUID` (hôte Linux uniquement) ;
  - la normalisation des labels ;
  - le shell de `initializeCommand` (`/bin/sh -c` ou `cmd /c`) ;
  - les options Podman.
- Conséquences : un client Windows avec un hôte Linux (WSL/SSH) retrouve l'alignement d'UID ; tests paramétrés par plateforme d'hôte.
