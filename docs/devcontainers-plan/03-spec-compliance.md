# 03 — Conformité à la spécification Dev Containers

Base : `main` = `16c9aa7ea6`. Référence normative et CLI : `03-spec-reference.md` (spec `devcontainers/spec@c95ffeed1d`,
CLI `devcontainers/cli@5dc7533314`). Côté Zed, les références `fichier:ligne` sont sur `main`, chemins relatifs à
`crates/dev_container/src/` sauf mention (`manifest` = `devcontainer_manifest.rs`, `json` = `devcontainer_json.rs`).
Chaque constat Zed a été relu dans le code (ou vient de `01-current-architecture.md`, lui-même vérifié).

Légende gravité : **B** bloquant (casse des configs courantes ou sécurité) · **M** majeur (écart normatif visible) ·
**m** mineur (écart rare ou cosmétique) · ✅ conforme. Colonne « Traité par » : contributions de la Phase 2 et PRs ouvertes.

## 1. Synthèse

| Domaine | État | Écarts B/M |
|---|---|---|
| Lifecycle | ❌ | quoting des hooks ; `initializeCommand` ; hooks features/metadata ; forme objet ; `waitFor` ; marqueurs ; reprise |
| Variables | ⚠️ | `devcontainerId` non conforme |
| Métadonnées d'image (fusion) | ⚠️ | fusion partielle (hooks, `remoteEnv`, `containerUser`… ignorés) |
| Identité / labels | ⚠️ | conformes en local ; faux dès que l'hôte ≠ client |
| `updateRemoteUserUID` | ⚠️ | UID lu sur le client ; condition sur l'OS du client |
| Environnement | ❌ | `userEnvProbe` ignoré ; `containerEnv` complet persisté et passé en argv |
| Mounts / workspace | ⚠️ | pas de traduction de chemins ; objet `mounts` ok |
| Options `run`, ports | ⚠️ | `forwardPorts` chaîne ignorés, `portsAttributes`/`shutdownAction`/`hostRequirements` ignorés |
| Compose | ✅/⚠️ | aligné sur le CLI (nom de projet, `runServices` #56293) ; recherche par id labels au lieu des labels compose |
| Features | ⚠️ | ordre (`installsAfter`/`dependsOn`) ; options tableau/nombre (#64025) ; 1ʳᵉ couche OCI seulement |
| Build | ⚠️ | `buildx` codé en dur |
| Variantes de moteur | ⚠️ | docker/podman seulement ; pas de wslc ; pas de démon distant |

## 2. Lifecycle

| # | Exigence (réf. `03-spec-reference.md` §2) | CLI de référence | Zed `main` | Écart | Grav. | Traité par |
|---|---|---|---|---|---|---|
| L1 | Chaîne → `/bin/sh -c "<chaîne>"` ; tableau → argv **sans shell** (MUST) | `injectHeadless.ts:505-561` | construit `["/bin/sh","-c",s]` (`json:366-367`) puis **aplati** `sh -c "<join(' ')>"` (`docker.rs:377-386`) → la forme chaîne s'exécute tronquée, la forme tableau passe par un shell | régression v1.15.0, issue **#62964** (repro `mkdir /tmp/zed-repro`) | **B** | **#63034** (pupeno), #62271 (marqueur postStart) ; aucune des 4 contributions |
| L2 | `initializeCommand` sur la **machine hôte**, à la création **et aux démarrages suivants** | à chaque `up`, avant la recherche du conteneur (`configContainer.ts:66`, `utils.ts:538-622`) ; `%ComSpec% /c` si hôte Windows | seulement dans `build_and_run` (`manifest:2308`) → **sauté** si le conteneur existe ; exit≠0 seulement loggé (`json:411-416`) ; `/bin/sh` même sous Windows (`json:366`) | 3 écarts | **B** | #63391 (forme chaîne sous Windows) ; #62680 / pupeno-remote (exécution sur l'hôte) |
| L3 | Échec d'un hook ⇒ les suivants ne s'exécutent pas, `up` échoue (MUST) | `injectHeadless.ts:526-540` | hooks conteneur : erreur propagée ✅ (`docker.rs:394-398`) ; `initializeCommand` : non (L2) | partiel | M | — |
| L4 | Forme objet : commandes **en parallèle**, toutes doivent réussir | `Promise.allSettled` | séquentiel, ordre d'itération d'une `HashMap` (**non déterministe**) (`json:375-390`, `manifest:2336`) | ordre et parallélisme | m | — |
| L5 | Hooks des **features** et du label `devcontainer.metadata`, dans l'ordre d'installation, **avant** ceux de l'utilisateur (« shall ») | `imageMetadata.ts:124-144,291-316` | seulement `config.*` (`manifest:2322-2428`) ; les hooks de features sont **recopiés dans le label** (`features.rs:280-286`) mais **jamais exécutés** | features cassées (ex. features qui font leur setup en `postCreateCommand`) | **B** | — |
| L6 | Marqueurs `.onCreateCommandMarker`, `.updateContentCommandMarker`, `.postCreateCommandMarker` (date de création), `.postStartCommandMarker` (date de démarrage) | `injectHeadless.ts:431-441` | seulement postStart (`manifest:3371-3406`) ; onCreate/updateContent/postCreate pilotés par `new_container` | re-création d'un conteneur existant non reconnue ; pas idempotent entre outils (VS Code ↔ Zed) | M | — |
| L7 | `postAttachCommand` à **chaque** attachement | à chaque `up` | au spawn uniquement, pas aux reconnexions du proxy (`manifest:2412-2425`) | | m | — |
| L8 | `waitFor` (défaut `updateContentCommand`) : l'outil peut rendre la main avant la fin des hooks suivants ; `postCreateCommand` « executed in the background » (SHOULD) | CLI bloquant, `waitFor` seulement avec `--skip-non-blocking-commands` | tout est bloquant ; `wait_for` parsé et jamais lu (`json:243`) | conforme au CLI, pas à l'esprit de la spec | m | — |
| L9 | Conteneur existant : hooks lus depuis le label `devcontainer.metadata` figé à la création | `imageMetadata.ts:44-48,441-462` | relit le devcontainer.json courant | divergence silencieuse après édition du json | m | — |
| L10 | Reprise d'un conteneur arrêté (ouverture depuis les récents) | — (outil) | ni `docker start`, ni hooks (`recent_projects/src/recent_projects.rs:2191-2208`) | | M | #60975 (Reconnect → start) |

## 3. Variables

| # | Exigence | Zed `main` | Écart | Grav. |
|---|---|---|---|---|
| V1 | `${devcontainerId}` : **sha256** du JSON des id labels à clés triées → base32, 52 caractères ; stable entre rebuilds, unique par hôte Docker (MUST) | `DefaultHasher` sur les labels → 16 caractères hex (`manifest:113-124`) | valeur ≠ CLI/VS Code (volumes nommés `…-${devcontainerId}` non partagés, ex. feature docker-in-docker) ; `DefaultHasher` non garanti stable entre versions de Rust | **M** |
| V2 | `${localEnv:VAR[:défaut]}` = env de la machine hôte (du CLI) | env du projet via shell local (`lib.rs:126-135`), substitution `manifest:182-186, 226` | ✅ en local ; en distant « hôte » = hôte du moteur (cf. 04) | ✅/⚠️ |
| V3 | `${containerEnv:VAR}` : dans `remoteEnv` | `runtime_remote_env` (`manifest:208-224`) | ✅ | ✅ |
| V4 | `${localWorkspaceFolder}` = dossier ouvert ; `${containerWorkspaceFolder}` = `workspaceFolder` calculé | `manifest:168-180, 2102-2127` ; `\`→`/` à la substitution | conforme en local ; valeur fausse si hôte ≠ client (espace A vs B) | ✅/⚠️ |
| V5 | Substitution des métadonnées d'image « at the time the value is applied » | métadonnées recopiées brutes dans le label (`manifest:3408-3437`) ; seul `remoteUser` des métadonnées est lu | voir M1 | m |

## 4. Métadonnées d'image (fusion)

| # | Exigence | Zed `main` | Écart | Grav. |
|---|---|---|---|---|
| M1 | Fusion par propriété (spec `devcontainer-reference.md:58-85`) : `init/privileged` OR ; `capAdd/securityOpt` union ; `mounts` collectés (dernier gagne par `target`) ; hooks collectés ; `remoteEnv/containerEnv` par variable ; `remoteUser/containerUser/userEnvProbe/shutdownAction/waitFor` dernier gagne ; `hostRequirements` max ; `forwardPorts` union | `build_merged_resources` (`manifest:793-926`) : mounts, capAdd, securityOpt, privileged, init, containerEnv, entrypoint ✅ ; `remoteUser` (`manifest:3254`) ✅ ; **ignorés** : hooks (L5), `remoteEnv`, `containerUser`, `userEnvProbe`, `forwardPorts`, `portsAttributes`, `overrideCommand`, `updateRemoteUserUID` | fusion partielle | M |
| M2 | Label `devcontainer.metadata` écrit à la création (tableau : image, features, devcontainer.json) | écrit (`manifest:2216-2253`, compose `1361-1402`), liste blanche de propriétés (`manifest:3412-3437`) | ✅ | ✅ |

## 5. Identité, labels, conteneur existant

| # | Exigence / référence | Zed `main` | Écart | Grav. |
|---|---|---|---|---|
| I1 | Labels `devcontainer.local_folder` (dossier ouvert) + `devcontainer.config_file` ; sous Windows `path.win32.normalize` + lecteur en minuscule | `identifying_labels` + `normalize_label_path` (`manifest:126-138, 3449-3465`) | ✅ en local ; la normalisation dépend de l'OS **client** (`cfg(windows)`) et non de l'hôte du moteur | ✅/⚠️ |
| I2 | Recherche : `ps -a --filter label=…` ; exclure les conteneurs `removing` ; repli ancien label | `docker.rs:256-266, 423-485` | pas de filtre `removing`, pas de repli ; `MultipleMatchingContainers` si >1 | m |
| I3 | Compose : conteneur existant retrouvé par `com.docker.compose.project` + `service` | par id labels | divergence (conteneurs créés par le CLI de référence non retrouvés si labels différents) | m |
| I4 | (Outil) identité de persistance stable au rebuild | `docker:{user}@{name}:{container_id}` (`remote/src/remote_identity.rs:49-53`) | issue **#56576** | **B** (perte de threads agent) |

## 6. `updateRemoteUserUID`

| # | Exigence | Zed `main` | Écart | Grav. |
|---|---|---|---|---|
| U1 | Linux uniquement ; défaut `true` ; ignoré si `remoteUser` root/numérique ; MAY sauter si pas de bind mount | activé si non-Windows **client** (`manifest:708-711, 1644-1687`) | condition sur l'OS client au lieu de la plateforme de l'hôte du moteur (cf. 04) ; vérifier root/numérique | M |
| U2 | UID/GID = utilisateur **local** (du CLI) | `id -u`/`id -g` lancés localement (`manifest:1699-1733`) | ✅ en local ; faux dès que l'hôte ≠ client | ✅/⚠️ |
| U3 | Mécanisme `updateUID.Dockerfile` (sed passwd/group, chown) ; Podman : préfixe `localhost/` | `manifest:1737-1787` ; `ENV {k}={v}` non échappé (`manifest:1835-1838`) | injection de lignes Dockerfile via `containerEnv` | m (sécurité) |

## 7. Environnement

| # | Exigence | Zed `main` | Écart | Grav. |
|---|---|---|---|---|
| E1 | `userEnvProbe` (défaut `loginInteractiveShell`) : variables du shell de l'utilisateur fusionnées pour les processus de l'outil | champ privé ignoré (`json:214`) ; l'env du serveur distant vient de `DockerExecConnection` (`remote/src/transport/docker.rs`) | PATH des features/`.bashrc` absent des processus Zed hors shell de login | M |
| E2 | `remoteEnv` : propre à l'outil, modifiable sans rebuild ; `containerEnv` : posé sur le conteneur | `remote_env` = **tout** `Config.Env` du conteneur (moins `HOME`) + `remoteEnv` (`manifest:208-223`) | (a) réinjecte en `-e K=V` sur **chaque** `docker exec` l'env complet du conteneur (visible dans `ps` de l'hôte) ; (b) **persisté en clair** en SQLite (`workspace/src/persistence.rs:1035, 1754`) → un `containerEnv` `${localEnv:GITHUB_TOKEN}` finit sur disque ; les logs sont rédigés depuis #63606 mais pas la DB | **B** (sécurité) |
| E3 | `remoteEnv` : valeur `null` = suppression | `HashMap<String,String>` → un `null` fait échouer le parse | m |

## 8. Workspace, mounts

| # | Exigence | Zed `main` | Écart | Grav. |
|---|---|---|---|---|
| W1 | Mount par défaut `type=bind,source=<dossier>,target=/workspaces/<basename>` ; `consistency=cached` hors Linux (pas avec Podman Linux) | `manifest:2137-2157` (sans `consistency`) | ✅ (`consistency` sans effet sur les moteurs récents) | ✅ |
| W2 | `workspaceMount`/`workspaceFolder` : obligatoires ensemble avec Dockerfile ? (divergence spec/CLI, cf. référence « points ambigus ») | `validate_devcontainer_contents` exige les deux ensemble (`json:308`) | plus strict que le CLI | m |
| W3 | `mounts` chaîne ou objet ; dédoublonnage par `target` (dernier gagne) | chaîne/objet ✅ (`json:77-96`) ; pas de dédoublonnage ; chaîne `--mount` non échappée (virgule) | | m |
| W4 | Source de bind-mount résolue par le **démon** | chemin client brut (`manifest:2146`) | correct seulement si client = hôte du démon (cf. 04) | ⚠️ |

## 9. Options `docker run`, ports, divers

| # | Exigence | Zed `main` | Écart | Grav. |
|---|---|---|---|---|
| R1 | `overrideCommand` défaut `true` (image/Dockerfile), `false` (compose) | `json:282` | ✅ | ✅ |
| R2 | `init`, `privileged`, `capAdd`, `securityOpt`, `runArgs` | `manifest:2159-2295` | ✅ (`runArgs` verbatim, comme le CLI) | ✅ |
| R3 | `forwardPorts` (nombre ou `"host:port"`), `appPort`, `portsAttributes`, `otherPortsAttributes` — le CLI n'implémente pas le forwarding (outil) | seules les entrées numériques sont publiées en `-p n:n` au `run` (`manifest:2265-2271`) ; chaînes ignorées ; pas de forwarding dynamique (`remote/src/transport/docker.rs:847-852`) | `-p n:n` publie sur **toutes les interfaces** (≠ forwarding local de VS Code) | M (sécurité réseau) — #63899 |
| R4 | `shutdownAction` (défaut `stopContainer` / `stopCompose`) | ignoré (`json:216`) | conteneurs laissés tournants | m |
| R5 | `hostRequirements` (vérification/avertissement) | ignoré (`json:244`) | | m |

## 10. Compose

| # | Exigence | Zed `main` | Écart | Grav. |
|---|---|---|---|---|
| C1 | `service` obligatoire ; `runServices` | `json:308` ; `runServices` (#56293, fusionnée) | ✅ | ✅ |
| C2 | Nom de projet : `COMPOSE_PROJECT_NAME` (env/.env) sinon dérivation CLI | `project_name`/`derive_project_name` (`manifest:2518, 2901`), aligné CLI | ✅ | ✅ |
| C3 | Overrides générés (build + runtime) | `docker_compose_build.json` / `_runtime.json` à **noms fixes** dans `temp_dir` (`manifest:1170, 1269, 1336`) | collision entre projets simultanés | m |

## 11. Features

| # | Exigence | Zed `main` | Écart | Grav. |
|---|---|---|---|---|
| F1 | Ordre : `installsAfter` (souple), `dependsOn` (dur, installe les dépendances), `overrideFeatureInstallOrder` | seul `overrideFeatureInstallOrder`, reste trié (`manifest:3041`) ; `installsAfter`/`dependsOn` **non lus** | features dépendantes installées dans le mauvais ordre ou manquantes | M |
| F2 | Options : types `string`/`boolean` (schéma) ; tableaux/nombres rencontrés en pratique | `FeatureOptionValue { Bool, String }` (`json:115-119`) → tableau/nombre = échec de parse | issue #63978 → **#64025** | M |
| F3 | Artefacts OCI : couche(s) de la feature | 1ʳᵉ couche seulement (`manifest:563-573`) | | m |
| F4 | `entrypoint`, `mounts`, `containerEnv`, `privileged`… des features fusionnés | ✅ (`manifest:793-926`) ; entrypoints concaténés dans un `sh -c` (`manifest:880-894`) | | ✅/m |
| F5 | Lifecycle des features | non exécutés | voir L5 | **B** |

## 12. Build et moteurs

| # | Exigence / référence | Zed `main` | Écart | Grav. |
|---|---|---|---|---|
| B1 | BuildKit **non requis** : repli sur image temporaire `dev_container_feature_content_temp` | repli seulement en Compose (`manifest:1843-1879`) ; image/Dockerfile : `buildx build` **codé en dur** (`manifest:1916`) | échec sans buildx (podman ancien, moteurs sans buildx) | M |
| B2 | `build.options`, `cacheFrom`, `target`, `args` | ✅ (`json:143-151`, `manifest:1897-2004`) | | ✅ |
| B3 | Ancienne syntaxe `dockerFile`/`context` au niveau racine (dépréciée, encore acceptée par le CLI — **non vérifié** dans la référence) | non parsée (`json:202-245`) | à vérifier | m |
| B4 | Variantes de CLI détectées par `docker -v` : `docker`, `podman`, **`wslc`** (`spec-shutdown/dockerUtils.ts:301-318`, vérifié à `5dc7533`) | booléen `use_podman` (settings) | pas de détection, pas de wslc | M (cf. `04-wslc.md`) |
| B5 | Podman Linux : `--security-opt label=disable` + `--userns=keep-id` si non-root et sans `--uidmap` | ajoutés dès que `docker_cli == "podman"` et que `runArgs` ne contient pas déjà l'option (`manifest:2186-2204`) — **sur tout OS**, sans test root/`--uidmap` | Podman machine (Windows/macOS) reçoit des options que le CLI réserve à Linux | m |
| B6 | Démon distant : le CLI hérite de `DOCKER_HOST`, n'adapte pas les chemins | idem (héritage de l'env du process Zed) | — (topologies : `04-topologies.md`) | — |

## 13. Corrections de conformité, regroupées en PRs candidates

Ordre suggéré (indépendant du travail « distant », chacune « une seule chose » + tests) :

| Ordre | PR candidate | Écarts couverts | Existant à reprendre / coordonner |
|---|---|---|---|
| S0 | Préserver l'argv des hooks (`docker exec` sans `join`) | L1 | **#63034** (pupeno) ; #62271 ; issue #62964 |
| S1 | `initializeCommand` : à chaque ouverture, échec fatal, `cmd /c` sur hôte Windows | L2 | #63391 |
| S2 | Hooks de features/metadata, marqueurs create, forme objet déterministe | L5, L6, L4, M1 (hooks) | — |
| S3 | `remote_env` : ne pas réinjecter/persister l'env complet du conteneur ; stocker seulement `remoteEnv` | E2 | — (sécurité ; à signaler en privé si jugé sensible) |
| S4 | `devcontainerId` conforme (sha256/base32) | V1 | — (migration : volumes existants nommés avec l'ancien id) |
| S5 | Ordre des features `installsAfter`/`dependsOn` ; options tableau/nombre | F1, F2 | **#64025** |
| S6 | Repli sans BuildKit pour image/Dockerfile | B1 | — |
| S7 | `userEnvProbe` | E1 | — |
| S8 | Ports : publier sur loopback / forwarding | R3 | **#63899** |
| S9 | Mineurs (`shutdownAction`, `hostRequirements`, dédoublonnage `mounts`, noms d'overrides compose, `ENV` échappé, `getent` quoting) | R4, R5, W3, C3, U3 | #62196 (USER Dockerfile) |

Hors conformité mais liés : identité stable (I4 → #60975, cf. `02-overlap-matrix.md` §6) et tout ce qui dépend de
l'**hôte** (U1, U2, V2, V4, W4, I1 → Phase 4/5).
