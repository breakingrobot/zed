# 09 — Guide de test manuel (Windows)

Installeur : `H:\Sources\zed-wt\artifacts\Zed-x86_64-final.exe`, construit depuis le haut de la pile
(`devcontainers/f29-manage-containers`), qui contient tout : correctifs A, WSL, SSH, confiance, ports, environnement de
l'hôte, refus explicites, actions de cycle de vie, marqueurs, et la parité VS Code (sections 6 et 7). Le
`remote_server-linux-x86_64` doit venir de la même branche : depuis F20, les ports passent par la connexion du serveur,
un ancien serveur ne sait pas les transférer.
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
   terminal du conteneur (root) : `apt-get update && apt-get install -y python3 && python3 -m http.server 8765`
   (`debian:12` n'a ni Python ni `nc`) ; depuis Windows, `curl http://localhost:8765` répond.

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

## 6. Parité VS Code

| Test | Attendu |
|---|---|
| marqueurs : `projects: reconnect dev container` sans rebuild | `on-create`, `update-content`, `post-create-*` ne gagnent pas de ligne |
| agent SSH (WSL) : `ssh-add -l` dans le terminal du conteneur | liste les clés de l'agent de la distro (`SSH_AUTH_SOCK=/tmp/zed-ssh-agent.sock`) |
| `git config user.name` dans le conteneur | identique à celui de l'hôte du moteur (copie de `~/.gitconfig`) |
| `waitFor` par défaut | la fenêtre s'ouvre après `update-content` ; `post-create-*`, `post-start`, `post-attach` tournent en tâches du terminal |
| port automatique : `python3 -m http.server 9000` dans le conteneur, sans `forwardPorts` | sous 3 s, `curl http://localhost:9000` répond depuis Windows |
| configuration modifiée : éditer `devcontainer.json` puis `projects: reconnect dev container` | notification « Rebuild Container » ; le bouton reconstruit le conteneur |

## 7. Parité VS Code, suite (F6–F29, jamais testée sur un vrai moteur)

Commencer par le projet WSL de la section 1, puis refaire les lignes « ports » et « identifiants » en SSH (section 2).

| Branche | Test | Attendu |
|---|---|---|
| F6/F13 | `"hostRequirements": { "cpus": 64, "memory": "512gb" }` puis rebuild | conteneur créé quand même, avertissement CPU et mémoire |
| F12 | `"hostRequirements": { "gpu": "optional" }` | pas d'erreur sans GPU NVIDIA ; `true` → avertissement |
| F7 | `projects: rebuild dev container without cache` | rebuild complet, couches non réutilisées (plus long) |
| F8 | `export ZED_PROBE=1` dans `~/.profile` du conteneur, reconnect | `echo $ZED_PROBE` dans le terminal → `1` ; `remoteEnv` garde la priorité |
| F9 | `"portsAttributes": { "9000": { "label": "web", "onAutoForward": "notify" } }`, `python3 -m http.server 9000` | notification nommant le port et « web » ; `openBrowser` ouvre le navigateur |
| F14 | port 9000 déjà pris sous Windows, relancer le serveur | transféré sur le port libre suivant ; avec `requireLocalPort` → message « non transféré » |
| F15 | Compose avec `"forwardPorts": ["db:5432"]`, projet SSH | `localhost:5432` joint le service `db` |
| F20 | deux fenêtres sur le même conteneur, un serveur sur 9000 | un seul transfert ; fermer une fenêtre ne le coupe pas |
| F21 | `projects: show forwarded ports` (ou menu du titre) | liste ; ouvrir, arrêter, taper un numéro pour transférer |
| F10 | fermer la dernière fenêtre du conteneur | `docker ps` : conteneur arrêté (`shutdownAction` par défaut) ; `"none"` → reste actif |
| F11 | réglage `"dev_container_dotfiles_repository": "<ton-id>/dotfiles"`, rebuild | `~/dotfiles` cloné, script d'installation lancé |
| F16 | créer un `devcontainer-lock.json` vide à côté de la config (avec une feature), rebuild | fichier rempli avec digests et `integrity` |
| F17 | rebuild une deuxième fois | features non retéléchargées (`%LOCALAPPDATA%\Zed…\devcontainer\features`) |
| F18 | stop / start / remove | plus rapides, pas de sonde BuildKit répétée (journal) |
| F19 | deux conteneurs différents | `docker volume ls` : `zed-remote-server` ; le 2e ne retélécharge pas le serveur (ne vaut pas pour une build dev avec `ZED_COPY_REMOTE_SERVER`) |
| F22 | dans le conteneur, `git push` HTTPS vers un dépôt privé | identifiants fournis par Windows, pas de demande dans le terminal |
| F23 | réglage `"dev_container_secrets_file"` vers un JSON `{"MY_TOKEN": "x"}` | `echo $MY_TOKEN` → `x` ; valeur absente de `docker inspect` et des réglages |
| F24 | `"customizations": { "zed": { "settings": { "tab_size": 2 } } }`, conteneur neuf | `zed: open server settings` contient `tab_size` |
| F25 | casser un `onCreateCommand` (`exit 1`) | erreur avec « Show Log » ; `projects: show dev container log` montre les commandes et `docker logs` |
| F26 | `docker run -d --name t debian sleep infinity` dans WSL, `projects: attach to running container` | connexion au conteneur `t` |
| F27 | `projects: clone repository in container volume` avec un dépôt public contenant un `.devcontainer` | volume monté sur `/workspaces`, conteneur construit |
| F28 | projet local avec un contexte Docker distant | projet local refusé avec un renvoi vers l'option volume |
| F29 | `projects: manage dev containers` | liste des conteneurs et de leur état ; ouvrir, arrêter, supprimer (avec confirmation) |

## 8. Que noter en cas d'échec

- Le message affiché, et le journal : palette → `zed: open log` (lignes `dev_container` et `remote`).
- `docker ps -a` et `docker logs <conteneur>` sur l'hôte du moteur.
