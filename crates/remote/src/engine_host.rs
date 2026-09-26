use std::{
    collections::HashMap,
    ffi::OsStr,
    path::{Path, PathBuf},
    process::Output,
};

use util::command::{Command, Stdio};

use crate::{SshConnectionOptions, WslConnectionOptions};

/// The machine where a dev container's engine CLI (`docker`, `podman`) runs.
///
/// This is also where the project's sources live: bind mounts, build contexts
/// and `initializeCommand` all refer to this machine, not to the one running Zed.
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
pub enum EngineHost {
    /// The machine running Zed.
    #[default]
    Local,
    /// A WSL distribution of the Windows machine running Zed.
    Wsl(WslConnectionOptions),
    /// A POSIX machine reached over SSH.
    Ssh(SshEngineHost),
}

/// How to reach an SSH engine host. It is persisted with the connection, so it
/// never holds secrets such as passwords: authentication must not prompt.
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
pub struct SshEngineHost {
    pub host: String,
    pub username: Option<String>,
    pub port: Option<u16>,
    /// Extra `ssh` arguments, such as `-i <identity file>`.
    #[serde(default)]
    pub args: Vec<String>,
}

impl From<&SshConnectionOptions> for SshEngineHost {
    fn from(options: &SshConnectionOptions) -> Self {
        Self {
            host: options.host.to_string(),
            username: options.username.clone(),
            port: options.port,
            args: options.args.clone().unwrap_or_default(),
        }
    }
}

impl EngineHost {
    /// Starts building a command that runs `program` on this host.
    pub fn command(&self, program: impl Into<String>) -> HostCommand {
        HostCommand {
            host: self.clone(),
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
            secret_env: Vec::new(),
            current_dir: None,
            interactive: false,
            forwarded_ports: Vec::new(),
        }
    }

    pub fn is_local(&self) -> bool {
        matches!(self, Self::Local)
    }

    /// Whether the machine running Zed can open the host's files directly, as
    /// [`Self::local_path`] returns them. When it cannot, they are read and
    /// written by running commands on the host.
    pub fn has_local_files(&self) -> bool {
        !matches!(self, Self::Ssh(_))
    }

    /// Whether the host runs Windows, which decides how shell scripts run on it
    /// and how its paths look.
    pub fn is_windows(&self) -> bool {
        match self {
            Self::Local => cfg!(windows),
            Self::Wsl(_) | Self::Ssh(_) => false,
        }
    }

    /// Reads the environment of a login shell on the host, where users set `PATH`
    /// and similar variables (`~/.profile`), as a terminal on the host sees it.
    pub async fn login_environment(&self) -> anyhow::Result<HashMap<String, String>> {
        let mut command = self.command("sh");
        command.args(["-c", r#"exec "${SHELL:-sh}" -lc 'env -0'"#]);
        let output = command.output().await?;
        anyhow::ensure!(
            output.status.success(),
            "failed to read the environment of {self:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(parse_nul_separated_environment(&output.stdout))
    }

    /// The variables of the host's login environment that decide which engine the
    /// container CLI talks to, and where it is. Commands started on a host other
    /// than the machine running Zed don't go through a login shell, so they are
    /// passed explicitly. The machine running Zed already loaded its own.
    ///
    /// It's loaded once per host and session, since a login shell is slow to start.
    pub async fn engine_environment(&self) -> Vec<(String, String)> {
        static ENGINE_ENVIRONMENTS: std::sync::LazyLock<
            parking_lot::Mutex<std::collections::HashMap<EngineHost, Vec<(String, String)>>>,
        > = std::sync::LazyLock::new(Default::default);

        if self.is_local() {
            return Vec::new();
        }
        if let Some(environment) = ENGINE_ENVIRONMENTS.lock().get(self) {
            return environment.clone();
        }
        match self.login_environment().await {
            Ok(environment) => {
                let variables = engine_variables(environment);
                ENGINE_ENVIRONMENTS
                    .lock()
                    .insert(self.clone(), variables.clone());
                variables
            }
            Err(error) => {
                log::warn!("Using the default engine environment: {error:#}");
                Vec::new()
            }
        }
    }

    /// Converts a path as the machine running Zed sees it into the same path as
    /// the host sees it, e.g. to pass it to the container engine.
    pub fn host_path(&self, local: &Path) -> String {
        match self {
            Self::Local => local.display().to_string(),
            Self::Wsl(options) => wsl_host_path(&local.to_string_lossy(), &options.distro_name),
            // SSH paths are POSIX; a Windows client joins them with backslashes.
            Self::Ssh(_) => local.to_string_lossy().replace('\\', "/"),
        }
    }

    /// Converts an absolute path on the host into a path the machine running Zed
    /// can read and write.
    pub fn local_path(&self, host: &str) -> PathBuf {
        match self {
            Self::Local => PathBuf::from(host),
            Self::Wsl(options) => PathBuf::from(format!(
                r"\\wsl.localhost\{}{}",
                options.distro_name,
                host.replace('/', r"\")
            )),
            Self::Ssh(_) => PathBuf::from(host),
        }
    }
}

const ENGINE_VARIABLES: &[&str] = &[
    "PATH",
    "DOCKER_HOST",
    "DOCKER_CONTEXT",
    "DOCKER_CONFIG",
    "DOCKER_CERT_PATH",
    "DOCKER_TLS_VERIFY",
    "CONTAINER_HOST",
    "XDG_RUNTIME_DIR",
];

fn engine_variables(mut environment: HashMap<String, String>) -> Vec<(String, String)> {
    ENGINE_VARIABLES
        .iter()
        .filter_map(|name| Some((name.to_string(), environment.remove(*name)?)))
        .collect()
}

pub fn parse_nul_separated_environment(output: &[u8]) -> HashMap<String, String> {
    String::from_utf8_lossy(output)
        .split('\0')
        .filter_map(|entry| entry.split_once('='))
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect()
}

/// Maps a Windows path to the path a WSL distribution sees: its own files
/// through `\\wsl.localhost\<distro>\…` (or `\\wsl$\…`), Windows drives
/// through their `/mnt/<drive>` mount.
fn wsl_host_path(path: &str, distro: &str) -> String {
    let path = path.strip_prefix(r"\\?\UNC\").map_or_else(
        || path.strip_prefix(r"\\?\").unwrap_or(path).to_string(),
        |rest| format!(r"\\{rest}"),
    );
    let slashed = path.replace('\\', "/");

    for share in ["//wsl.localhost/", "//wsl$/"] {
        let Some(rest) = strip_prefix_ignore_case(&slashed, share) else {
            continue;
        };
        let (name, rest) = rest.split_once('/').unwrap_or((rest, ""));
        if name.eq_ignore_ascii_case(distro) {
            return format!("/{rest}");
        }
    }

    let bytes = slashed.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        let drive = (bytes[0] as char).to_ascii_lowercase();
        let rest = slashed[2..].trim_start_matches('/');
        return if rest.is_empty() {
            format!("/mnt/{drive}")
        } else {
            format!("/mnt/{drive}/{rest}")
        };
    }

    slashed
}

fn strip_prefix_ignore_case<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    let head = value.get(..prefix.len())?;
    head.eq_ignore_ascii_case(prefix)
        .then(|| &value[prefix.len()..])
}

/// A command to run on an [`EngineHost`].
///
/// Arguments are passed to the program as-is on every host: no shell parses them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostCommand {
    host: EngineHost,
    program: String,
    args: Vec<String>,
    env: Vec<(String, String)>,
    /// Variables whose values stay off the command line where the host allows.
    secret_env: Vec<(String, String)>,
    current_dir: Option<String>,
    interactive: bool,
    forwarded_ports: Vec<u16>,
}

impl HostCommand {
    pub fn arg(&mut self, arg: impl AsRef<OsStr>) -> &mut Self {
        self.args.push(arg.as_ref().to_string_lossy().into_owned());
        self
    }

    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        for arg in args {
            self.arg(arg);
        }
        self
    }

    pub fn env(&mut self, key: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.env.push((key.into(), value.into()));
        self
    }

    /// Sets a variable that, unlike [`Self::env`], doesn't appear in the arguments of
    /// the process started here: locally and in WSL, which gets it through
    /// `WSLENV`. SSH can only pass it on the remote command line.
    pub fn secret_env(&mut self, key: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.secret_env.push((key.into(), value.into()));
        self
    }

    /// The non-empty values of the secret variables, in each form they may take in
    /// the arguments of [`Self::to_command`], so that logs of it can redact them.
    pub fn secret_values(&self) -> Vec<String> {
        let mut values = Vec::new();
        for (_, value) in &self.secret_env {
            if value.is_empty() {
                continue;
            }
            // How `posix_quote` writes the value into the SSH command line.
            let quoted = value.replace('\'', r"'\''");
            if quoted != *value {
                values.push(quoted);
            }
            values.push(value.clone());
        }
        values
    }

    /// The environment of the process started here, for callers that start it
    /// from [`Self::to_command`]'s program and arguments alone.
    pub fn process_env(&self) -> Vec<(String, String)> {
        match &self.host {
            EngineHost::Local => self.env.iter().chain(&self.secret_env).cloned().collect(),
            EngineHost::Wsl(_) => {
                let mut env = self.secret_env.clone();
                if let Some(wslenv) = self.wslenv() {
                    env.push(("WSLENV".to_string(), wslenv));
                }
                env
            }
            EngineHost::Ssh(_) => Vec::new(),
        }
    }

    /// `WSLENV` extended with the secret variables, which WSL then passes to the
    /// distribution.
    fn wslenv(&self) -> Option<String> {
        if self.secret_env.is_empty() {
            return None;
        }
        let mut entries: Vec<String> = std::env::var("WSLENV")
            .ok()
            .filter(|wslenv| !wslenv.is_empty())
            .into_iter()
            .collect();
        entries.extend(self.secret_env.iter().map(|(key, _)| format!("{key}/u")));
        Some(entries.join(":"))
    }

    /// Sets the working directory, a path on the host.
    pub fn current_dir(&mut self, dir: impl AsRef<Path>) -> &mut Self {
        self.current_dir = Some(dir.as_ref().to_string_lossy().into_owned());
        self
    }

    /// Forwards `port` of this machine to the same port of an SSH host while the
    /// command runs. Other hosts share this machine's `localhost`, or reach it
    /// through WSL's localhost forwarding, so nothing needs forwarding.
    pub fn forward_port(&mut self, port: u16) -> &mut Self {
        self.forwarded_ports.push(port);
        self
    }

    /// Whether the command talks to a terminal, e.g. an interactive shell.
    pub fn interactive(&mut self, interactive: bool) -> &mut Self {
        self.interactive = interactive;
        self
    }

    pub fn get_program(&self) -> &str {
        &self.program
    }

    pub fn get_args(&self) -> &[String] {
        &self.args
    }

    pub fn host(&self) -> &EngineHost {
        &self.host
    }

    /// Builds the process that runs this command from the machine running Zed.
    pub fn to_command(&self) -> Command {
        match &self.host {
            EngineHost::Local => {
                let mut command = Command::new(&self.program);
                command.args(&self.args);
                for (key, value) in self.env.iter().chain(&self.secret_env) {
                    command.env(key, value);
                }
                if let Some(dir) = &self.current_dir {
                    command.current_dir(dir);
                }
                command
            }
            EngineHost::Wsl(options) => {
                let mut command = Command::new("wsl.exe");
                command.args(self.wsl_args(options));
                for (key, value) in self.process_env() {
                    command.env(key, value);
                }
                command
            }
            EngineHost::Ssh(options) => {
                let mut command = Command::new("ssh");
                command.args(self.ssh_args(options));
                command
            }
        }
    }

    /// `ssh` hands the remote command to the user's login shell as a single
    /// string, so every part of it is quoted for a POSIX shell.
    fn ssh_args(&self, options: &SshEngineHost) -> Vec<String> {
        let mut args = Vec::new();
        if let Some(port) = options.port {
            args.extend(["-p".to_string(), port.to_string()]);
        }
        args.extend(options.args.iter().cloned());
        // Fail instead of waiting for a password nobody can type.
        args.extend(["-o".to_string(), "BatchMode=yes".to_string()]);
        if let Some(control_path) = ssh_control_path(options) {
            args.extend([
                "-o".to_string(),
                "ControlMaster=auto".to_string(),
                "-o".to_string(),
                format!("ControlPath={}", control_path.display()),
                "-o".to_string(),
                "ControlPersist=5m".to_string(),
            ]);
        }
        for port in &self.forwarded_ports {
            args.extend(["-L".to_string(), format!("{port}:localhost:{port}")]);
        }
        args.push(if self.interactive { "-t" } else { "-T" }.to_string());
        args.push(match &options.username {
            Some(username) => format!("{username}@{}", options.host),
            None => options.host.clone(),
        });
        args.push("--".to_string());

        let mut script = String::new();
        if let Some(dir) = &self.current_dir {
            script.push_str(&format!("cd {} && ", posix_quote(dir)));
        }
        script.push_str("exec");
        if !self.env.is_empty() || !self.secret_env.is_empty() {
            script.push_str(" env");
            for (key, value) in self.env.iter().chain(&self.secret_env) {
                script.push(' ');
                script.push_str(&posix_quote(&format!("{key}={value}")));
            }
        }
        for part in std::iter::once(&self.program).chain(&self.args) {
            script.push(' ');
            script.push_str(&posix_quote(part));
        }
        args.push(script);
        args
    }

    /// `wsl.exe --exec` runs the program without a shell, so the arguments reach it
    /// unchanged. The Windows environment is not inherited, hence `env`.
    fn wsl_args(&self, options: &WslConnectionOptions) -> Vec<String> {
        let mut args = vec!["--distribution".to_string(), options.distro_name.clone()];
        if let Some(user) = &options.user {
            args.extend(["--user".to_string(), user.clone()]);
        }
        if let Some(dir) = &self.current_dir {
            args.extend(["--cd".to_string(), dir.clone()]);
        }
        args.push("--exec".to_string());
        if !self.env.is_empty() {
            args.push("env".to_string());
            args.extend(self.env.iter().map(|(key, value)| format!("{key}={value}")));
        }
        args.push(self.program.clone());
        args.extend(self.args.iter().cloned());
        args
    }

    pub async fn output(&self) -> std::io::Result<Output> {
        if let Some(output) = self.output_over_connection(&[]).await {
            return Ok(output);
        }
        self.to_command().output().await
    }

    /// Runs the command with `input` on its standard input.
    pub async fn output_with_stdin(&self, input: &[u8]) -> std::io::Result<Output> {
        use futures::AsyncWriteExt as _;

        if let Some(output) = self.output_over_connection(input).await {
            return Ok(output);
        }
        let mut command = self.to_command();
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(input).await?;
            stdin.close().await?;
        }
        child.output().await
    }
}

impl HostCommand {
    /// Runs an SSH host's command through Zed's connection to that host, when a
    /// project of that host is open. Windows' OpenSSH can't share connections, so
    /// each `ssh` would open a new one: Zed runs dozens of commands to build and
    /// start a dev container. Falls back to `ssh` when that fails, e.g. with an
    /// older remote server.
    async fn output_over_connection(&self, input: &[u8]) -> Option<Output> {
        if !cfg!(windows) || self.interactive || !self.forwarded_ports.is_empty() {
            return None;
        }
        let EngineHost::Ssh(options) = &self.host else {
            return None;
        };
        let client = HostCommandChannel::client_for(options)?;
        let request = rpc::proto::RunHostCommand {
            project_id: rpc::proto::REMOTE_SERVER_PROJECT_ID,
            program: self.program.clone(),
            args: self.args.clone(),
            env: self.env.iter().chain(&self.secret_env).cloned().collect(),
            cwd: self.current_dir.clone(),
            stdin: input.to_vec(),
        };
        match client.request(request).await {
            Ok(response) => Some(Output {
                status: exit_status(response.exit_code.unwrap_or(255)),
                stdout: response.stdout,
                stderr: response.stderr,
            }),
            Err(error) => {
                log::debug!(
                    "Running {} through Zed's SSH connection failed, using ssh: {error:#}",
                    self.program
                );
                None
            }
        }
    }
}

#[cfg(windows)]
fn exit_status(code: i32) -> std::process::ExitStatus {
    use std::os::windows::process::ExitStatusExt as _;
    std::process::ExitStatus::from_raw(code as u32)
}

#[cfg(unix)]
fn exit_status(code: i32) -> std::process::ExitStatus {
    use std::os::unix::process::ExitStatusExt as _;
    std::process::ExitStatus::from_raw(code << 8)
}

type SshHostKey = (String, Option<String>, Option<u16>);

static HOST_COMMAND_CHANNELS: std::sync::LazyLock<
    parking_lot::Mutex<Vec<(u64, SshHostKey, rpc::AnyProtoClient)>>,
> = std::sync::LazyLock::new(Default::default);
static NEXT_HOST_COMMAND_CHANNEL_ID: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);

/// Zed's connection to an SSH host, available to run that host's engine commands
/// for as long as it lives.
pub struct HostCommandChannel {
    id: u64,
}

impl HostCommandChannel {
    pub fn register(
        host: String,
        username: Option<String>,
        port: Option<u16>,
        client: rpc::AnyProtoClient,
    ) -> Self {
        let id = NEXT_HOST_COMMAND_CHANNEL_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        HOST_COMMAND_CHANNELS
            .lock()
            .push((id, (host, username, port), client));
        Self { id }
    }

    fn client_for(options: &SshEngineHost) -> Option<rpc::AnyProtoClient> {
        let key = (options.host.clone(), options.username.clone(), options.port);
        HOST_COMMAND_CHANNELS
            .lock()
            .iter()
            .rev()
            .find(|(_, channel_key, _)| *channel_key == key)
            .map(|(_, _, client)| client.clone())
    }
}

impl Drop for HostCommandChannel {
    fn drop(&mut self) {
        HOST_COMMAND_CHANNELS
            .lock()
            .retain(|(id, _, _)| *id != self.id);
    }
}

/// Where the engine commands for `options` share one SSH connection, which saves
/// a new SSH connection per command: Zed runs dozens of them to build and start a
/// container. Windows' OpenSSH doesn't support connection sharing.
fn ssh_control_path(options: &SshEngineHost) -> Option<std::path::PathBuf> {
    use sha2::{Digest as _, Sha256};

    if cfg!(windows) {
        return None;
    }
    let directory = paths::temp_dir().join("ssh");
    std::fs::create_dir_all(&directory).ok()?;
    // Socket paths are limited to about a hundred bytes, hence a short name.
    let digest = Sha256::digest(
        format!(
            "{}\0{}\0{}\0{}",
            options.username.as_deref().unwrap_or_default(),
            options.host,
            options.port.unwrap_or_default(),
            options.args.join("\0")
        )
        .as_bytes(),
    );
    let name: String = digest
        .iter()
        .take(8)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Some(directory.join(name))
}

/// Quotes `value` as a single word for a POSIX shell.
fn posix_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', r"'\''"))
}

impl std::fmt::Display for HostCommand {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.program)?;
        for arg in &self.args {
            write!(f, " {arg}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wsl(user: Option<&str>) -> EngineHost {
        EngineHost::Wsl(WslConnectionOptions {
            distro_name: "Ubuntu".to_string(),
            user: user.map(ToString::to_string),
        })
    }

    #[test]
    fn secret_variables_stay_off_the_command_line_locally_and_in_wsl() {
        for host in [EngineHost::Local, wsl(None)] {
            let mut command = host.command("docker");
            command
                .args(["exec", "-e", "API_TOKEN", "container", "env"])
                .secret_env("API_TOKEN", "s3cret");
            let args = command
                .to_command()
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            assert!(
                !args.iter().any(|arg| arg.contains("s3cret")),
                "{host:?}: {args:?}"
            );
            let process_env = command.process_env();
            assert!(
                process_env.contains(&("API_TOKEN".to_string(), "s3cret".to_string())),
                "{host:?}: {process_env:?}"
            );
        }

        let mut command = wsl(None).command("docker");
        command.secret_env("API_TOKEN", "s3cret");
        let wslenv = command
            .process_env()
            .into_iter()
            .find(|(key, _)| key == "WSLENV")
            .map(|(_, value)| value);
        assert!(wslenv.is_some_and(|wslenv| wslenv.ends_with("API_TOKEN/u")));
    }

    #[test]
    fn local_command_is_built_as_is() {
        let mut command = EngineHost::Local.command("docker");
        command
            .args(["run", "--label", "a b"])
            .env("DOCKER_BUILDKIT", "1");
        let command = command.to_command();
        assert_eq!(command.get_program(), "docker");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            ["run", "--label", "a b"]
        );
    }

    #[test]
    fn wsl_command_passes_arguments_without_a_shell() {
        let mut command = wsl(None).command("docker");
        command.args(["run", "--label", "a b; rm -rf /", "$HOME"]);
        let command = command.to_command();
        assert_eq!(command.get_program(), "wsl.exe");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                "--distribution",
                "Ubuntu",
                "--exec",
                "docker",
                "run",
                "--label",
                "a b; rm -rf /",
                "$HOME"
            ]
        );
    }

    #[test]
    fn wsl_command_sets_user_directory_and_environment() {
        let mut command = wsl(Some("dev")).command("docker");
        command
            .arg("build")
            .env("DOCKER_BUILDKIT", "0")
            .current_dir("/home/dev/project");
        let command = command.to_command();
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                "--distribution",
                "Ubuntu",
                "--user",
                "dev",
                "--cd",
                "/home/dev/project",
                "--exec",
                "env",
                "DOCKER_BUILDKIT=0",
                "docker",
                "build"
            ]
        );
    }

    #[test]
    fn only_local_windows_hosts_are_windows() {
        assert_eq!(EngineHost::Local.is_windows(), cfg!(windows));
        assert!(!wsl(None).is_windows());
        assert!(!ssh().is_windows());
    }

    fn ssh() -> EngineHost {
        EngineHost::Ssh(SshEngineHost {
            host: "build.example.com".to_string(),
            username: Some("dev".to_string()),
            port: Some(2222),
            args: vec!["-i".to_string(), "/keys/id".to_string()],
        })
    }

    #[test]
    fn ssh_command_quotes_every_word_for_the_remote_shell() {
        let mut command = ssh().command("docker");
        command
            .args(["run", "--label", "it's; rm -rf /", "$HOME"])
            .env("DOCKER_BUILDKIT", "1")
            .current_dir("/home/dev/my project");
        let command = command.to_command();
        assert_eq!(command.get_program(), "ssh");
        assert_eq!(
            command.get_args().collect::<Vec<_>>(),
            [
                "-p",
                "2222",
                "-i",
                "/keys/id",
                "-o",
                "BatchMode=yes",
                "-T",
                "dev@build.example.com",
                "--",
                r"cd '/home/dev/my project' && exec env 'DOCKER_BUILDKIT=1' 'docker' 'run' '--label' 'it'\''s; rm -rf /' '$HOME'",
            ]
        );
    }

    #[test]
    fn interactive_ssh_commands_allocate_a_terminal() {
        let mut command = ssh().command("docker");
        command
            .args(["exec", "-it", "container", "bash"])
            .interactive(true);
        let args = command
            .to_command()
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert!(args.contains(&"-t".to_string()));
        assert_eq!(
            args.last().unwrap(),
            "exec 'docker' 'exec' '-it' 'container' 'bash'"
        );
    }

    #[test]
    fn ssh_commands_share_one_connection_where_openssh_can() {
        let args = ssh()
            .command("docker")
            .to_command()
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            args.contains(&"ControlMaster=auto".to_string()),
            !cfg!(windows)
        );
    }

    #[test]
    fn parses_nul_separated_environment_and_keeps_engine_variables() {
        let environment = parse_nul_separated_environment(
            b"PATH=/home/dev/bin:/usr/bin\0DOCKER_HOST=unix:///run/user/1000/docker.sock\0MULTI=a\nb\0EMPTY=\0NO_EQUALS\0",
        );
        assert_eq!(environment.len(), 4);
        assert_eq!(environment["MULTI"], "a\nb");
        assert_eq!(environment["EMPTY"], "");
        assert_eq!(
            engine_variables(environment),
            [
                ("PATH".to_string(), "/home/dev/bin:/usr/bin".to_string()),
                (
                    "DOCKER_HOST".to_string(),
                    "unix:///run/user/1000/docker.sock".to_string()
                ),
            ]
        );
    }

    #[test]
    fn ssh_commands_forward_ports() {
        let mut command = ssh().command("docker");
        command
            .args(["exec", "-i", "container", "proxy"])
            .forward_port(3000);
        let args = command
            .to_command()
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        let forward = args.iter().position(|arg| arg == "-L").unwrap();
        assert_eq!(args[forward + 1], "3000:localhost:3000");

        let mut local = EngineHost::Local.command("docker");
        local.forward_port(3000);
        assert!(local.to_command().get_args().all(|arg| arg != "-L"));
    }

    #[test]
    fn ssh_paths_use_forward_slashes() {
        let host = ssh();
        assert!(!host.has_local_files());
        assert_eq!(
            host.host_path(Path::new(r"/home/dev/project\.devcontainer")),
            "/home/dev/project/.devcontainer"
        );
        assert_eq!(host.local_path("/home/dev"), PathBuf::from("/home/dev"));
    }

    #[test]
    fn wsl_paths_round_trip() {
        let host = wsl(None);
        let cases = [
            (
                r"\\wsl.localhost\Ubuntu\home\dev\project",
                "/home/dev/project",
            ),
            (r"\\wsl$\Ubuntu\home\dev", "/home/dev"),
            (
                r"\\?\UNC\wsl.localhost\ubuntu\tmp\build dir",
                "/tmp/build dir",
            ),
            (
                r"C:\Users\dev\AppData\Local\Zed\server",
                "/mnt/c/Users/dev/AppData/Local/Zed/server",
            ),
            (r"\\?\D:\", "/mnt/d"),
            (
                r"\\wsl.localhost\Debian\home",
                "//wsl.localhost/Debian/home",
            ),
        ];
        for (local, expected) in cases {
            assert_eq!(host.host_path(Path::new(local)), expected, "{local}");
        }
        assert_eq!(
            host.local_path("/home/dev/project"),
            PathBuf::from(r"\\wsl.localhost\Ubuntu\home\dev\project")
        );
        assert_eq!(
            host.host_path(&host.local_path("/home/dev/project/.devcontainer")),
            "/home/dev/project/.devcontainer"
        );
    }

    #[test]
    fn local_paths_are_unchanged() {
        let path = Path::new("/home/dev/project");
        assert_eq!(
            EngineHost::Local.host_path(path),
            path.display().to_string()
        );
        assert_eq!(EngineHost::Local.local_path("/home/dev/project"), path);
    }
}
