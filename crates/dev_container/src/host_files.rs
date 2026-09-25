use std::{io, path::Path, sync::Arc};

use anyhow::{Context as _, Result, anyhow};
use fs::Fs;
use remote::{EngineHost, HostCommand};

/// The project's files, which live on the engine host.
///
/// When the machine running Zed can open them (locally, or through
/// `\\wsl.localhost`), they are read through [`Fs`]; otherwise, by running
/// commands on the host.
#[derive(Clone)]
pub(crate) struct HostFiles {
    host: EngineHost,
    fs: Arc<dyn Fs>,
}

/// Exit codes of the scripts below, telling "missing" apart from real failures.
const MISSING: i32 = 3;
const IS_A_DIRECTORY: i32 = 4;

impl HostFiles {
    pub(crate) fn new(host: EngineHost, fs: Arc<dyn Fs>) -> Self {
        Self { host, fs }
    }

    pub(crate) async fn load(&self, path: &Path) -> Result<String> {
        if self.host.has_local_files() {
            return self.fs.load(path).await;
        }
        let bytes = self.load_bytes(path).await?;
        String::from_utf8(bytes).with_context(|| format!("{} is not UTF-8", path.display()))
    }

    pub(crate) async fn load_bytes(&self, path: &Path) -> Result<Vec<u8>> {
        if self.host.has_local_files() {
            return self.fs.load_bytes(path).await;
        }
        let output = self
            .script(
                r#"[ -e "$1" ] || exit 3; [ -d "$1" ] && exit 4; exec cat -- "$1""#,
                path,
            )
            .output()
            .await?;
        match output.status.code() {
            Some(0) => Ok(output.stdout),
            Some(MISSING) => Err(io::Error::from(io::ErrorKind::NotFound).into()),
            Some(IS_A_DIRECTORY) => Err(io::Error::from(io::ErrorKind::IsADirectory).into()),
            _ => Err(anyhow!(
                "failed to read {}: {}",
                path.display(),
                String::from_utf8_lossy(&output.stderr)
            )),
        }
    }

    pub(crate) async fn write(&self, path: &Path, contents: &[u8]) -> Result<()> {
        if self.host.has_local_files() {
            return self.fs.write(path, contents).await;
        }
        let output = self
            .script(r#"cat > "$1""#, path)
            .output_with_stdin(contents)
            .await?;
        if !output.status.success() {
            return Err(anyhow!(
                "failed to write {}: {}",
                path.display(),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        Ok(())
    }

    pub(crate) async fn is_dir(&self, path: &Path) -> bool {
        if self.host.has_local_files() {
            return self.fs.is_dir(path).await;
        }
        self.test("-d", path).await
    }

    /// Copies the folder `source` on the host into `destination` on the machine
    /// running Zed.
    pub(crate) async fn copy_dir_to_local(&self, source: &Path, destination: &Path) -> Result<()> {
        if self.host.has_local_files() {
            return copy_dir(&*self.fs, source, destination).await;
        }
        let output = self
            .script(r#"cd "$1" && exec tar -cf - ."#, source)
            .output()
            .await?;
        if !output.status.success() {
            return Err(anyhow!(
                "failed to archive {}: {}",
                source.display(),
                String::from_utf8_lossy(&output.stderr)
            ));
        }
        self.fs.create_dir(destination).await?;
        async_tar::Archive::new(futures::io::Cursor::new(output.stdout))
            .unpack(destination)
            .await
            .with_context(|| format!("failed to unpack {}", source.display()))
    }

    async fn test(&self, flag: &str, path: &Path) -> bool {
        let mut command = self.host.command("test");
        command.args([flag, &self.host.host_path(path)]);
        command
            .output()
            .await
            .is_ok_and(|output| output.status.success())
    }

    /// Runs a POSIX `script` on the host with `path` as its `$1`.
    fn script(&self, script: &str, path: &Path) -> HostCommand {
        let mut command = self.host.command("sh");
        command.args(["-c", script, "sh", &self.host.host_path(path)]);
        command
    }
}

pub(crate) async fn copy_dir(fs: &dyn Fs, source: &Path, destination: &Path) -> Result<()> {
    for (item_path, is_dir) in fs::read_dir_items(fs, source).await? {
        let relative = item_path.strip_prefix(source)?;
        let dest_path = destination.join(relative);
        if is_dir {
            fs.create_dir(&dest_path).await?;
        } else {
            let content = fs.load_bytes(&item_path).await?;
            fs.write(&dest_path, &content).await?;
        }
    }
    Ok(())
}
