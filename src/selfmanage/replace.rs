use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};

pub fn install(binary: &[u8], executable: &Path) -> Result<()> {
    let directory = executable
        .parent()
        .with_context(|| format!("`{}` has no directory", executable.display()))?;

    let staged = tempfile::Builder::new()
        .prefix(".git-vault-update-")
        .tempfile_in(directory)
        .with_context(|| format!("cannot write into `{}`", directory.display()))?;

    fs::write(staged.path(), binary)
        .with_context(|| format!("cannot write `{}`", staged.path().display()))?;
    make_executable(staged.path())?;

    let retired = retire(executable)?;

    staged
        .persist(executable)
        .with_context(|| format!("cannot replace `{}`", executable.display()))?;

    if let Some(path) = retired {
        delete_after_exit(&path);
    }

    Ok(())
}

pub fn remove(executable: &Path) -> Result<()> {
    if let Some(retired) = retire(executable)? {
        delete_after_exit(&retired);
        return Ok(());
    }

    fs::remove_file(executable).with_context(|| format!("cannot remove `{}`", executable.display()))
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    fs::set_permissions(path, fs::Permissions::from_mode(0o755))
        .with_context(|| format!("cannot make `{}` executable", path.display()))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
#[expect(
    clippy::unnecessary_wraps,
    reason = "the Windows implementation can fail, and both must share one signature"
)]
const fn retire(_executable: &Path) -> Result<Option<PathBuf>> {
    Ok(None)
}

#[cfg(windows)]
fn retire(executable: &Path) -> Result<Option<PathBuf>> {
    let retired = executable.with_extension(format!("old-{}", std::process::id()));

    let _gone = fs::remove_file(&retired);
    fs::rename(executable, &retired).with_context(|| {
        format!(
            "cannot move `{}` aside, which Windows requires before replacing a running program",
            executable.display()
        )
    })?;

    Ok(Some(retired))
}

#[cfg(unix)]
const fn delete_after_exit(_path: &Path) {}

#[cfg(windows)]
fn delete_after_exit(path: &Path) {
    use std::os::windows::process::CommandExt as _;

    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let script = format!(
        "Wait-Process -Id {} -ErrorAction SilentlyContinue; Remove-Item -LiteralPath '{}' -Force -ErrorAction SilentlyContinue",
        std::process::id(),
        path.display()
    );

    let _spawned = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-WindowStyle",
            "Hidden",
            "-Command",
            &script,
        ])
        .creation_flags(DETACHED_PROCESS | CREATE_NO_WINDOW)
        .spawn();
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::{install, remove};

    #[test]
    fn installing_replaces_the_file_in_place() {
        let directory = TempDir::new().unwrap();
        let binary = directory.path().join("git-vault");
        fs::write(&binary, b"old").unwrap();

        install(b"new", &binary).unwrap();

        assert_eq!(fs::read(&binary).unwrap(), b"new");
    }

    #[cfg(unix)]
    #[test]
    fn an_installed_binary_is_executable() {
        use std::os::unix::fs::PermissionsExt as _;

        let directory = TempDir::new().unwrap();
        let binary = directory.path().join("git-vault");
        fs::write(&binary, b"old").unwrap();

        install(b"new", &binary).unwrap();

        let mode = fs::metadata(&binary).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o755);
    }

    #[test]
    fn removing_deletes_the_file() {
        let directory = TempDir::new().unwrap();
        let binary = directory.path().join("git-vault");
        fs::write(&binary, b"here").unwrap();

        remove(&binary).unwrap();

        assert!(!binary.exists());
    }

    #[test]
    fn installing_into_a_missing_directory_says_so() {
        let directory = TempDir::new().unwrap();
        let binary = directory.path().join("nowhere/git-vault");

        assert!(install(b"new", &binary).is_err());
    }
}
