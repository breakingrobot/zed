# 09 — Guide de test manuel (Windows)

Installeur : `H:\Sources\zed-wt\artifacts\Zed-x86_64-final.exe` (branche `devcontainers/e1-lifecycle-actions`, qui
contient tout : correctifs A, WSL, SSH, confiance, ports, environnement de l'hôte, refus explicites, actions de cycle de vie).
Il s'installe dans le profil, sans droits administrateur, sous le nom « Zed Dev », à côté d'un Zed habituel.

## 0. Préparation (une fois)

1. Lancer l'installeur. Il n'est pas signé : SmartScreen → « Informations complémentaires » → « Exécuter quand même ».
2. Vérifier que la variable utilisateur `ZED_COPY_REMOTE_SERVER` pointe vers
   `H:\Sources\zed-wt\artifacts\remote_server-linux-x86_64` (déjà fait sur cette machine ; à refaire sur un autre PC).
   Sans elle, Zed Dev essaie de compiler le serveur et la connexion au conteneur échoue.
3. Démarrer WSL (`wsl -d Ubuntu`) : Docker 29.1.3 y tourne ; `sshd` de test écoute sur le port 2222.

## 1. Projet WSL (Docker de la distro)

1. Dans Ubuntu : `cp -r /mnt/h/Sources/zed/docs/devcontainers-plan/fixtures/lifecycle-forms ~/` puis
   `sed -i 's/\r$//' ~/lifecycle-forms/.devcontainer/*`.
2. Dans Zed Dev : ouvrir `~/lifecycle-forms` via WSL (modale Remote Projects → WSL → Ubuntu).
3. Si Zed signale un projet non approuvé : l'approuver (fenêtre de sécurité).
4. Accepter la suggestion « Open in Container », ou palette → `projects: open dev container`.
5. Attendu : une nouvelle fenêtre connectée au conteneur ; dans son terminal :
   `ls /tmp/zed-fixture` montre `on-create`, `update-content`, `post-create-a`, `post create b`, `post-start`, `post-attach`.
6. Dans Ubuntu : `docker ps` montre le conteneur, labels `devcontainer.local_folder=/home/robot/lifecycle-forms`.

## 2. Projet SSH (`localhost:2222`, même Docker)

1. Ajouter dans `%USERPROFILE%\.ssh\config` :
   ```
   Host zed-wsl-ssh
     HostName localhost
     Port 2222
     User robot
     IdentityFile ~/.ssh/zed_devcontainer_test
   ```
2. Dans Ubuntu : `cp -r ~/lifecycle-forms ~/lifecycle-forms-ssh`.
3. Dans Zed Dev : Remote Projects → SSH → `zed-wsl-ssh` → ouvrir `/home/robot/lifecycle-forms-ssh`.
4. Approuver le projet si demandé, puis `projects: open dev container`.
5. Attendu : comme en 1 ; `docker ps` montre un second conteneur (labels `…/lifecycle-forms-ssh`).
6. Ports : ajouter `"forwardPorts": [8765]` au `devcontainer.json`, lancer `projects: rebuild dev container`, puis dans le
   conteneur `python3 -m http.server 8765` (ou `nc -l -p 8765`) ; depuis Windows, `curl http://localhost:8765` répond.

## 3. Projet Windows natif (Podman)

1. `podman machine start` ; dans les réglages de Zed Dev : `"use_podman": true`.
2. Copier une fixture sur un disque Windows, ouvrir le dossier dans Zed Dev, puis `projects: open dev container`.
3. Attendu : conteneur créé par Podman ; `initializeCommand` (fixture `initialize-command`) exécuté par `cmd /c`.
4. Remettre `"use_podman": false` après le test si le Zed habituel partage ces réglages.

## 4. Actions de cycle de vie (depuis une fenêtre de conteneur)

| Action (palette) | Attendu |
|---|---|
| `projects: reconnect dev container` | reconnexion ; après `docker stop` manuel, le conteneur redémarre |
| `projects: restart dev container` | `post-start` gagne une ligne ; les processus du conteneur sont tués |
| `projects: rebuild dev container` | nouveau conteneur (nouvel id), même entrée dans les projets récents |
| `projects: stop dev container` | conteneur arrêté, projet rouvert sur l'hôte (WSL/SSH/local) |
| `projects: delete dev container` | conteneur supprimé, projet rouvert sur l'hôte |

## 5. Refus attendus

| Situation | Message |
|---|---|
| projet partagé en collaboration | « Dev containers can't be opened from a project shared with you. » |
| depuis une fenêtre déjà dans un conteneur | « This project is already open in a dev container. » |
| hôte SSH Windows | « Dev containers over SSH need a Linux or macOS host. » |
| projet local avec `DOCKER_HOST=ssh://…` | explique que le moteur est sur une autre machine |

## 6. Que noter en cas d'échec

- Le message affiché, et le journal : palette → `zed: open log` (lignes `dev_container` et `remote`).
- `docker ps -a` et `docker logs <conteneur>` sur l'hôte du moteur.
