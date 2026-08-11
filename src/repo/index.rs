use std::path::Path;
use std::process::Command;

use anyhow::{Context as _, Result, bail};

pub fn staged_blob(worktree: &Path, rel: &str) -> Result<Option<Vec<u8>>> {
    let spec = format!(":{rel}");
    let output = Command::new("git")
        .current_dir(worktree)
        .args(["cat-file", "blob", &spec])
        .output()
        .context("cannot run git to read the staged vault")?;

    if output.status.success() {
        return Ok(Some(output.stdout));
    }

    Ok(None)
}

pub fn staged_for_commit(worktree: &Path) -> Result<Vec<String>> {
    let output = Command::new("git")
        .current_dir(worktree)
        .args(["diff", "--cached", "--name-only", "--no-renames"])
        .output()
        .context("cannot run git to list what is staged")?;

    if !output.status.success() {
        bail!("`git diff --cached` failed");
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_owned)
        .collect())
}

pub fn tracked(worktree: &Path, paths: &[String]) -> Result<Vec<String>> {
    if paths.is_empty() {
        return Ok(Vec::new());
    }

    let mut command = Command::new("git");
    command
        .current_dir(worktree)
        .args(["ls-files", "--cached", "--"]);
    command.args(paths);

    let output = command
        .output()
        .context("cannot run git to list tracked files")?;

    if !output.status.success() {
        bail!("`git ls-files` failed");
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(str::to_owned)
        .collect())
}

pub fn untrack(worktree: &Path, rel: &str) -> Result<Vec<String>> {
    let before = tracked(worktree, &[rel.to_owned()])?;

    for path in &before {
        let status = Command::new("git")
            .current_dir(worktree)
            .args(["update-index", "--force-remove", "--", path])
            .status()
            .context("cannot run git to untrack a path")?;

        if !status.success() {
            bail!("`git update-index --force-remove -- {path}` failed");
        }
    }

    Ok(before)
}

pub fn config(worktree: &Path, key: &str) -> Result<Option<String>> {
    let output = Command::new("git")
        .current_dir(worktree)
        .args(["config", "--local", "--get", key])
        .output()
        .context("cannot run git to read the local configuration")?;

    if !output.status.success() {
        return Ok(None);
    }

    Ok(Some(
        String::from_utf8_lossy(&output.stdout)
            .trim_end()
            .to_owned(),
    ))
}

pub fn index_flag(worktree: &Path, rel: &str) -> Result<Option<char>> {
    let output = Command::new("git")
        .current_dir(worktree)
        .args(["ls-files", "-v", "--", rel])
        .output()
        .context("cannot run git to inspect the index")?;

    if !output.status.success() {
        return Ok(None);
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .and_then(|line| line.chars().next()))
}
