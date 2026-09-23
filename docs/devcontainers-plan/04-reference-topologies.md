# 04 — Topologies : comment les implémentations de référence les gèrent

- Date : 2026-09-23. Recherche documentaire uniquement (aucun code Zed modifié).
- Objet : cas où le moteur de conteneurs **n'est pas** (ou pas seulement) sur la machine de l'éditeur. Pour chaque topologie : qui lance le CLI docker, où vivent les sources, quel chemin sert de source de bind-mount et **qui le résout**, comment le contexte de build et les fichiers générés atteignent le démon, où tourne `initializeCommand`, comment l'éditeur se connecte, UID, pièges.
- Légende : **[lu]** = lu dans la source citée ; **[log]** = déduit d'un log publié dans une issue ; **[communauté]** = source non officielle (forum, blog tiers) ; **non vérifié** = connaissance générale non relue ici.

## Sources et versions

| Réf. | Source | Version / commit lu |
|---|---|---|
| VSD | `microsoft/vscode-docs` (Markdown source de code.visualstudio.com), lu via `gh api …/contents/<fichier>` | `DateApproved: 9/16/2026` sur les pages lues |
| CLI | `github.com/devcontainers/cli` (clone `--depth 1`) | `5dc7533314b5` (2026-08-28) |
| FEAT | `devcontainers/features` `src/docker-outside-of-docker/{devcontainer-feature.json,NOTES.md,install.sh}` | feature v1.10.1 |
| SPEC | https://containers.dev/implementors/json_reference/ et https://containers.dev/supporting | lu 2026-09-23 |
| DD | docs.docker.com (WSL, best practices) + issues/forums Docker | lu 2026-09-23 |
| DP | `github.com/loft-sh/devpod` (clone sparse `pkg`,`cmd`) | `5a0efcbff661` (2025-11-14 — dépôt peu actif depuis ; forks non examinés) |
| JB | jetbrains.com/help/idea (FAQ, limitations, prérequis Dev Containers) | lu 2026-09-23 |
| GHCS | docs.github.com Codespaces « deep dive » + containers.dev/supporting | lu 2026-09-23 |
| POD | docs.podman.io (`podman-machine-init`, `podman-run`) | « latest » au 2026-09-23 |

### Faits transverses (valables pour toutes les topologies)

| Fait | Source |
|---|---|
| L'extension VS Code Dev Containers **embarque le CLI de référence** et l'exécute avec le `node` du serveur VS Code **sur l'hôte « docker »** : `…/.vscode-server/…/node ~/.vscode-remote-containers/dist/dev-containers-cli-0.417.0/dist/spec-node/devContainersSpecCLI.js up --workspace-folder /home/… --mount type=volume,source=vscode,target=/vscode --update-remote-user-uid-default on --mount-workspace-git-root …` | [log] https://github.com/microsoft/vscode-remote-release/issues/11059 (log « Dev Containers 0.417.0 over Remote - SSH ») |
| Le CLI open source ne construit qu'un hôte `local` (`getCLIHost` → `createLocalCLIHostFromExecFunctions`, `type: 'local'`), mais le type prévoit `'local' \| 'wsl' \| 'container' \| 'ssh'` : les autres variantes sont fournies par l'extension propriétaire. | [lu] CLI `src/spec-common/cliHost.ts:18`, `:56-60`, `:62-64` |
| Le CLI ne gère **pas** `DOCKER_HOST`/`DOCKER_CONTEXT` lui-même : il hérite de l'environnement et appelle `dockerPath` (`--docker-path`, défaut `docker`). Aucune occurrence de `DOCKER_HOST` dans `src/`. | [lu] CLI `src/spec-node/devContainers.ts:174`, `devContainersSpecCLI.ts:134-135` ; grep vide |
| Variante de CLI détectée par `docker -v` : `docker`, `podman`, **`wslc`** (runtime de conteneurs natif WSL). | [lu] CLI `src/spec-shutdown/dockerUtils.ts:301-322` |
| Montage par défaut du workspace : `type=bind,source=<hostMountFolder>,target=/workspaces/<basename>` où `hostMountFolder` = racine git (si `--mount-workspace-git-root`) **calculée dans le système de fichiers du CLI**. `consistency=` ajouté seulement si `cliHost.platform !== 'linux'` (« Podman does not tolerate consistency= »). | [lu] CLI `src/spec-node/utils.ts:415-475` (l. 463-467) |
| Le **démon** résout la source d'un bind-mount dans **son** système de fichiers ; le client (CLI/compose) lit localement Dockerfile, contexte de build et YAML compose. « The Docker daemon doesn't read the compose YAML path; the docker compose client/plugin process reads it locally. » | [lu] réponse mainteneur, https://github.com/devcontainers/cli/issues/1166 |
| Features : dossier temporaire `<tmpdir>/devcontainercli-<user>/container-features/<ver>-<ts>` sur l'hôte du CLI, transmis via `docker buildx build --build-context` (BuildKit ≥ 0.8) sinon via une image temporaire `dev_container_feature_content_temp` → **transporté par le client**, fonctionne avec un démon distant. | [lu] CLI `src/spec-node/utils.ts:635-646`, `containerFeatures.ts:96`, `:236-246` |
| Contexte de build du Dockerfile : chemin passé en argument au client (`args.push(await uriToWSLFsPath(getDockerContextPath(...)))`) → tarball envoyé par le client. | [lu] CLI `src/spec-node/singleContainer.ts:249` |
| `initializeCommand` : « run on the **host machine** » ; le CLI l'exécute via `cliHost.exec` avec `/bin/sh -c` (ou `%ComSpec% /c` si `cliHost.platform === 'win32'`). « Hôte » = là où tourne le CLI. | [lu] SPEC json_reference ; CLI `src/spec-node/utils.ts:538-575`, appel `configContainer.ts:66` |
| `updateRemoteUserUID` : n'agit que si `cliHost.platform === 'linux'` (ou darwin si `updateRemoteUserUIDOnMacOS`, forcé à `false`) ; ignoré si `remoteUser` est `root` ou numérique ; produit une image `<nom>-uid`. L'UID cible est celui **de l'utilisateur du CLI**. | [lu] CLI `src/spec-node/containerFeatures.ts:421-443`, `devContainers.ts:248` |
| Podman sous Linux : `--security-opt label=disable` + `--userns=keep-id` si `remoteUser` ≠ root et pas de `--uidmap/--gidmap` dans `runArgs`. | [lu] CLI `src/spec-node/singleContainer.ts:438-451`, appel l. 413 |
| Connexion VS Code ↔ conteneur : `docker exec -i -u … /bin/sh` (« Run in container ») ; binaires du serveur VS Code dans le volume nommé `vscode` monté en `/vscode`, puis `~/.vscode-server` de l'utilisateur ; ssh-agent relayé (`SSH_AUTH_SOCK in container (/tmp/vscode-ssh-auth-….sock) forwarded to remote host`). | [log] vscode-remote-release#11059 |

---

## T1 — Local (Linux natif, Docker Desktop macOS/Windows natif)

| Aspect | Comportement de référence | Source |
|---|---|---|
| Qui lance le CLI docker | Processus de l'éditeur (extension / CLI) sur la machine locale ; `docker` du PATH ou `dev.containers.dockerPath`. | [lu] VSD `docs/devcontainers/containers.md:29-45` ; CLI `devContainers.ts:174` |
| Où vivent les sources | Disque local (Linux : ext4… ; macOS : APFS ; Windows : NTFS). | VSD `containers.md` |
| Source du bind-mount | Chemin local natif (`C:\…` sous Windows, `/Users/…` sous macOS). Linux : résolu par le démon local directement. macOS/Windows : le démon est dans une VM ; Docker Desktop partage les dossiers (File Sharing) — sans backend WSL2, il faut cocher les lecteurs/dossiers dans **Resources > File Sharing**. | [lu] VSD `docs/devcontainers/tips-and-tricks.md:51-68`, `faq.md:22` |
| Transfert du contexte | Client docker local → démon (tarball / BuildKit session). | [lu] CLI `singleContainer.ts:249` |
| `initializeCommand` | Machine locale ; `cmd.exe /c` sous Windows, `/bin/sh -c` ailleurs. | [lu] CLI `utils.ts:558-560` |
| Connexion éditeur | `docker exec` + serveur VS Code dans le conteneur ; ports via `forwardPorts` (tunnel dans la session exec) ou `appPort` (publication). | [lu] VSD `containers.md:471-503` ; [log] #11059 |
| UID | Linux : `updateRemoteUserUID` aligne l'UID du `remoteUser` sur l'UID local. macOS : fichiers montés « act as if they are owned by the container user ». Windows : fichiers vus comme `root`, rwx pour tous, pas de mappage possible. | [lu] VSD `remote/advancedcontainers/add-nonroot-user.md:14-30` |
| Pièges | Perf des bind-mounts dans une VM (macOS/Windows) → « Clone Repository in Container Volume » ou volumes ciblés (`node_modules`). Snap Docker Ubuntu et Docker Toolbox non supportés. Fins de ligne Windows vs conteneur. | [lu] VSD `improve-performance.md`, `containers.md:590-600`, `:84` |

## T2 — Windows + sources dans une distro WSL2 + Docker Desktop (intégration WSL)

| Aspect | Comportement de référence | Source |
|---|---|---|
| Qui lance le CLI docker | Deux entrées documentées : (a) fenêtre **WSL** puis « Reopen in Container » → extension et CLI dans la distro ; (b) « Open Folder in Container… » sur `\\wsl$\…` depuis Windows. Dans les logs, l'extension lance `wsl -d <distro> -e /bin/sh -c …` puis exécute ses commandes « in host » (= dans la distro) avec un `node` déposé sous `~/.vscode-remote-containers`. `dev.containers.executeInWSL` (+ `executeInWSLDistro`) force l'exécution dans WSL. | [lu] VSD `containers.md:141-150` ; [log] vscode-remote-release#9194 ; réglage : [communauté] résultats de recherche + issue #11005 (réglage existant, doc officielle **non lue**) |
| Où vivent les sources | ext4 de la distro (`/home/<u>/…`). Recommandé pour la perf et les événements inotify : « Linux containers only receive file change events… if the original files are stored in the Linux filesystem ». | [lu] DD https://docs.docker.com/desktop/features/wsl/best-practices/ ; VSD `improve-performance.md:14-18` |
| Source du bind-mount | Chemin **Linux de la distro** (`/home/u/proj`). Pour un chemin Windows de fichier (`file:` URI), le CLI hébergé en WSL le convertit par `wslpath -u`. Le moteur Docker Desktop vit dans la distro `docker-desktop` ; la traduction « chemin de la distro appelante → chemin visible du moteur » est faite par Docker Desktop (répertoires `/mnt/wsl/docker-desktop-bind-mounts/<distro>/<hash>`, préfixe `/run/desktop/mnt/host/…` pour les lecteurs Windows et `/run/desktop/mnt/host/wsl` pour `/mnt/wsl`). | [lu] CLI `src/spec-node/utils.ts:61-74` ; DD « Docker Desktop runs inside its own `docker-desktop` WSL distribution » (https://docs.docker.com/desktop/features/wsl/) ; mécanisme de traduction : **[communauté]** https://forums.docker.com/t/…/94097, https://github.com/docker/for-win/issues/14271 |
| Transfert du contexte | CLI `docker` de la distro (installé par l'intégration WSL) → socket proxifié vers le moteur ; contexte lu dans la distro. | [lu] DD (intégration activée par « Settings > Resources > WSL Integration ») ; détail du proxy **non vérifié** |
| `initializeCommand` | Là où tourne le CLI : **dans la distro** (`/bin/sh -c`) en mode WSL. | [lu] CLI `utils.ts:558-560` + [log] #9194 (« Run in host » = distro) |
| Connexion éditeur | `docker exec` depuis le côté qui pilote (distro) ; serveur VS Code installé dans le conteneur. | [log] #9194 (« Run in container ») |
| UID | CLI en WSL ⇒ `cliHost.platform === 'linux'` ⇒ `updateRemoteUserUID` actif, aligné sur l'UID de l'utilisateur de la distro. | [lu] CLI `containerFeatures.ts:425` |
| Pièges | Mélange CLI Windows (`docker.exe`) / CLI WSL : l'extension a sondé WSL même avec un `docker.exe` Windows (#9194) ; `executeInWSL` réglé globalement redirige **tous** les dev containers (#11005). Sources sous `/mnt/c` : lent, pas d'inotify. Lecteurs réseau/non-C: mal traduits (docker/for-win#14271). | [log]/[lu] issues citées |

## T3 — Windows + sources dans WSL2 + Docker CE installé DANS la distro (sans Docker Desktop)

| Aspect | Comportement de référence | Source |
|---|---|---|
| Qui lance le CLI docker | VS Code : ouvrir le dossier via l'extension **WSL** (le CLI tourne dans la distro), ou `dev.containers.executeInWSL: true` + `dev.containers.dockerPath` (ex. `/usr/bin/docker`). | [lu] VSD `remote/advancedcontainers/docker-options.md:26-30` ; réglages : [communauté] |
| Où vivent les sources | ext4 de la même distro que `dockerd`. | idem |
| Source du bind-mount | Chemin Linux de la distro, **résolu directement** par `dockerd` qui partage le système de fichiers (aucune traduction). | déduction du modèle (CLI `utils.ts:463-467`) |
| Transfert du contexte | Local à la distro. | — |
| `initializeCommand` | Dans la distro. | CLI `utils.ts:538-575` |
| Connexion éditeur | `docker exec` lancé dans la distro. Un `docker.exe` Windows ne voit pas ce démon sans exposition TCP/contexte. DevPod, pour ce cas, **réécrit les chemins** quand `DOCKER_HOST=tcp://` sous Windows : `C:\x\y` → `/mnt/c/x/y`. | [lu] DP `pkg/driver/docker/docker.go:587-605` (`EnsurePath`) |
| UID | CLI Linux ⇒ `updateRemoteUserUID` actif (UID de l'utilisateur WSL). | [lu] CLI `containerFeatures.ts:425` |
| Pièges | `dockerd` ne démarre pas sans systemd (WSL ≥ 0.67.6 : activer systemd). Arrêt de la distro (`wsl --shutdown`/timeout) arrête le démon. Variante récente : **wslc** (runtime WSL natif) reconnu par le CLI (`CLIVariant.Wslc`) — pas de `--init/--privileged/--cap-add/--security-opt`, `--mount` converti en `-v`, pas de compose ni de features selon un guide tiers. | [lu] VSD `docker-options.md:30` ; CLI `singleContainer.ts:363-372`, `:453-490`, `utils.ts:217` ; wslc : [communauté] https://wslcontainers.com/guides/vscode-dev-containers/ |

## T4 — Éditeur local + hôte SSH avec Docker, sources sur l'hôte (« Remote-SSH puis Reopen in Container »)

| Aspect | Comportement de référence | Source |
|---|---|---|
| Qui lance le CLI docker | **L'hôte SSH** : Dev Containers s'exécute côté serveur VS Code distant et lance le CLI embarqué avec le `node` du serveur. « You do not even need to have a Docker client installed locally. » Idem Remote-Tunnels. | [lu] VSD `develop-remote-host.md:14-36` ; [log] #11059 (l. « Start: Run: /home/…/.vscode-server/…/node …/devContainersSpecCLI.js up ») |
| Où vivent les sources | Système de fichiers de l'hôte SSH. | idem |
| Source du bind-mount | Chemin **de l'hôte SSH** (`--workspace-folder /home/liuyc/Projects/…`), résolu par le `dockerd` du même hôte : aucune traduction. | [log] #11059 |
| Transfert du contexte | Local à l'hôte SSH. | — |
| `initializeCommand` | **Sur l'hôte SSH**, pas sur le poste de l'utilisateur (log : `Start: Run: /bin/sh -c ./.devcontainer/init-cache.sh` exécuté par le CLI distant). | [log] #11059 ; CLI `utils.ts:560` |
| Connexion éditeur | Client local ↔ serveur VS Code sur l'hôte (SSH) ↔ `docker exec` vers le conteneur ; serveur VS Code copié depuis le volume `vscode` (`/vscode/vscode-server/bin/linux-x64/<commit>`) ; ssh-agent chaîné conteneur → hôte distant. | [log] #11059 |
| UID | `--update-remote-user-uid-default on` ; plateforme de l'hôte SSH = linux ⇒ alignement sur l'UID de l'utilisateur SSH. | [log] #11059 ; [lu] CLI `containerFeatures.ts:425` |
| Autres implémentations | **DevPod** (provider `ssh`, « machine »/non-machine) : injecte son agent sur l'hôte via la commande du provider, lance `'<agent>' agent workspace up --workspace-info …` à distance ; le runner devcontainer (donc `initializeCommand`, avec `sh -c`/`COMSPEC`) tourne **dans l'agent distant** ; IDE connecté en SSH via `helper ssh-server --stdio` à travers le tunnel. **JetBrains** : « Creating a Dev Container from an existing SSH backend-client connection is not supported » → utiliser Docker-over-SSH. | [lu] DP `cmd/up.go:496-560`, `pkg/devcontainer/run.go:95-110`, `:180-200`, doc https://devpod.sh/docs/how-it-works/overview ; JB https://www.jetbrains.com/help/idea/dev-container-limitations.html |
| Pièges | Hôte Alpine non supporté par Remote-SSH (Colima). Passphrase de clé SSH / incompatibilité OpenSSH Windows ≤ 8.8. X11/GPG à relayer sur deux sauts (#8550, #11059). Hôte distant sous Windows : sondes WSL locales inutiles (#11800). | [lu] VSD `docker-options.md:38`, `containers.md:596-600` ; issues citées |

## T5 — Éditeur local + `DOCKER_HOST=ssh://…` / contexte docker distant, sources locales

| Aspect | Comportement de référence | Source |
|---|---|---|
| Qui lance le CLI docker | Machine locale ; démon choisi par `containers.environment.DOCKER_HOST`/`DOCKER_CONTEXT` (réglage de l'extension Container Tools, honoré par Dev Containers), variables d'environnement au lancement de `code`, ou contexte courant. Repli : `ssh -NL localhost:23750:/var/run/docker.sock user@host` + `DOCKER_HOST=tcp://localhost:23750`. | [lu] VSD `develop-remote-host.md:38-139` ; `tips-and-tricks.md:218-240` |
| Où vivent les sources | Localement… mais **inutilisables** comme bind-mount : « Docker does **not** support mounting (binding) your local filesystem into a remote dev container, so Visual Studio Code's default `devcontainer.json` behavior to use your local source code will not work. » | [lu] VSD `develop-remote-host.md:12` |
| Source du bind-mount | La référence **ne traduit rien et n'envoie rien** : l'utilisateur doit réécrire `workspaceMount` : (1) volume nommé `source=remote-workspace,target=/workspace,type=volume` puis `git clone` dans le conteneur (même technique que « Clone Repository in Container Volume ») ; (2) bind d'un chemin **de l'hôte distant** `source=/absolute/path/on/remote/machine,…`. Compose : idem dans `docker-compose.yml`. | [lu] VSD `develop-remote-host.md:58-84`, `:141-202` |
| Transfert du contexte | Le client local envoie contexte de build et contenu des features (BuildKit `--build-context`) → fonctionne. Les chemins relatifs des YAML compose sont résolus **côté client** en chemins locaux absolus puis interprétés par le démon distant → faux. | [lu] CLI `containerFeatures.ts:96`, `:236-246` ; cli#1166 |
| `initializeCommand` | Machine locale (CLI local). | CLI `utils.ts:538-575` |
| Connexion éditeur | `docker exec` via le même `DOCKER_HOST` (tunnel SSH du client docker). | [lu] VSD `develop-remote-host.md:56` (« attach to containers on the remote host ») |
| UID | Calculé sur la plateforme **du CLI local** : Linux local ⇒ UID local appliqué alors que les fichiers (volume/chemin distant) appartiennent à un autre UID ; macOS/Windows ⇒ aucun alignement. | [lu] CLI `containerFeatures.ts:425` (conséquence déduite) |
| Autres implémentations | **DevPod** : si le dossier d'origine n'existe pas sur la machine de l'agent, **téléverse un tar** du dossier local (respecte `.devpodignore`) par le tunnel, l'extrait dans le dossier de contenu de l'agent, puis le bind-mounte (`Upload folder to server`). **JetBrains** (Docker via SSH) : ne propose pas « Mount Sources » en SSH (« no need to use such a complicated and slow method »), clone dans un volume `jb_devcontainer_sources_xxx` via un conteneur auxiliaire `alpine/git` ; exige Docker **local** pour assembler le contexte ; backend IDE copié une fois dans le volume `jb_devcontainers_shared_volume`. | [lu] DP `pkg/agent/agent.go:188-198`, `cmd/agent/workspace/up.go:308-317`, `:407-419`, `pkg/agent/tunnelserver/tunnelserver.go:377-386` ; JB https://www.jetbrains.com/help/idea/faq-about-dev-containers.html |
| Pièges | `--mount type=bind` vers un chemin absent échoue (« bind source path does not exist ») ; `-v` le crée vide côté distant (**non vérifié** ici, comportement Docker connu). ssh-agent requis et souvent inactif sous Windows/Linux. Ports publiés liés à l'interface du distant. `${localEnv:…}` = poste local, pas l'hôte du démon. | [lu] VSD `develop-remote-host.md:54`, `:94` |

## T6 — Éditeur lui-même dans un conteneur (socket docker monté : DooD / DinD)

| Aspect | Comportement de référence | Source |
|---|---|---|
| Qui lance le CLI docker | Le conteneur de l'éditeur/outil, via le socket de l'hôte (DooD) ou un `dockerd` interne (DinD). Feature DooD : monte `/var/run/docker.sock` (hôte) → `/var/run/docker-host.sock`, `securityOpt: label=disable`, entrypoint `docker-init.sh` ; accès non-root par ajustement du GID du groupe `docker`, sinon relais `socat` vers `/var/run/docker.sock`. | [lu] FEAT `devcontainer-feature.json` (`mounts`, `socketPath` défaut `/var/run/docker-host.sock`, `securityOpt`), `install.sh:16-18`, `:424-510` |
| Où vivent les sources | Dans le conteneur de l'outil (souvent bind d'un dossier hôte, ou volume). | — |
| Source du bind-mount | **Chemin du démon (hôte)**, pas celui vu dans le conteneur : « the path inside the container may not match the path of the directory on the host ». Solutions documentées : `remoteEnv: { "LOCAL_WORKSPACE_FOLDER": "${localWorkspaceFolder}" }` (ou `HOST_PROJECT_PATH`) puis réécrire les chemins ; ou monter le workspace **au même chemin** (`workspaceFolder: "${localWorkspaceFolder}"`, `workspaceMount: "source=${localWorkspaceFolder},target=${localWorkspaceFolder},type=bind"`). Impossible si le workspace est un volume (« Clone Repository in Container Volume » : pas de `${localWorkspaceFolder}`) → DinD. DinD : les chemins du conteneur fonctionnent tels quels, mais pas de cache partagé avec l'hôte. | [lu] VSD `use-docker-kubernetes.md:22-65` ; FEAT `NOTES.md:1-54` |
| Transfert du contexte | Client dans le conteneur → contexte envoyé correctement. Tout ce que le CLI met dans `/tmp` **puis bind-monte** est cherché dans le `/tmp` de l'hôte (cas `devcontainer features test`) ; contournement : monter `/tmp/devcontainercli` à l'identique ou déplacer `TMPDIR`. | [lu] https://github.com/devcontainers/cli/issues/923 |
| `initializeCommand` | Là où tourne le CLI = **dans le conteneur de l'outil**, pas sur l'hôte réel. | CLI `utils.ts:538-575` (déduction) |
| Connexion éditeur | `docker exec` via le socket (même démon). | — |
| UID | CLI en Linux ⇒ UID **de l'utilisateur du conteneur outil**, qui peut différer du propriétaire des fichiers sur l'hôte. | CLI `containerFeatures.ts:425` (déduction) |
| Référence proche | **Codespaces** : VM hôte, dépôt cloné dans `/workspaces` de la VM puis monté ; « Codespaces ignores 'bind' mounts with the exception of the Docker socket » ; `${localEnv}` = hôte cloud. Le type d'hôte `'container'` existe dans le CLI (utilisé par l'extension, non documenté). | [lu] GHCS https://docs.github.com/en/codespaces/about-codespaces/deep-dive ; SPEC https://containers.dev/supporting ; CLI `cliHost.ts:18` |
| Pièges | Architecture hôte = conteneur requise (DooD). Dossiers hors workspace non montables. Compose : utiliser `${LOCAL_WORKSPACE_FOLDER:-./}`. SELinux : `label=disable` nécessaire pour le socket. | [lu] FEAT `NOTES.md:3-5`, `:25-37` |

## T7 — Podman (rootless, `podman machine` sous Windows/macOS, `userns keep-id`, SELinux)

| Aspect | Comportement de référence | Source |
|---|---|---|
| Qui lance le CLI | `dev.containers.dockerPath = podman` (Podman ≥ 5) ; CLI : `--docker-path podman` ; détection `CLIVariant.Podman` via `podman -v`. Compose : `podman compose` → délègue à Docker Compose ou podman-compose. | [lu] VSD `docker-options.md:40-46` ; CLI `dockerUtils.ts:307-322` |
| Où vivent les sources | Linux : local. macOS : local, VM `podman machine` avec montage par défaut `$HOME:$HOME`. Windows : machine = distro WSL, lecteurs sous `/mnt/c/…` (« passing `--volume` is redundant »). | [lu] POD https://docs.podman.io/en/latest/markdown/podman-machine-init.1.html |
| Source du bind-mount | Linux : chemin local. macOS : même chemin grâce au montage identique `$HOME`. Windows : le client podman distant traduit les chemins Windows vers `/mnt/<lecteur>/…` ; bogues de traduction connus avec podman-compose (`/mnt/d` préfixé à tort). | POD (ci-dessus) ; traduction côté client : [communauté] https://github.com/containers/podman/issues/22684 (**non lu en détail**) |
| Transfert du contexte | Client → API de la machine (tarball). BuildKit `--build-context` : si Podman+SELinux actif (`getenforce` ≠ Disabled et `SELinuxEnabled`), le CLI ajoute `label=disable`. | [lu] CLI `containerFeatures.ts:246`, `:359`, `:363-377` |
| `initializeCommand` | Hôte du CLI (poste). | CLI `utils.ts:538-575` |
| Connexion éditeur | `podman exec`. | — |
| UID | Linux rootless : `--userns=keep-id` (UID hôte = UID dans le conteneur) si `remoteUser` ≠ root et pas d'`--uidmap/--gidmap`. Hors Linux : rien. DevPod : `--userns keep-id` si `IsPodman()` et UID ≠ 0 ; puis réalignement UID sous Linux si `updateRemoteUserUID` non faux. Interaction keep-id + `updateRemoteUserUID` du CLI : **non vérifiée**. | [lu] CLI `singleContainer.ts:438-451` ; DP `pkg/driver/docker/docker.go:293-300`, `:391` |
| SELinux | Le CLI **n'ajoute pas** `:z/:Z` : il désactive l'étiquetage (`--security-opt label=disable`) pour Podman sous Linux. `:z` = étiquette partagée, `:Z` = privée ; ré-étiquetage récursif, dangereux sur `$HOME`. | [lu] CLI `singleContainer.ts:440` ; POD https://docs.podman.io/en/latest/markdown/podman-run.1.html |
| Pièges | `consistency=` refusé par Podman (le CLI ne l'ajoute que hors Linux… donc **ajouté** sur macOS/Windows avec podman machine — tolérance **non vérifiée**). JetBrains : Podman « under development », Colima non supporté. | [lu] CLI `utils.ts:464` ; JB https://www.jetbrains.com/help/idea/prerequisites-for-dev-containers.html |

---

## Matrice de synthèse

| Topologie | CLI docker où | Sources où | Source du bind-mount (résolue par) | Transfert contexte/fichiers générés | `initializeCommand` où | Connexion éditeur | UID |
|---|---|---|---|---|---|---|---|
| T1 local | Poste | Poste | Chemin local (démon local ; VM + File Sharing sur mac/Win) | Client → démon | Poste (`sh`/`cmd`) | `docker exec` + serveur dans conteneur | Linux : `updateRemoteUserUID` ; mac/Win : mappage par Docker Desktop |
| T2 WSL + Docker Desktop | Distro WSL (via `wsl -d … sh`, `executeInWSL`) | ext4 distro | Chemin Linux distro (`wslpath -u` si besoin ; traduction Docker Desktop vers distro `docker-desktop`) | Client distro → proxy DD | Distro | `docker exec` depuis la distro | Aligné sur UID WSL |
| T3 WSL + Docker CE | Distro | ext4 distro | Chemin distro (dockerd même FS) | Local distro | Distro | `docker exec` dans distro (DevPod : TCP + `C:\`→`/mnt/c`) | Aligné sur UID WSL |
| T4 Remote-SSH → conteneur | Hôte SSH (CLI embarqué, node du serveur) | Hôte SSH | Chemin hôte SSH (démon distant, sans traduction) | Local à l'hôte | Hôte SSH | Client ↔ SSH ↔ `docker exec` ; serveur via volume `vscode` | Aligné sur UID SSH |
| T5 `DOCKER_HOST` distant | Poste | Poste (inutilisables) | **Aucune traduction** : volume nommé + clone, ou chemin distant écrit à la main (DevPod : upload tar ; JetBrains : clone en volume) | Build/features OK ; chemins compose faux | Poste | `docker exec` via tunnel du client | Faux/absent (plateforme du poste) |
| T6 outil en conteneur (DooD/DinD) | Conteneur outil | Conteneur outil (souvent bind hôte) | DooD : chemin **hôte** requis (`${localWorkspaceFolder}`, même-chemin) ; DinD : chemin conteneur | Build OK ; fichiers `/tmp` montés → faux | Conteneur outil | `docker exec` via socket | UID du conteneur outil |
| T7 Podman | Poste (`podman`) | Poste ; VM mac `$HOME:$HOME`, Win `/mnt/c` | Chemin local (identique mac ; traduit Win) | Client → API machine ; `label=disable` si SELinux | Poste | `podman exec` | Linux : `--userns=keep-id` ; sinon rien |

## Leçons pour Zed

1. Modéliser explicitement trois lieux distincts : **hôte de l'éditeur**, **hôte du CLI** (où tournent `docker`, `initializeCommand`, lectures de config), **hôte du démon** (qui résout les bind-mounts). La référence les fait coïncider (hôte CLI = hôte démon) dans T1–T4 et échoue/renonce ailleurs.
2. Stratégie VS Code la plus robuste : **déplacer le CLI** là où sont sources et démon (distro WSL, hôte SSH) plutôt que traduire des chemins. Zed a déjà des connexions distantes (SSH/WSL) : lancer la création via `RemoteConnection` sur cet hôte.
3. Utiliser le **même hôte** pour la création **et** pour `docker exec` de la connexion (écueil noté dans la branche pupeno-wsl, cf. `02-contrib-pupeno-wsl.md`).
4. `initializeCommand` s'exécute sur l'hôte du CLI (distro/hôte SSH), avec `sh -c` sauf si l'hôte du CLI est Windows (`%ComSpec% /c`) — ne pas le lancer sur le poste Windows quand le CLI est en WSL.
5. `updateRemoteUserUID` : décider selon la **plateforme de l'hôte du CLI** et l'UID de **cet** utilisateur ; désactiver quand le démon est distant sans partage de FS (T5) ou quand l'outil est conteneurisé (T6).
6. T5 : ne pas prétendre monter les sources locales. Détecter un démon distant (`docker context inspect`, `DOCKER_HOST` non-unix) et proposer volume nommé + clone, chemin distant explicite, ou (option DevPod) upload tar — jamais un échec silencieux sur un bind vide.
7. Tout ce que Zed génère et veut **monter** (scripts, config fusionnée) doit être dans le FS du démon ; ce qui passe par le **contexte de build** (`--build-context`, Dockerfile) est transporté par le client et marche partout. Préférer le second.
8. T6 : exposer/consommer `${localWorkspaceFolder}` « vu de l'hôte » et supporter le motif « même chemin » ; documenter que DinD évite le problème.
9. WSL : conversion `wslpath -u` pour les URI Windows ; interdire/avertir pour les sources sous `/mnt/c` (perf, pas d'inotify).
10. Détecter la variante de CLI par `docker -v` (docker/podman/wslc) et adapter : pas de `consistency=` avec Podman Linux, `label=disable` + `--userns=keep-id` pour Podman rootless, sous-ensemble d'options pour wslc.
11. Laisser `DOCKER_HOST`/`DOCKER_CONTEXT` hériter de l'environnement **et** offrir un réglage Zed équivalent à `containers.environment` / `dev.containers.dockerPath` / `executeInWSL`(+ distro), réglable par projet.
12. Labels d'identification (`devcontainer.local_folder`, `devcontainer.config_file`) : utiliser le chemin **de l'hôte du CLI**, sinon les conteneurs ne se retrouvent pas entre Windows et WSL.
13. Relais d'agents (ssh-agent, GPG) : prévoir le chaînage multi-sauts (conteneur → hôte SSH/WSL → poste) comme VS Code.
14. Distribuer le serveur distant de Zed dans le conteneur via un **volume partagé** (cf. volume `vscode` et `jb_devcontainers_shared_volume`) plutôt qu'un bind depuis le poste, pour fonctionner avec un démon distant.
15. Journaliser chaque commande avec son hôte (« Run in host / Run in container » de VS Code) : c'est ce qui rend ces topologies diagnostiquables.

## Non vérifié / lacunes

- Code de l'extension VS Code (propriétaire) : comportement WSL/SSH déduit des docs et de logs d'issues, pas du code. Documentation officielle de `dev.containers.executeInWSL` non trouvée (seulement issues et guides tiers).
- Mécanique interne de traduction des bind-mounts de Docker Desktop (proxy, `docker-desktop-bind-mounts`) : sources communautaires uniquement.
- `host.docker.internal` : page réseau Docker Desktop lue sans mention exploitable ; non documenté ici.
- Podman sous Windows : traduction de chemins côté client non lue dans le code ; tolérance de `consistency=` non vérifiée.
- JetBrains : transport client ↔ backend dans le conteneur non documenté dans les pages lues.
- Codespaces : valeur de `${localWorkspaceFolder}` et mécanisme de connexion (tunnel/SSH) non vérifiés.
