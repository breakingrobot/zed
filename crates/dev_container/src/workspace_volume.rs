//! Dev containers whose sources live in a container volume rather than in a
//! folder of the engine host, like VS Code's "Clone Repository in Container
//! Volume": faster on engines that run in a VM, like Docker Desktop.

use std::path::{Path, PathBuf};

use remote::EngineHost;
use util::ResultExt as _;

use crate::{
    devcontainer_api::DevContainerError,
    docker::{Docker, DockerClient as _},
};

/// The image that clones repositories into volumes and copies them out.
const GIT_IMAGE: &str = "alpine/git:latest";

/// Where the copies of cloned repositories are kept, one folder per volume. Zed
/// reads the dev container configuration and builds from a copy, while the
/// container works on the volume.
pub(crate) fn volume_copies_directory() -> PathBuf {
    paths::devcontainer_dir().join("volumes")
}

/// The volume whose copy `project_directory` is, if it's one.
pub(crate) fn workspace_volume_of(project_directory: &Path) -> Option<String> {
    let relative = project_directory
        .strip_prefix(volume_copies_directory())
        .ok()?;
    let volume = relative.components().next()?;
    Some(volume.as_os_str().to_string_lossy().into_owned())
}

/// The repository's folder name, from its URL, e.g. `zed` for
/// `https://github.com/zed-industries/zed.git` or `git@github.com:zed-industries/zed`.
pub(crate) fn repository_name(url: &str) -> Option<String> {
    let url = url.trim().trim_end_matches('/');
    let last = url.rsplit(['/', ':']).next()?;
    let name = last.strip_suffix(".git").unwrap_or(last);
    let is_safe = !name.is_empty()
        && name != "."
        && name != ".."
        && name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "-_.".contains(character));
    is_safe.then(|| name.to_string())
}

/// A volume name for `url` that stays the same when the repository is cloned
/// again.
pub(crate) fn volume_name(url: &str, repository: &str) -> String {
    use sha2::{Digest, Sha256};

    let digest = format!("{:x}", Sha256::digest(url.trim().as_bytes()));
    format!("zed-{}-{}", repository.to_lowercase(), &digest[..12])
}

/// Clones `url` into a volume, unless an earlier clone is there, and returns the
/// folder of this machine to open as the dev container's project.
pub async fn clone_repository_in_volume(
    url: &str,
    use_podman: bool,
) -> Result<PathBuf, DevContainerError> {
    let repository = repository_name(url).ok_or_else(|| {
        DevContainerError::DevContainerValidationFailed(format!(
            "{url} doesn't look like the URL of a git repository"
        ))
    })?;
    let volume = volume_name(url, &repository);
    let docker = docker(use_podman).await;

    run(&docker, &["volume", "create", &volume]).await?;
    let mount = format!("type=volume,source={volume},target=/workspaces");
    let target = format!("/workspaces/{repository}");
    run(
        &docker,
        &[
            "run",
            "--rm",
            "--mount",
            &mount,
            "--entrypoint",
            "sh",
            GIT_IMAGE,
            "-c",
            r#"[ -d "$2/.git" ] || git clone -- "$1" "$2""#,
            "sh",
            url.trim(),
            &target,
        ],
    )
    .await?;
    refresh_volume_copy(&volume, use_podman).await?;
    Ok(volume_copies_directory().join(&volume).join(repository))
}

/// Replaces the copy of `volume` on this machine with what the volume holds now.
/// `docker cp` streams it, so this also works with an engine on another machine.
pub(crate) async fn refresh_volume_copy(
    volume: &str,
    use_podman: bool,
) -> Result<(), DevContainerError> {
    let copy = volume_copies_directory().join(volume);
    let recreate = async {
        if smol::fs::metadata(&copy).await.is_ok() {
            smol::fs::remove_dir_all(&copy).await?;
        }
        smol::fs::create_dir_all(&copy).await
    };
    recreate.await.map_err(|error| {
        log::error!("Failed to prepare {}: {error}", copy.display());
        DevContainerError::FilesystemError
    })?;

    let docker = docker(use_podman).await;
    let source = format!("type=volume,source={volume},target=/source,readonly");
    let container_id = run(&docker, &["create", "--mount", &source, GIT_IMAGE]).await?;
    let container_id = container_id.trim();
    let copied = run(
        &docker,
        &[
            "cp",
            &format!("{container_id}:/source/."),
            &copy.display().to_string(),
        ],
    )
    .await;
    run(&docker, &["rm", "-f", container_id]).await.log_err();
    copied.map(|_| ())
}

async fn docker(use_podman: bool) -> Docker {
    Docker::without_builds(
        if use_podman { "podman" } else { "docker" },
        EngineHost::Local,
    )
    .await
}

/// Runs a docker command and returns its standard output.
async fn run(docker: &Docker, args: &[&str]) -> Result<String, DevContainerError> {
    let mut command = docker.docker_command();
    command.args(args);
    let output = command.output().await.map_err(|error| {
        log::error!("Failed to run {command}: {error}");
        DevContainerError::CommandFailed(command.get_program().to_string())
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        log::error!("{command} failed: {stderr}");
        return Err(DevContainerError::DevContainerUpFailed(stderr.into_owned()));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(test)]
mod tests {
    #[test]
    fn repository_names_come_from_the_url() {
        for (url, name) in [
            ("https://github.com/zed-industries/zed.git", Some("zed")),
            ("https://github.com/zed-industries/zed/", Some("zed")),
            ("git@github.com:zed-industries/zed.git", Some("zed")),
            ("git@example.com:project", Some("project")),
            ("https://example.com/../..", None),
            ("https://example.com/a b", None),
            ("", None),
        ] {
            assert_eq!(super::repository_name(url).as_deref(), name, "{url}");
        }
    }

    #[test]
    fn volumes_are_named_after_the_repository_and_url() {
        let first = super::volume_name("https://github.com/zed-industries/zed.git", "zed");
        assert!(first.starts_with("zed-zed-"), "{first}");
        assert_eq!(
            first,
            super::volume_name(" https://github.com/zed-industries/zed.git ", "zed")
        );
        assert_ne!(
            first,
            super::volume_name("https://github.com/someone/zed.git", "zed")
        );
    }

    #[test]
    fn volume_copies_know_their_volume() {
        let copy = super::volume_copies_directory()
            .join("zed-zed-0123456789ab")
            .join("zed");
        assert_eq!(
            super::workspace_volume_of(&copy).as_deref(),
            Some("zed-zed-0123456789ab")
        );
        assert_eq!(
            super::workspace_volume_of(std::path::Path::new("/src/zed")),
            None
        );
    }
}
