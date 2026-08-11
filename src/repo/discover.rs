use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Repo {
    common_dir: PathBuf,
    git_dir: PathBuf,
    worktree: PathBuf,
}

impl Repo {
    pub fn discover() -> Result<Self> {
        let cwd = env::current_dir().context("cannot read the current directory")?;

        match env::var_os("GIT_DIR") {
            Some(git_dir) => {
                let worktree = match env::var_os("GIT_WORK_TREE") {
                    Some(path) => canonical(Path::new(&path))?,
                    None => cwd,
                };
                let git_dir = canonical(Path::new(&git_dir))?;
                let common_dir = common_dir_of(&git_dir)?;
                Ok(Self {
                    common_dir,
                    git_dir,
                    worktree,
                })
            }
            None => Self::discover_from(&cwd),
        }
    }

    pub fn discover_from(start: &Path) -> Result<Self> {
        for candidate in start.ancestors() {
            let Some(git_dir) = git_dir_at(candidate)? else {
                continue;
            };

            return Ok(Self {
                worktree: canonical(candidate)?,
                common_dir: common_dir_of(&git_dir)?,
                git_dir,
            });
        }

        bail!(
            "not inside a git worktree, looking upwards from `{}`",
            start.display()
        )
    }

    pub fn worktree(&self) -> &Path {
        &self.worktree
    }

    pub fn operation_in_progress(&self) -> bool {
        [
            "BISECT_LOG",
            "CHERRY_PICK_HEAD",
            "MERGE_HEAD",
            "REVERT_HEAD",
            "rebase-apply",
            "rebase-merge",
            "sequencer",
        ]
        .iter()
        .any(|name| self.git_dir.join(name).exists())
    }

    pub fn common_dir(&self) -> &Path {
        &self.common_dir
    }

    pub fn vault_dir(&self) -> PathBuf {
        self.common_dir.join("vault")
    }

    pub fn key_path(&self) -> PathBuf {
        self.vault_dir().join("key")
    }

    pub fn is_unlocked(&self) -> bool {
        self.key_path().is_file()
    }
}

fn git_dir_at(dir: &Path) -> Result<Option<PathBuf>> {
    let dot_git = dir.join(".git");

    if dot_git.is_dir() {
        return canonical(&dot_git).map(Some);
    }

    if !dot_git.is_file() {
        return Ok(None);
    }

    let contents = fs::read_to_string(&dot_git)
        .with_context(|| format!("cannot read `{}`", dot_git.display()))?;
    let target = contents
        .lines()
        .find_map(|line| line.trim().strip_prefix("gitdir:"))
        .map(str::trim)
        .with_context(|| format!("`{}` carries no `gitdir:` line", dot_git.display()))?;

    canonical(&dir.join(target)).map(Some)
}

fn common_dir_of(git_dir: &Path) -> Result<PathBuf> {
    let pointer = git_dir.join("commondir");

    if !pointer.is_file() {
        return Ok(git_dir.to_path_buf());
    }

    let contents = fs::read_to_string(&pointer)
        .with_context(|| format!("cannot read `{}`", pointer.display()))?;
    let target = contents
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .with_context(|| format!("`{}` is empty", pointer.display()))?;

    canonical(&git_dir.join(target))
}

fn canonical(path: &Path) -> Result<PathBuf> {
    fs::canonicalize(path)
        .map(crate::paths::without_verbatim_prefix)
        .with_context(|| format!("cannot resolve `{}`", path.display()))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::Repo;

    #[test]
    fn finds_a_plain_repository_from_a_subdirectory() {
        let dir = TempDir::new().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        fs::create_dir(root.join(".git")).unwrap();
        let deep = root.join("deep/sub");
        fs::create_dir_all(&deep).unwrap();

        let repo = Repo::discover_from(&deep).unwrap();

        assert_eq!(repo.worktree(), root);
        assert_eq!(repo.key_path(), root.join(".git/vault/key"));
        assert!(!repo.is_unlocked());
    }

    #[test]
    fn a_linked_worktree_shares_the_common_directory() {
        let dir = TempDir::new().unwrap();
        let base = fs::canonicalize(dir.path()).unwrap();

        let main_git = base.join("main/.git");
        let worktree_git = main_git.join("worktrees/feature");
        fs::create_dir_all(&worktree_git).unwrap();
        fs::write(worktree_git.join("commondir"), "../..\n").unwrap();

        let linked = base.join("feature");
        fs::create_dir(&linked).unwrap();
        fs::write(
            linked.join(".git"),
            format!("gitdir: {}\n", worktree_git.display()),
        )
        .unwrap();

        let repo = Repo::discover_from(&linked).unwrap();

        assert_eq!(repo.worktree(), linked);
        assert_eq!(repo.key_path(), main_git.join("vault/key"));
    }

    #[test]
    fn a_relative_gitdir_pointer_resolves_against_the_worktree() {
        let dir = TempDir::new().unwrap();
        let base = fs::canonicalize(dir.path()).unwrap();

        let worktree_git = base.join("main/.git/worktrees/feature");
        fs::create_dir_all(&worktree_git).unwrap();
        fs::write(worktree_git.join("commondir"), "../..").unwrap();

        let linked = base.join("feature");
        fs::create_dir(&linked).unwrap();
        fs::write(
            linked.join(".git"),
            "gitdir: ../main/.git/worktrees/feature",
        )
        .unwrap();

        let repo = Repo::discover_from(&linked).unwrap();

        assert_eq!(repo.vault_dir(), base.join("main/.git/vault"));
    }

    #[test]
    fn fails_outside_a_worktree() {
        let dir = TempDir::new().unwrap();
        let path = fs::canonicalize(dir.path()).unwrap();

        let error = Repo::discover_from(&path).unwrap_err();

        assert!(error.to_string().contains("not inside a git worktree"));
    }
}
