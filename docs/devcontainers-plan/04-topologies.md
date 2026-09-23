# 04 — Topologies : état de Zed, besoins, faisabilité

Sources : `01-current-architecture.md` (code `main` `16c9aa7ea6`), `02-overlap-matrix.md`, `03-spec-compliance.md`,
`04-reference-topologies.md` (VS Code / CLI de référence / DevPod / JetBrains, sourcé), `04-wslc.md` (WSLc, sourcé +
sondage local). Sondage local de cette machine (2026-09-23, lecture seule) : pas de `docker` sous Windows ; Podman 6.0.2
sans machine ; distro `Ubuntu` (WSL2) sans docker ni podman (`id -u` = 1000) ; `C:\Program Files\WSL\wslc.exe` 2.9.12.0
(hors `PATH` du process courant, cf. `04-wslc.md` §Emplacement).

## 0. Modèle : trois lieux (+ le conteneur)

| Lieu | Rôle | Aujourd'hui dans Zed |
|---|---|---|
| **Client** | UI Zed, `Fs` local, identifiants HTTP/OCI | tout |
| **Hôte CLI** | où tournent `docker`/`podman`/`wslc`, `initializeCommand`, `id -u`, lecture de la config, où vivent les sources | = client (`01` H1) |
| **Hôte démon** | résout les **sources de bind-mount** ; reçoit le contexte de build envoyé par le CLI | supposé = client (`01` H7) |
| Conteneur | hooks, serveur distant Zed | via `docker exec` depuis le client |

Règle tirée de la référence (`04-reference-topologies.md`, leçons 1-3, 7) : **déplacer le CLI là où sont les sources et le
démon** (hôte CLI = hôte démon), utiliser **le même hôte** pour la création et pour la connexion, et ne faire passer par le
client que ce que le CLI **transporte** (contexte de build, `--build-context`), jamais ce qui doit être **monté**.

## 1. Matrice des topologies

✅ fonctionne · ⚠️ partiel/fragile · ❌ ne fonctionne pas · ? non vérifié

| # | Topologie | Client | Hôte CLI | Hôte démon | Zed `main` | #62680 | pupeno-wsl | pupeno-remote | Référence (VS Code/CLI) |
|---|---|---|---|---|---|---|---|---|---|
| T1a | Linux local, Docker/Podman | L | L | L | ✅ | ✅ | ✅ | ✅ | ✅ |
| T1b | macOS, Docker Desktop / Podman machine | M | M | VM (partage de fichiers) | ✅ ? | = | = | = | ✅ |
| T1c | Windows natif, sources `C:\`, Docker Desktop / Podman machine | W | W | VM | ⚠️ `initializeCommand` chaîne cassé (`/bin/sh`), `kill` absent, pas d'UID (normal) | = | = | = | ✅ (`%ComSpec%`) |
| T2 | Windows + sources dans WSL2 + Docker Desktop (intégration WSL) | W | **distro** | VM Docker Desktop | ❌ refus « remote project » (`recent_projects.rs:508`) | ⚠️ câblé, jamais testé | ⚠️ **cible principale** ; `initializeCommand` cassé | ⚠️ tests unitaires | ✅ (CLI lancé dans la distro) |
| T3 | Windows + sources dans WSL2 + Docker CE **dans** la distro | W | distro | distro | ❌ | ⚠️ ? | ❌ (garde « même moteur ») | ⚠️ ? | ✅ |
| T4 | Client + hôte SSH (sources et Docker sur l'hôte) | any | **hôte SSH** | hôte SSH | ❌ refus | ✅ (hôte POSIX) | ❌ | ✅ | ✅ (Remote-SSH puis Reopen) |
| T4w | idem, hôte SSH **Windows** | any | hôte SSH Win | VM | ❌ | ❌ | ❌ | ⚠️ (PowerShell ; env ignoré par `build_command_windows`) | ⚠️ ? |
| T5 | Sources locales + `DOCKER_HOST=ssh://…` / contexte distant | any | client | distant | ⚠️ build OK, **mounts vides/faux**, UID faux, sans avertissement | = | = | = | ⚠️ (pas de traduction ; volume + clone recommandé) |
| T6a | Zed (client) **dans** un conteneur, socket docker monté (DooD) | conteneur | conteneur | hôte physique | ⚠️ mounts faux (chemins du conteneur donnés au démon hôte) | = | = | = | ⚠️ (`${localWorkspaceFolder}` hôte, motif « même chemin ») |
| T6b | Projet déjà ouvert **dans** un dev container → ouvrir un dev container depuis celui-ci (DinD/DooD) | any | conteneur | conteneur (DinD) ou hôte (DooD) | ❌ refus | ❌ refus explicite (`UnsupportedHost`) | ❌ refus | ❌ refus | ⚠️ |
| T7 | Podman rootless / podman machine | L/M/W | idem T1 | VM ou local | ⚠️ options Linux appliquées partout (`manifest:2186-2204`) ; `buildx` codé en dur | = | = | = | ✅ (conditions Linux) |
| T8 | **WSLc** (`wslc.exe`, WSL ≥ 2.9.3, préversion) | W | W (CLI `wslc`) | VM utilitaire WSLC (dockerd 25 / API 1.44, privé) | ❌ | ❌ | ❌ | ❌ | ⚠️ `CLIVariant.Wslc` (devcontainers/cli ≥ 0.88, PR #1249) |

Légende colonnes Client/Hôte : L Linux, M macOS, W Windows. « = » : même état que `main`.

## 2. Besoins par topologie

| # | Hôte CLI | Connexion (proxy, upload serveur) | Chemins | `initializeCommand` / UID / labels | Spécifique |
|---|---|---|---|---|---|
| T1c | local | local | — | `cmd /c` pour la forme chaîne (#63391) ; `kill` → `TerminateProcess`/`Child::kill` | — |
| T2 | `RemoteConnection` WSL du projet (`build_command`, déjà quoté : `remote/src/transport/wsl.rs:527-575`) | **via la distro** (`wsl.exe … docker exec -i`) pour ne pas dépendre du CLI Windows | chemins Linux de la distro ; Docker Desktop les traduit | dans la distro ; UID de l'utilisateur WSL ; labels chemin Linux | l'intégration WSL doit être activée pour la distro (détection + message) |
| T3 | idem T2 | idem T2 | chemins distro (démon local à la distro) | idem T2 | aucun partage avec un éventuel Docker Windows |
| T4 | `RemoteConnection` SSH (ControlMaster réutilisé) | via SSH (`ssh … docker exec -i`), upload serveur en flux `exec -i` (#62680) | chemins de l'hôte | sur l'hôte ; UID de l'utilisateur SSH ; labels chemins hôte | fichiers générés : un dossier temporaire **par build**, nettoyé ; features OCI téléchargées sur le client puis uploadées (ou téléchargées sur l'hôte) |
| T4w | idem T4 | idem | style Windows | `cmd /c` ; pas d'UID | `build_command_windows` n'envoie pas l'env (`remote/src/transport/ssh.rs:1968-1999`) → passer l'env en `-e`/args |
| T5 | local | local | **pas de bind-mount de sources locales** | UID désactivé | détecter `DOCKER_HOST`/contexte non-unix (`docker context inspect`) et **refuser clairement** ou proposer volume + clone (hors périmètre initial) |
| T6a | local (conteneur) | local | traduire `local_workspace_folder` → chemin hôte (inspect du conteneur courant : `Mounts[].Source` pour la destination qui contient le projet) ; sinon refuser | UID du conteneur outil | détection « je suis dans un conteneur » (`/.dockerenv`, cgroup, `container` env) |
| T6b | l'hôte = le conteneur du projet (connexion `Docker`) → **récursif** | via `docker exec` imbriqué | chemins du conteneur (DinD) | dans le conteneur | nécessite `DockerHost::Docker(…)` récursif, explicitement exclu par #62680 ; à reporter |
| T7 | local | local | — | `--userns=keep-id`, `label=disable` **seulement Linux** ; `--sig-proxy` | repli sans BuildKit (`03` B1) |
| T8 | local (Windows), binaire `wslc` (repli `%ProgramFiles%\WSL\wslc.exe`) | `wslc exec -i` (robustesse non vérifiée) ; `wslc container cp` | **chemins Windows uniquement** ; `--mount` → `-v` | `cmd /c` ; pas d'UID ; labels chemins Windows | variante de CLI (`docker -v` contient « wslc ») ; pas de `--format` template ; `list --format json` ; retirer `--init/--privileged/--cap-add/--security-opt/--sig-proxy` ; **pas de compose, pas de `--build-context`** (features à repenser) ; pas de sources dans une distro |

## 3. Capacités transverses nécessaires (entrée de la Phase 5)

| Capacité | Topologies | Contribution la plus proche |
|---|---|---|
| **K1** Exécuter une commande sur l'hôte CLI (argv préservé, env, cwd, stdin) | T2, T3, T4, T4w | `DevContainerHost` (#62680) / `ProjectCommandBuilder` (pupeno-wsl) / `ProjectHost` (pupeno-remote) |
| **K2** Lire/écrire des fichiers sur l'hôte CLI | T2–T4w | `ProjectHost` (pupeno-remote) ; RPC `Fs` du `remote_client` à évaluer |
| **K3** Chemins typés par style d'hôte | T2–T4w, T6a, T8 | `HostPathBuf` (pupeno-remote) |
| **K4** Connexion au conteneur **via le même hôte** (persistée) | T2–T4w | `DockerHost` (#62680) |
| **K5** Identité stable qualifiée par l'hôte | toutes | #60975 × #62680 (`02-overlap-matrix.md` §6) |
| **K6** Variante de CLI `docker`/`podman`/`wslc` détectée et adaptée | T7, T8 | aucune ; référence `CLIVariant` du CLI |
| **K7** Décisions selon la plateforme de **l'hôte** (UID, labels, `cmd /c` vs `/bin/sh`) | T1c, T2–T4w, T8 | pupeno-wsl / pupeno-remote (`engine.is_posix()`) |
| **K8** Détection de topologie non supportée + message actionnable | T5, T6a, T6b, T8/compose | `unsupported_reason` (pupeno-wsl) |
| **K9** Traduction de source de mount vers la vue du démon | T6a (et T5 si un jour) | aucune |
| **K10** Journal « exécuté sur &lt;hôte&gt; : &lt;argv rédigé&gt; » | toutes | redaction #63606 à conserver sur tous les chemins |

## 4. Priorisation proposée

| Priorité | Topologies | Justification |
|---|---|---|
| P0 | T1a/b/c, T7 | corriger l'existant (conformité `03`) avant d'élargir ; T1c et T7 ont des défauts propres |
| P1 | **T4** (SSH) puis **T2/T3** (WSL) | demande la plus forte (#59500, #56252, 3 contributions) ; même mécanisme (K1–K5, K7) ; WSL = SSH avec un autre `RemoteConnection` |
| P2 | T8 (WSLc) | préversion (GA visée automne 2026), sous-ensemble sans compose ni features `--build-context` ; derrière un réglage expérimental |
| P3 | T6a | détection + traduction de chemin ; utile pour les utilisateurs Zed-dans-conteneur, faible volume |
| Hors périmètre initial (refus explicite + message) | T4w, T5, T6b | complexité / cas rares ; T5 « volume + clone » = autre fonctionnalité |

## 5. Faisabilité des tests sur cette machine

| Topologie | Prérequis manquant | Action requise (décision utilisateur) |
|---|---|---|
| T1c | moteur Windows | `podman machine init/start` (Podman 6.0.2 installé) **ou** Docker Desktop |
| T2 | Docker Desktop + intégration WSL | installation Docker Desktop |
| T3 | Docker CE dans `Ubuntu` | `apt install docker-ce` dans la distro (sudo) |
| T4 | un hôte SSH avec Docker | réutiliser `Ubuntu` : `openssh-server` + Docker CE, SSH en `localhost` → teste le chemin SSH sans seconde machine |
| T7 | podman machine | cf. T1c |
| T8 | rien (wslc présent) | lancer effectivement des conteneurs `wslc` (création de session/VM) — non fait, sondage limité aux `--help` |
| T6a | Zed dans un conteneur | image Linux avec Zed + socket monté (CI Linux plutôt que cette machine) |

Aucune de ces actions n'a été faite : elles modifient la machine. À planifier en Phase 7 avec ton accord.

## 6. Questions ouvertes pour le point d'arrêt 2

1. Périmètre : confirmes-tu P0 → P1 (SSH puis WSL) → P2 (WSLc expérimental), avec T4w/T5/T6b refusés explicitement ?
2. WSL : pour T2, la connexion passe **par la distro** (option B de `02-overlap-matrix.md` §5), ce qui abandonne la garde
   « même moteur » de pupeno-wsl. D'accord ?
3. WSLc : adaptateur CLI (`wslc`) plutôt que SDK COM (`04-wslc.md` Q7) — et acceptes-tu une première version sans
   compose ni features ?
4. Sécurité : `03` E2 (env du conteneur persisté en clair dans la base Zed) — le traiter comme une PR publique normale,
   ou le signaler d'abord en privé à l'équipe Zed ?
5. Tests réels (§5) : quelles installations autorises-tu (Podman machine, Docker CE dans Ubuntu, sshd local) ?
