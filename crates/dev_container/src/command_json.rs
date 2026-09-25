use std::{path::PathBuf, process::Output, sync::Arc};

use async_trait::async_trait;
use serde::Deserialize;
use util::command::Command;

use crate::devcontainer_api::DevContainerError;

pub(crate) struct DefaultCommandRunner;

impl DefaultCommandRunner {
    pub(crate) fn new() -> Self {
        Self
    }
}

#[async_trait]
impl CommandRunner for DefaultCommandRunner {
    async fn run_command(&self, command: &mut Command) -> Result<Output, std::io::Error> {
        command.output().await
    }
}

#[async_trait]
pub(crate) trait CommandRunner: Send + Sync {
    async fn run_command(&self, command: &mut Command) -> Result<Output, std::io::Error>;
}

/// Where the commands that create and start a dev container, and their output,
/// are written for the user, since the connection modal only shows a status.
#[derive(Clone)]
pub(crate) struct DevContainerLog {
    path: PathBuf,
}

impl DevContainerLog {
    /// Starts a new log at `path`, replacing the one of the previous start.
    pub(crate) async fn start(path: PathBuf) -> Option<Self> {
        if let Some(parent) = path.parent() {
            smol::fs::create_dir_all(parent).await.ok()?;
        }
        match smol::fs::write(&path, "").await {
            Ok(()) => Some(Self { path }),
            Err(error) => {
                log::warn!(
                    "Can't write the dev container log {}: {error}",
                    path.display()
                );
                None
            }
        }
    }

    pub(crate) async fn record(&self, command: &Command, output: &Result<Output, std::io::Error>) {
        use futures::AsyncWriteExt as _;

        let mut entry = format!("$ {}\n", describe_command(command));
        match output {
            Ok(output) => {
                entry.push_str(&String::from_utf8_lossy(&output.stdout));
                entry.push_str(&String::from_utf8_lossy(&output.stderr));
                if !output.status.success() {
                    entry.push_str(&format!("[{}]\n", output.status));
                }
            }
            Err(error) => entry.push_str(&format!("[failed to run: {error}]\n")),
        }
        if !entry.ends_with('\n') {
            entry.push('\n');
        }
        let result = async {
            let mut file = smol::fs::OpenOptions::new()
                .append(true)
                .open(&self.path)
                .await?;
            file.write_all(entry.as_bytes()).await?;
            file.flush().await
        }
        .await;
        if let Err(error) = result {
            log::warn!("Failed to write the dev container log: {error}");
        }
    }
}

/// The command line, without the values of the environment variables that
/// `docker exec -e` or `docker run -e` passes, which may be secret.
fn describe_command(command: &Command) -> String {
    let mut words = vec![command.get_program().to_string_lossy().into_owned()];
    let mut previous_was_env_flag = false;
    for arg in command.get_args() {
        let arg = arg.to_string_lossy();
        let word = match arg.split_once('=') {
            Some((name, _)) if previous_was_env_flag => format!("{name}=<redacted>"),
            _ => arg.into_owned(),
        };
        previous_was_env_flag = word == "-e" || word == "--env";
        words.push(word);
    }
    words.join(" ")
}

/// Runs commands with `inner`, recording them in the dev container's log.
pub(crate) struct LoggingCommandRunner {
    pub(crate) inner: Arc<dyn CommandRunner>,
    pub(crate) log: DevContainerLog,
}

#[async_trait]
impl CommandRunner for LoggingCommandRunner {
    async fn run_command(&self, command: &mut Command) -> Result<Output, std::io::Error> {
        let output = self.inner.run_command(command).await;
        self.log.record(command, &output).await;
        output
    }
}

pub(crate) async fn evaluate_json_command<T>(
    mut command: Command,
) -> Result<Option<T>, DevContainerError>
where
    T: for<'de> Deserialize<'de>,
{
    let output = command.output().await.map_err(|e| {
        log::error!("Error running command {:?}: {e}", command);
        DevContainerError::CommandFailed(command.get_program().display().to_string())
    })?;

    deserialize_json_output(output).map_err(|e| {
        log::error!("Error running command {:?}: {e}", command);
        DevContainerError::CommandFailed(command.get_program().display().to_string())
    })
}

pub(crate) async fn evaluate_yaml_command<T>(
    mut command: Command,
) -> Result<Option<T>, DevContainerError>
where
    T: for<'de> Deserialize<'de>,
{
    let output = command.output().await.map_err(|e| {
        log::error!("Error running command {:?}: {e}", command);
        DevContainerError::CommandFailed(command.get_program().display().to_string())
    })?;

    deserialize_yaml_output(output).map_err(|e| {
        log::error!("Error running command {:?}: {e}", command);
        DevContainerError::CommandFailed(command.get_program().display().to_string())
    })
}

pub(crate) fn deserialize_yaml_output<T>(output: Output) -> Result<Option<T>, String>
where
    T: for<'de> Deserialize<'de>,
{
    if output.status.success() {
        let raw = String::from_utf8_lossy(&output.stdout);
        if raw.is_empty() || raw.trim() == "[]" || raw.trim() == "{}" {
            return Ok(None);
        }
        serde_yaml::from_str(&raw)
            .map(Some)
            .map_err(|e| format!("Error deserializing from raw yaml: {e}"))
    } else {
        let std_err = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "Sent non-successful output; cannot deserialize. StdErr: {std_err}"
        ))
    }
}

pub(crate) fn deserialize_json_output<T>(output: Output) -> Result<Option<T>, String>
where
    T: for<'de> Deserialize<'de>,
{
    if output.status.success() {
        let raw = String::from_utf8_lossy(&output.stdout);
        if raw.is_empty() || raw.trim() == "[]" || raw.trim() == "{}" {
            return Ok(None);
        }
        serde_json_lenient::from_str(&raw)
            .map_err(|e| format!("Error deserializing from raw json: {e}"))
    } else {
        let std_err = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "Sent non-successful output; cannot deserialize. StdErr: {std_err}"
        ))
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn dev_container_log_records_commands_without_env_values() {
        smol::block_on(async {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("logs").join("dev_container.log");
            let log = super::DevContainerLog::start(path.clone()).await.unwrap();

            let mut command = util::command::new_command("docker");
            command.args([
                "exec",
                "-e",
                "TOKEN=s3cret",
                "-e",
                "API_KEY",
                "container",
                "true",
            ]);
            let output = Ok(std::process::Output {
                status: std::process::ExitStatus::default(),
                stdout: b"built\n".to_vec(),
                stderr: b"warning".to_vec(),
            });
            log.record(&command, &output).await;
            log.record(
                &util::command::new_command("missing"),
                &Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
            )
            .await;

            let contents = std::fs::read_to_string(&path).unwrap();
            assert!(!contents.contains("s3cret"), "{contents}");
            assert!(
                contents.starts_with(
                    "$ docker exec -e TOKEN=<redacted> -e API_KEY container true\nbuilt\nwarning\n"
                ),
                "{contents}"
            );
            assert!(
                contents.contains("$ missing\n[failed to run: "),
                "{contents}"
            );

            // A new start replaces the previous log.
            super::DevContainerLog::start(path.clone()).await.unwrap();
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
        });
    }

    use std::process::ExitStatus;

    use crate::docker::{DockerComposeConfig, DockerComposeServiceBuild};

    use super::*;

    fn success_output(stdout: &str) -> Output {
        Output {
            status: ExitStatus::default(),
            stdout: stdout.as_bytes().to_vec(),
            stderr: Vec::new(),
        }
    }

    #[derive(Debug, Deserialize, PartialEq)]
    struct TestItem {
        id: String,
    }

    #[test]
    fn test_deserialize_newline_delimited_json_rejected() {
        // Strict single-value contract: NDJSON must be rejected. Commands that
        // may legitimately return multiple rows (e.g. `docker ps`) parse their
        // output themselves rather than routing through this helper.
        let output = success_output("{\"id\":\"first\"}\n{\"id\":\"second\"}\n");
        let result: Result<Option<TestItem>, String> = deserialize_json_output(output);
        assert!(result.is_err(), "expected parse error, got {result:?}");
    }

    #[test]
    fn test_deserialize_empty_output() {
        let output = success_output("");
        let result: Option<TestItem> = deserialize_json_output(output).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_deserialize_empty_object() {
        let output = success_output("{}");
        let result: Option<TestItem> = deserialize_json_output(output).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_deserialize_yaml_docker_compose_config() {
        let yaml = indoc::indoc! {"
            name: my-project
            services:
              app:
                image: node:18
                command:
                  - sleep
                  - infinity
                build:
                  context: .
                  dockerfile: Dockerfile
              db:
                image: postgres:15
            volumes: {}
        "};
        let output = success_output(yaml);
        let result: DockerComposeConfig = deserialize_yaml_output(output)
            .expect("deserialization should succeed")
            .expect("result should not be None");

        assert_eq!(result.name, Some("my-project".to_string()));
        assert_eq!(result.services.len(), 2);

        let app = result
            .services
            .get("app")
            .expect("app service should exist");
        assert_eq!(app.image, Some("node:18".to_string()));
        assert_eq!(
            app.command,
            vec!["sleep".to_string(), "infinity".to_string()]
        );
        assert_eq!(
            app.build,
            Some(DockerComposeServiceBuild {
                context: Some(".".to_string()),
                dockerfile: Some("Dockerfile".to_string()),
                ..Default::default()
            })
        );

        let db = result.services.get("db").expect("db service should exist");
        assert_eq!(db.image, Some("postgres:15".to_string()));
    }

    #[test]
    fn test_deserialize_yaml_empty_output() {
        let output = success_output("");
        let result: Option<DockerComposeConfig> = deserialize_yaml_output(output).unwrap();
        assert_eq!(result, None);
    }

    #[test]
    fn test_deserialize_yaml_empty_object() {
        let output = success_output("{}");
        let result: Option<DockerComposeConfig> = deserialize_yaml_output(output).unwrap();
        assert_eq!(result, None);
    }
}
