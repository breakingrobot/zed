# Données GitHub brutes — index

Récupérées le 2026-09-22 via l'API REST publique (non authentifiée), complétées le 2026-09-23 via `gh` authentifié (lecture seule).

## PRs suivies

| PR | État | Auteur | Titre | MAJ | Commentaires | CI (check-runs head) |
|---|---|---|---|---|---|---|
| #60975 | open | alex-berger | Add lifecycle management for dev containers | 2026-08-28 | 8 | aucun check-run |
| #62680 | open | alexdhill | Add support for dev containers on remote host over SSH | 2026-09-11 | 7 | {'skipped': 10, 'success': 1} |
| #56293 | merged | YauhenVasileusky | dev_container: Respect runServices for Docker Compose | 2026-06-04 | 6 | {'failure': 1, 'skipped': 21, 'success': 20} |

## Issues suivies

| Issue | État | Raison | Auteur | Titre | Commentaires |
|---|---|---|---|---|---|
| #51664 | closed | completed | conbruno | use_podman option for devcontainer does not work on windows. | 5 |
| #52397 | closed | not_planned | tdegrunt | Zed in devcontainer can't access dev-container credsStore | 4 |
| #53635 | closed | completed | prashanth057 | Devcontainer feature doesnt work when using podman on Windows | 3 |
| #53848 | closed | not_planned | blkerby | Devcontainers not starting | 11 |
| #54452 | closed | completed | JakeIsMeh | Can't start devcontainers If the 6th character of a devcontainer's name is a space and needs an intermediate build (e.g. for features) | 2 |
| #56576 | open |  | maresb | Agent thread fails to load after dev container rebuild (RemoteConnectionIdentity keyed on ephemeral container_id) | 7 |
| #59500 | open |  | macraig | Support connecting to remote devcontainers (devcontainer on remote host) | 9 |
| #59907 | open |  | macraig | Remote REPL + Remote Devcontainers | 0 |

Discussion #56252 : voir `discussion-56252.md` (résumé, non verbatim).

## Ouverts sous `area:dev containers` (47 éléments, 11 PRs)

| # | Type | Auteur | Titre | Créé |
|---|---|---|---|---|
| #64025 | PR | harshaygadekar | dev_container: Support array and numeric feature options (#63978) | 2026-09-10 |
| #63899 | PR | cmdr-chara | dev_container: Bind forwarded ports to loopback | 2026-09-07 |
| #63613 | PR | senid231 | remote: Fix switching git worktrees inside a dev container | 2026-09-02 |
| #63568 | issue | senid231 | Switching git worktree inside a dev container fails after exactly 60s with `Remote server exited with status 15` | 2026-09-01 |
| #63391 | PR | shrirajh | dev_container: Fix string-form initializeCommand on Windows hosts | 2026-08-29 |
| #63049 | issue | epfister-cyncly | Resolved TypeScript completion details are not displayed in devcontainer sessions | 2026-08-21 |
| #63034 | PR | pupeno | Preserve Docker exec command arguments | 2026-08-21 |
| #62964 | issue | jackdouglas | devcontainer: string-form lifecycle scripts lose their argument boundaries (v1.15.0 regression) | 2026-08-20 |
| #62928 | issue | EyMaddis | Clicking on file:// paths from agent outputs do not open files in dev containers | 2026-08-20 |
| #62760 | issue | Tolsto | Dev container `forwardPorts` are exposed on all host interfaces instead of localhost | 2026-08-17 |
| #62680 | PR | alexdhill | Add support for dev containers on remote host over SSH | 2026-08-15 |
| #62601 | issue | LiberQuack | Java debugger fails from devcontainer | 2026-08-13 |
| #62576 | issue | insunaa | Failed to connect to Dev Container | 2026-08-13 |
| #62271 | PR | voedipus | devcontainer: fix postStartCommand shell escaping in marker script | 2026-08-06 |
| #62196 | PR | Yuxin-Qiao | dev_container: Preserve Dockerfile USER in features image | 2026-08-05 |
| #61223 | issue | rectalogic | Zed devcontainers ignores Dockerfile USER | 2026-07-17 |
| #60975 | PR | alex-berger | Add lifecycle management for dev containers | 2026-07-14 |
| #60687 | issue | ghost | Devcontainer fails to start with compose port | 2026-07-09 |
| #60668 | issue | TheBarbellCoder | Dev Containers: Zed tries to pull Docker build stage names like base/devbase as images when multiple configs are present | 2026-07-09 |
| #60458 | issue | vincentkelleher | Devcontainer OCI fails to fetch features through corporate Artifactory | 2026-07-06 |
| #59502 | issue | kiuma | Missing language server in devcontainer | 2026-06-17 |
| #59500 | issue | macraig | Support connecting to remote devcontainers (devcontainer on remote host) | 2026-06-17 |
| #59347 | issue | salvatore-greco | Failing to connect to dev container closes the editor | 2026-06-15 |
| #59116 | issue | chloerei | High CPU usage when using devcontainer | 2026-06-11 |
| #58831 | issue | nnullcolumn | Dev Containers Docs: question/contradiction in limitations | 2026-06-08 |
| #58794 | issue | jvatic | devcontainer fails to start when using buildx fallback | 2026-06-07 |
| #58703 | issue | wallzero | Devcontainers+Podman works but compose does not | 2026-06-05 |
| #58500 | PR | KyleBarton | Relabel mounts for podman/buildkit cases, to account for SELinux | 2026-06-04 |
| #57337 | issue | secondl1ght | Project search loads forever | 2026-05-21 |
| #57039 | issue | bkhl | Failure to open devcontainer with relative paths in runArgs | 2026-05-18 |
| #56576 | issue | maresb | Agent thread fails to load after dev container rebuild (RemoteConnectionIdentity keyed on ephemeral container_id) | 2026-05-12 |
| #56516 | issue | Mirkbot | GH_COPILOT_TOKEN not being picket up in devcontainer - Copilot not working | 2026-05-12 |
| #55886 | issue | Jxhnn | DevContainer - Failed to install | 2026-05-06 |
| #55864 | issue | cloudcalvin | Zed sometimes persists a stale devcontainer container ID and fails to reconnect after container recreation | 2026-05-06 |
| #55831 | issue | mccormack-harry | Devcontainer name forces _ for COMPOSE_PROJECT_NAME | 2026-05-05 |
| #54257 | issue | blue42u | Dev Container: Podman permission denied uploading server binary | 2026-04-19 |
| #54196 | issue | khanhthanhdev | Dev Container takes too long to initialize | 2026-04-17 |
| #53624 | issue | chloerei | AI agent + git worktree not work with devcontainer | 2026-04-10 |
| #53529 | issue | chloerei | devcontainer changes the default PATH environment of the image | 2026-04-09 |
| #53170 | PR | zdeneklapes | dev_container: Fix various startup issues | 2026-04-04 |
| #49924 | issue | maksimr | debugpy doesn't work in devcontainer | 2026-02-23 |
| #49199 | issue | gilesknap | shells inside devcontainers do not have remoteEnv environment variables set | 2026-02-14 |
| #48483 | issue | Pioneer-1-1 | Zed failing to start podman containers | 2026-02-05 |
| #47701 | issue | secondl1ght | MCP servers do not work inside devcontainers | 2026-01-26 |
| #47580 | issue | secondl1ght | Git commit not working in Zed UI inside devcontainer but working in terminal | 2026-01-25 |
| #47422 | issue | bramvbilsen | Codex agent authentication via OpenAI account not possible in DevContainers | 2026-01-22 |
| #47121 | issue | secondl1ght | SSH and GPG agents not forwarded to devcontainer | 2026-01-19 |

## PRs connexes (via `gh`, 2026-09-23 ; fichiers `pr-<n>-gh.json`)

| PR | État | Auteur | Titre | Diff | Revues | Commentaires MEMBER/OWNER |
|---|---|---|---|---|---|---|
| #63034 | open | pupeno | Preserve Docker exec command arguments | +318/−184 | — | 0 |
| #62271 | open | voedipus | devcontainer: fix postStartCommand shell escaping in marker script | +15/−13 | — | 0 |
| #63391 | open | shrirajh | dev_container: Fix string-form initializeCommand on Windows hosts | +179/−10 | — | 0 |
| #62196 | open | Yuxin-Qiao | dev_container: Preserve Dockerfile USER in features image | +321/−12 | — | 0 |
| #63899 | open | cmdr-chara | dev_container: Bind forwarded ports to loopback | +225/−1 | — | 0 |
| #63613 | open | senid231 | remote: Fix switching git worktrees inside a dev container | +178/−31 | — | 0 |
| #58500 | open | KyleBarton | Relabel mounts for podman/buildkit cases, to account for SELinux | +62/−1 | — | 0 |
| #53170 | open | zdeneklapes | dev_container: Fix various startup issues | +685/−69 | — | 0 |
| #64025 | open | harshaygadekar | dev_container: Support array and numeric feature options (#63978) | +127/−4 | copilot-pull-request-reviewer:COMMENTED, harshaygadekar:COMMENTED | 0 |
| #63898 | merged (merged) | cole-miller | fs: Replace `GlobalWatcher` with an `OsWatcher` that contains one backend | +281/−352 | Anthony-Eid:APPROVED | 0 |

Issue #62964 (open, jackdouglas) : « devcontainer: string-form lifecycle scripts lose their argument boundaries (v1.15.0 regression) » — reproduction de `sh -c "/bin/sh -c mkdir /tmp/zed-repro"` ; correctif proposé : #63034. Fichier `issue-62964-gh.json`.

CI #60975 confirmée par `gh pr checks` : seul `verification/cla-signed`. CI #62680 : `route-pr` pass, 10 jobs `skipping`.
