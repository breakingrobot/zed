# ADR-001 — Trois lieux explicites ; le CLI se déplace vers l'hôte des sources

- Statut : proposé (2026-09-23)
- Contexte : `main` suppose client = hôte CLI = hôte démon (`01` H1, H3, H7). La référence (VS Code) exécute le CLI
  **là où sont les sources et le démon** (distro WSL, hôte SSH) et ne traduit pas les chemins
  (`04-reference-topologies.md`, faits transverses, leçons 1-3).
- Décision : modéliser **client**, **hôte moteur** (CLI + démon, = hôte des sources) et **conteneur**. L'hôte moteur d'un
  projet est l'hôte de sa connexion (`Local` ou `RemoteConnection` SSH/WSL). Aucune traduction de chemin en v1.
- Alternatives rejetées :
  - traduire les chemins client → vue du démon (T5, `DOCKER_HOST`) : impossible sans partage de FS ; la référence y renonce ;
  - garde « même moteur » (pupeno-wsl) : exclut T3/T4 (`02-contrib-pupeno-wsl.md`).
- Conséquences : `initializeCommand`, `id -u`, lecture de config et labels se font sur l'hôte ; T5 et T6a demandent un
  traitement dédié (ADR-009).
