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
- `initializeCommand` runs on the SSH host, from your project folder.
- `${localEnv:VAR}` reads the environment of a login shell on the SSH host.
- While connected, Zed forwards numeric `forwardPorts` and `appPort` from the SSH host to the same ports on your machine.
- Zed copies the files it generates for builds (feature content, compose overrides) to a temporary folder on the host, and removes it after the build.

To open one, open your project over SSH, then run "Reopen in Dev Container" from the prompt or choose "Connect Dev Container" in the Remote Projects modal.

## Editing the dev container configuration

If you modify `.devcontainer/devcontainer.json`, or the Dockerfile or Compose files it uses, Zed does not rebuild the container automatically. When you next open or reconnect to the container, Zed notices that the configuration changed since the container was created and offers to rebuild it. You can also run {#action projects::RebuildDevContainer} from the dev container window at any time: Zed removes the container, builds it again and reconnects.

## Managing a Dev Container {#managing}

From a dev container window, the command palette and the dev container menu in the title bar offer:

- {#action projects::ReconnectDevContainer}: reconnects, starting the container first if it stopped.
- {#action projects::RestartDevContainer}: stops and starts the container, which ends every process in it, then reconnects.
- {#action projects::RebuildDevContainer}: removes the container, builds it again and reconnects.
- {#action projects::RebuildDevContainerWithoutCache}: like rebuilding, but builds every image layer again instead of reusing the container engine's build cache, and pulls newer base images when building with BuildKit.
- {#action projects::StopDevContainer}: stops the container and reopens the project on its host.
- {#action projects::DeleteDevContainer}: stops and removes the container, then reopens the project on its host. Anything not stored in the project folder or a volume is lost.

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

Like VS Code, Zed keeps the features in `devcontainer-lock.json` (`.devcontainer-lock.json` for a `.devcontainer.json`), next to the configuration, when that file exists. Zed installs each feature listed there at its recorded digest, fails if the downloaded feature doesn't match its recorded `integrity`, and updates the file after resolving the features. To start using a lockfile, create an empty `devcontainer-lock.json`, or one with the Dev Container CLI, and commit it.

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

## Troubleshooting {#troubleshooting}

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
