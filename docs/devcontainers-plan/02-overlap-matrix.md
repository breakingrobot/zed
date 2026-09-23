# 02 — Matrice de recouvrement des contributions

Synthèse de `02-contrib-pr-60975.md`, `02-contrib-pr-62680.md`, `02-contrib-pupeno-wsl.md`, `02-contrib-pupeno-remote.md`
(détails et références `fichier:ligne` dans chacun) et de mesures faites ici (`git merge-tree`, builds). Base `main` = `16c9aa7ea6`.

## 1. Vue d'ensemble

| | #60975 lifecycle | #62680 SSH (+WSL) | pupeno `wsl-devcontainers` | pupeno `remote-devcontainer` |
|---|---|---|---|---|
| Auteur / statut | alex-berger, PR ouverte | alexdhill, PR ouverte | pupeno, branche sans PR | pupeno, branche sans PR |
| Taille | 12 commits (3 merges) | 36 commits (7 merges), 27 fichiers, +4 241/−662 | 10 commits, 15 fichiers, +1 068/−342 | 16 commits (1 merge), 34 fichiers, +6 123/−1 016 |
| Retard sur `main` | 353 | 159 | 566 | 369 |
| IA déclarée | « implemented with AI assistance, supervised » | « primarily AI generated » | « fully vibe coded… haven't read the code » | « mostly AI » |
| Revue mainteneur | aucune | aucune | — | — |
| CI | aucun check-run | 1 succès (`route-pr`), 10 `skipped` : **aucun test exécuté** | — | — |
| Conflits textuels avec `main` (`git merge-tree`) | 1 (`workspace/src/persistence.rs`) | 0 | 2 (`devcontainer_manifest.rs`, `dev_container_suggest.rs`) | 3 (`devcontainer_json.rs`, `dev_container_suggest.rs`, `recent_projects.rs`) |
| Compile après intégration | ✅ (rebase) — mais tests `remote` ❌ E0063 (conflit sémantique avec #63606) | ❌ **tête de PR** : E0425 `cli_auto_open` (`dev_container_suggest.rs:138`) ; ✅ après suppression de 26 lignes | ✅ | ✅ après 2 corrections d'une ligne |
| `cargo test -p dev_container` (Windows) | 122/122 | 131 ✅ / **1 ❌** (`the_container_user_takes_its_id_from_the_host`, `cfg(windows)`) | 120/120 | 161/161 (+ `remote --lib` 56/56) |

⚠️ Méthodo : un target cargo partagé entre worktrees a produit une fausse erreur E0277 pour #62680 (artefacts `sqlez`/`util`
réutilisés) ; l'agent a basculé sur `H:\Sources\zed-wt\target-pr-62680`. Les résultats **verts** obtenus sur le target partagé
(#60975, pupeno-wsl) ne sont pas concernés par ce mode d'échec, mais P7 devra imposer un target par worktree.

## 2. Matrice fonctionnalité × contribution

✅ traité · ⚠️ partiel/défaut notable · ❌ absent · — hors sujet

| Fonctionnalité | #60975 | #62680 | pupeno-wsl | pupeno-remote |
|---|---|---|---|---|
| Moteur sur hôte SSH | — | ✅ | ❌ (garde « même moteur ») | ✅ |
| Moteur dans une distro WSL | — | ⚠️ câblé, jamais testé | ⚠️ seulement Docker Desktop + intégration WSL | ⚠️ testé unitairement, pas en réel |
| Hôte SSH **Windows** | — | ❌ (POSIX supposé : `test -f`, `cat`, `mkdir -p`) | ❌ | ⚠️ PowerShell prévu ; env ignoré par `build_command_windows` (`ssh.rs:1999`) |
| WSLc | — | ❌ | ❌ | ❌ |
| Zed dans un conteneur / imbriqué | — | ❌ refusé explicitement | ❌ refusé | ❌ refusé |
| Commandes docker via `RemoteConnection::build_command` | — | ✅ `DevContainerHost::command` | ✅ `ProjectCommandBuilder` | ✅ `ProjectHost::start_process` |
| Connexion (proxy, upload serveur, terminaux) via l'hôte | — | ✅ `DockerHost` + `docker_command` | ❌ CLI local | ✅ `HostDocker` |
| `id -u`/`-g` sur l'hôte | — | ⚠️ oui, mais désactivé si **client** Windows | ✅ selon l'OS de l'hôte | ✅ selon l'OS de l'hôte |
| `initializeCommand` sur l'hôte | ❌ | ✅ | ❌ (reste local, cwd Linux) | ✅ |
| Lecture config sur l'hôte | — | ⚠️ `test -f` + `cat` (2 allers-retours) | ⚠️ `Fs` local via UNC `\\wsl.localhost` | ✅ `cat` / PowerShell |
| Fichiers de build générés | — | générés localement, uploadés dans `~/.zed_devcontainer` (**jamais nettoyé**, options de features persistées) | `mktemp -d` sur l'hôte, un par appel, jamais nettoyé | écrits sur l'hôte par stdin, jamais nettoyés, noms `zed-<pid>` |
| Upload remote_server hors local | — | `docker exec -i … cat >` (flux) | — | fichier temp sur l'hôte + `docker cp` |
| Port forwarding | — | ⚠️ via l'hôte, bridge Linux seulement | ❌ | ❌ |
| Identité stable au rebuild (#56576) | ✅ `(local_folder, config_file, user)` | ❌ garde `container_id` | ❌ | ❌ garde `container_id` |
| Identité qualifiée par l'hôte | ❌ | ✅ `…@{host persistence_key}` | ❌ | ✅ `host-docker:<hôte>:<root>:<config>:…` |
| Stop / Start / Restart / Rebuild / Delete | ✅ (sans Compose) | ❌ | ❌ | ⚠️ annulation du démarrage seulement |
| Réouverture d'un conteneur arrêté | ✅ Reconnect → `docker start` | ❌ | ❌ | ❌ |
| Quoting des hooks (`join(" ")`, cf. #63034) | ❌ inchangé | ❌ inchangé | ❌ inchangé | ❌ inchangé |
| Sécurité — secrets | — | ⚠️ options de features persistées sur l'hôte | — | ❌ `password` SSH persisté en JSON (vérifié : `SshConnectionOptions` sérialise `password`, `ssh.rs:136-141` ; `persistence.rs:1772` de la branche) ; ⚠️ `stderr` non rédigé sur le chemin distant (contourne #63606) |
| Tests d'intégration avec faux `RemoteConnection` | partiels (identité, DB) | ✅ `FakeRemoteConnection` | ❌ | ✅ `RecordingProjectHost` |

## 3. Conflits entre contributions (`git merge-tree`, fichiers en conflit)

| | #62680 | pupeno-wsl | pupeno-remote |
|---|---|---|---|
| **#60975** | 7 | 3 | 7 |
| **#62680** | | 7 | **16** |
| **pupeno-wsl** | | | 8 |

Fichiers chauds (présents dans ≥ 4 paires) : `dev_container/src/devcontainer_manifest.rs`, `devcontainer_api.rs`, `lib.rs`,
`recent_projects/src/recent_projects.rs`, `remote/src/remote_identity.rs`, `workspace/src/persistence.rs`.
Conclusion : **aucune combinaison de deux contributions ne peut être fusionnée telle quelle** ; il faut un modèle unique.

## 4. Abstraction d'exécution : trois formes du même besoin

Besoin : « lancer `docker …` (et quelques utilitaires) **là où vit le moteur**, avec l'argv préservé, en réutilisant le transport du projet ».

| Critère | `DevContainerHost` (#62680) | `ContainerEngine` + `ProjectCommandBuilder` (pupeno-wsl) | `ProjectHost` + `ProjectHostCapability` (pupeno-remote) |
|---|---|---|---|
| Forme | `enum { Local, Remote(Arc<dyn RemoteConnection>) }` + `command(program, args, env, cwd)` | `struct { connection: Option<Arc<dyn RemoteConnection>> }` + builder | struct `ProjectHost` (remote crate) + trait `?Send` (dev_container) + `HostPathBuf` |
| Réutilise `build_command` | ✅ | ✅ (signature miroir) | ✅ |
| Argv local strictement identique à `main` | ✅ `Command` direct | ✅ (test `local_engine_runs_the_program_directly`) | ✅ `Command::new` |
| `current_dir` | ✅ | ❌ (`ProjectCommand` n'en a pas) | ✅ |
| stdin / flux | via `docker_command` côté transport | ❌ | ✅ (`write_file` par stdin, `into_child`) |
| Opérations FS sur l'hôte | ad hoc (`read_file_from_host`) | via `Fs` local + UNC (WSL seulement) | ✅ `read/write/is_file/is_dir/copy_dir/create_dir_all/stage_assets` |
| Chemins typés hôte | `path_style/join/parent` | `join_path/normalize_path` | ✅ `HostPathBuf { text, PathStyle }` |
| Hôte Windows | ❌ | ❌ | ⚠️ |
| Taille / risque de revue | moyenne | **petite** | **grande** (+ `desktop_boundary.rs`, scanner de source en test) |
| Crate | `dev_container` | `dev_container` | `remote` + `dev_container` |

Recommandation pour P5 (à valider) : **une seule** abstraction, du côté `dev_container`, de la forme minimale de
`DevContainerHost`/`ProjectCommandBuilder` (enum `Local | Remote(Arc<dyn RemoteConnection>)` qui délègue à `build_command`),
enrichie de `current_dir`/stdin et d'un **petit** jeu d'opérations FS inspiré de `ProjectHost`, avec `HostPathBuf` comme
type de chemin. Écarter le scanner `desktop_boundary.rs` (contrainte de style, pas de fonctionnalité).

## 5. Connexion au conteneur

| Option | Principe | Pour | Contre |
|---|---|---|---|
| A. CLI local + garde « même moteur » (pupeno-wsl) | création via l'hôte, connexion via `docker` local ; `docker info` ID égal des deux côtés | aucun changement du transport | exclut SSH, Docker CE dans WSL, WSLc, imbriqué ; faux négatifs si Docker Desktop local arrêté |
| B. `DockerHost` plat dans `DockerConnectionOptions` (#62680) | `host: DockerHost { Local, Ssh, Wsl }`, `#[serde(default)]` ; `connect_docker_host` via le pool | rétrocompatible, réutilise le pool/ControlMaster, non récursif volontairement | variantes dupliquées de `RemoteConnectionOptions` ; persistance SSH perd `args`/`port_forwards` |
| C. Nouvelle variante `RemoteConnectionOptions::HostDocker` (pupeno-remote) | options hôte + racine + config + conteneur | porte aussi l'origine (root, config) utile au rebuild | nouvelle variante partout (`match`), persiste `password`, identité garde `container_id` |

Recommandation (à valider en P5) : **B**, en y ajoutant l'origine `(project_root, config_file)` de #60975/C, et en
persistant l'hôte via la **même** normalisation que la ligne SSH existante (sans `password`, en conservant `args` utiles).

## 6. Identité : fusion #60975 × #62680 (× pupeno-remote)

- #60975 : `DockerIdentityKey::DevContainer { local_folder, config_file }` + `remote_user` → stable au rebuild, **sans hôte**
  (`remote/src/remote_identity.rs:31` de la PR) ; chemins non normalisés comme les labels ; `""` non traité comme absent.
- #62680 : `Docker { container_id, name, remote_user, host: Option<String> }` → qualifiée par l'hôte, **instable** au rebuild.
- pupeno-remote : `HostDocker { project_host, project_root, devcontainer_config, container_id, name, remote_user }` → hôte + origine, mais garde `container_id`.

Clé proposée (à trancher en P5, ADR dédié) :
`docker:{engine_host_key}|{project_root normalisé comme le label}|{config_file normalisé}|{remote_user}`
où `engine_host_key` = `local` ou la `persistence_key` de l'hôte (SSH/WSL, sans secret), et `container_id`/`name` deviennent des
**attributs runtime** mis à jour (mécanisme UPDATE de #60975). Repli `ContainerId` conservé pour les conteneurs sans labels.
Migration : append-only (leçon du rebase #60975), backfill best-effort des lignes existantes.

## 7. Sort de chaque morceau

| Morceau | Source | Sort |
|---|---|---|
| Routage des commandes docker via `build_command` | #62680 `DevContainerHost` / pupeno-wsl `ProjectCommandBuilder` / pupeno-remote `ProjectHost` | **Fusionner** en une abstraction (cf. §4) |
| `HostPathBuf` (chemin typé par style d'hôte) | pupeno-remote | **Reprendre** |
| UID / labels selon l'OS de l'**hôte** au lieu de `cfg(windows)` | pupeno-wsl, pupeno-remote | **Reprendre** (corrige aussi le test rouge de #62680) |
| `initializeCommand` sur l'hôte | #62680, pupeno-remote | **Reprendre** |
| Lecture de config sur l'hôte | pupeno-remote (≻ #62680 `test -f`+`cat`, ≻ UNC pupeno-wsl) | **Adapter** ; préférer le `Fs` RPC du `remote_client` quand disponible |
| Staging du contexte de build | #62680 `~/.zed_devcontainer` / pupeno-wsl `mktemp` / pupeno-remote temp hôte | **Adapter** : un dossier par build, **nettoyé**, pas de secrets persistés |
| `DockerHost` + `connect_docker_host` | #62680 | **Reprendre** (cf. §5 B) |
| Upload serveur par flux `docker exec -i` | #62680 | **Reprendre** ; unifier la sémantique d'upload de répertoire (cp vs tar) |
| Garde « même moteur » (`docker info` ID) | pupeno-wsl | **Remplacer** par la connexion via l'hôte ; réutiliser l'idée d'**identité moteur** pour détecter WSLc/Docker Desktop partagé en P4 |
| `unsupported_reason(project)` | pupeno-wsl | **Adapter** (point d'entrée unique des refus) |
| `open_in_dev_container` pour projets distants | pupeno-wsl | **Reprendre** (petite PR) ; vérifier le recouvrement avec #63898 signalé par l'agent pupeno-remote |
| Identité stable + UPDATE runtime + migration | #60975 | **Adapter** (ajout de l'hôte, normalisation) |
| Ops stop/start/rm/rebuild | #60975 | **Adapter** : via l'abstraction d'hôte, Compose, rebuild non destructif |
| UX lifecycle (actions, overlay, menu, sidebar, callout agent) | #60975 | **Réduire / redécouper** après identité |
| Port forwarding via l'hôte | #62680 | **Reporter** (PR séparée, à coordonner avec #63899) |
| Découverte de `.devcontainer` gitignoré ; annulation du démarrage | pupeno-remote | **Reprendre** (petites PRs indépendantes) |
| `4e42f009d7` (correctifs Dockerfile/getent/env) | #62680 | **Remplacer** par les PRs dédiées (#62196, #62271…) |
| `desktop_boundary.rs` | pupeno-remote | **Abandonner** |
| Quoting des hooks | aucune (≠ #63034) | **Prérequis** : #63034 ou équivalent, avant tout le reste |
| Persistance du `password` SSH ; `stderr` non rédigé | pupeno-remote | **Ne pas reprendre** ; test de non-régression en P7 |

## 8. Conséquences pour la suite

- Contraintes amont (`CONTRIBUTING.md`) : fonctionnalité non confirmée par le staff → **discussion d'abord** (#59500 existe,
  ouverte par macraig, `authorAssociation: CONTRIBUTOR` — statut staff **non établi** (corrigé après `08`) ; aucune validation
  de design ; le lieu prévu par `CONTRIBUTING.md` pour une proposition est une **Discussion** : #56252) ; **3 PRs ouvertes max par auteur** ; pas de « giant refactorings » ;
  politique IA. Les quatre contributions déclarent une forte part d'IA et aucune n'a eu de revue : la pile de PRs (P6) devra
  être petite, testée, et portée par des humains qui la comprennent.
- Coordination : trois auteurs actifs (alex-berger, alexdhill, pupeno) ; pupeno a déjà proposé de découper son travail et
  cherche à contacter alex-berger (#60975). Le message de coordination (P6, brouillon non posté) doit proposer **le modèle
  unique** ci-dessus et un ordre de fusion, pas une cinquième implémentation concurrente.
- Ordre pressenti (à confirmer en P6) : (0) quoting des hooks (#63034) → (1) abstraction d'hôte sans changement de comportement
  local → (2) UID/labels/`initializeCommand` selon l'hôte → (3) identité stable qualifiée par l'hôte (#56576) → (4) connexion
  via l'hôte (SSH) → (5) WSL → (6) lifecycle UX → (7) port forwarding. WSLc et imbriqué : après P4.
