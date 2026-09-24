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

If you modify `.devcontainer/devcontainer.json`, Zed does not rebuild or reload the container automatically. After changing configuration, run {#action projects::RebuildDevContainer} from the dev container window: Zed removes the container, builds it again and reconnects.

## Managing a Dev Container {#managing}

From a dev container window, the command palette and the dev container menu in the title bar offer:

- {#action projects::ReconnectDevContainer}: reconnects, starting the container first if it stopped.
- {#action projects::RestartDevContainer}: stops and starts the container, which ends every process in it, then reconnects.
- {#action projects::RebuildDevContainer}: removes the container, builds it again and reconnects.
- {#action projects::StopDevContainer}: stops the container and reopens the project on its host.
- {#action projects::DeleteDevContainer}: stops and removes the container, then reopens the project on its host. Anything not stored in the project folder or a volume is lost.

These actions work the same for containers running locally, in WSL, or on an SSH host.

## Working in a Dev Container

Once connected, Zed operates inside the container environment for tasks, terminals, and language servers.
Files are linked from your workspace into the container according to the dev container specification.

Zed connects once the container is created and the lifecycle command set by `waitFor` has run (`updateContentCommand` by default). The later lifecycle commands, such as `postCreateCommand`, `postStartCommand` and `postAttachCommand`, then run as tasks in the Terminal Panel, where you can follow their output. If one fails, the ones after it don't run.

When a program in the container starts listening on a TCP port, Zed forwards it to the same port on your machine, so `http://localhost:<port>` reaches it. Zed skips ports that are already in use on your machine, such as those published with `forwardPorts` or `appPort`.

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

- **Configuration changes:** Updates to `devcontainer.json` do not trigger an automatic rebuild; run Rebuild Dev Container.
- **Remote projects:** Dev containers open from local, WSL, and SSH projects. SSH hosts running Windows are not supported.
- **SSH hosts:** The login shell on the host must be POSIX-compatible (such as `bash`, `zsh`, or `sh`).
- **Ports on SSH hosts:** If a port that Zed forwards from an SSH host is already in use on your machine, that port is not forwarded.

## See also

- [Remote Development](./remote-development.md) for connecting to remote servers over SSH.
- [Tasks](./tasks.md) for running commands in the integrated terminal.
