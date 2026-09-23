# 04 — WSLc (WSL Containers, `wslc.exe`) comme moteur de Dev Containers pour Zed sous Windows

Date : 2026-09-23
Machine : Windows 11 Pro 10.0.26200.8524, WSL 2.9.12.0 (pré-version), noyau 6.18.40.1-1, distro Ubuntu (WSL2, en marche).

Légende des sources : **[local]** = sondage sur cette machine ; **[src]** = code de github.com/microsoft/WSL (branche `master`, lu le 2026-09-23 ; peut être plus récent que la 2.9.12) ; **[doc]** = learn.microsoft.com ; **[rel]** = notes de version GitHub ; **[issue #n]** = microsoft/WSL#n ; **[devc]** = github.com/devcontainers/cli.

---

## Sondage local

Toutes les commandes ont été lancées en lecture seule (`--help`, `version`, `info`, `list`). Remarque : `wslc info` affichait `Sessions : 0` ; après `wslc list -a`, `wslc system session list` affiche une session `wslc-cli-Robot`. **Les commandes de lecture qui touchent le moteur démarrent donc la session par défaut (sa VM)**, ce qui est à retenir pour Zed : un simple `ps` a un coût de démarrage à froid.

### Emplacement et PATH

```text
$ where wslc                       (Git Bash et PowerShell)
Information : impossible de trouver des fichiers pour le(s) modèle(s) spécifié(s).

PS> Get-ChildItem "C:\Program Files\WSL" -Filter *.exe
container.exe      8906040      <- même taille que wslc.exe (alias documenté)
wslc.exe           8906040
wslcsession.exe    7703864
wslservice.exe     7342944
wsl.exe, wslg.exe, wslhost.exe, wslrelay.exe, msrdc.exe, msal.wsl.proxy.exe

PS> [Environment]::GetEnvironmentVariable('Path','Machine') -split ';' | ? { $_ -match 'WSL' }
C:\Program Files\WSL\
```

`C:\Program Files\WSL\` est bien dans le PATH machine, mais pas dans l'environnement des processus déjà lancés (ceux démarrés avant la mise à jour de WSL). **Zed doit donc prévoir un repli sur le chemin absolu `%ProgramFiles%\WSL\wslc.exe`.**

### Versions

```text
$ wsl --version
Version WSL : 2.9.12.0
Version du noyau : 6.18.40.1-1
Version WSLg : 1.0.79
Version MSRDC : 1.2.7214
Version direct3D : 1.611.1-81528511
Version de DXCore : 10.0.26100.1-240331-1435.ge-release
Version de Windows : 10.0.26200.8524

$ wsl -l -v
  NAME      STATE           VERSION
* Ubuntu    Running         2

$ wslc --version        ->  wslc 2.9.12.0
$ wslc version          ->  wslc 2.9.12.0
$ wslc version --format json
{"Client":{"Version":"2.9.12.0"}}
```

Point important : `wslc -v` / `--version` affiche « wslc … ». C'est la chaîne sur laquelle `devcontainers/cli` s'appuie pour détecter la variante (voir Q7).

### `wslc info`, sessions, listes

```text
$ wslc info
Client :
Version WSL : 2.9.12.0
...
Fichier de paramètres : C:\Users\Robot\AppData\Local\wslc\settings.yaml
Serveur :
Version du gestionnaire de session : 2.9.12
Sessions : 0

$ wslc system session list --verbose     (avant)
[wslc] Found 0 sessions
$ wslc list -a --format json              (sortie vide : aucun conteneur)
$ wslc images
REPOSITORY   TAG   IMAGE ID       CREATED   SIZE
$ wslc network list
NETWORK ID     NAME      DRIVER    SCOPE
3264210cba72   bridge    bridge    local
38dd0da1abe3   host      host      local
f0441a89e1df   none      null      local
$ wslc volume list
DRIVER   VOLUME NAME
$ wslc system session list --verbose     (après)
ID   PID du créateur   Nom complet
1    30520             wslc-cli-Robot
```

Stockage de la session : `%LOCALAPPDATA%\wslc\sessions\wslc-cli-Robot\storage.vhdx` (~600 Mo) et `swap.vhdx`. Aucun canal nommé de type docker n'a été trouvé dans `\\.\pipe\` (on n'y voit que `wsl_debugshell_*` et des canaux mojo/crashpad sans rapport).

### `wslc --help` (verbes de premier niveau)

```text
Utilisation : wslc [global-options] [<commande>] [options]
Commandes :
  container image network registry settings system volume
  attach build create exec export images import info inspect kill list load
  login logout logs pull push remove restart rmi run save start stats stop tag version
Options :  -v --version | -? --help
Options globales : --session  Spécifiez la session à utiliser
```

Groupes :

- `container` : attach, **cp**, create, exec, export, inspect, kill, logs, list, prune, remove, restart, run, start, stats, stop
- `image` : build, remove, inspect, list, load, import, prune, pull, push, save, tag
- `network` : create, remove, inspect, list, prune, connect, disconnect
- `volume` : create, remove, inspect, list, prune
- `registry` : login, logout
- `system` : info, session (enter, list, run, shell, terminate)
- `settings` : reset

Alias notables :

- `list` = `ls` = `ps` = `container ls|ps|list`
- `remove` = `rm` = `delete`
- `rmi` = `image rm|remove|delete`
- `images` = `image ls`

Verbes **absents** du premier niveau (tous renvoient `Commande non reconnue`) : `cp` (disponible seulement en `wslc container cp`), `events`, `buildx`, `compose`, `pause`, `top`, `port`, `wait`, `update`, `rename`, `commit`, `diff`, `context`.

### Options par verbe (reproduites depuis les `--help` locaux, en français, sans l'option `--help` ni l'option globale `--session`)

**run / create** (identiques ; `-d --detach` existe seulement sur `run`) :

```text
--cidfile  --cpus  -d --detach  --dns  --dns-option  --dns-search  --domainname
--entrypoint  -e --env  --env-file  --gpus  --health-cmd  --health-interval
--health-retries  --health-start-period  --health-timeout  -h --hostname
-i --interactive  --ip  -l --label  -m --memory  --mount  --name  --network
--network-alias  --no-healthcheck  -p --publish  -P --publish-all
--pull (always|missing|never)  --rm  --shm-size  --stop-signal  --stop-timeout
--tmpfs  -t --tty  --ulimit  -u --user  -v --volume  -w --workdir
```

Absents : `--init`, `--privileged`, `--cap-add`, `--cap-drop`, `--security-opt`, `--device`, `--platform`, `--sig-proxy`, `-a/--attach`, `--add-host`, `--group-add`, `--read-only`, `--userns`, `--ipc`, `--pid`, `--volumes-from`, `--restart`.

**exec** : `wslc exec [options] <container-id> <command> [<arguments>...]`

```text
-d --detach  -e --env  --env-file  -i --interactive  -t --tty  -u --user  -w --workdir
```

**list/ps** :

```text
-a --all  -f --filter  --format (json|table)  -n --last  -l --latest  --no-trunc  -q --quiet  -s --size
```

**inspect** : `wslc inspect [options] <object-id>...`

```text
-t --type (image|container|network|volume)  -s --size
-f --format  « json pour sortie sur une seule ligne » (par défaut : JSON indenté)
```

`container inspect` et `image inspect` acceptent le même `-f/--format`.

**build** : `wslc build [options] <path>`

```text
--build-arg  --pull  --target  -f --file (Dockerfile, "-" = stdin)  --iidfile  -l --label  --no-cache
-o --output (spéc. buildx : type=local,dest=… | type=tar,dest=…)
--progress (auto|tty|plain|quiet)  --secret  -t --tag  --verbose
```

Absents : `--build-context`, `--platform`, `--load`, `--cache-from/--cache-to`, `--ssh`, `--network`, `--add-host`.

**container cp** : `wslc container cp [options] <source> <target>`, avec source/destination `LOCAL_PATH`, `CONTAINER:PATH` ou `-` (stdin). Options : `-a --archive` (« accepté pour la compatibilité avec l'interface de ligne de commande Docker ») et `-q --quiet`.

**Autres verbes** :

- `start [-a --attach] [-i --interactive] <id>`
- `stop [-s --signal] [-t --time] <id>...`
- `restart [-s] [-t --timeout]`
- `kill [-s]`
- `remove [-f --force] [-v --volumes] <id>...`
- `attach <id>`
- `logs [--details] [-f --follow] [-n --tail] [-t --timestamps] [--since] [--until]`
- `export [-o]`
- `import <file|-> [image]`
- `load [-i] [-q]`
- `save [-o]`
- `pull [-q]`
- `push [-a --all-tags] [-q]`
- `rmi [-f] [--no-prune]`
- `tag <src> <dst>`
- `images [-a] [-f] [--format json|table] [--no-trunc] [-q] [--verbose]`
- `stats [-a] [--format] [--no-trunc]`
- `login [-p] [--password-stdin] [-u] [server]`
- `logout [server]`
- `info [--format]`
- `version [--format]`
- `container prune [--filter] [-f]`
- `network create [-d --driver] [-o] [-l] [--gateway] [--internal] [--ip-range] [--subnet]`
- `network list [-f] [--format] [--no-trunc] [-q]`
- `volume create [-d --driver guest|vhd] [-o] [-l]`
- `system session list [--verbose]`

---

## Q1. Qu'est-ce que WSLc : produit, statut, prérequis, où tournent les conteneurs

**Produit.** « WSL container » (WSLC) est une fonctionnalité intégrée à WSL. Elle a deux composants :

- le CLI `wslc.exe`, avec son alias `container.exe` ;
- une API pour applications Windows, distribuée dans le NuGet `Microsoft.WSL.Containers` (C, C++/WinRT, C#).

Microsoft la présente comme une alternative intégrée qui évite d'installer Docker Desktop. Sources : [doc] https://learn.microsoft.com/en-us/windows/wsl/wsl-container et [devblog] https://devblogs.microsoft.com/commandline/wsl-container-is-now-available-for-public-preview/ (annonce à Build 2026).

**Statut.** Préversion publique depuis WSL **2.9.3** (29 juin 2026, [rel] 2.9.3 : « WSL Containers (WSLC) is now in Public Preview »). Toutes les versions 2.9.x, y compris la 2.9.12 du 2026-09-14, sont marquées `prerelease=true` sur GitHub ; la branche stable est en 2.7.14 (2026-09-11). La disponibilité générale est visée pour l'**automne 2026** (presse et devblog : helpnetsecurity, Phoronix). Il n'y a pas encore de GA au 2026-09-23.

**Version minimale.** WSL ≥ 2.9.3, installée avec `wsl --update --pre-release` ([doc]). En pratique, les fonctions utiles à un outil Dev Containers arrivent plus tard :

| Fonction | Version |
|---|---|
| `container cp` | 2.9.4/2.9.8 |
| passage à `docker buildx build` | 2.9.8 |
| `--mount` | ajouté le 2026-08-18, présent dans l'aide de la 2.9.12 |
| sortie `inspect` et `list` alignée sur Docker | 2.9.10 |
| `restart` | 2.9.12 |
| correction des bind mounts virtiofs vus comme appartenant à root | 2.9.12 |

**Prérequis Windows.**

- Il faut la plateforme de machine virtuelle et le paquet WSL : l'API expose `WSLC_COMPONENT_FLAG_VIRTUAL_MACHINE_PLATFORM` et `WSLC_COMPONENT_FLAG_WSL_PACKAGE` ([src] `doc/docs/api-reference/c/enumerations/wslccomponentflags.md`).
- Windows Server 2025 fonctionne ; Server 2022 échoue, d'après des retours d'utilisateurs et non de Microsoft ([issue #41047]).
- Windows 11 26200 fonctionne ([local]).
- Aucune liste officielle de builds Windows minimales n'a été trouvée : **non vérifié**.

**Où tournent les conteneurs.**

- Les conteneurs tournent dans une **VM utilitaire dédiée par « session »**, distincte des distros WSL. `wslcsession.exe` la pilote depuis Windows, et `%LOCALAPPDATA%\wslc\sessions\<nom>\storage.vhdx` contient le stockage des images et conteneurs ([local]).
- Dans cette VM, WSLC lance **`containerd` puis `dockerd`** : `"/usr/bin/dockerd", "--containerd", c_containerdSocket` dans [src] `src/windows/wslcsession/WSLCSession.cpp`. Le client est [src] `DockerHTTPClient.cpp`, et [src] `src/windows/inc/docker_schema.h` précise « Targets the daemon API version bundled with WSLC's dockerd (currently v25.0.3, API v1.44) ».
- Les builds passent par `docker buildx build --builder default` **dans la VM**.
- La session par défaut du CLI s'appelle `wslc-cli-<utilisateur>`. Les applications utilisant le SDK créent leurs propres sessions (option globale `--session`).
- Le moteur est **distinct de Docker Desktop** : magasin d'images séparé, pas de partage (wslcontainers.com, guide VS Code ; tiers).

## Q2. Interfaces : CLI, API/SDK, endpoint compatible Docker ?

- **CLI** : voir le sondage local ci-dessus (liste exhaustive des verbes et options de la 2.9.12).
- **API/SDK** :
  - DLL C `wslcsdk` et projections C++/WinRT et C# dans le NuGet `Microsoft.WSL.Containers` ([doc] ; [src] `src/windows/WslcSDK/`).
  - Objets : `WslcService`, `Session`, `Container`, `Process` (stdout/stderr/stdin, signaux, code de sortie) ([doc]).
  - Le transport sous-jacent est **COM hors processus** vers `wslservice`/`wslcsession` : interfaces IDL [src] `src/windows/service/inc/wslc.idl`, `WSLCShared.idl`, `WSLCCompat.idl`.
  - La projection C++/WinRT est « in preview and is subject to breaking changes » ([doc]).
  - Une API C accessible par FFI depuis Rust existe (`wslccreatesession`, `wslccreatecontainer`, `wslccreatecontainerprocess`, `wslcgetprocessiohandle`, etc. ; [src] `doc/docs/api-reference/c/`). Elle est donc utilisable depuis Rust en théorie, mais sans `build` visible dans la liste des API C (**non vérifié**) ni `exec` sur un conteneur existant ouvert par un autre client ([issue #40990] « WSL.Container API for existing containers and sessions », ouverte).
- **Pas de REST, pas d'endpoint compatible Docker Engine exposé à l'hôte** :
  - Le `dockerd` interne n'est joignable que par `wslcsession` via un canal privé.
  - La demande « Support DOCKER_HOST via Docker Engine API-compatible endpoint » est **ouverte** ([issue #40976], label `feature,wslc`).
  - [local] : aucun canal nommé docker ou WSLC dans `\\.\pipe\`.
  - Sur un site tiers : « There is no endpoint to point it at » (wslcontainers.com/guides/testcontainers).
  - Conséquence : **ni `docker` CLI, ni `docker context`, ni `docker compose` ne peuvent cibler WSLc aujourd'hui.**

## Q3. Compatibilité des arguments « docker » utilisés par un outil Dev Containers

Point de départ : les `--help` locaux de la 2.9.12, complétés par [src] et [devc] PR #1249 (« Add WSLC support », chrmarti, fusionnée le 2026-06-24, CLI v0.88.0).

| Commande / option docker | WSLc 2.9.12 | Remarque / source |
|---|---|---|
| `run -d` | Supportée | [local] |
| `run --mount type=bind,source=…,target=…` | Supportée partiellement | Présente dans l'aide. `consistency=`, `bind-propagation`, `bind-nonrecursive` et `volume-opt/label/driver` sont **refusés avec erreur « unsupported »** ([src] `src/windows/common/MountSpecParsing.cpp`, tableau `c_fieldDefinitions` + `ThrowUnsupported`). devcontainers/cli convertit **toujours** `--mount` en `-v src:dst` pour wslc (`convertMountToVolume`) |
| `run -v src:dst[:ro]` | Supportée | Source résolue **comme chemin Windows**, voir Q4 |
| `run -e`, `--env-file` | Supportée | [local] |
| `run -l/--label` | Supportée | [local] ; mais les labels de l'**image** ne sont pas hérités par le conteneur ([issue #41152], ouverte) |
| `run --entrypoint` | Supportée | [local] |
| `run --init` | **Absente** | [local] ; devcontainers/cli la retire pour wslc (« wslc does not support --init, --privileged, --cap-add, or --security-opt ») |
| `run --privileged` | **Absente** | idem ; « by-design limitation due to some security concerns », à l'étude ([issue #41545], dkbennett, MEMBER) |
| `run --cap-add` / `--cap-drop` | **Absente** | idem |
| `run --security-opt` | **Absente** | idem ; seccomp non contournable ([issue #41503]) |
| `run -p [ip:]h:c` | Supportée | publié sur l'hôte Windows (Q5) ; `-P` aussi |
| `run -w`, `-u`, `--name`, `--rm`, `-i`, `-t`, `-h` | Supportée | [local] |
| `run --sig-proxy=false`, `-a STDOUT` | **Absente** | devcontainers/cli les retire pour wslc |
| `run --platform` | **Absente** | architecture de l'hôte seulement (endjin, wslcontainers.com) |
| `run --gpus`, `--tmpfs`, `--network`, `--shm-size`, `--ulimit` | Supportée | [local] |
| `exec -i -t -u -w -e -d` | Supportée | [local] ; « Fix hang with exec/run -i with self-exiting command and open stdin » ([rel] 2.9.4) |
| `ps -a -q` | Supportée | `list/ls/ps` |
| `ps --filter label=k=v` | Supportée (à confirmer) | `-f/--filter` générique `clé=valeur` transmis à dockerd (« --all and --filter status= are honored by the Docker daemon », [src] `ContainerTasks.cpp`) ; devcontainers/cli s'en sert pour wslc (`listContainers(params,false,labelFilters)`) |
| `ps --format '{{json .}}'` | **Absente** | `--format` accepte seulement `json` ou `table` ([src] `SpecParsing.cpp::GetFormatTypeFromString`). `--format json` produit du JSON par ligne, aux clés alignées sur Docker ([rel] 2.9.10 « Align container list format with Docker specifications ») |
| `inspect --format '{{json .}}'` / templates Go | **Absente** | `-f/--format` accepte seulement `json` (une ligne) ; la sortie par défaut est un tableau JSON indenté. Schéma = **sous-ensemble** de Docker ([src] `wslc_schema.h::InspectContainer` : `Id, Name, Created, Image, State{Status,Running,ExitCode,…}, HostConfig{NetworkMode,Memory,NanoCpus,Ulimits}, Config{Image,Env,Cmd,Entrypoint,User,WorkingDir,Labels,…}, Ports (niveau racine), Mounts[{Type,Name,Source,Destination,ReadWrite}], Labels, NetworkSettings{Networks}`). `Config.Image` manquait encore en 2.9.7 ([issue #41321]) |
| `inspect --type image` | Supportée | `Config.Labels`, `Config.User`, `Config.Env`, etc. |
| `build -f -t --build-arg --target --label --no-cache --pull --secret -o --iidfile` | Supportée | [local] ; buildx/BuildKit **dans la VM** ([src] `WSLCSession.cpp`) |
| `buildx build`, `--build-context`, `--load`, `--platform`, `--cache-*` | **Absente** | [local] (`buildx` non reconnu) ; devcontainers/cli ne peut pas utiliser `--build-context` hors `CLIVariant.Docker`. Les « features » devcontainer ne fonctionnent pas avec wslc, faute des métadonnées BuildKit et d'héritage des labels (wslcontainers.com ; [issue #41152]) |
| `cp` | Équivalent : **`wslc container cp`** | Pas d'alias au premier niveau (`wslc cp` → « Commande non reconnue ») ; `-a` accepté et ignoré ; utilise `tar.exe` côté Windows ([src]) ; bugs ouverts : liens symboliques ([issue #41309]), RAM ([issue #41100]) |
| `start`, `stop`, `restart`, `kill`, `rm -f`, `logs -f` | Supportée | [local] |
| `events` | **Absente** | devcontainers/cli remplace `events` par du polling `ps` toutes les 500 ms pour wslc |
| `version --format '{{.Server.Version}}'` | **Absente** | seulement `json|table` ; devcontainers/cli appelle `version` « simple » pour wslc |
| `compose …` | **Absente** | [issue #40948] « Compose support in WSLc » (ouverte, créée par le PM Craig Loewen) |
| `pull`, `push`, `login/logout`, `tag`, `save/load`, `images` | Supportée | [local] |
| `docker context` / `DOCKER_HOST` | **Absente** | [issue #40976] |

## Q4. Système de fichiers : bind mounts

- **Chemins Windows** : `-v C:\src\proj:/workspaces/proj` fonctionne. Côté service, `PrepareBindMount` fonctionne ainsi :
  - il exige un chemin absolu (`MessagePathNotAbsolute`) et le canonicalise ;
  - si c'est un fichier, il monte le répertoire parent ;
  - il partage le dossier Windows dans la VM sous `/mnt/<GUID>` (`MountWindowsFolder`) ;
  - il passe `"/mnt/<GUID>:<cible>:rw|ro"` à dockerd ;
  - il **crée le répertoire source s'il manque** ([src] `WSLCContainer.cpp`, `MountVolumes`).

  `inspect` renvoie le chemin Windows comme `Source` des bind mounts (« Bind mounts are populated from m_mountedVolumes so their inspect source is the Windows host path »). Dans le conteneur, on voit la cible (`/workspaces/...`).
- **Transport** : virtiofs est le défaut pour WSLC (« up to 2x faster », devblog et presse), avec un seul périphérique virtiofs agrégé pour tous les partages ([rel] 2.9.8 « Use single virtiofs device for all shares », « aggregate share feature in WSLC »). Plan9 est réutilisé pour certains partages ([rel] 2.9.11/2.9.12).
- **Propriétaire des fichiers** : jusqu'à la 2.9.11, les fichiers apparaissaient comme appartenant à root. C'est corrigé en 2.9.12 ([rel] « Fix virtiofs bind mounts exposing files as root-owned (#40719) ») ; le mapping UID effectif vis-à-vis de `remoteUser` reste **non vérifié**.
- **Chemins d'une distro WSL : non supportés.**
  - Tout `-v` est interprété comme un chemin Windows. `-v /var/run/docker.sock:…` crée silencieusement le **répertoire** `C:\var\run\docker.sock` ([issue #40957], commentaire du 2026-09-14 sur 2.9.11).
  - `\\wsl.localhost\Ubuntu\…` échoue.
  - Sur le blog endjin : « Running wslc against the WSL filesystem simply isn't supported yet ».
  - L'exécution de `wslc` depuis une distro est une demande ouverte ([issue #41105]).
- **Performance** :
  - Pour un chemin Windows, virtiofs fait mieux que 9p, mais reste « still much, much slower than having the repo natively inside the WSL filesystem » (endjin, tiers).
  - Autre option : pas de bind, un **volume nommé** (`volume create`, pilotes `guest` ou `vhd`) dans lequel on clone le dépôt.
  - Un contexte de build situé dans une distro doit traverser `\\wsl.localhost` (Plan 9, ~26× à 73× plus lent qu'en natif, [issue #41480]). Le contexte de build est de toute façon monté depuis Windows en lecture seule (`mountInVm(Options->ContextPath, TRUE)`).

## Q5. Réseau

- `-p hôte:conteneur` publie sur l'hôte Windows : « `wslc run -d --rm -p 8080:80 --name web nginx` … `curl localhost:8080` » ([doc] tutoriel), puis `http://localhost:8000/` depuis un navigateur Windows.
- Implémentation : relais `wslrelay` ou chemin virtioNet, avec allocation de port éphémère côté hôte pour `-P`/port 0 ([src] `WSLCContainer.cpp::BuildPortMappings`). UDP et IPv6 sont supportés depuis la 2.9.8 ([rel]).
- Réseaux Docker classiques : `bridge`, `host`, `none`, réseaux personnalisés, alias, `container:<id>` ([local] + [rel] 2.9.3).
- Bugs ouverts : publication TCP après déconnexions répétées ([issue #41052]) ; réécriture du port source UDP ([issue #40999]).
- L'adresse de liaison par défaut (127.0.0.1 ou 0.0.0.0) est **non vérifiée**.

## Q6. Images, registres, auth, build, compose

- **Images et registres** :
  - `pull/push/tag/save/load/import/images/rmi/prune`, `login` (`-u`, `-p`, `--password-stdin`, PAT) et `logout` ([local]).
  - Identifiants stockés via Windows Credential Manager ou un fichier ([src] `WinCredStorage.cpp`, `FileCredStorage.cpp`).
  - Demandes ouvertes : credential helpers ([issue #41030]), proxy d'entreprise ([issue #40981], [issue #40945]), miroirs ([issue #40951]).
  - Les autorités de certification racines de Windows sont recopiées dans la VM ([src]).
- **Build** :
  - Dockerfile/Containerfile supporté via **buildx/BuildKit dans la VM** ([rel] 2.9.8 « switch image build from docker build to docker buildx build »).
  - Options disponibles : `--target`, `--build-arg`, `--secret`, `-o`.
  - `--build-context` et `--platform` sont absents, et il n'y a pas de contexte tar via stdin ([issue #41629]).
- **Compose** : absent ([issue #40948] ouverte). Les configurations `dockerComposeFile` ne peuvent pas fonctionner.

## Q7. Implication pour Zed

Flux actuel de Zed (lu dans `crates/dev_container/src/docker.rs`, `devcontainer_manifest.rs` et `crates/remote/src/transport/docker.rs`) :

| Opération Zed | Commande émise aujourd'hui | Équivalent WSLc |
|---|---|---|
| Détection de buildx | `docker buildx version` | aucun : considérer que BuildKit est interne et forcer le chemin sans buildx |
| Pull | `pull -- <image>` | `wslc pull <image>` (vérifier que `--` est accepté : **non vérifié**) |
| Recherche du conteneur | `ps -a --filter label=… --format={{ json . }}` | `wslc list -a --filter label=… --format json` : **pas de templates Go**, parseur à adapter (JSON par ligne, clés « Docker-like ») |
| Inspect | `inspect --format={{json . }} <id>` | `wslc inspect --type container <id>` (tableau JSON par défaut) ; schéma réduit (`Ports` à la racine, pas de `NetworkSettings.Ports`, `Config.Labels` sans héritage des labels d'image) |
| Build (Dockerfile simple, mise à jour UID) | `build -f -t --build-arg` | équivalent direct |
| Build des « features » | `buildx build --load --build-context … --target …` | **aucun** (pas de `--build-context`) : il faudrait copier les features dans le contexte ou dans un Dockerfile généré |
| Run | `run --sig-proxy=false -d --mount … -l … [--privileged] [--init] [--cap-add] [--security-opt] -p -e --entrypoint` | `wslc run -d -v src:dst -l -p -e --entrypoint` : **retirer** `--sig-proxy`, `--init`, `--privileged`, `--cap-add`, `--security-opt` (avec un avertissement à l'utilisateur) ; `--mount` → `-v` (ou `--mount` sans `consistency`) ; source Windows obligatoire |
| Start | `start <id>` | équivalent |
| Exec (cycle de vie, shell) | `exec -w -u -e <id> sh -c …` | équivalent |
| Proxy stdio du serveur distant | `exec -e … -u … -w … -i <id> <bin> proxy --identifier …` | `wslc exec -i` existe (bug de blocage corrigé en 2.9.4) ; la robustesse du flux binaire long et des reconnexions est **non vérifiée** |
| Envoi du binaire | `cp -a src <id>:dst` puis `exec chown` | `wslc container cp src <id>:dst` (verbe différent) |
| Compose | `compose config/build/up -d` | **aucun** |

Réponses aux options :

- **(a) Via un CLI docker-compatible (`docker`, `docker context`, `DOCKER_HOST`) : non.** Il n'y a pas d'endpoint Docker Engine ([issue #40976]). Utiliser `wslc` à la place de `docker`, comme le fait VS Code avec `dev.containers.dockerPath = "wslc"`, marche seulement **avec des adaptations par variante**. La preuve de référence est devcontainers/cli ≥ 0.88.0 (PR #1249), qui introduit `enum CLIVariant { Docker, Podman, Wslc }` détecté via `-v`, et pour wslc :
  - `--mount` → `-v` ;
  - pas de `--init/--privileged/--cap-add/--security-opt/--sig-proxy/-a` ;
  - polling de `ps` au lieu d'`events` ;
  - `version` sans `--format` ;
  - pas de `--build-context`.
- **(b) Via un adaptateur spécifique : oui, c'est la voie réaliste.** Zed a déjà un booléen `use_podman`. Il faudrait le généraliser en variante `Docker | Podman | Wslc` et adapter : le nom du verbe `cp`, le parsing JSON sans templates, la traduction des montages en chemin Windows, le retrait des flags non supportés, le chemin de build des features sans `--build-context`, et une erreur claire pour compose. Le SDK COM/C (`Microsoft.WSL.Containers`) est une autre option, mais il n'expose pas de build dans l'API C listée, n'ouvre pas les conteneurs d'une autre session et reste en préversion. Il est déconseillé à ce stade.
- **(c) Limites dures.** Il n'y a pas de compose, pas de projet dans le système de fichiers d'une distro WSL (seulement des chemins Windows ou des volumes nommés), pas de `--privileged` (docker-in-docker impossible), pas de `--platform`, et les features ne sont pas fiables (labels d'image non hérités, pas de `--build-context`).

---

## Conclusion pour Zed

1. WSLc (WSL ≥ 2.9.3, **préversion**, GA visée à l'automne 2026) est un vrai moteur OCI : dockerd 25.0.3, containerd et BuildKit tournent dans une VM utilitaire par session. Il n'expose **aucune** API Docker à l'hôte.
2. Ni `docker`, ni `docker context`, ni compose ne peuvent donc le cibler. Seul un **adaptateur CLI `wslc`** est viable ; il reprend la variante `CLIVariant.Wslc` de devcontainers/cli 0.88 (PR #1249).
3. Opérations couvertes : pull, build simple, run -d, start/stop/rm, exec (dont `exec -i` pour le proxy stdio), `container cp`, `list --filter label=`, et `inspect` en JSON.
4. Adaptations obligatoires : pas de templates `--format` ; `cp` devient `container cp` ; `--mount` passe en `-v` ; retirer `--init/--privileged/--cap-add/--security-opt/--sig-proxy` ; chemins Windows uniquement.
5. Bloquants : compose, features (`--build-context`, labels d'image), dépôts dans une distro WSL, docker-in-docker.
6. Recommandation : prévoir une variante « wslc » **expérimentale** derrière un réglage (sur le modèle de `use_podman`), limitée aux configurations `image`/`dockerFile` sur un chemin Windows. Revalider à la GA.

Confiance : **élevée** sur la surface CLI (sondage local de la 2.9.12) et sur l'architecture (code source). **Moyenne** sur le comportement réel de `list --filter`, du schéma `inspect` et d'`exec -i` en 2.9.12 (lus dans le code de `master`, non exécutés). **Faible** sur la performance et le mapping UID (sources tierces).
