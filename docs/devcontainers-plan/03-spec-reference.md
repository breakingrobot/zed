# 03 — Référence normative Dev Containers (spec containers.dev + CLI de référence)

- **Date** : 2026-09-23
- **Objet** : référence sourcée pour l'implémentation Dev Containers dans Zed. Pour chaque sujet : exigence de la spec, comportement du CLI de référence, et où s'exécute quoi.
- **Sources consultées (lues directement, clones `--depth 1`)** :
  - `devcontainers/spec` @ `c95ffeed1d059abfe9ffbe79762dc2fa4e7c2421` (commit du 2026-03-20). Les pages containers.dev correspondent aux fichiers du repo :
    - `implementors/json_reference/` = `docs/specs/devcontainerjson-reference.md`
    - `implementors/spec/` = `docs/specs/devcontainer-reference.md`
    - `implementors/features/` = `docs/specs/devcontainer-features.md`
    - `implementors/json_schema/` = `schemas/devContainer.base.schema.json`
    - Seule `json_reference/` a été vérifiée en ligne (WebFetch) : textes identiques pour `initializeCommand`, l'échec des lifecycle, `updateRemoteUserUID` et `devcontainerId`. Le site affiche en plus `hostRequirements.gpu`, qui dans le repo ne figure que dans `gpu-host-requirement.md` et le schéma.
  - `devcontainers/cli` @ `5dc7533314b5ba7ec3875c30143dfe1aec644870` (branche main, commit du 2026-08-28).
- **Conventions de citation** :
  - `spec:<fichier>:<ligne>` désigne un fichier du repo spec (`docs/specs/` omis pour les .md).
  - `schema:<ligne>` désigne `schemas/devContainer.base.schema.json`.
  - `cli:<fichier>:<ligne>` désigne un fichier du repo cli (`src/` omis).
  - Niveaux : **MUST**, **SHOULD**, **MAY**, **DÉFAUT**, **INFO** (descriptif, sans mot-clé normatif). La spec emploie rarement des mots-clés RFC 2119 en majuscules ; quand le niveau est déduit d'une formulation (« should », « must », « is required »), il est noté entre parenthèses.
  - « non vérifié » : je ne l'ai pas lu moi-même dans le code ou le texte.

---

## 1. Variables et substitution

| Exigence | Niveau | Source | Comportement CLI réf. | fichier:ligne |
|---|---|---|---|---|
| Syntaxe `${variableName}`, utilisable dans « certain string values » | INFO | spec:devcontainerjson-reference.md:151 | Regex `/\$\{(.*?)\}/g`. Les arguments sont séparés par `:`, donc une valeur par défaut ne peut pas contenir `:` : seul `args[1]` est retenu. | cli:spec-common/variableSubstitution.ts:63,81-92 |
| `${localEnv:VAR}` = variable d'environnement de la **machine hôte**. Une variable absente donne une chaîne vide. Défaut possible via `${localEnv:VAR:default}`. | DÉFAUT | spec:devcontainerjson-reference.md:155 | Lit `cliHost.env`, c'est-à-dire `process.env` du CLI. L'alias `${env:VAR}` est accepté. Sous Windows, recherche insensible à la casse (clés passées en minuscules). Absente sans défaut : `''`. Sans nom de variable : erreur. | cli:spec-common/variableSubstitution.ts:96-98,65-74,137-159 |
| `${containerEnv:VAR[:default]}` : valeur dans le conteneur démarré, **seulement dans `remoteEnv`** selon la spec | (MUST implicite) | spec:devcontainerjson-reference.md:156 | Substitué après le démarrage à partir de `inspect.Config.Env` du conteneur, et sur **toute** la config (config et mergedConfig), pas seulement `remoteEnv`. | cli:spec-common/injectHeadless.ts:345-346 ; cli:spec-common/variableSubstitution.ts:41-44,117-125 |
| `${localWorkspaceFolder}` / `${localWorkspaceFolderBasename}` : dossier local ouvert | INFO | spec:devcontainerjson-reference.md:157,159 | Valeur = `workspace.rootFolderPath` = `path.resolve(--workspace-folder)`, c'est-à-dire le dossier ouvert et **pas** la racine git. Basename calculé avec `path.win32` si la plateforme est win32. Non défini : le texte `${...}` est laissé tel quel. | cli:spec-node/configContainer.ts:94-100 ; cli:spec-common/variableSubstitution.ts:100-104 ; cli:spec-node/devContainersSpecCLI.ts:271 |
| `${containerWorkspaceFolder}` / `...Basename` | INFO | spec:devcontainerjson-reference.md:158,160 | Valeur = `workspaceFolder` **calculé** : défaut `/workspaces/<basename racine git>/<chemin relatif>`, voir §7. Elle est elle-même substituée d'abord. Basename en `path.posix`. | cli:spec-node/configContainer.ts:93-97 ; cli:spec-common/variableSubstitution.ts:30-32,106-110 |
| `${devcontainerId}` : identifiant unique parmi les dev containers **du même hôte Docker** et **stable entre rebuilds**, alphanumérique uniquement | MUST | spec:devcontainer-features.md:117 ; spec:devcontainer-id-variable.md:19 | Voir l'algorithme ci-dessous. | cli:spec-common/variableSubstitution.ts:36-39,161-171 |
| Propriétés qui supportent `${devcontainerId}` (devcontainer.json) : `name`, `runArgs`, `initializeCommand`, les 5 lifecycle, `workspaceFolder`, `workspaceMount`, `mounts`, `containerEnv`, `remoteEnv`, `containerUser`, `remoteUser`, `customizations`. Côté feature : `entrypoint`, `mounts`, `customizations`. Ne doit pas servir au build d'image. | (SHOULD) | spec:devcontainerjson-reference.md:161 ; spec:devcontainer-features.md:115 | Le CLI substitue sur **toute** la config sans restreindre aux propriétés listées. | cli:spec-node/configContainer.ts:58 |
| Substitution des métadonnées d'image : « Variables in string values will be substituted at the time the value is applied » | INFO | spec:devcontainer-reference.md:87 | Les métadonnées `raw` sont conservées non substituées (label) ; `config` est substitué avec le même contexte que devcontainer.json. | cli:spec-node/imageMetadata.ts:468-475,291-316 |

**Algorithme `devcontainerId`, identique dans la spec et le CLI** (spec:devcontainer-features.md:142-164 ; cli:spec-common/variableSubstitution.ts:161-171) :

```js
JSON.stringify(idLabels, Object.keys(idLabels).sort())  // clés triées, sans espaces
-> sha256(utf-8) -> BigInt(hex).toString(32).padStart(52,'0')
```

`idLabels` vaut, dans l'ordre de priorité :

1. les `--id-label` fournis ;
2. sinon `{devcontainer.local_folder: <dossier>, devcontainer.config_file: <chemin config>}` (nouveaux labels) ;
3. sinon, pour un conteneur existant sans `config_file`, `{devcontainer.local_folder}` seul (anciens labels).

Références : cli:spec-node/utils.ts:721-766.

Conséquences sur la stabilité :

- L'id change si le chemin du dossier ou celui de la config change.
- Sous Windows (depuis ce commit), les chemins sont normalisés (`path.win32.normalize` + lettre de lecteur en minuscule) **avant** le hash (cli:spec-node/utils.ts:656-670,729-732). Un même dossier ouvert via `C:\x` ou `c:\x` produit donc le même id. En revanche, un conteneur créé par une version plus ancienne du CLI avec `C:` en majuscule donne un id différent. Il est seulement retrouvé par le fallback (cli:spec-node/utils.ts:672-698).

**Ordre de substitution (CLI)** :

| Étape | Variables substituées | Machine | Référence |
|---|---|---|---|
| 1. Lecture de la config | `localEnv`, `localWorkspaceFolder*`, `containerWorkspaceFolder*` | Hôte du CLI | cli:spec-node/configContainer.ts:94-101 |
| 2. Après calcul des `idLabels` | `devcontainerId` | Hôte du CLI | cli:spec-node/configContainer.ts:57-58 |
| 3. `initializeCommand` | Utilise la config déjà substituée aux étapes 1-2 | Hôte du CLI | cli:spec-node/configContainer.ts:66 |
| 4. Après démarrage, sur config et mergedConfig | `containerEnv` | Valeurs lues via `docker inspect` | cli:spec-common/injectHeadless.ts:345-346 |

---

## 2. Lifecycle

| Exigence | Niveau | Source | Comportement CLI réf. | fichier:ligne |
|---|---|---|---|---|
| `initializeCommand` s'exécute sur la **machine hôte**, « during container creation and on subsequent starts ». Peut s'exécuter plusieurs fois par session. Pour un service cloud, l'hôte est dans le cloud. | INFO | spec:devcontainerjson-reference.md:71 ; spec:devcontainer-reference.md:197-201 | Exécuté à **chaque** `devcontainer up`, avant toute recherche ou création de conteneur, même si le conteneur existe et tourne déjà. Chaîne : `/bin/sh -c`, ou `%ComSpec% /c` sous Windows. Tableau : exécution directe (sous Windows, `/` remplacé par `\` dans argv[0]). Objet : commandes en parallèle (`Promise.all`). cwd = cwd du CLI (dossier workspace). env = `dockerEnv` (env du CLI). Un échec lève une erreur fatale, sauf SIGINT (code 130), qui est seulement journalisé. | cli:spec-node/configContainer.ts:66 ; cli:spec-node/utils.ts:538-622 |
| `onCreateCommand`, `updateContentCommand`, `postCreateCommand` : exécutés **dans** le conteneur principal, en séquence, à la première création | INFO | spec:devcontainer-reference.md:229-231 ; spec:devcontainerjson-reference.md:72-74 | Chaque hook est protégé par un marqueur `<userDataFolder>/.<hook>Marker` contenant `containerInfo.Created`. Le hook tourne si le marqueur diffère, puis le marqueur est réécrit. `userDataFolder` = `$HOME/.devcontainer` par défaut (`--container-data-folder`). `updateContentCommand` est relancé de force avec `--prebuild`. | cli:spec-common/injectHeadless.ts:431-435,443-450,333-335 |
| `postStartCommand` : « each time the container is successfully started » | INFO | spec:devcontainerjson-reference.md:75 ; spec:devcontainer-reference.md:255-259 | Marqueur `.postStartCommandMarker` contenant `State.StartedAt`. Relancé uniquement si le conteneur a redémarré. | cli:spec-common/injectHeadless.ts:437-441 |
| `postAttachCommand` : « each time a tool has successfully attached » | INFO | spec:devcontainerjson-reference.md:76 | Relancé à **chaque** `up`, sans marqueur (`doRun=true`), sauf avec `--skip-post-attach`. | cli:spec-common/injectHeadless.ts:409-411,452-454 |
| Formes : chaîne → `/bin/sh` ; tableau → sans shell ; objet → commandes nommées en parallèle, chacune devant réussir | MUST (objet : « Each command must exit successfully ») | spec:devcontainerjson-reference.md:79,109-112 ; spec:devcontainer-reference.md:263-269 | Chaîne → `['/bin/sh','-c',cmd]` ; tableau → argv direct ; objet → `Promise.allSettled`, qui attend toutes les commandes puis relance la première erreur. Exécution via `docker exec` avec PTY, en `remoteUser`. cwd = `remoteWorkspaceFolder` ou, à défaut, `$HOME`. env = `remoteEnv` probé + secrets. | cli:spec-common/injectHeadless.ts:505-561 |
| Échec : « If one of the lifecycle scripts fails, any subsequent scripts will not be executed » | (MUST) | spec:devcontainerjson-reference.md:81 ; spec:features-contribute-lifecycle-scripts.md:116-117 | Lève une `ContainerError` qui interrompt la chaîne et fait échouer `up`. SIGINT est seulement journalisé et la suite continue. | cli:spec-common/injectHeadless.ts:526-540 |
| `waitFor` : bloquer jusqu'au hook indiqué. **Défaut `updateContentCommand`**. Enum : `initializeCommand`, `onCreateCommand`, `updateContentCommand`, `postCreateCommand`, `postStartCommand`. | DÉFAUT | spec:devcontainer-reference.md:231 ; spec:devcontainerjson-reference.md:77 ; schema:412-421 | `defaultWaitFor = 'updateContentCommand'`. Le CLI **n'exécute pas en arrière-plan** : `waitFor` ne sert qu'avec `--skip-non-blocking-commands`, qui s'arrête après le hook visé. Sans ce flag, `up` exécute tout de façon séquentielle et bloquante, `postAttach` compris. | cli:spec-common/injectHeadless.ts:134,368-413 ; cli:spec-node/devContainersSpecCLI.ts:156 |
| « By default, `postCreateCommand` is executed in the background after reporting the successful creation » | (SHOULD) | spec:devcontainer-reference.md:230 | Non respecté par le CLI, qui bloque. C'est à l'outil client d'implémenter l'arrière-plan. | cli:spec-common/injectHeadless.ts:391 |
| Lifecycle des features : pour chaque hook, commandes des features dans l'ordre d'installation, **toujours avant** celle de l'utilisateur. Chaque entrée est bloquante ; la forme objet reste parallèle à l'intérieur de l'entrée. Cwd = project workspace folder. Pas d'`initializeCommand` pour les features. | (MUST : « shall ») | spec:devcontainer-features.md:67-73 ; spec:features-contribute-lifecycle-scripts.md:23-31 | Ordre = ordre du tableau `devcontainer.metadata` : métadonnées de l'image de base, puis features dans l'ordre d'installation, puis devcontainer.json. `origin` = id de feature ou `devcontainer.json`. | cli:spec-node/imageMetadata.ts:124-144,291-316 ; cli:spec-common/injectHeadless.ts:457-467 |
| Conteneur existant : quelles commandes ? | — (non spécifié) | — | Si le conteneur porte les id labels, les hooks proviennent du **label `devcontainer.metadata` du conteneur**, figé à la création. Seuls `remoteUser`, `userEnvProbe` et `remoteEnv` sont relus depuis le devcontainer.json courant. | cli:spec-node/imageMetadata.ts:44-48,441-462 |
| Dotfiles : installés après `postCreate`, avant `postStart` | INFO (spécifique CLI) | — | `--dotfiles-*` | cli:spec-common/injectHeadless.ts:396-398 |

**Où s'exécute quoi** :

- `initializeCommand` : sur la machine du CLI/client. Avec un démon distant, ce n'est **pas** la machine du démon.
- Tous les autres hooks : dans le conteneur, via `docker exec`, en `remoteUser`.
- Marqueurs : dans le FS du conteneur (`~/.devcontainer/`). Ils disparaissent avec le conteneur, ce qui relance les hooks create après un rebuild.
- `/var/devcontainer/.patchEtcEnvironmentMarker` et `.patchEtcProfileMarker` : patch root de `/etc/environment` et `/etc/profile`, effectué une seule fois (cli:spec-common/injectHeadless.ts:754-775).

---

## 3. Métadonnées d'image `devcontainer.metadata`

| Exigence | Niveau | Source | Comportement CLI réf. | fichier:ligne |
|---|---|---|---|---|
| Label `devcontainer.metadata` = chaîne JSON contenant un tableau (une entrée par feature, puis devcontainer.json) ou un objet unique | INFO | spec:devcontainer-reference.md:25-52 | Parse : un tableau est accepté tel quel, un objet unique devient `[obj]`, et une valeur invalide est ignorée avec un log. | cli:spec-node/imageMetadata.ts:477-494 |
| À fusionner avec devcontainer.json à la création ; devcontainer.json est considéré « last » | (SHOULD) | spec:devcontainer-reference.md:27,87 | Tableau = `[...baseImage, ...features (ordre d'install), pick(devcontainer.json)]` | cli:spec-node/imageMetadata.ts:291-316 |
| Propriétés portées par le label | INFO | spec:devcontainer-reference.md:58-85 | devcontainer.json : `pickConfigProperties`. Features : `pickFeatureProperties`. | cli:spec-node/imageMetadata.ts:17-67 |
| Écriture du label | — | — | Via `LABEL` dans le Dockerfile étendu. Pour une config `image` sans feature, via `-l` sur `docker run` sans build. En Compose, via le fichier override. | cli:spec-node/imageMetadata.ts:496-510 ; cli:spec-node/containerFeatures.ts:152-160 ; cli:spec-node/singleContainer.ts:418 |

**Fusion par propriété, spec (spec:devcontainer-reference.md:58-85) comparée au CLI `mergeConfiguration` (cli:spec-node/imageMetadata.ts:157-200)** :

| Propriété | Spec | CLI (ligne) |
|---|---|---|
| `id` | non fusionné | sert uniquement d'origine des hooks (124-144) |
| `init`, `privileged` | true si au moins un est true | `some()` (173-174) |
| `capAdd`, `securityOpt` | union sans doublons | `Set` (175-176,280-283) |
| `entrypoint` | liste collectée | `entrypoints[]` (177), injectés dans le script d'entrypoint (§9) |
| `mounts` | liste collectée ; « Conflicts: Last source wins » | dédoublonnage par **`target`** : le dernier gagne, l'ordre est conservé (264-278) |
| 5 hooks lifecycle | liste collectée | `xxxCommands[]` (180-184) |
| `waitFor`, `containerUser`, `remoteUser`, `userEnvProbe`, `shutdownAction`, `otherPortsAttributes` | la dernière valeur gagne | `reversed.find(truthy)` (185-195) |
| `overrideCommand`, `updateRemoteUserUID` | la dernière valeur gagne | dernier **booléen** trouvé (191,196) |
| `remoteEnv`, `containerEnv` | par variable, la dernière gagne | `Object.assign` (189-190) |
| `portsAttributes` | par port (pas par attribut), le dernier gagne | `Object.assign` (192) |
| `forwardPorts` | union, le dernier gagne si le mapping change | normalise `n` en `localhost:n`, applique un `Set`, puis reconvertit en nombre (202-210) |
| `hostRequirements` | la valeur max gagne | max de cpus, memory, storage (octets, suffixes kb/mb/gb/tb) ; gpu fusionné : false/undefined < 'optional' < true/objet, avec max de cores et memory (212-262) |
| `customizations` | fusion laissée aux outils | `{ [outil]: [valeur1, valeur2, ...] }`, un tableau par namespace (158-167) |

---

## 4. Labels d'identification et `--id-label`

| Exigence | Niveau | Source | Comportement CLI réf. | fichier:ligne |
|---|---|---|---|---|
| Identification par labels : exemple `devcontainer.local_folder` = chemin du dossier local | MAY (« Implementations may choose ») | spec:devcontainer-features.md:119-123 | Constantes `devcontainer.local_folder`, `devcontainer.config_file`. | cli:spec-node/singleContainer.ts:17-18 |
| Valeurs exactes | — | — | `local_folder` = `path.resolve(cwd, --workspace-folder)`, c'est-à-dire le dossier ouvert (pas la racine git). `config_file` = `configPath.fsPath`, chemin absolu du devcontainer.json (ou `--config`). Sous **Windows** (`process.platform==='win32'`) : `path.win32.normalize` + lettre de lecteur en minuscule. Ailleurs : valeur brute. **Aucune** conversion WSL ou `\\wsl$`. | cli:spec-node/devContainersSpecCLI.ts:271 ; cli:spec-node/configContainer.ts:43-57 ; cli:spec-node/utils.ts:656-670,729-732 |
| Recherche du conteneur | — | — | `docker ps -a --filter label=k=v` sur les nouveaux labels. Sous Windows, repli sur une comparaison normalisée. Repli ensuite sur l'ancien label seul, en ignorant les conteneurs qui ont déjà `config_file`. Avec `--remove-existing-container`, les conteneurs à anciens labels sont supprimés. Les conteneurs en état `removing` sont exclus. | cli:spec-node/utils.ts:721-766 ; cli:spec-node/singleContainer.ts:321-325 ; cli:spec-shutdown/dockerUtils.ts:168 |
| `--id-label name=value` (répétable) | — | — | Format `.+=.+` validé. S'il est fourni, **remplace** entièrement les labels dérivés : `local_folder` et `config_file` ne sont plus posés. Il sert à la recherche, à l'étiquetage et au calcul de `devcontainerId`. Si `--id-label` et `--override-config` sont absents, `--workspace-folder` vaut par défaut `cwd`. | cli:spec-node/devContainersSpecCLI.ts:143,181-189,275 ; cli:spec-node/utils.ts:722-727 |
| Docker Compose | — | — | Le conteneur existant est retrouvé par `com.docker.compose.project` + `com.docker.compose.service`, et **non** par les id labels. Les id labels sont quand même ajoutés via l'override. | cli:spec-node/dockerCompose.ts:24-25,47-48,411,630-636 |

---

## 5. `updateRemoteUserUID`

| Exigence | Niveau | Source | Comportement CLI réf. | fichier:ligne |
|---|---|---|---|---|
| « optional task for **Linux (only)** ». S'exécute si `updateRemoteUserUID` vaut true et que `containerUser`/`remoteUser` est défini. L'image est modifiée **avant** la création pour aligner UID/GID sur « the current local user ». **MAY** être sauté sans bind mounts ou si le moteur traduit déjà les UID. | MAY / DÉFAUT `true` | spec:devcontainer-reference.md:219,225 ; spec:devcontainerjson-reference.md:19 ; schema:221-224 (« On by default when opening from a local folder ») | Conditions : `--update-remote-user-uid-default` ≠ `never` (défaut `on`) ; valeur config si booléenne, sinon défaut ; `cliHost.platform === 'linux'`, macOS jamais en CLI (`updateRemoteUserUIDOnMacOS: false`). Utilisateur ciblé = `remoteUser`, sinon `-u` des runArgs, sinon `containerUser`, sinon USER de l'image. Sauté si `root` ou numérique. | cli:spec-node/containerFeatures.ts:421-443 ; cli:spec-node/singleContainer.ts:40-41 ; cli:spec-node/devContainers.ts:248 ; cli:spec-node/devContainersSpecCLI.ts:151 |
| Source de l'UID | — | « local user » (non défini plus précisément) | `process.getuid()`/`getgid()` du **processus CLI** (hôte local). Avec un démon distant, c'est donc l'UID de la machine client. | cli:spec-common/cliHost.ts:85-86 ; cli:spec-node/containerFeatures.ts:472-473 |
| Mécanisme | — | — | `docker build` de `scripts/updateUID.Dockerfile` : sed sur `/etc/passwd` et `/etc/group`, `chown -R $HOME`. Aucune action si l'UID est déjà pris par un autre utilisateur. Si le GID est pris, l'ancien GID est conservé. Image produite `vsc-<basename>-<sha256(cwd)>-uid`. Podman : préfixe `localhost/` pour une image locale. Non appliqué au rebuild d'un conteneur Compose existant (`noBuild`). | cli:scripts/updateUID.Dockerfile:12-32 ; cli:spec-node/containerFeatures.ts:445-483 ; cli:spec-node/utils.ts:624-633 ; cli:spec-node/dockerCompose.ts:406 |
| Podman rootless | — | — | Sur Linux avec Podman : `--security-opt label=disable`, plus `--userns=keep-id` si l'utilisateur distant n'est pas root et qu'aucun `--uidmap`/`--gidmap` n'est présent dans runArgs. | cli:spec-node/singleContainer.ts:438-451 |

---

## 6. `userEnvProbe`, `remoteEnv` et `containerEnv`

| Exigence | Niveau | Source | Comportement CLI réf. | fichier:ligne |
|---|---|---|---|---|
| `userEnvProbe` ∈ {`none`, `interactiveShell`, `loginShell`, `loginInteractiveShell`} ; **défaut `loginInteractiveShell`** ; shell = shell par défaut de l'utilisateur | DÉFAUT | spec:devcontainerjson-reference.md:20 ; schema:423-432 | Défaut `--default-user-env-probe=loginInteractiveShell`. Shell = `$SHELL` du conteneur, sinon shell de `getent passwd`, sinon `/bin/sh`. Arguments `-lic` / `-lc` / `-ic` ; pwsh reçoit `-Login -Command`. Commande `cat /proc/self/environ`, avec repli sur `printenv`. Avertissement après 2 s, abandon après 10 s. Résultat mis en cache si `--container-session-data-folder` est fourni. | cli:spec-node/devContainersSpecCLI.ts:51,150 ; cli:spec-common/injectHeadless.ts:300-302,777-800,842-935 |
| Les variables probées sont fusionnées avec les variables Remote pour tous les processus injectés, sans imposer ce shell à chaque sous-processus | (SHOULD) | spec:devcontainer-reference.md:146,233,239 | Priorité croissante : `shellEnv` < `--remote-env` < `config.remoteEnv` (fusionné). Les secrets sont ajoutés par-dessus pour les hooks. | cli:spec-common/injectHeadless.ts:359-366,518 |
| `containerEnv` : posé sur le conteneur, statique (un rebuild est nécessaire pour le changer). `remoteEnv` : propre à l'outil et à ses sous-processus, appliqué après l'ENTRYPOINT, modifiable sans rebuild. | INFO | spec:devcontainerjson-reference.md:15-16 ; spec:devcontainer-reference.md:139-142 | `containerEnv` fusionné → `-e K=V` sur `docker run`, ou `environment:` dans l'override Compose. **Aussi** écrit en `ENV` dans l'image étendue quand il y a des features (le `$` y est échappé). `remoteEnv` → env de chaque `docker exec`. | cli:spec-node/singleContainer.ts:353-358 ; cli:spec-node/dockerCompose.ts:558-560 ; cli:spec-node/containerFeatures.ts:269 |
| Features : `containerEnv` ajouté en `ENV` avant l'exécution de la feature | INFO | spec:devcontainer-features.md:551 | `generateContainerEnvs` avant chaque `RUN` de feature (v2). | cli:spec-configuration/containerFeaturesConfiguration.ts:336,366-376 |

---

## 7. `workspaceMount`, `workspaceFolder`, `mountWorkspaceGitRoot`, consistency et `mounts`

| Exigence | Niveau | Source | Comportement CLI réf. | fichier:ligne |
|---|---|---|---|---|
| Un montage par défaut **should** rendre le code accessible ; il pointe de préférence vers la racine du repo (`.git`) ; bind non obligatoire | (SHOULD) | spec:devcontainer-reference.md:152-160 | — | — |
| Point de montage par défaut | DÉFAUT **incohérent** | spec:devcontainer-reference.md:156 (« defaults to `/workspace` ») ; schema:538-541 (« `/workspaces/$project` ») | `/workspaces/<basename(hostMountFolder)>` | cli:spec-node/utils.ts:429 |
| `workspaceMount` exige `workspaceFolder`, et réciproquement ; syntaxe `--mount` | (MUST : « Requires ») | spec:devcontainerjson-reference.md:48-49 | Pas d'erreur. Chaque propriété absente est calculée indépendamment : `'workspaceMount' in config` suffit (une chaîne vide supprime le montage). | cli:spec-node/utils.ts:423-469 ; cli:spec-node/singleContainer.ts:350 |
| Montage calculé | — | — | `type=bind,source=<hostMountFolder>,target=<containerMountFolder>[,consistency=X]`. `consistency` est omis sous Linux, car Podman ne le tolère pas. Ailleurs : `--workspace-mount-consistency`, dont le défaut CLI est `cached`, avec repli `consistent`. Si le chemin contient une virgule, `source=`/`target=` sont mis entre guillemets. `source` = chemin hôte **brut vu par le CLI**, par exemple `C:\src\x` sous Windows. | cli:spec-node/utils.ts:463-468 ; cli:spec-node/devContainersSpecCLI.ts:139 |
| `workspaceFolder` par défaut | DÉFAUT | spec:devcontainerjson-reference.md:49 (« automatic source code mount location ») | `posix.join(containerMountFolder, relative(gitRoot, dossier ouvert))`, séparateurs `\` convertis en `/`. | cli:spec-node/utils.ts:459-462 |
| `mountWorkspaceGitRoot` | — (**absent de la spec**, flag CLI) | — | `--mount-workspace-git-root`, défaut **true** : monte la racine git trouvée au-dessus du dossier. `--mount-git-worktree-common-dir` (défaut false) monte en plus le common-dir des worktrees à `gitdir` relatif. | cli:spec-node/devContainersSpecCLI.ts:141-142 ; cli:spec-node/utils.ts:405-407,430-457 |
| Compose : `workspaceFolder` **requis** (schéma) ou défaut `"/"` (doc) | Divergence | schema:699-703 vs spec:devcontainerjson-reference.md:59 | `config.workspaceFolder || '/'`. Aucun montage n'est ajouté : c'est au fichier Compose de monter le code. | cli:spec-node/dockerCompose.ts:116-118 ; cli:spec-node/utils.ts:416-422 |
| `mounts` : chaîne au format `--mount`, ou objet `{type: bind\|volume, source?, target}` ; `additionalProperties: false` | INFO | spec:devcontainerjson-reference.md:27 ; schema:236-248,705-728 | Chaîne → `--mount <chaîne>` inchangée. Objet → `--mount type=T[,src=S],dst=D` ; les autres attributs sont perdus. En Compose : syntaxe courte `source:target` (type et options perdus) ; les volumes nommés sont déclarés en top-level `volumes:`. `--mount` CLI : regex `type=(bind\|volume),source=..,target=..[,external=..]`. Avec **wslc** : conversion en `-v src:dst`. | cli:spec-node/dockerfileUtils.ts:280-294 ; cli:spec-configuration/containerFeaturesConfiguration.ts:116-120 ; cli:spec-node/dockerCompose.ts:522-526,738-748 ; cli:spec-node/devContainersSpecCLI.ts:53 ; cli:spec-node/singleContainer.ts:379-387,453-477 |

**Où s'exécute quoi** : les chemins `source` des bind mounts sont interprétés **par le démon**, alors que le CLI les calcule avec le système de fichiers et la plateforme de la machine client (voir §13).

---

## 8. Ports, `shutdownAction`, `hostRequirements`, `overrideCommand`, options `docker run`

| Exigence | Niveau | Source | Comportement CLI réf. | fichier:ligne |
|---|---|---|---|---|
| `forwardPorts` : nombres ou `"host:port"`, du conteneur principal vers la machine locale ; défaut `[]` | DÉFAUT | spec:devcontainerjson-reference.md:12 | **Non implémenté** par le CLI : seulement fusionné et renvoyé dans le résultat (`--include-merged-configuration`). Le transfert relève du client. | cli:spec-node/imageMetadata.ts:194,202-210 |
| `portsAttributes` / `otherPortsAttributes` (`label`, `protocol`, `onAutoForward` défaut `notify`, `requireLocalPort` et `elevateIfNeeded` défaut false) | DÉFAUT | spec:devcontainerjson-reference.md:13-14,93-103 | Fusion uniquement (192-193) ; aucun effet côté CLI. | cli:spec-node/imageMetadata.ts:192-193 |
| `appPort` : ports **publiés** (`-p`), image/Dockerfile uniquement ; l'application doit écouter sur `0.0.0.0` | DÉFAUT `[]` | spec:devcontainerjson-reference.md:47,170 ; schema:501-515 | Nombre `n` → `-p 127.0.0.1:n:n` ; chaîne → `-p <chaîne>` inchangée. Lu depuis `config`, non fusionné. | cli:spec-node/singleContainer.ts:346-348 |
| `shutdownAction` : `none` \| `stopContainer` (défaut image/Dockerfile) \| `stopCompose` (défaut Compose) | DÉFAUT | spec:devcontainerjson-reference.md:22 ; schema:522-528,686-692 | Fusionné seulement. Le CLI n'arrête jamais rien (il n'a pas de fenêtre). Responsabilité du client. | cli:spec-node/imageMetadata.ts:195 |
| `hostRequirements` : `cpus`, `memory`, `storage` ; `gpu` : bool \| `'optional'` \| `{cores, memory}` (défaut false). Cloud : sélection de machine ; sinon avertissement. | INFO | spec:devcontainerjson-reference.md:83-91 ; spec:gpu-host-requirement.md:11-29 ; schema:433-470 | Seul **`gpu`** a un effet : `--gpus all` si le support est détecté (`--gpu-availability`, défaut `detect`) ; en Compose, `deploy.resources.reservations.devices`. Avertissement si requis sans être `optional`. cpus, memory et storage ne sont ni vérifiés ni signalés (non vérifié au-delà du grep). | cli:spec-node/singleContainer.ts:327-340 ; cli:spec-node/dockerCompose.ts:536-546 |
| `overrideCommand` : défaut **true** pour image/Dockerfile, **false** pour Compose ; true exécute `while sleep 1000; do :; done` | DÉFAUT | spec:devcontainerjson-reference.md:21 ; schema:530-533,694-697 | **Toujours** `--entrypoint /bin/sh -c "echo Container started; trap 'exit 0' 15; <entrypoints features>; exec \"$@\"; while sleep 1 & wait $!; do :; done" -`. Si `overrideCommand === false` (valeur explicite), l'Entrypoint et le Cmd de l'image sont ajoutés comme `$@`. Compose : si `overrideCommand` est falsy, l'entrypoint/command du service ou de l'image est conservé. | cli:spec-node/singleContainer.ts:389-401 ; cli:spec-node/dockerCompose.ts:527-556 |
| `init`, `privileged` (défaut false), `capAdd`, `securityOpt` (défaut `[]`) | DÉFAUT | spec:devcontainerjson-reference.md:23-26 | `--init`, `--privileged`, `--cap-add`, `--security-opt` issus du merge. **Omis avec wslc.** Compose : `init: true`, `privileged`, `cap_add`, `security_opt`. | cli:spec-node/singleContainer.ts:362-377 ; cli:spec-node/dockerCompose.ts:556-565 |
| `runArgs` (image/Dockerfile) | DÉFAUT `[]` | spec:devcontainerjson-reference.md:50 | Ordre des arguments de `docker run` : `--sig-proxy=false -a STDOUT -a STDERR`, `-p`, montage workspace, montages additionnels, `mounts`, labels, `-e`, `-u containerUser`, args podman, **runArgs**, `--gpus`, init/privileged/cap/secopt, `--entrypoint`, labels extra, image, cmd. Un `-u` dans runArgs est pris en compte pour le calcul de l'UID. | cli:spec-node/singleContainer.ts:403-421,275-286 |
| `containerUser` : défaut root ou dernier `USER` ; `remoteUser` : défaut = utilisateur du conteneur | DÉFAUT | spec:devcontainerjson-reference.md:17-18 ; spec:devcontainer-reference.md:166-173 | `-u <containerUser fusionné>`. `remoteUser` = `mergedConfig.remoteUser`, sinon `Config.User` du conteneur, sinon `root`. | cli:spec-node/singleContainer.ts:360 ; cli:spec-node/utils.ts:506 |

---

## 9. Docker Compose

| Exigence | Niveau | Source | Comportement CLI réf. | fichier:ligne |
|---|---|---|---|---|
| Requis : `dockerComposeFile` (chaîne ou tableau, relatif au devcontainer.json, l'ordre compte), `service` ; `workspaceFolder` requis dans le **schéma** | MUST (« Required ») | spec:devcontainerjson-reference.md:56-59 ; spec:devcontainer-reference.md:121-129 ; schema:699-703 | Tableau → `-f` dans l'ordre. Tableau **vide** → `COMPOSE_FILE` (env ou `.env` du cwd), séparateur = `path.delimiter`, et `--env-file <cwd>/.env`. Service absent de `docker compose config` → erreur. Le mode Compose exige un workspace. | cli:spec-configuration/configuration.ts:233-258 ; cli:spec-node/dockerCompose.ts:43-46,352-355 ; cli:spec-node/configContainer.ts:71-75 |
| `runServices` : défaut **tous** les services ; arrêtés à la déconnexion sauf `shutdownAction: none` | DÉFAUT | spec:devcontainerjson-reference.md:58 | `up -d [runServices..., + service s'il manque]`. Sans runServices : tous les services. `--no-recreate` si le conteneur existe. `build` limité de la même façon. | cli:spec-node/dockerCompose.ts:420-430,276-281 |
| Nom de projet | — (non spécifié) | — | Ordre : `COMPOSE_PROJECT_NAME` (env du CLI) → `COMPOSE_PROJECT_NAME=` dans `<cwd>/.env` → `name:` du compose effectif (`devcontainer` n'est retenu que s'il est écrit explicitement dans un fragment) → si le dossier du 1er fichier compose est `<workspace>/.devcontainer`, alors `<basename(workspace)>_devcontainer` → sinon basename du dossier du 1er fichier. Normalisation : minuscules, seuls `[-_a-z0-9]` sont conservés (Compose ≥ 1.21), sinon `[a-z0-9]`. | cli:spec-node/dockerCompose.ts:638-702 |
| Overrides générés | — | — | (1) Build : `docker-compose.devcontainer.build-<ts>.yml` (Dockerfile-with-features, target, context vide, args `BUILDKIT_INLINE_CACHE`, `additional_contexts` si Compose ≥ 2.17 et Docker). (2) Démarrage : `docker-compose.devcontainer.containerFeatures-<ts>-<uuid>.yml` (image `-uid`, entrypoint `/bin/sh -c` enveloppant, `command`, `init`, `user`, `environment`, `privileged`, `cap_add`, `security_opt`, labels = id labels + `devcontainer.metadata`, `volumes`, gpu). Écrits dans `<persistedFolder>/docker-compose/`. `persistedFolder` = `--user-data-folder`, sinon tmp `devcontainercli[-user]`. Au redémarrage, les overrides sont relus via le label `com.docker.compose.project.config_files`. `version:` du 1er fichier recopié en préfixe. `$` échappé en `$$`. | cli:spec-node/dockerCompose.ts:152-305,335-452,469-573 ; cli:spec-node/devContainers.ts:151 ; cli:spec-node/utils.ts:644-646 |
| Nom d'image par défaut d'un service buildé | — | — | `<projet>-<service>` (Compose ≥ 2.8) ou `<projet>_<service>` | cli:spec-node/dockerCompose.ts:463-467 |
| Détection de Compose v1/v2 | — | — | `docker compose version --short`, repli sur `docker-compose version --short` (`--docker-compose-path`). Pour lire la config en v2 : `config --profile '*'`. | cli:spec-node/dockerCompose.ts:704-731,581-584 |

---

## 10. Features

| Exigence | Niveau | Source | Comportement CLI réf. | fichier:ligne |
|---|---|---|---|---|
| `devcontainer-feature.json` : `id`, `version`, `name` requis ; `id` = nom du dossier, en minuscules | MUST | spec:devcontainer-features.md:28-34 | — | — |
| Référence : OCI `registry/ns/feature[:semver]` (`:latest` implicite), URI HTTPS de tgz, ou chemin relatif `./` ; ids insensibles à la casse, **normalisés en minuscules** | (SHOULD) | spec:devcontainer-features.md:193,212-226 | Est local si l'id commence par `./`, `../` ou `/`. Les chemins absolus sont refusés. Le chemin local doit rester **sous `<workspace>/.devcontainer/`**, sinon refus. | cli:spec-configuration/containerFeaturesConfiguration.ts:724-725,834-880 |
| Local : `.devcontainer/` à la racine du project workspace folder, sous-dossier contenant `devcontainer-feature.json` et `install.sh` | MUST | spec:devcontainer-features-distribution.md:169-185 | idem ci-dessus | idem |
| Options : `type` ∈ {`boolean`, `string`} uniquement ; `proposals`/`enum` ; `default` string ou bool. Valeur utilisateur string ou boolean. Une chaîne seule équivaut à `{version: "..."}`. | MUST (schéma : `const`) | spec:devcontainer-features.md:94-101,195-210 ; schemas/devContainerFeature.schema.json:223-249 | Pas de validation stricte du type (non vérifié en détail). Les valeurs sont écrites telles quelles, `"${values[name]}"`. Tableaux et nombres ne sont pas prévus : ils seraient sérialisés par `String()` (non testé). | cli:spec-node/containerFeatures.ts:393-419 ; cli:spec-configuration/containerFeaturesConfiguration.ts:1221-1270 |
| Résolution des options → `devcontainer-features.env`, lignes `NAME=value`. `NAME` = `id.replace(/[^\w_]/g,'_').replace(/^[\d_]+/g,'_').toUpperCase()`. Les options omises exportent leur défaut. | MUST (« will ») | spec:devcontainer-features.md:421-438 | `getSafeId` est identique. Fichier sourcé avec `set -a` puis `./install.sh` sous `set -e`. `_CONTAINER_USER`, `_REMOTE_USER`, `_CONTAINER_USER_HOME` et `_REMOTE_USER_HOME` viennent de `devcontainer-features.builtin.env`. | cli:spec-node/containerFeatures.ts:26-29,254-259 ; cli:spec-configuration/containerFeaturesConfiguration.ts:280-304 |
| `install.sh` exécuté en root pendant le build d'image ; bit exécutable ; `/bin/sh` par défaut | (SHOULD) | spec:devcontainer-features.md:240,252-254 | Un `RUN` (couche) par feature, `USER root`, puis retour à `_DEV_CONTAINERS_IMAGE_USER`. | cli:spec-configuration/containerFeaturesConfiguration.ts:205-234,330-362 |
| `_REMOTE_USER` / `_CONTAINER_USER` (+ `_HOME`) transmis aux scripts ; `_REMOTE_USER` = `_CONTAINER_USER` par défaut | (MUST : « are passed ») | spec:devcontainer-features.md:103-111 | `containerUser` = dernier `containerUser` des métadonnées, sinon `user` du service Compose, sinon USER de l'image ou du Dockerfile. | cli:spec-node/containerFeatures.ts:385-390 |
| `entrypoint` (feature) : script lancé au démarrage du conteneur | INFO | spec:devcontainer-features.md:45 | Tous les entrypoints fusionnés sont concaténés dans le script `/bin/sh -c`, avant `exec "$@"`. | cli:spec-node/singleContainer.ts:389-395 ; cli:spec-node/dockerCompose.ts:551-555 |
| `mounts`, `containerEnv`, `privileged`, `init`, `capAdd`, `securityOpt` des features | INFO | spec:devcontainer-features.md:40-51,549-551 | Via le label de métadonnées puis le merge (§3). | cli:spec-node/imageMetadata.ts:58-67 |
| Ordre d'installation : `dependsOn` (dur, récursif, avec options ; **must** être satisfait, sinon la création échoue), `installsAfter` (souple, non récursif, ignoré si la feature cible n'est pas installée), `overrideFeatureInstallOrder` (`roundPriority = n - idx`, ne peut pas contourner les dépendances) | MUST / SHOULD | spec:devcontainer-features.md:260-327,403-417 ; spec:feature-dependencies.md:118-223 | Implémentation fidèle, détaillée ci-dessous. | cli:spec-configuration/containerFeaturesOrder.ts:294-343,571-667 |
| Tri stable intra-round : ressource, puis tag, puis options, puis nom canonique | (SHOULD) | spec:feature-dependencies.md:161-173 | `compareTo` : OCI → égalité si digest et options égaux, sinon ressource (alias legacyIds compris), puis tag (`localeCompare`, **lexical et non semver**), puis options, puis digest. Local → chemin résolu puis options. tgz → URI puis options. | cli:spec-configuration/containerFeaturesOrder.ts:206-292 |
| OCI : manifeste `application/vnd.devcontainers`, couche `application/vnd.devcontainers.layer.v1+tar` nommée `devcontainer-feature-<id>.tgz`, annotation `dev.containers.metadata`. Sans annotation, l'outil **MUST** télécharger l'archive pour lire les métadonnées. | MUST | spec:devcontainer-features-distribution.md:97-167 ; spec:feature-dependencies.md:73-106 | Constantes identiques. | cli:spec-configuration/containerCollectionsOCI.ts:13-15 |
| Authentification OCI | — (spécifique à l'environnement) | spec:devcontainer-reference.md:107 | Ordre : `DEVCONTAINERS_OCI_AUTH=registry\|user\|token,...` → `~/.docker/config.json` (ou `$DOCKER_CONFIG`) avec `credHelpers[registry]`, `credsStore` ou `auths` → `GITHUB_TOKEN` (ghcr.io uniquement, si `GITHUB_HOST` est absent ou vaut github.com) → anonyme. **Lu sur la machine du CLI**, indépendamment du démon. | cli:spec-configuration/httpOCIRegistry.ts:286-386 |
| Résolution des dépendances seulement à la création initiale ; ensuite l'image ou le conteneur porte le résultat | INFO | spec:feature-dependencies.md:236-240 | Cohérent avec §2 (métadonnées figées sur le conteneur). | cli:spec-node/imageMetadata.ts:441-462 |

**Algorithme d'ordre (CLI)** :

1. Graphe : `dependsOn` est ajouté récursivement au worklist ; les `installsAfter` sont des arêtes souples (containerFeaturesOrder.ts:345-470).
2. `overrideFeatureInstallOrder` : pour l'index `i`, `roundPriority = len - i`, appliqué par correspondance souple (même ressource OCI ou legacyId, même chemin, même URI). Un id impossible à résoudre provoque une **erreur** (:294-343).
3. Les arêtes `installsAfter` qui pointent hors du worklist sont supprimées (:604-617).
4. Rounds : on retient les nœuds dont toutes les dépendances dures sont déjà installées (égalité stricte) et toutes les souples aussi (correspondance souple). Parmi eux, seuls ceux de `roundPriority` maximale sont gardés, puis triés par `compareTo` et ajoutés (:623-660).
5. Un round vide signale un cycle : log « Circular dependency detected! » et retour `undefined` (:633-638).

---

## 11. Build

| Exigence | Niveau | Source | Comportement CLI réf. | fichier:ligne |
|---|---|---|---|---|
| `build.dockerfile` requis en mode Dockerfile, relatif au devcontainer.json. `build.context` : défaut `"."` d'après la doc, relatif au devcontainer.json. `args` (variables autorisées), `options` (défaut `[]`), `target`, `cacheFrom` (chaîne ou tableau → `--cache-from`). | MUST / DÉFAUT | spec:devcontainerjson-reference.md:41-46 ; spec:devcontainer-reference.md:109-119 ; schema:560,614-640 | Sans `context`, le contexte = **dossier du Dockerfile**, et non celui du devcontainer.json (proches, mais pas identiques). Args : `--build-arg`, `options` ajoutés tels quels, `--target`. `cacheFrom` ignoré avec `--build-no-cache`, qui ajoute `--no-cache --pull`. | cli:spec-node/utils.ts:492-498 ; cli:spec-node/singleContainer.ts:124-273 |
| BuildKit requis ? | Non spécifié | — | **Non requis.** BuildKit est détecté via `docker buildx version`. S'il est présent : `buildx build --load` et `BUILDKIT_INLINE_CACHE=1`. Contenu des features via `--build-context` si BuildKit ≥ 0.8. Sans BuildKit : `docker build` classique et image temporaire `dev_container_feature_content_temp` (`FROM scratch; COPY`). `--platform`/`--push` exigent BuildKit. `--buildkit never` force le mode classique. | cli:spec-shutdown/dockerUtils.ts:254-270 ; cli:spec-node/devContainers.ts:213-222 ; cli:spec-node/containerFeatures.ts:237-246,325-361 ; cli:spec-node/singleContainer.ts:179-206 |
| Image étendue | — | — | Dockerfile + features → `<feature prefix>` + Dockerfile utilisateur (le dernier stage est nommé `dev_container_auto_added_stage_label` si besoin, ou bien `build.target`) + stages features → `Dockerfile-with-features`, tag `vsc-<basename>-<sha256(cwd)>`. Image + features → `Dockerfile.extended`, tag `…-features`. Image sans feature → aucun build, labels posés au `run`. `# syntax=docker/dockerfile:1.4` ajouté si nécessaire (moteur < 23). | cli:spec-node/singleContainer.ts:133-176 ; cli:spec-node/containerFeatures.ts:31-139,199-217,271-278 ; cli:spec-node/utils.ts:624-633 |
| Podman | Non spécifié | — | Détection si `docker -v` contient « podman » (ou « wslc »). Effets : sortie JSON de `ps`, arguments `--userns=keep-id` et `label=disable` (Linux), préfixe `localhost/` pour updateUID, pas de `additional_contexts` Compose. Pas de `consistency=` sous Linux. `--docker-path podman` pour l'utiliser. | cli:spec-shutdown/dockerUtils.ts:301-318,227 ; cli:spec-node/singleContainer.ts:438-451 ; cli:spec-node/dockerCompose.ts:191 ; cli:spec-node/utils.ts:464 |

---

## 12. `--docker-path`, `--docker-compose-path` et démon distant

| Exigence | Niveau | Source | Comportement CLI réf. | fichier:ligne |
|---|---|---|---|---|
| Démon ou contexte distant | **Non spécifié** : la spec ne mentionne ni `DOCKER_HOST` ni `docker context` (grep vide dans spec et cli) | — | Le CLI **ne gère rien explicitement**. Il appelle `--docker-path` (défaut `docker`) et `--docker-compose-path` (défaut `docker-compose`, seulement en repli v1), en leur transmettant **tout l'env du process** (`dockerEnv: cliHost.env`). `DOCKER_HOST`, `DOCKER_CONTEXT` et `docker context use` sont donc honorés de façon transitive par le binaire docker. | cli:spec-node/devContainers.ts:174-180,243 ; cli:spec-node/devContainersSpecCLI.ts:134-135 |
| Chemins de bind-mount avec un démon distant | — | spec:devcontainer-reference.md:154 (le bind peut être inaccessible aux environnements cloud) | Le CLI **suppose un FS partagé** : `source=` est le chemin local, sans traduction ni vérification. Un démon distant qui ne voit pas ce chemin monte un dossier vide ou inexistant. `initializeCommand`, la lecture du Dockerfile/compose, l'UID de `updateRemoteUserUID` et l'auth OCI restent **côté client**. Les contextes de build sont envoyés par `docker build` au démon distant (comportement standard de docker). | cli:spec-node/utils.ts:463-468 ; cli:spec-common/cliHost.ts:57-90 |
| `isLocalContainer` | — | — | Toujours `false` dans le CLI : pas de `tunnelInformation`. | cli:spec-node/devContainers.ts:139 |

---

## 13. WSL et Windows

| Exigence | Niveau | Source | Comportement CLI réf. | fichier:ligne |
|---|---|---|---|---|
| WSL | **Non spécifié** | — | `getCLIHost` renvoie toujours un hôte `local` (`platform = process.platform`). Le type `'wsl'` existe (héritage VS Code), mais cette branche n'est jamais atteinte dans le CLI : `uriToWSLFsPath` (`wslpath -u`) n'agit que si `cliHost.type === 'wsl'`. **Aucune** gestion de `\\wsl$`, `\\wsl.localhost` ou `/mnt/wsl` (grep vide). Pour WSL, la pratique consiste à lancer le CLI **dans** la distro, qui voit alors des chemins Linux ; le démon Docker Desktop les résout via son intégration WSL (non vérifié, hors code). | cli:spec-common/cliHost.ts:18,57-66 ; cli:spec-node/utils.ts:61-74 |
| Windows natif | — | — | Les chemins `C:\...` sont passés tels quels à `--mount source=` ; Docker Desktop les traduit (comportement démon, non vérifié). `initializeCommand` passe par `cmd.exe /c`. `localEnv` est insensible à la casse. Labels normalisés (lettre de lecteur en minuscule). `updateRemoteUserUID` jamais appliqué (plateforme ≠ linux). `consistency=cached` ajouté. | cli:spec-node/utils.ts:464,559-560,656-670 ; cli:spec-common/variableSubstitution.ts:65-74 ; cli:spec-node/containerFeatures.ts:425 |
| **wslc** (nouveau variant de CLI, CHANGELOG « Add WSLc support », PR #1249) | — | — | Détecté via `docker -v`. Effets : `--mount` converti en `-v`, `--init`/`--privileged`/`--cap-add`/`--security-opt` omis, pas de `--sig-proxy`/`-a`, pas d'events (polling), `docker version` simplifié. | cli:spec-shutdown/dockerUtils.ts:301-318 ; cli:spec-node/singleContainer.ts:350-351,363-377,405 ; cli:spec-shutdown/dockerUtils.ts:178 ; cli:spec-node/devContainers.ts:235 |

---

## Points ambigus / divergences spec vs CLI

1. **Point de montage par défaut** : la spec dit `/workspace` (spec:devcontainer-reference.md:156), le schéma dit `/workspaces/$project` (schema:540), et le CLI utilise `/workspaces/<basename racine git>` (cli:spec-node/utils.ts:429). Il faut suivre le CLI.
2. **`workspaceFolder` en Compose** : requis dans le schéma (schema:699-703) ; défaut `"/"` dans la doc (spec:devcontainerjson-reference.md:59) et dans le CLI (cli:spec-node/dockerCompose.ts:117).
3. **`workspaceMount` et `workspaceFolder` doivent aller ensemble** (spec:devcontainerjson-reference.md:48-49) : le CLI ne l'impose pas et calcule chacun indépendamment.
4. **`postCreateCommand` en arrière-plan par défaut** et `waitFor` (spec:devcontainer-reference.md:230-231) : le CLI exécute tout de façon bloquante. `waitFor` n'agit qu'avec `--skip-non-blocking-commands`, ce qui laisse à l'outil client l'exécution en arrière-plan.
5. **`initializeCommand`** : la spec dit « creation and subsequent starts ». Le CLI l'exécute à **chaque `up`**, y compris sur un conteneur déjà en marche.
6. **`postAttachCommand`** : relancé à chaque `up` sans marqueur. Le sens d'« attach » est laissé à l'outil.
7. **Conflits de `mounts`** : la spec dit « Last source wins » ; le CLI dédoublonne par **`target`**.
8. **`${containerEnv:...}`** : documenté pour `remoteEnv` seulement ; le CLI le substitue dans toute la config après le démarrage.
9. **`${devcontainerId}`** : la spec limite la liste des propriétés et le déconseille pour le build ; le CLI substitue partout.
10. **Syntaxe `${localEnv:VAR:default}`** : un défaut contenant `:` est tronqué dans le CLI (split sur `:`). Non précisé par la spec.
11. **Objets `mounts`** : seuls `type`, `source` et `target` sont transmis. Les options supplémentaires sont perdues, conformément au schéma (`additionalProperties: false`). En Compose, `type` disparaît aussi (syntaxe courte).
12. **`updateRemoteUserUID`** : la spec dit « Linux only », défaut true, « current local user ». Le CLI prend l'UID du **processus CLI** : avec un démon distant, c'est l'UID du client, pas celui de l'hôte du démon. Il utilise `remoteUser` en priorité sur `containerUser`, et le saute si l'utilisateur est root ou numérique.
13. **`build.context`** : la doc donne `"."` (dossier du devcontainer.json) comme défaut ; le CLI prend le **dossier du Dockerfile**.
14. **`overrideCommand`** : la spec décrit `while sleep 1000`. Le CLI remplace **toujours** l'entrypoint par un script `/bin/sh`, même quand `overrideCommand` vaut false (il relaie alors Entrypoint et Cmd de l'image). Cela sert à injecter les entrypoints des features.
15. **`forwardPorts`, `portsAttributes`, `shutdownAction`, `hostRequirements.cpus/memory/storage`** : fusionnés mais non implémentés par le CLI. Ils relèvent du client.
16. **Tri stable des features** : la spec trie les tags « oldest to newest » (sémantique) ; le CLI utilise `localeCompare`, donc un tri lexical.
17. **`overrideFeatureInstallOrder` qui référence une feature absente** : la spec dit « may fail » ; le CLI échoue si la feature ne peut pas être résolue (fetch). Si elle se résout sans être installée, aucune erreur n'est levée (non vérifié au-delà de la lecture de :324-338).
18. **Conteneur existant** : le CLI ignore les changements du devcontainer.json (hooks, mounts, containerEnv, etc.) sauf `remoteUser`, `userEnvProbe` et `remoteEnv` (cli:spec-node/imageMetadata.ts:44-48). La spec n'en dit rien ; un rebuild est nécessaire.
19. **Démon distant, WSL, `\\wsl$`** : ni la spec ni le CLI ne les traitent. Le CLI suppose un système de fichiers partagé entre le client et le démon. Toute traduction de chemins (Windows vers WSL, local vers distant) reste à la charge de l'implémenteur, c'est-à-dire de Zed.
20. **Identifiants de conteneur Compose** : retrouvés par projet et service, pas par id labels. Changer le nom de projet (par exemple via `COMPOSE_PROJECT_NAME`) « perd » donc le conteneur.
21. **Normalisation Windows des labels** : elle dépend de `process.platform` et non de la plateforme de `cliHost`. Elle est récente (fallbacks legacy, cli:spec-node/utils.ts:672-719). Les conteneurs créés par d'anciennes versions ont un `devcontainerId` différent.
22. **Options de features de type tableau ou nombre** : interdites par le schéma (`boolean` | `string`). Le CLI ne valide pas et sérialise la valeur telle quelle (non testé).
23. **Divergence site/repo** : `hostRequirements.gpu` est absent de la table de `devcontainerjson-reference.md` dans le repo, mais présent sur containers.dev et dans le schéma.
