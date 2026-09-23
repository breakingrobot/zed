# ADR-005 — Identité stable, qualifiée par l'hôte

- Statut : proposé (2026-09-23)
- Contexte : l'identité `docker:{user}@{name}:{container_id}` (`remote/src/remote_identity.rs:49-53`) fait perdre les
  threads après un rebuild (#56576). #60975 la stabilise sans l'hôte ; #62680 la qualifie par l'hôte sans la stabiliser.
- Décision :
  - clé `docker:{engine_host_key}|{project_root}|{config_file}|{remote_user}` ;
  - `engine_host_key` = `local` ou la `persistence_key` de la connexion SSH/WSL (sans secret) ;
  - `project_root` et `config_file` normalisés **comme les labels** (selon la plateforme de l'hôte) ; chaîne vide = absent ;
  - `container_id` et `name` deviennent des attributs runtime, mis à jour par UPDATE (mécanisme de #60975) ;
  - repli sur `container:{id}` pour les conteneurs sans labels.
- Migration : colonnes ajoutées en fin de liste des migrations (leçon du rebase de #60975) ; backfill best-effort.
- Alternative rejetée : garder `container_id` dans la clé (pupeno-remote), ce qui ne résout pas #56576.
- Conséquences : le changement d'`Eq`/`Hash` de `ProjectGroupKey` introduit par #60975 fait l'objet d'une PR séparée et justifiée.
