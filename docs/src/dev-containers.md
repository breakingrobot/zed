---
title: Dev Containers - Zed
description: Open projects in dev containers with Zed. Reproducible development environments using devcontainer.json configuration.
---

# Dev Containers

Dev Containers provide a consistent, reproducible development environment by defining your project's dependencies, tools, and settings in a container configuration.

If your repository includes a `.devcontainer/devcontainer.json` file, Zed can open a project inside a development container.

## Requirements

- Docker or Podman must be installed and available in your `PATH`. If you use `podman`, you must set the `use_podman` setting in your Zed settings.json to true.
- Your project must contain a `.devcontainer/devcontainer.json` directory/file.
- The project must be [trusted](./worktree-trust.md). Opening a dev container runs code from the repository, such as `initializeCommand`, Dockerfiles and features, so Zed shows the security modal instead while the project is in Restricted Mode.

By default Zed builds dev container images with BuildKit when the `docker buildx` plugin is available. If your Docker-compatible engine lacks an integrated BuildKit (for example, Apple Container accessed through a Docker-API bridge), set `"dev_container_use_buildkit": false` in your settings.json to use the classic Docker builder instead.

## Using Dev Containers in Zed

### Automatic prompt

When you open a project that contains the `.devcontainer/devcontainer.json` directory/file, Zed will display a prompt asking whether to open the project inside the dev container. Choosing "Open in Container" will:

1. Build the dev container image (if needed).
2. Launch the container.
3. Reopen the project connected to the container environment.

### Manual open

If you dismiss the prompt or want to reopen the project inside a container later, you can use Zed's command palette to run the "Project: Open Remote" command and select the option to open the project in a dev container.
Alternatively, you can reach for the Remote Projects modal (through the {#kb projects::OpenRemote} binding) and choose the "Connect Dev Container" option.

### Adding a configuration from a template

To add a dev container to a project that doesn't have one, run {#action projects::InitializeDevContainer} and pick a template of the [official collection](https://containers.dev/templates), like VS Code's "Add Dev Container Configuration Files". Zed asks for the template's options and the features to add, then writes `.devcontainer/devcontainer.json` into the project.

### Cloning a repository in a container volume

Run {#action projects::CloneRepositoryInContainerVolume} and type a repository's URL to clone it into a volume of the container engine on your machine and open it in its dev container, like VS Code's "Clone Repository in Container Volume". The sources stay in the volume, mounted at `/workspaces`, which is faster than a folder of your machine when the engine runs in a virtual machine, as Docker Desktop does. The repository needs a `.devcontainer/devcontainer.json` or `.devcontainer.json`.

Zed clones with the `alpine/git` image, so the repository must be reachable without credentials, and keeps a copy of it under `devcontainer/volumes` in its data folder to read the configuration and build from. It refreshes that copy from the volume each time it starts or rebuilds the container. Docker Compose configurations mount what their Compose files say, so they use the copy rather than the volume.

This also works when Docker uses a container engine on another machine, through `DOCKER_HOST` or the current Docker context (`docker context use`): Zed builds with contexts sent from your machine and keeps the sources in a volume of that engine. For a project folder on your machine, such an engine can't mount it: open the project over SSH on the engine's machine instead.

### Attaching to a running container

To work in a container that Zed didn't create, such as one started with `docker run` or Docker Compose, run {#action projects::AttachToRunningContainer} and pick the container. Zed lists the running containers of the engine the current project uses (locally, in WSL, or on an SSH host), connects as the container's user and opens its working directory. Without a dev container configuration, Zed doesn't rebuild, stop or remove attached containers.

## WSL projects {#wsl-projects}

On Windows, you can open a dev container from a project that lives in a WSL distribution. Zed then runs the container engine inside that distribution, where your sources are:

- `docker` (or `podman`) must be installed in the distribution and available in its `PATH`. Docker Desktop works through its WSL integration.
- `initializeCommand` runs in the distribution, from your project folder.
- `${localEnv:VAR}` reads the environment of a login shell in the distribution.
- Bind mounts use the distribution's paths, such as `/home/you/project`.
- Ports from numeric `forwardPorts` and `appPort` are published in the distribution. From Windows, reach them through WSL's localhost forwarding.

To open one:

1. Open your project through WSL (for example with `zed --wsl Ubuntu /home/you/project`, or from the Remote Projects modal).
2. Run "Reopen in Dev Container" from the prompt, or open the Remote Projects modal and choose "Connect Dev Container".

## SSH projects {#ssh-projects}

You can open a dev container from a project on a Linux or macOS machine you reach over SSH. Zed runs the container engine on that machine, where your sources are:

- `docker` (or `podman`) must be installed on the SSH host.
- SSH must log in without a prompt, for example with a key from your SSH agent or an `IdentityFile` in `~/.ssh/config`. Zed runs `ssh` with `BatchMode=yes` and does not store passwords.
- On Linux and macOS, Zed's commands for the container engine share one SSH connection to the host, kept open for 5 minutes after the last one. Windows' OpenSSH doesn't support this, so there each command opens its own connection.
- `initializeCommand` runs on the SSH host, from your project folder.
- `${localEnv:VAR}` reads the environment of a login shell on the SSH host.
- While connected, Zed forwards numeric `forwardPorts` and `appPort` from the SSH host to the same ports on your machine.
- Zed copies the files it generates for builds (feature content, compose overrides) to a temporary folder on the host, and removes it after the build.

To open one, open your project over SSH, then run "Reopen in Dev Container" from the prompt or choose "Connect Dev Container" in the Remote Projects modal.

### Git worktrees

When the project folder is a worktree created with `git worktree add`, such as the worktrees that coding agents create, Zed also mounts the Git folder of the main working tree into the container, at the path the worktree's `.git` file names. Git then works in the container even for a worktree created inside another dev container, whose `.git` file names a container path like `/workspaces/project/.git`: Zed finds that folder in a parent folder of the worktree. Outside containers, such a worktree only works with Git once its links are relative, which Git 2.48 and later can do with `git worktree repair --relative-paths`.

## Editing the dev container configuration

Zed validates `devcontainer.json` and `devcontainer-feature.json` against the Dev Container specification's schemas, like VS Code, and completes their properties. The JSON language server downloads the schemas, so this needs a network connection.

If you modify `.devcontainer/devcontainer.json`, or the Dockerfile or Compose files it uses, Zed does not rebuild the container automatically. When you next open or reconnect to the container, Zed notices that the configuration changed since the container was created and offers to rebuild it. You can also run {#action projects::RebuildDevContainer} from the dev container window at any time: Zed removes the container, builds it again and reconnects.

## Managing a Dev Container {#managing}

From a dev container window, the command palette and the dev container menu in the title bar offer:

- {#action projects::ReconnectDevContainer}: reconnects, starting the container first if it stopped.
- {#action projects::RestartDevContainer}: stops and starts the container, which ends every process in it, then reconnects.
- {#action projects::RebuildDevContainer}: removes the container, builds it again and reconnects.
- {#action projects::RebuildDevContainerWithoutCache}: like rebuilding, but builds every image layer again instead of reusing the container engine's build cache, and pulls newer base images when building with BuildKit.
- {#action projects::StopDevContainer}: stops the container and reopens the project on its host.
- {#action projects::DeleteDevContainer}: stops and removes the container, then reopens the project on its host. Anything not stored in the project folder or a volume is lost.

Run {#action projects::ManageDevContainers} from any window to list the dev containers of the project's container engine, running or stopped, with their project folders, like VS Code's Remote Explorer. Confirm one to open its project in it, starting it if it stopped; the secondary confirm stops a running container, or removes a stopped one after asking.

These actions work the same for containers running locally, in WSL, or on an SSH host.

When you close the last window connected to a dev container, or quit Zed, Zed carries out the configuration's `shutdownAction`: `stopContainer` (the default for image and Dockerfile configurations) stops the container, `stopCompose` (the default for Docker Compose configurations) stops every container of the Compose project, and `none` keeps them running. `stopCompose` runs `docker compose --project-name <project> stop`, which needs Docker Compose v2, or a Podman compose provider that can stop a project by name.

## Working in a Dev Container

Once connected, Zed operates inside the container environment for tasks, terminals, and language servers.
Files are linked from your workspace into the container according to the dev container specification.

Tasks, terminals and language servers get the container's environment, then the variables set by the remote user's shell profile, then `remoteEnv`, each overriding the one before. Zed reads the profile's variables when it first connects after the container starts, by starting the user's login shell as `userEnvProbe` asks: `loginInteractiveShell` (the default) runs it with `-lic`, `interactiveShell` with `-ic`, `loginShell` with `-lc`, and `none` skips it.

If the configuration, or the metadata of the image it starts from, sets `hostRequirements`, Zed keeps the highest of each and compares `cpus` and `memory` with what the container engine reports before creating the container. When the engine has less, Zed still creates the container and shows a warning. `storage` isn't checked. When `gpu` is `true` (or an object of minimums), Zed gives the container the engine's GPUs, with `--gpus all` or a Compose GPU reservation; when it's `"optional"`, Zed does so only if Docker has the NVIDIA container runtime. The minimums of a `gpu` object aren't checked.

Zed connects once the container is created and the lifecycle command set by `waitFor` has run (`updateContentCommand` by default). The later lifecycle commands, such as `postCreateCommand`, `postStartCommand` and `postAttachCommand`, then run as tasks in the Terminal Panel, where you can follow their output. If one fails, the ones after it don't run.

When a program in the container starts listening on a TCP port, Zed forwards it to the same port on your machine, so `http://localhost:<port>` reaches it. The forwarded connections travel through Zed's connection to the container, whether its engine runs locally, in WSL or on an SSH host. When several windows are connected to one container, one of them forwards each port. Ports published with `forwardPorts` or `appPort` already reach your machine, so Zed leaves them alone. In a Docker Compose configuration, `forwardPorts` entries such as `"db:5432"` publish a port of another service; when the engine runs on an SSH host, Zed forwards them to your machine too. When another program on your machine already uses the port, Zed forwards it to the next free port and says which one, unless the port's `requireLocalPort` in `portsAttributes` is `true`: then Zed tells you the port isn't forwarded. Ports whose `protocol` is `https` open with `https://`.

Run {#action projects::ShowForwardedPorts} (also in the dev container menu of the title bar) to list the forwarded ports: confirm one to open it in your browser, or use the secondary confirm to stop forwarding it until you forward it again. To forward a port that isn't forwarded automatically, type its number and confirm.

When Zed starts forwarding a port, it follows the port's `onAutoForward` in `portsAttributes` (or `otherPortsAttributes`): `notify` (the default) shows a notification naming the port and its `label`, `openBrowser` and `openPreview` open `http://localhost:<port>` in your browser, `openBrowserOnce` does so only the first time the port is forwarded while Zed runs, and `silent` forwards it without telling you.

To keep a port from being forwarded, set its `onAutoForward` to `ignore` in `portsAttributes`. Keys can be a port or a range such as `"3000-3010"`. Set `otherPortsAttributes` to `{ "onAutoForward": "ignore" }` to forward only the ports listed in `portsAttributes`:

```json
{
  "portsAttributes": {
    "5432": { "onAutoForward": "ignore" }
  }
}
```

## Feature lockfile

Like VS Code, Zed keeps the features in `devcontainer-lock.json` (`.devcontainer-lock.json` for a `.devcontainer.json`), next to the configuration, when that file exists. Zed installs each feature listed there at its recorded digest, fails if the downloaded feature doesn't match its recorded `integrity`, and updates the file after resolving the features. To start using a lockfile, create an empty `devcontainer-lock.json`, or one with the Dev Container CLI, and commit it. Zed and VS Code read and write the same lockfile. When you change a feature's version in the configuration, Zed resolves it again and updates its entry; if a feature no longer matches its `integrity` and you trust the new content, remove its entry to update it.

Zed keeps the features it downloads in its data folder, under `devcontainer/features`, and reuses them in later builds instead of downloading them again. Delete that folder to clear the cache.

## Dotfiles

To bring your shell and tool configuration into every new dev container, set a dotfiles repository in your settings, like VS Code's `dotfiles.*` settings:

```json [settings]
{
  "dev_container_dotfiles_repository": "your-github-id/dotfiles",
  "dev_container_dotfiles_install_command": "install.sh",
  "dev_container_dotfiles_target_path": "~/dotfiles"
}
```

After creating a container and running its `onCreateCommand`, `updateContentCommand` and `postCreateCommand`, Zed clones the repository (a Git URL, or `owner/repository` on GitHub) into `dev_container_dotfiles_target_path` (`~/dotfiles` by default) in the container and runs `dev_container_dotfiles_install_command` there. Without an install command, Zed runs the first of `install.sh`, `install`, `bootstrap.sh`, `bootstrap`, `script/bootstrap`, `setup.sh`, `setup` and `script/setup` in the repository, or links the repository's dotfiles into your home folder if it has none. The container needs `git`, and access to the repository. If installing the dotfiles fails, the container is still used, and the error is in Zed's log.

## Secrets

To give dev containers tokens or passwords without writing them into the configuration, put them in a JSON file of variable names and values, like the Dev Container CLI's `--secrets-file`, and point Zed to it:

```json [settings]
{
  "dev_container_secrets_file": "~/.config/dev-container-secrets.json"
}
```

Lifecycle commands, terminals, tasks and the remote server in the container get these variables. Zed reads the file each time it starts or connects to a dev container and doesn't store the values; it passes them to `docker exec` through its environment rather than its arguments. When the container engine runs on an SSH host, the values are part of the command Zed runs there.

## SSH agent

Like VS Code, `ssh` and `git` in a dev container use the keys of your SSH agent, through `SSH_AUTH_SOCK` (`/tmp/zed-ssh-agent.sock`). When the container engine runs on your Linux machine, in WSL, or in Docker Desktop for macOS, Zed mounts the agent into the container. Otherwise, such as on an SSH host, on another machine's engine or with Docker Desktop for Windows, the remote server relays the agent's requests to your machine through Zed's connection. On Windows, Zed uses the agent of Windows' OpenSSH (the "OpenSSH Authentication Agent" service), unless `SSH_AUTH_SOCK` names another pipe.

## GnuPG agent

To sign commits in a dev container with your GnuPG keys, like VS Code, install GnuPG in the container (for example with the `ghcr.io/devcontainers/features/common-utils` feature, or `gnupg2` in your image). When the container's GnuPG has no agent of its own, the remote server relays its agent socket to the agent on your machine, through its restricted "extra" socket (`gpgconf --list-dirs agent-extra-socket`). On Windows, this is the agent of Gpg4win. Your private keys stay on your machine; import your public key in the container for `gpg --list-keys` to show it.

## Git credentials

Like VS Code, Zed lets git in a dev container use the git credentials of your machine: when git in the container needs a password or token, for example to push over HTTPS, your machine's `git credential fill` answers it through Zed's connection. Zed sets this up by making its helper git's `credential.helper` in the container, unless one is already configured there. Git on your machine doesn't prompt in a terminal for this, but a credential manager that shows a window, such as Git Credential Manager, can.

## Remote server

Zed runs its remote server in the container. Containers created by Zed mount a `zed-remote-server` volume at `/zed-remote-server`, where Zed keeps a copy of the server, so other containers on the same engine reuse it instead of downloading it again. Remove the volume with `docker volume rm zed-remote-server` to clear it; containers created before this don't have the volume and download the server themselves.

## Extensions

You can specify extensions in `.devcontainer/devcontainer.json` under the "customizations" field like so:

```json
{
  ...
  "customizations": {
    "zed": {
      "extensions": ["vue", "ruby"],
    },
    "vscode": {
      ...
    },
    "codespaces": {
      ...
    },
  }
}
```

Note that extensions load for the Zed session, so these extensions will exist on your local Zed instances as well.

## Settings

Zed settings under `customizations.zed.settings` apply to the remote server in the container, like VS Code's `customizations.vscode.settings`. When Zed creates the container, it writes them into the server's settings file (`~/.config/zed/settings.json` in the container, which {#action zed::OpenServerSettings} opens), over the settings already there. Settings from the metadata of the image and of features apply too, and the configuration's win:

```json
{
  "customizations": {
    "zed": {
      "settings": {
        "tab_size": 2
      }
    }
  }
}
```

## Troubleshooting {#troubleshooting}

- **Seeing what went wrong:** Zed records the commands it runs to create or start a dev container, such as the image build and the lifecycle commands before it connects, with their output. When starting fails, choose "Show Log" in the error, or run {#action projects::ShowDevContainerLog} at any time; in a dev container window, the container's own log (`docker logs`) follows. Each dev container has its own `dev_container-<id>.log` file in Zed's logs folder, and values of environment variables are left out.

- **Podman on Windows fails with "controller `pids` is not available":** WSL 2.9 doesn't enable the `pids` cgroup controller, which Podman's default process limit needs. Add this to `%APPDATA%\containers\containers.conf`, then retry:

  ```toml
  [containers]
  pids_limit = 0
  ```

## Known Limitations

> **Note:** This feature is still in development.

- **Configuration changes:** Updates to `devcontainer.json` do not trigger an automatic rebuild; Zed offers one when you next open or reconnect to the container.
- **Remote projects:** Dev containers open from local, WSL, and SSH projects. SSH hosts running Windows are not supported.
- **SSH hosts:** The login shell on the host must be POSIX-compatible (such as `bash`, `zsh`, or `sh`).
- **Ports on SSH hosts:** If a port that Zed forwards from an SSH host is already in use on your machine, that port is not forwarded.

## See also

- [Remote Development](./remote-development.md) for connecting to remote servers over SSH.
- [Tasks](./tasks.md) for running commands in the integrated terminal.
