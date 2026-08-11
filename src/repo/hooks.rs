use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Installed {
    AlreadyOurs,
    Foreign,
    Written,
}

const PRE_COMMIT: &str = "\
#!/bin/sh
# Installed by git-vault. Seals the live secrets, so a commit can never carry a
# stale vault. It stages nothing: what a commit carries is what you staged.
# Remove this hook and `git vault seal` becomes your job.
exec git-vault hook pre-commit
";

fn hook_path(common_dir: &Path, name: &str) -> PathBuf {
    common_dir.join("hooks").join(name)
}

pub fn install_pre_commit(common_dir: &Path) -> Result<Installed> {
    let path = hook_path(common_dir, "pre-commit");

    if let Ok(existing) = fs::read_to_string(&path) {
        if existing.contains("git-vault hook pre-commit") {
            return Ok(Installed::AlreadyOurs);
        }
        return Ok(Installed::Foreign);
    }

    let parent = path
        .parent()
        .with_context(|| format!("`{}` has no parent directory", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("cannot create `{}`", parent.display()))?;

    write_executable(&path, PRE_COMMIT)
        .with_context(|| format!("cannot write `{}`", path.display()))?;

    Ok(Installed::Written)
}

pub const fn pre_commit_line() -> &'static str {
    "git-vault hook pre-commit || exit 1"
}

#[cfg(unix)]
fn write_executable(path: &Path, contents: &str) -> Result<()> {
    use std::io::Write as _;
    use std::os::unix::fs::OpenOptionsExt as _;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o755)
        .open(path)?;
    file.write_all(contents.as_bytes())?;
    Ok(())
}

#[cfg(not(unix))]
fn write_executable(path: &Path, contents: &str) -> Result<()> {
    fs::write(path, contents)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::{Installed, install_pre_commit};

    #[test]
    fn a_fresh_repository_gets_the_hook() {
        let dir = TempDir::new().unwrap();

        assert_eq!(install_pre_commit(dir.path()).unwrap(), Installed::Written);

        let written = std::fs::read_to_string(dir.path().join("hooks/pre-commit")).unwrap();
        assert!(written.contains("git-vault hook pre-commit"), "{written}");
    }

    #[test]
    fn installing_twice_changes_nothing() {
        let dir = TempDir::new().unwrap();

        install_pre_commit(dir.path()).unwrap();

        assert_eq!(
            install_pre_commit(dir.path()).unwrap(),
            Installed::AlreadyOurs
        );
    }

    #[test]
    fn somebody_elses_hook_is_left_alone() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("hooks")).unwrap();
        std::fs::write(
            dir.path().join("hooks/pre-commit"),
            "#!/bin/sh\nmake lint\n",
        )
        .unwrap();

        assert_eq!(install_pre_commit(dir.path()).unwrap(), Installed::Foreign);
        assert_eq!(
            std::fs::read_to_string(dir.path().join("hooks/pre-commit")).unwrap(),
            "#!/bin/sh\nmake lint\n"
        );
    }
}
