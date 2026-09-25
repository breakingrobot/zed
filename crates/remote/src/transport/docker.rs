use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use async_trait::async_trait;
use collections::HashMap;
use parking_lot::Mutex;
use release_channel::{AppCommitSha, AppVersion, ReleaseChannel};
use semver::Version as SemanticVersion;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::time::Instant;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use util::ResultExt;
use util::command::Stdio;
use util::redact::{is_valid_environment_name, redact_command};
use util::shell::ShellKind;
use util::{
    paths::{PathStyle, RemotePathBuf},
    rel_path::RelPath,
};

use futures::channel::mpsc::{Sender, UnboundedReceiver, UnboundedSender};
use gpui::{App, AppContext, AsyncApp, Task};
use rpc::proto::Envelope;

use crate::{
    EngineHost, HostCommand, RemoteArch, RemoteClientDelegate, RemoteConnection,
    RemoteConnectionOptions, RemoteOs, RemotePlatform,
    remote_client::{CommandTemplate, Interactive},
    transport::parse_platform,
};

#[derive(
    Debug,
    Default,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct DockerConnectionOptions {
    pub name: String,
    pub container_id: String,
    pub remote_user: String,
    /// The dev container's host project folder (the `devcontainer.local_folder`
    /// label). Together with `config_file` this forms the connection's stable
    /// identity across container rebuilds, whose `container_id` is ephemeral.
    /// `None` for a hypothetical Docker remote without dev-container labels, in
    /// which case the identity falls back to `container_id`.
    #[serde(default)]
    pub local_folder: Option<String>,
    /// The dev container's config file on the host (the `devcontainer.config_file`
    /// label). See `local_folder`.
    #[serde(default)]
    pub config_file: Option<String>,
    pub upload_binary_over_docker_exec: bool,
    pub use_podman: bool,
    pub remote_env: BTreeMap<String, String>,
    /// The machine where the container engine runs. Connections saved before
    /// this existed ran the engine locally.
    #[serde(default)]
    pub host: EngineHost,
    /// Ports the container publishes on `host`, forwarded to this machine while
    /// connected when `host` is reached over SSH.
    #[serde(default)]
    pub forward_ports: Vec<u16>,
    /// Which ports that start listening in the container are forwarded automatically.
    #[serde(default)]
    pub auto_forward: AutoForwardPorts,
    /// What happens to the container once no window is connected to it.
    #[serde(default)]
    pub shutdown_action: ShutdownAction,
}

/// What happens to a dev container once no window is connected to it, from its
/// `shutdownAction`.
#[derive(
    Debug,
    Default,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum ShutdownAction {
    /// Keep it running.
    #[default]
    None,
    /// Stop the container.
    StopContainer,
    /// Stop every container of the Compose project.
    StopCompose { project: String },
}

/// Which of the ports that start listening in a dev container are forwarded
/// automatically, from its `portsAttributes` and `otherPortsAttributes`.
#[derive(
    Debug,
    Default,
    Clone,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
)]
pub struct AutoForwardPorts {
    /// Port ranges given an explicit `onAutoForward`. The first one that contains a
    /// port decides for it.
    #[serde(default)]
    pub rules: Vec<AutoForwardRule>,
    /// Whether the ports that no rule covers are left alone.
    #[serde(default)]
    pub ignore_other_ports: bool,
    /// How the user learns that a port no rule covers is forwarded.
    #[serde(default)]
    pub other_ports_notice: ForwardNotice,
}

#[derive(
    Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize,
)]
pub struct AutoForwardRule {
    pub start: u16,
    pub end: u16,
    pub forward: bool,
    /// The name shown for ports in the range.
    #[serde(default)]
    pub label: Option<String>,
    /// How the user learns that a port in the range is forwarded.
    #[serde(default)]
    pub notice: ForwardNotice,
    /// Whether ports in the range are only forwarded to the same port here, from
    /// `requireLocalPort`.
    #[serde(default)]
    pub require_local_port: bool,
    /// Whether ports in the range serve HTTPS, from `protocol`.
    #[serde(default)]
    pub https: bool,
}

/// What happens when a port starts being forwarded, from `onAutoForward`.
#[derive(
    Debug,
    Default,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    serde::Serialize,
    serde::Deserialize,
)]
pub enum ForwardNotice {
    /// Tell the user which port is forwarded.
    #[default]
    Notify,
    /// Open the forwarded port in the browser.
    OpenBrowser,
    /// Open the forwarded port in the browser the first time it's forwarded.
    OpenBrowserOnce,
    /// Forward without telling the user.
    Silent,
}

/// A port of a dev container that started being forwarded to this machine, or
/// couldn't be.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForwardedPort {
    pub container_id: String,
    pub port: u16,
    /// The port on this machine that reaches it, or `None` when `port` is taken
    /// here and its `requireLocalPort` forbids using another one.
    pub local_port: Option<u16>,
    pub label: Option<String>,
    pub notice: ForwardNotice,
    pub https: bool,
}

/// Where dev container connections report the ports they start forwarding, so the
/// UI can tell the user about them.
pub struct ForwardedPortListener(pub UnboundedSender<ForwardedPort>);

impl gpui::Global for ForwardedPortListener {}

impl AutoForwardPorts {
    pub fn forwards(&self, port: u16) -> bool {
        self.forwarding(port).is_some()
    }

    fn rule(&self, port: u16) -> Option<&AutoForwardRule> {
        self.rules
            .iter()
            .find(|rule| (rule.start..=rule.end).contains(&port))
    }

    /// Whether `port` may only be forwarded to the same port on this machine.
    pub fn requires_local_port(&self, port: u16) -> bool {
        self.rule(port).is_some_and(|rule| rule.require_local_port)
    }

    /// Whether `port` serves HTTPS.
    pub fn uses_https(&self, port: u16) -> bool {
        self.rule(port).is_some_and(|rule| rule.https)
    }

    /// How `port` is announced once forwarded, with its label, or `None` if it
    /// isn't forwarded.
    pub fn forwarding(&self, port: u16) -> Option<(Option<&str>, ForwardNotice)> {
        match self.rule(port) {
            Some(rule) => rule.forward.then(|| (rule.label.as_deref(), rule.notice)),
            None => (!self.ignore_other_ports).then_some((None, self.other_ports_notice)),
        }
    }
}

impl DockerConnectionOptions {
    /// The `devcontainer.local_folder` and `devcontainer.config_file` labels this
    /// container was created from, when both are known.
    pub fn dev_container_labels(&self) -> Option<(&str, &str)> {
        match (self.local_folder.as_deref(), self.config_file.as_deref()) {
            (Some(local_folder), Some(config_file))
                if !local_folder.is_empty() && !config_file.is_empty() =>
            {
                Some((local_folder, config_file))
            }
            _ => None,
        }
    }
}

pub(crate) struct DockerExecConnection {
    proxy_process: Mutex<Option<u32>>,
    /// Forwards the ports that start listening in the container while connected.
    port_forwarding: Mutex<Option<Task<()>>>,
    remote_dir_for_server: String,
    remote_binary_relpath: Option<Arc<RelPath>>,
    connection_options: DockerConnectionOptions,
    remote_platform: Option<RemotePlatform>,
    os_version: Option<String>,
    path_style: Option<PathStyle>,
    shell: String,
    /// See [`EngineHost::engine_environment`].
    engine_environment: Vec<(String, String)>,
}

impl DockerExecConnection {
    pub async fn new(
        connection_options: DockerConnectionOptions,
        delegate: Arc<dyn RemoteClientDelegate>,
        cx: &mut AsyncApp,
    ) -> Result<Self> {
        let mut this = Self {
            proxy_process: Mutex::new(None),
            port_forwarding: Mutex::new(None),
            remote_dir_for_server: "/".to_string(),
            remote_binary_relpath: None,
            connection_options,
            remote_platform: None,
            os_version: None,
            path_style: None,
            shell: "sh".to_owned(),
            engine_environment: Vec::new(),
        };
        this.engine_environment = this.connection_options.host.engine_environment().await;
        let (release_channel, version, commit) = cx.update(|cx| {
            (
                ReleaseChannel::global(cx),
                AppVersion::global(cx),
                AppCommitSha::try_global(cx),
            )
        });
        let remote_platform = this.check_remote_platform().await?;

        this.path_style = match remote_platform.os {
            RemoteOs::Windows => Some(PathStyle::Windows),
            _ => Some(PathStyle::Unix),
        };

        this.remote_platform = Some(remote_platform);
        log::info!("Remote platform discovered: {:?}", this.remote_platform);

        this.os_version = this.discover_os_version(remote_platform.os).await;
        log::info!("Remote OS version discovered: {:?}", this.os_version);

        this.shell = this.discover_shell().await;
        log::info!("Remote shell discovered: {}", this.shell);

        this.remote_dir_for_server = this.docker_user_home_dir().await?.trim().to_string();

        this.remote_binary_relpath = Some(
            this.ensure_server_binary(
                &delegate,
                release_channel,
                version,
                &this.remote_dir_for_server,
                commit,
                cx,
            )
            .await?,
        );

        Ok(this)
    }

    fn docker_cli(&self) -> &str {
        if self.connection_options.use_podman {
            "podman"
        } else {
            "docker"
        }
    }

    /// Builds a docker CLI command that runs on the engine host.
    fn docker_command(&self, args: &[impl AsRef<str>]) -> util::command::Command {
        docker_command(
            &self.connection_options,
            self.docker_cli(),
            &self.engine_environment,
            args,
        )
    }

    /// Run a shell command inside the container and reliably extract its output
    /// using unique delimiters, so that shell initialization noise (e.g. from
    /// BASH_ENV or .bashrc) does not corrupt the result.
    async fn run_docker_exec_delimited(&self, script: &str) -> Result<String> {
        const MARKER: &str = "=====ZED_DELIM_7f3a9c=====";
        let wrapped =
            format!("printf '{MARKER}'; {script}; __exit=$?; printf '{MARKER}'; exit $__exit");
        let output = self
            .run_docker_exec("sh", None, &Default::default(), &["-c", &wrapped])
            .await?;
        let start = output.find(MARKER).map(|i| i + MARKER.len()).unwrap_or(0);
        let end = output[start..]
            .find(MARKER)
            .map(|i| start + i)
            .unwrap_or(output.len());
        Ok(output[start..end].to_string())
    }

    async fn discover_shell(&self) -> String {
        let default_shell = "sh";
        match self.run_docker_exec_delimited("echo $SHELL").await {
            Ok(shell) => match shell.trim() {
                "" => {
                    log::info!("$SHELL is not set, checking passwd for user");
                }
                shell => {
                    return shell.to_owned();
                }
            },
            Err(e) => {
                log::error!("Failed to get $SHELL: {e}. Checking passwd for user");
            }
        }

        match self
            .run_docker_exec_delimited("getent passwd \"$(id -un)\" | cut -d: -f7")
            .await
        {
            Ok(shell) => match shell.trim() {
                "" => {
                    log::info!("No shell found in passwd, falling back to {default_shell}");
                }
                shell => {
                    return shell.to_owned();
                }
            },
            Err(e) => {
                log::info!("Error getting shell from passwd: {e}. Falling back to {default_shell}");
            }
        }
        default_shell.to_owned()
    }

    async fn check_remote_platform(&self) -> Result<RemotePlatform> {
        let uname = self.run_docker_exec_delimited("uname -sm").await?;
        parse_platform(&uname)
    }

    /// Best-effort detection of the container's OS version for telemetry.
    async fn discover_os_version(&self, os: RemoteOs) -> Option<String> {
        let (program, args) = super::os_version_command(os);
        match self
            .run_docker_exec(program, None, &Default::default(), args)
            .await
        {
            Ok(output) => super::parse_os_version(os, &output),
            Err(error) => {
                log::warn!("Failed to determine remote OS version: {error:#}");
                None
            }
        }
    }

    async fn ensure_server_binary(
        &self,
        delegate: &Arc<dyn RemoteClientDelegate>,
        release_channel: ReleaseChannel,
        version: SemanticVersion,
        remote_dir_for_server: &str,
        commit: Option<AppCommitSha>,
        cx: &mut AsyncApp,
    ) -> Result<Arc<RelPath>> {
        let remote_platform = self
            .remote_platform
            .context("No remote platform defined; cannot proceed.")?;

        let version_str = match release_channel {
            ReleaseChannel::Nightly => {
                let commit = commit.map(|s| s.full()).unwrap_or_default();
                format!("{}-{}", version, commit)
            }
            ReleaseChannel::Dev => "build".to_string(),
            _ => version.to_string(),
        };
        let binary_name = format!(
            "zed-remote-server-{}-{}",
            release_channel.dev_name(),
            version_str
        );
        let dst_path =
            paths::remote_server_dir_relative().join(RelPath::from_unix_str(&binary_name).unwrap());

        let binary_exists_on_server = self
            .run_docker_exec(
                &dst_path.display(self.path_style()),
                Some(&remote_dir_for_server),
                &Default::default(),
                &["version"],
            )
            .await
            .is_ok();
        #[cfg(any(debug_assertions, feature = "build-remote-server-binary"))]
        if let Some(remote_server_path) = super::build_remote_server_from_source(
            &remote_platform,
            delegate.as_ref(),
            binary_exists_on_server,
            cx,
        )
        .await?
        {
            let tmp_path = paths::remote_server_dir_relative().join(
                RelPath::from_unix_str(&format!(
                    "download-{}-{}",
                    std::process::id(),
                    remote_server_path.file_name().unwrap().to_string_lossy()
                ))
                .unwrap(),
            );
            self.upload_local_server_binary(
                &remote_server_path,
                &tmp_path,
                &remote_dir_for_server,
                delegate,
                cx,
            )
            .await?;
            self.extract_server_binary(&dst_path, &tmp_path, &remote_dir_for_server, delegate, cx)
                .await?;
            return Ok(dst_path.into());
        }

        if binary_exists_on_server {
            return Ok(dst_path.into());
        }

        let cached_binary = format!(
            "{SERVER_CACHE_PATH}/{binary_name}-{}-{}",
            remote_platform.os.as_str(),
            remote_platform.arch.as_str()
        );
        let installed_binary = format!(
            "{remote_dir_for_server}/{}",
            dst_path.display(self.path_style())
        );
        // Development builds all share one name, so a cached one may be stale.
        let use_cache = release_channel != ReleaseChannel::Dev;
        if use_cache
            && self
                .run_as_root(
                    RESTORE_CACHED_SERVER_SCRIPT,
                    &[&cached_binary, &installed_binary],
                )
                .await
                .is_ok()
        {
            log::info!("Reused the remote server from the {SERVER_CACHE_VOLUME} volume");
            return Ok(dst_path.into());
        }

        let wanted_version = cx.update(|cx| match release_channel {
            ReleaseChannel::Nightly => Ok(None),
            ReleaseChannel::Dev => {
                anyhow::bail!(
                    "ZED_BUILD_REMOTE_SERVER is not set and no remote server exists at ({:?})",
                    dst_path
                )
            }
            _ => Ok(Some(AppVersion::global(cx))),
        })?;

        let tmp_path_gz = paths::remote_server_dir_relative().join(
            RelPath::from_unix_str(&format!(
                "{}-download-{}.gz",
                binary_name,
                std::process::id()
            ))
            .unwrap(),
        );
        if !self.connection_options.upload_binary_over_docker_exec
            && let Some(url) = delegate
                .get_download_url(remote_platform, release_channel, wanted_version.clone(), cx)
                .await?
        {
            match self
                .download_binary_on_server(&url, &tmp_path_gz, &remote_dir_for_server, delegate, cx)
                .await
            {
                Ok(_) => {
                    self.extract_server_binary(
                        &dst_path,
                        &tmp_path_gz,
                        &remote_dir_for_server,
                        delegate,
                        cx,
                    )
                    .await
                    .context("extracting server binary")?;
                    if use_cache {
                        self.cache_server_binary(&installed_binary, &cached_binary)
                            .await;
                    }
                    return Ok(dst_path.into());
                }
                Err(e) => {
                    log::error!(
                        "Failed to download binary on server, attempting to download locally and then upload it the server: {e:#}",
                    )
                }
            }
        }

        let src_path = delegate
            .download_server_binary_locally(remote_platform, release_channel, wanted_version, cx)
            .await
            .context("downloading server binary locally")?;
        self.upload_local_server_binary(
            &src_path,
            &tmp_path_gz,
            &remote_dir_for_server,
            delegate,
            cx,
        )
        .await
        .context("uploading server binary")?;
        self.extract_server_binary(
            &dst_path,
            &tmp_path_gz,
            &remote_dir_for_server,
            delegate,
            cx,
        )
        .await
        .context("extracting server binary")?;
        if use_cache {
            self.cache_server_binary(&installed_binary, &cached_binary)
                .await;
        }
        Ok(dst_path.into())
    }

    /// Runs a POSIX `script` in the container as root, which owns the server cache
    /// volume, with `args` as its `$1`, `$2`, …
    async fn run_as_root(&self, script: &str, args: &[&str]) -> Result<String> {
        let mut exec_args = vec![
            "-u".to_string(),
            "root".to_string(),
            self.connection_options.container_id.clone(),
            "sh".to_string(),
            "-c".to_string(),
            script.to_string(),
            "sh".to_string(),
        ];
        exec_args.extend(args.iter().map(|arg| arg.to_string()));
        self.run_docker_command("exec", &exec_args).await
    }

    /// Keeps a copy of the server in the cache volume, when the container has it,
    /// for the other containers of the engine.
    async fn cache_server_binary(&self, installed_binary: &str, cached_binary: &str) {
        if let Err(error) = self
            .run_as_root(CACHE_SERVER_SCRIPT, &[installed_binary, cached_binary])
            .await
        {
            log::debug!("Didn't cache the remote server: {error:#}");
        }
    }

    async fn docker_user_home_dir(&self) -> Result<String> {
        self.run_docker_exec_delimited("echo $HOME").await
    }

    async fn extract_server_binary(
        &self,
        dst_path: &RelPath,
        tmp_path: &RelPath,
        remote_dir_for_server: &str,
        delegate: &Arc<dyn RemoteClientDelegate>,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        delegate.set_status(Some("Extracting remote development server"), cx);
        let server_mode = 0o755;

        let shell_kind = ShellKind::Posix;
        let orig_tmp_path = tmp_path.display(self.path_style());
        let server_mode = format!("{:o}", server_mode);
        let server_mode = shell_kind
            .try_quote(&server_mode)
            .context("shell quoting")?;
        let dst_path = dst_path.display(self.path_style());
        let dst_path = shell_kind.try_quote(&dst_path).context("shell quoting")?;
        let script = if let Some(tmp_path) = orig_tmp_path.strip_suffix(".gz") {
            let orig_tmp_path = shell_kind
                .try_quote(&orig_tmp_path)
                .context("shell quoting")?;
            let tmp_path = shell_kind.try_quote(&tmp_path).context("shell quoting")?;
            format!(
                "gunzip -f {orig_tmp_path} && chmod {server_mode} {tmp_path} && mv {tmp_path} {dst_path}",
            )
        } else {
            let orig_tmp_path = shell_kind
                .try_quote(&orig_tmp_path)
                .context("shell quoting")?;
            format!("chmod {server_mode} {orig_tmp_path} && mv {orig_tmp_path} {dst_path}",)
        };
        let args = shell_kind.args_for_shell(false, script.to_string());
        self.run_docker_exec(
            "sh",
            Some(&remote_dir_for_server),
            &Default::default(),
            &args,
        )
        .await
        .log_err();
        Ok(())
    }

    async fn upload_local_server_binary(
        &self,
        src_path: &Path,
        tmp_path_gz: &RelPath,
        remote_dir_for_server: &str,
        delegate: &Arc<dyn RemoteClientDelegate>,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        if let Some(parent) = tmp_path_gz.parent() {
            self.run_docker_exec(
                "mkdir",
                Some(remote_dir_for_server),
                &Default::default(),
                &["-p", parent.display(self.path_style()).as_ref()],
            )
            .await?;
        }

        let src_stat = smol::fs::metadata(&src_path).await?;
        let size = src_stat.len();

        let t0 = Instant::now();
        delegate.set_status(Some("Uploading remote development server"), cx);
        log::info!(
            "uploading remote development server to {:?} ({}kb)",
            tmp_path_gz,
            size / 1024
        );
        self.upload_file(src_path, tmp_path_gz, remote_dir_for_server)
            .await
            .context("failed to upload server binary")?;
        log::info!("uploaded remote development server in {:?}", t0.elapsed());
        Ok(())
    }

    async fn upload_and_chown(
        docker_cli: String,
        connection_options: DockerConnectionOptions,
        engine_environment: Vec<(String, String)>,
        src_path: String,
        dst_path: String,
    ) -> Result<()> {
        let env = &engine_environment;
        if !connection_options.host.has_local_files() {
            Self::stream_into_container(
                &docker_cli,
                &connection_options,
                env,
                &src_path,
                &dst_path,
            )
            .await?;
        } else {
            Self::copy_into_container(&docker_cli, &connection_options, env, &src_path, &dst_path)
                .await?;
        }
        Self::chown(&docker_cli, &connection_options, env, &dst_path).await
    }

    /// `docker cp` reads the source on the engine host, which must see our files.
    async fn copy_into_container(
        docker_cli: &str,
        connection_options: &DockerConnectionOptions,
        engine_environment: &[(String, String)],
        src_path: &str,
        dst_path: &str,
    ) -> Result<()> {
        let host_src_path = connection_options.host.host_path(Path::new(src_path));
        let mut command = docker_command(
            connection_options,
            docker_cli,
            engine_environment,
            &[
                "cp".to_string(),
                "-a".to_string(),
                host_src_path,
                format!("{}:{}", connection_options.container_id, dst_path),
            ],
        );
        command.kill_on_drop(true);

        let output = command.output().await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            log::debug!("failed to upload via docker cp {src_path} -> {dst_path}: {stderr}",);
            anyhow::bail!(
                "failed to upload via docker cp {} -> {}: {}",
                src_path,
                dst_path,
                stderr,
            );
        }
        Ok(())
    }

    /// Sends a file or folder of this machine into the container through
    /// `docker exec`'s standard input, for engine hosts that can't read our files.
    async fn stream_into_container(
        docker_cli: &str,
        connection_options: &DockerConnectionOptions,
        engine_environment: &[(String, String)],
        src_path: &str,
        dst_path: &str,
    ) -> Result<()> {
        let src = Path::new(src_path);
        let (script, input) = if smol::fs::metadata(src).await?.is_dir() {
            let mut archive = async_tar::Builder::new(Vec::new());
            archive.append_dir_all(".", src).await?;
            (
                r#"mkdir -p "$1" && tar -xf - -C "$1""#,
                archive.into_inner().await?,
            )
        } else {
            (
                r#"mkdir -p "$(dirname "$1")" && cat > "$1""#,
                smol::fs::read(src).await?,
            )
        };
        let mut command = engine_command(connection_options, docker_cli, engine_environment);
        command.args([
            "exec",
            "-i",
            &connection_options.container_id,
            "sh",
            "-c",
            script,
            "sh",
            dst_path,
        ]);
        let output = command.output_with_stdin(&input).await?;
        if !output.status.success() {
            anyhow::bail!(
                "failed to upload {src_path} -> {dst_path}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        Ok(())
    }

    async fn chown(
        docker_cli: &str,
        connection_options: &DockerConnectionOptions,
        engine_environment: &[(String, String)],
        dst_path: &str,
    ) -> Result<()> {
        let mut chown_command = docker_command(
            connection_options,
            docker_cli,
            engine_environment,
            &[
                "exec".to_string(),
                connection_options.container_id.clone(),
                "chown".to_string(),
                format!(
                    "{}:{}",
                    connection_options.remote_user, connection_options.remote_user,
                ),
                dst_path.to_string(),
            ],
        );
        chown_command.kill_on_drop(true);

        let output = chown_command.output().await?;

        if output.status.success() {
            return Ok(());
        }

        let stderr = String::from_utf8_lossy(&output.stderr);
        log::debug!("failed to change ownership for via chown: {stderr}",);
        anyhow::bail!(
            "failed to change ownership for zed_remote_server via chown: {}",
            stderr,
        );
    }

    async fn upload_file(
        &self,
        src_path: &Path,
        dest_path: &RelPath,
        remote_dir_for_server: &str,
    ) -> Result<()> {
        log::debug!("uploading file {:?} to {:?}", src_path, dest_path);

        let src_path_display = src_path.display().to_string();
        let dest_path_str = dest_path.display(self.path_style());
        let full_server_path = format!("{}/{}", remote_dir_for_server, dest_path_str);

        Self::upload_and_chown(
            self.docker_cli().to_string(),
            self.connection_options.clone(),
            self.engine_environment.clone(),
            src_path_display,
            full_server_path,
        )
        .await
    }

    async fn run_docker_command(
        &self,
        subcommand: &str,
        args: &[impl AsRef<str>],
    ) -> Result<String> {
        let mut all_args = vec![subcommand.to_string()];
        all_args.extend(args.iter().map(|arg| arg.as_ref().to_string()));
        let output = self.docker_command(&all_args).output().await?;
        log::debug!(
            "{}: {:?}",
            redact_arguments(self.docker_cli(), subcommand, args),
            output.status
        );
        if !output.status.success() {
            anyhow::bail!(docker_command_error(
                self.docker_cli(),
                subcommand,
                args,
                &String::from_utf8_lossy(&output.stderr),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    fn docker_exec_arguments(
        &self,
        inner_program: &str,
        working_directory: Option<&str>,
        env: &HashMap<String, String>,
        program_args: &[impl AsRef<str>],
    ) -> Vec<String> {
        let mut args = match working_directory {
            Some(dir) => vec!["-w".to_string(), dir.to_string()],
            None => vec![],
        };

        args.push("-u".to_string());
        args.push(self.connection_options.remote_user.clone());

        push_environment(&mut args, &self.connection_options.remote_env);
        push_environment(&mut args, env);

        args.push(self.connection_options.container_id.clone());
        args.push(inner_program.to_string());

        for arg in program_args {
            args.push(arg.as_ref().to_owned());
        }
        args
    }

    async fn run_docker_exec(
        &self,
        inner_program: &str,
        working_directory: Option<&str>,
        env: &HashMap<String, String>,
        program_args: &[impl AsRef<str>],
    ) -> Result<String> {
        let args = self.docker_exec_arguments(inner_program, working_directory, env, program_args);
        self.run_docker_command("exec", args.as_ref()).await
    }

    async fn download_binary_on_server(
        &self,
        url: &str,
        tmp_path_gz: &RelPath,
        remote_dir_for_server: &str,
        delegate: &Arc<dyn RemoteClientDelegate>,
        cx: &mut AsyncApp,
    ) -> Result<()> {
        if let Some(parent) = tmp_path_gz.parent() {
            self.run_docker_exec(
                "mkdir",
                Some(remote_dir_for_server),
                &Default::default(),
                &["-p", parent.display(self.path_style()).as_ref()],
            )
            .await?;
        }

        delegate.set_status(Some("Downloading remote development server on host"), cx);

        match self
            .run_docker_exec(
                "curl",
                Some(remote_dir_for_server),
                &Default::default(),
                &[
                    "-f",
                    "-L",
                    url,
                    "-o",
                    &tmp_path_gz.display(self.path_style()),
                ],
            )
            .await
        {
            Ok(_) => {}
            Err(e) => {
                if self
                    .run_docker_exec("which", None, &Default::default(), &["curl"])
                    .await
                    .is_ok()
                {
                    return Err(e);
                }

                log::info!("curl is not available, trying wget");
                match self
                    .run_docker_exec(
                        "wget",
                        Some(remote_dir_for_server),
                        &Default::default(),
                        &[url, "-O", &tmp_path_gz.display(self.path_style())],
                    )
                    .await
                {
                    Ok(_) => {}
                    Err(e) => {
                        if self
                            .run_docker_exec("which", None, &Default::default(), &["wget"])
                            .await
                            .is_ok()
                        {
                            return Err(e);
                        } else {
                            anyhow::bail!("Neither curl nor wget is available");
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn kill_inner(&self) -> Result<()> {
        self.port_forwarding.lock().take();
        if let Some(pid) = self.proxy_process.lock().take() {
            if let Ok(_) = kill_process_command(pid).spawn() {
                Ok(())
            } else {
                Err(anyhow::anyhow!("Failed to kill process"))
            }
        } else {
            Ok(())
        }
    }
}

/// The volume that keeps the remote server binaries, mounted in the dev containers
/// Zed creates so that they share one download, like VS Code's `vscode` volume.
pub const SERVER_CACHE_VOLUME: &str = "zed-remote-server";
/// Where [`SERVER_CACHE_VOLUME`] is mounted in dev containers.
pub const SERVER_CACHE_PATH: &str = "/zed-remote-server";

/// Copies `$2`, a server installed in the container, into the cache volume as `$1`
/// without leaving a partial copy behind.
const CACHE_SERVER_SCRIPT: &str =
    r#"[ -d "$(dirname "$2")" ] || exit 0; cp "$1" "$2.partial" && mv -f "$2.partial" "$2""#;

/// Installs `$1` from the cache volume as `$2`, owned by the container's user, or
/// fails when the cache doesn't have it.
const RESTORE_CACHED_SERVER_SCRIPT: &str = r#"[ -x "$1" ] || exit 1
owner="$(stat -c %u:%g "$(dirname "$(dirname "$2")")")"
mkdir -p "$(dirname "$2")"
cp "$1" "$2.partial" && mv -f "$2.partial" "$2"
chown "$owner" "$(dirname "$2")" "$2""#;

/// Forwards ports that listen in the container to the same ports on this machine,
/// like VS Code's automatic port forwarding. Each accepted connection runs the
/// remote server's `tcp-relay` in the container through `docker exec`, so it works
/// wherever the engine runs (locally, in WSL, or over SSH).
#[derive(Clone)]
struct PortRelay {
    connection_options: DockerConnectionOptions,
    docker_cli: String,
    engine_environment: Vec<(String, String)>,
    remote_dir_for_server: String,
    server_binary: String,
    executor: gpui::BackgroundExecutor,
    listener: Option<UnboundedSender<ForwardedPort>>,
}

impl PortRelay {
    const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(3);

    async fn forward_listening_ports(self) {
        let mut handled = std::collections::HashSet::new();
        let mut forwards = Vec::new();
        loop {
            match self.listening_ports().await {
                Ok(ports) => {
                    for port in ports {
                        // Ports the engine publishes already reach this machine.
                        if !handled.insert(port)
                            || self.connection_options.forward_ports.contains(&port)
                        {
                            continue;
                        }
                        let auto_forward = &self.connection_options.auto_forward;
                        let Some((label, notice)) = auto_forward.forwarding(port) else {
                            continue;
                        };
                        let listener =
                            bind_local_port(port, auto_forward.requires_local_port(port)).await;
                        let local_port = listener
                            .as_ref()
                            .and_then(|listener| listener.local_addr().ok())
                            .map(|address| address.port());
                        if let (Some(listener), Some(local_port)) = (listener, local_port) {
                            log::info!(
                                "Forwarding dev container port {port} to localhost:{local_port}"
                            );
                            forwards.push(self.executor.spawn(self.clone().accept(listener, port)));
                        } else {
                            log::warn!(
                                "Not forwarding dev container port {port}: it's in use here"
                            );
                        }
                        if let Some(forwarded_ports) = &self.listener {
                            forwarded_ports
                                .unbounded_send(ForwardedPort {
                                    container_id: self.connection_options.container_id.clone(),
                                    port,
                                    local_port,
                                    label: label.map(str::to_string),
                                    notice,
                                    https: auto_forward.uses_https(port),
                                })
                                .ok();
                        }
                    }
                }
                Err(error) => log::debug!("Failed to list the dev container's ports: {error:#}"),
            }
            self.executor.timer(Self::POLL_INTERVAL).await;
        }
    }

    async fn listening_ports(&self) -> Result<std::collections::BTreeSet<u16>> {
        let mut command = engine_command(
            &self.connection_options,
            &self.docker_cli,
            &self.engine_environment,
        );
        command.args([
            "exec",
            &self.connection_options.container_id,
            "sh",
            "-c",
            "cat /proc/net/tcp /proc/net/tcp6 2>/dev/null",
        ]);
        let output = command.output().await?;
        Ok(parse_listening_ports(&String::from_utf8_lossy(
            &output.stdout,
        )))
    }

    async fn accept(self, listener: smol::net::TcpListener, port: u16) {
        while let Ok((stream, _)) = listener.accept().await {
            let relay = self.clone();
            self.executor
                .spawn(async move {
                    if let Err(error) = relay.relay(stream, port).await {
                        log::debug!("Port {port} relay ended: {error:#}");
                    }
                })
                .detach();
        }
    }

    async fn relay(&self, stream: smol::net::TcpStream, port: u16) -> Result<()> {
        use futures::{AsyncReadExt as _, AsyncWriteExt as _};

        let mut command = engine_command(
            &self.connection_options,
            &self.docker_cli,
            &self.engine_environment,
        );
        command.args([
            "exec",
            "-i",
            "-u",
            &self.connection_options.remote_user,
            "-w",
            &self.remote_dir_for_server,
            &self.connection_options.container_id,
            &self.server_binary,
            "tcp-relay",
            "--port",
            &port.to_string(),
        ]);
        let mut command = command.to_command();
        command
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = command.spawn()?;
        let mut child_stdin = child.stdin.take().context("relay has no stdin")?;
        let mut child_stdout = child.stdout.take().context("relay has no stdout")?;
        let (mut from_client, mut to_client) = stream.split();
        let upload = async {
            futures::io::copy(&mut from_client, &mut child_stdin)
                .await
                .ok();
            child_stdin.close().await.ok();
        };
        let download = async {
            futures::io::copy(&mut child_stdout, &mut to_client)
                .await
                .ok();
            to_client.close().await.ok();
        };
        futures::future::join(upload, download).await;
        Ok(())
    }
}

/// Listens on this machine for connections to forward to `port` of the container:
/// on the same port when it's free, else, unless `require_local_port`, on the next
/// free port after it, like VS Code.
async fn bind_local_port(port: u16, require_local_port: bool) -> Option<smol::net::TcpListener> {
    if let Ok(listener) = smol::net::TcpListener::bind(("127.0.0.1", port)).await {
        return Some(listener);
    }
    if require_local_port {
        return None;
    }
    for candidate in port.saturating_add(1)..=port.saturating_add(100) {
        if let Ok(listener) = smol::net::TcpListener::bind(("127.0.0.1", candidate)).await {
            return Some(listener);
        }
    }
    smol::net::TcpListener::bind(("127.0.0.1", 0)).await.ok()
}

/// The TCP ports listed as listening (state `0A`) in `/proc/net/tcp` and `tcp6`.
fn parse_listening_ports(proc_net_tcp: &str) -> std::collections::BTreeSet<u16> {
    proc_net_tcp
        .lines()
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let local_address = fields.nth(1)?;
            let state = fields.nth(1)?;
            if state != "0A" {
                return None;
            }
            let (_, port) = local_address.rsplit_once(':')?;
            u16::from_str_radix(port, 16).ok()
        })
        .collect()
}

fn engine_command(
    connection_options: &DockerConnectionOptions,
    docker_cli: &str,
    engine_environment: &[(String, String)],
) -> HostCommand {
    let mut command = connection_options.host.command(docker_cli);
    for (key, value) in engine_environment {
        command.env(key, value);
    }
    command
}

fn docker_command(
    connection_options: &DockerConnectionOptions,
    docker_cli: &str,
    engine_environment: &[(String, String)],
    args: &[impl AsRef<str>],
) -> util::command::Command {
    let mut command = engine_command(connection_options, docker_cli, engine_environment);
    command.args(args.iter().map(|arg| arg.as_ref()));
    command.to_command()
}

/// Builds the command that stops the local `docker exec` proxy process. Windows has
/// no `kill` executable, so reconnecting to a dev container failed there before this.
fn kill_process_command(pid: u32) -> util::command::Command {
    if cfg!(target_os = "windows") {
        let mut command = util::command::new_command("taskkill");
        command.args(["/PID", &pid.to_string(), "/T", "/F"]);
        command
    } else {
        let mut command = util::command::new_command("kill");
        command.arg(pid.to_string());
        command
    }
}

#[async_trait(?Send)]
impl RemoteConnection for DockerExecConnection {
    fn has_wsl_interop(&self) -> bool {
        false
    }
    fn start_proxy(
        &self,
        unique_identifier: String,
        reconnect: bool,
        incoming_tx: UnboundedSender<Envelope>,
        outgoing_rx: UnboundedReceiver<Envelope>,
        connection_activity_tx: Sender<()>,
        delegate: Arc<dyn RemoteClientDelegate>,
        cx: &mut AsyncApp,
    ) -> Task<Result<i32>> {
        // We'll try connecting anew every time we open a devcontainer, so proactively try to kill any old connections.
        if !self.has_been_killed() {
            if let Err(e) = self.kill_inner() {
                return Task::ready(Err(e));
            };
        }

        delegate.set_status(Some("Starting proxy"), cx);

        let Some(remote_binary_relpath) = self.remote_binary_relpath.clone() else {
            return Task::ready(Err(anyhow!("Remote binary path not set")));
        };

        let mut docker_args = vec!["exec".to_string()];

        push_environment(&mut docker_args, &self.connection_options.remote_env);
        for env_var in ["RUST_LOG", "RUST_BACKTRACE", "ZED_GENERATE_MINIDUMPS"] {
            if let Some(value) = std::env::var(env_var).ok() {
                docker_args.push("-e".to_string());
                docker_args.push(format!("{env_var}={value}"));
            }
        }

        docker_args.extend([
            "-u".to_string(),
            self.connection_options.remote_user.to_string(),
            "-w".to_string(),
            self.remote_dir_for_server.clone(),
            "-i".to_string(),
            self.connection_options.container_id.to_string(),
        ]);

        let val = remote_binary_relpath
            .display(self.path_style())
            .into_owned();
        docker_args.push(val);
        docker_args.push("proxy".to_string());
        docker_args.push("--identifier".to_string());
        docker_args.push(unique_identifier);
        if reconnect {
            docker_args.push("--reconnect".to_string());
        }
        // The proxy lives as long as the connection, and so do its port forwards.
        let mut host_command = engine_command(
            &self.connection_options,
            self.docker_cli(),
            &self.engine_environment,
        );
        host_command.args(&docker_args);
        for port in &self.connection_options.forward_ports {
            host_command.forward_port(*port);
        }
        let mut command = host_command.to_command();
        command
            .kill_on_drop(true)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let Ok(child) = command.spawn() else {
            return Task::ready(Err(anyhow::anyhow!(
                "Failed to start remote server process"
            )));
        };

        let mut proxy_process = self.proxy_process.lock();
        *proxy_process = Some(child.id());

        let relay = PortRelay {
            connection_options: self.connection_options.clone(),
            docker_cli: self.docker_cli().to_string(),
            engine_environment: self.engine_environment.clone(),
            remote_dir_for_server: self.remote_dir_for_server.clone(),
            server_binary: remote_binary_relpath
                .display(self.path_style())
                .into_owned(),
            executor: cx.background_executor().clone(),
            listener: cx.update(|cx| {
                cx.try_global::<ForwardedPortListener>()
                    .map(|listener| listener.0.clone())
            }),
        };
        *self.port_forwarding.lock() = Some(cx.background_spawn(relay.forward_listening_ports()));

        cx.spawn(async move |cx| {
            super::handle_rpc_messages_over_child_process_stdio(
                child,
                incoming_tx,
                outgoing_rx,
                connection_activity_tx,
                cx,
            )
            .await
            .and_then(|status| {
                if status != 0 {
                    anyhow::bail!("Remote server exited with status {status}");
                }
                Ok(0)
            })
        })
    }

    fn upload_directory(
        &self,
        src_path: PathBuf,
        dest_path: RemotePathBuf,
        cx: &App,
    ) -> Task<Result<()>> {
        let dest_path_str = dest_path.to_string();
        let src_path_display = src_path.display().to_string();

        let upload_task = Self::upload_and_chown(
            self.docker_cli().to_string(),
            self.connection_options.clone(),
            self.engine_environment.clone(),
            src_path_display,
            dest_path_str,
        );

        cx.background_spawn(upload_task)
    }

    async fn kill(&self) -> Result<()> {
        self.kill_inner()
    }

    fn has_been_killed(&self) -> bool {
        self.proxy_process.lock().is_none()
    }

    fn build_command(
        &self,
        program: Option<String>,
        args: &[String],
        env: &HashMap<String, String>,
        working_dir: Option<String>,
        _port_forward: Option<(u16, String, u16)>,
        interactive: Interactive,
    ) -> Result<CommandTemplate> {
        let mut parsed_working_dir = None;

        let path_style = self.path_style();

        if let Some(working_dir) = working_dir {
            let working_dir = RemotePathBuf::new(working_dir, path_style).to_string();

            const TILDE_PREFIX: &'static str = "~/";
            if working_dir.starts_with(TILDE_PREFIX) {
                let working_dir = working_dir.trim_start_matches("~").trim_start_matches("/");
                parsed_working_dir =
                    Some(format!("{}/{}", self.remote_dir_for_server, working_dir));
            } else {
                parsed_working_dir = Some(working_dir);
            }
        }

        let mut inner_program = Vec::new();

        if let Some(program) = program {
            inner_program.push(program);
            for arg in args {
                inner_program.push(arg.clone());
            }
        } else {
            inner_program.push(self.shell());
            inner_program.push("-l".to_string());
        };

        let mut docker_args = vec![
            "exec".to_string(),
            "-u".to_string(),
            self.connection_options.remote_user.clone(),
        ];

        if let Some(parsed_working_dir) = parsed_working_dir {
            docker_args.push("-w".to_string());
            docker_args.push(parsed_working_dir);
        }

        push_environment(&mut docker_args, &self.connection_options.remote_env);
        push_environment(&mut docker_args, env);

        match interactive {
            Interactive::Yes => docker_args.push("-it".to_string()),
            Interactive::No => docker_args.push("-i".to_string()),
        }
        docker_args.push(self.connection_options.container_id.to_string());

        docker_args.append(&mut inner_program);

        let mut command = engine_command(
            &self.connection_options,
            self.docker_cli(),
            &self.engine_environment,
        );
        command
            .args(&docker_args)
            .interactive(interactive == Interactive::Yes);
        let command = command.to_command();
        Ok(CommandTemplate {
            program: command.get_program().to_string_lossy().into_owned(),
            args: command
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect(),
            // Docker-exec pipes in environment via the "-e" argument
            env: Default::default(),
        })
    }

    fn build_forward_ports_command(
        &self,
        _forwards: Vec<(u16, String, u16)>,
    ) -> Result<CommandTemplate> {
        Err(anyhow::anyhow!("Not currently supported for docker_exec"))
    }

    fn connection_options(&self) -> RemoteConnectionOptions {
        RemoteConnectionOptions::Docker(self.connection_options.clone())
    }

    fn path_style(&self) -> PathStyle {
        self.path_style.unwrap_or(PathStyle::Unix)
    }

    fn remote_platform(&self) -> RemotePlatform {
        // Docker containers are always Linux; the platform is populated during
        // setup, so this fallback is only for the brief pre-detection window.
        self.remote_platform.unwrap_or(RemotePlatform {
            os: RemoteOs::Linux,
            arch: RemoteArch::X86_64,
        })
    }

    fn remote_os_version(&self) -> Option<String> {
        self.os_version.clone()
    }

    fn shell(&self) -> String {
        self.shell.clone()
    }

    fn default_system_shell(&self) -> String {
        String::from("/bin/sh")
    }
}

fn push_environment<'a>(
    args: &mut Vec<String>,
    environment: impl IntoIterator<Item = (&'a String, &'a String)>,
) {
    for (name, value) in environment {
        if !is_valid_environment_name(name) {
            log::warn!("Skipping environment variable with invalid name");
            continue;
        }
        args.push("-e".to_string());
        args.push(format!("{name}={value}"));
    }
}

fn redact_arguments(docker_cli: &str, subcommand: &str, args: &[impl AsRef<str>]) -> String {
    let mut redacted = format!("{docker_cli:?} {subcommand:?}");
    let mut next_argument_is_environment = false;

    for arg in args {
        let arg = arg.as_ref();
        let argument = if next_argument_is_environment {
            next_argument_is_environment = false;
            redact_environment(arg)
        } else if arg == "-e" {
            next_argument_is_environment = true;
            arg.to_string()
        } else {
            redact_command(arg)
        };
        write!(redacted, " {argument:?}").ok();
    }

    redacted
}

fn docker_command_error(
    docker_cli: &str,
    subcommand: &str,
    args: &[impl AsRef<str>],
    stderr: &str,
) -> String {
    format!(
        "failed to run command {}: {}",
        redact_arguments(docker_cli, subcommand, args),
        redact_command(stderr)
    )
}

fn redact_environment(assignment: &str) -> String {
    match assignment.split_once('=') {
        Some((name, _)) => format!("{name}=<redacted>"),
        None => assignment.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kills_the_proxy_with_a_command_available_on_the_platform() {
        let command = kill_process_command(4242);
        let arguments: Vec<_> = command
            .get_args()
            .map(|argument| argument.to_string_lossy().into_owned())
            .collect();

        if cfg!(target_os = "windows") {
            assert_eq!(command.get_program(), "taskkill");
            assert_eq!(arguments, ["/PID", "4242", "/T", "/F"]);
        } else {
            assert_eq!(command.get_program(), "kill");
            assert_eq!(arguments, ["4242"]);
        }
    }

    #[cfg(unix)]
    #[test]
    fn server_binaries_round_trip_through_the_cache_volume() {
        use std::os::unix::fs::PermissionsExt as _;

        let run = |script: &str, first: &std::path::Path, second: &std::path::Path| {
            let mut command = util::command::new_command("sh");
            command
                .arg("-c")
                .arg(script)
                .arg("sh")
                .arg(first)
                .arg(second);
            smol::block_on(command.output()).unwrap()
        };
        let root = tempfile::tempdir().unwrap();
        let home = root.path().join("home");
        let cache = root.path().join("cache");
        let installed = home.join(".zed_server/zed-remote-server-stable-1.0.0");
        let cached = cache.join("zed-remote-server-stable-1.0.0-linux-x86_64");
        std::fs::create_dir_all(installed.parent().unwrap()).unwrap();
        std::fs::write(&installed, "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(&installed, std::fs::Permissions::from_mode(0o755)).unwrap();

        // Without the volume, there's nothing to cache into or restore from.
        assert!(
            run(super::CACHE_SERVER_SCRIPT, &installed, &cached)
                .status
                .success()
        );
        assert!(!cached.exists());
        assert!(
            !run(super::RESTORE_CACHED_SERVER_SCRIPT, &cached, &installed)
                .status
                .success()
        );

        std::fs::create_dir(&cache).unwrap();
        assert!(
            run(super::CACHE_SERVER_SCRIPT, &installed, &cached)
                .status
                .success()
        );
        assert!(cached.is_file());

        std::fs::remove_dir_all(home.join(".zed_server")).unwrap();
        let output = run(super::RESTORE_CACHED_SERVER_SCRIPT, &cached, &installed);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(std::fs::read_to_string(&installed).unwrap(), "#!/bin/sh\n");
        assert_ne!(
            std::fs::metadata(&installed).unwrap().permissions().mode() & 0o111,
            0
        );
    }

    #[test]
    fn busy_ports_are_forwarded_to_the_next_free_port_unless_required() {
        smol::block_on(async {
            let taken = smol::net::TcpListener::bind(("127.0.0.1", 0))
                .await
                .unwrap();
            let port = taken.local_addr().unwrap().port();

            let fallback = super::bind_local_port(port, false).await.unwrap();
            let fallback_port = fallback.local_addr().unwrap().port();
            assert_ne!(fallback_port, port);

            assert!(super::bind_local_port(port, true).await.is_none());

            drop(taken);
            let same = super::bind_local_port(port, true).await.unwrap();
            assert_eq!(same.local_addr().unwrap().port(), port);
        });
    }

    #[test]
    fn auto_forwarding_follows_the_first_matching_rule() {
        let auto_forward = super::AutoForwardPorts {
            rules: vec![
                super::AutoForwardRule {
                    start: 3000,
                    end: 3000,
                    forward: true,
                    label: Some("Web".to_string()),
                    notice: super::ForwardNotice::OpenBrowser,
                    require_local_port: true,
                    https: true,
                },
                super::AutoForwardRule {
                    start: 3000,
                    end: 3010,
                    forward: false,
                    label: None,
                    notice: super::ForwardNotice::Notify,
                    require_local_port: false,
                    https: false,
                },
            ],
            ignore_other_ports: false,
            other_ports_notice: super::ForwardNotice::Silent,
        };
        assert!(auto_forward.forwards(3000));
        assert!(!auto_forward.forwards(3005));
        assert!(auto_forward.forwards(8080));
        assert_eq!(
            auto_forward.forwarding(3000),
            Some((Some("Web"), super::ForwardNotice::OpenBrowser))
        );
        assert_eq!(auto_forward.forwarding(3005), None);
        assert!(auto_forward.requires_local_port(3000));
        assert!(auto_forward.uses_https(3000));
        assert!(!auto_forward.requires_local_port(8080));
        assert!(!auto_forward.uses_https(8080));
        assert_eq!(
            auto_forward.forwarding(8080),
            Some((None, super::ForwardNotice::Silent))
        );

        let only_listed = super::AutoForwardPorts {
            ignore_other_ports: true,
            ..auto_forward
        };
        assert!(only_listed.forwards(3000));
        assert!(!only_listed.forwards(8080));
        assert!(super::AutoForwardPorts::default().forwards(8080));
    }

    #[test]
    fn finds_listening_ports_in_proc_net_tcp() {
        let proc_net_tcp = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 1 1 0000000000000000 100 0 0 10 0
   1: 0100007F:0CEA 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 2 1 0000000000000000 100 0 0 10 0
   2: 0100007F:1F90 0100007F:9C40 01 00000000:00000000 00:00000000 00000000     0        0 3 1 0000000000000000 20 4 30 10 -1
  sl  local_address                         remote_address                        st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 00000000000000000000000000000000:2382 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 4 1 0000000000000000 100 0 0 10 0
";
        assert_eq!(
            super::parse_listening_ports(proc_net_tcp)
                .into_iter()
                .collect::<Vec<_>>(),
            [3306, 8080, 9090]
        );
    }

    #[test]
    fn redacts_forwarded_env() {
        let connection = connection(&[
            ("DATABASE_URL", "postgres://user:password@host/db"),
            ("GH_TOKEN", "ghp_supersecret"),
            ("PATH", "/usr/bin"),
            ("lowercase_token", "secret"),
        ]);

        assert_eq!(
            redacted_docker_exec(&connection, &[], &["-c", "echo hi"]),
            concat!(
                "\"docker\" \"exec\" \"-w\" \"/workspace\" \"-u\" \"user\"",
                " \"-e\" \"DATABASE_URL=<redacted>\" \"-e\" \"GH_TOKEN=<redacted>\"",
                " \"-e\" \"PATH=<redacted>\" \"-e\" \"lowercase_token=<redacted>\"",
                " \"container_id\" \"sh\" \"-c\" \"echo hi\""
            )
        );
    }

    #[test]
    fn preserves_argument_boundaries_and_escapes_control_characters() {
        let connection = connection(&[]);

        assert_eq!(
            redacted_docker_exec(&connection, &[], &["-c", "first argument\nsecond argument"]),
            concat!(
                "\"docker\" \"exec\" \"-w\" \"/workspace\" \"-u\" \"user\"",
                " \"container_id\" \"sh\" \"-c\" \"first argument\\nsecond argument\""
            )
        );
    }

    #[test]
    fn redacts_assignments_in_command_arguments() {
        assert_eq!(
            redact_arguments(
                "docker",
                "exec",
                &["container_id", "sh", "-c", "GH_TOKEN=ghp_supersecret run"]
            ),
            concat!(
                "\"docker\" \"exec\" \"container_id\" \"sh\" \"-c\"",
                " \"GH_TOKEN=\\\"[REDACTED]\\\" run\""
            )
        );
    }

    #[test]
    fn redacts_failure_message() {
        let connection = connection(&[("API_KEY", "remote-secret")]);
        let env = [("COMMAND_SECRET".to_string(), "command-secret".to_string())]
            .into_iter()
            .collect::<HashMap<_, _>>();
        let args = connection.docker_exec_arguments("sh", None, &env, &["-c", "echo hi"]);

        assert_eq!(
            docker_command_error(
                connection.docker_cli(),
                "exec",
                &args,
                "sh: 1: /usr/local/cargo/bin/zed-remote-server: not found\nGH_TOKEN=ghp_supersecret run",
            ),
            concat!(
                "failed to run command \"docker\" \"exec\" \"-u\" \"user\"",
                " \"-e\" \"API_KEY=<redacted>\" \"-e\" \"COMMAND_SECRET=<redacted>\"",
                " \"container_id\" \"sh\" \"-c\" \"echo hi\"",
                ": sh: 1: /usr/local/cargo/bin/zed-remote-server: not found\nGH_TOKEN=\"[REDACTED]\" run"
            )
        );
    }

    #[test]
    fn skips_invalid_env_names() {
        let connection = connection(&[
            ("", "empty"),
            ("NAME=VALUE", "equals"),
            ("LINE\nBREAK", "newline"),
            ("NAME\0NUL", "nul"),
            ("GH_TOKEN", "ghp_supersecret"),
        ]);
        let env = HashMap::default();
        let program_args = Vec::<&str>::new();
        let args = connection.docker_exec_arguments("sh", None, &env, &program_args);

        assert_eq!(
            args,
            vec![
                "-u",
                "user",
                "-e",
                "GH_TOKEN=ghp_supersecret",
                "container_id",
                "sh"
            ]
        );
    }

    #[test]
    fn uses_podman_cli() {
        let mut connection = connection(&[("GH_TOKEN", "ghp_supersecret")]);
        connection.connection_options.use_podman = true;

        assert_eq!(
            redacted_docker_exec(&connection, &[], &[]),
            concat!(
                "\"podman\" \"exec\" \"-w\" \"/workspace\" \"-u\" \"user\"",
                " \"-e\" \"GH_TOKEN=<redacted>\" \"container_id\" \"sh\""
            )
        );
    }

    fn connection(remote_env: &[(&str, &str)]) -> DockerExecConnection {
        DockerExecConnection {
            proxy_process: Mutex::new(None),
            remote_dir_for_server: "/tmp/zed".to_string(),
            remote_binary_relpath: None,
            connection_options: DockerConnectionOptions {
                name: "container".to_string(),
                container_id: "container_id".to_string(),
                remote_user: "user".to_string(),
                local_folder: None,
                config_file: None,
                upload_binary_over_docker_exec: false,
                use_podman: false,
                remote_env: remote_env
                    .iter()
                    .map(|(key, value)| (key.to_string(), value.to_string()))
                    .collect(),
                host: EngineHost::Local,
                forward_ports: Vec::new(),
                auto_forward: Default::default(),
                shutdown_action: Default::default(),
            },
            remote_platform: None,
            os_version: None,
            path_style: None,
            shell: "/bin/sh".to_string(),
            engine_environment: Vec::new(),
            port_forwarding: Mutex::new(None),
        }
    }

    fn redacted_docker_exec(
        connection: &DockerExecConnection,
        env: &[(&str, &str)],
        program_args: &[&str],
    ) -> String {
        let env = env
            .iter()
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect::<HashMap<_, _>>();
        let args = connection.docker_exec_arguments("sh", Some("/workspace"), &env, program_args);
        redact_arguments(connection.docker_cli(), "exec", &args)
    }
}
