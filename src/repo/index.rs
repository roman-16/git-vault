use std::io::Write as _;
use std::path::Path;
use std::process::{Command, Stdio};

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

pub fn in_head(worktree: &Path, rel: &str) -> Result<bool> {
    let spec = format!("HEAD:{rel}");
    let output = Command::new("git")
        .current_dir(worktree)
        .args(["cat-file", "-e", &spec])
        .output()
        .context("cannot run git to look inside the last commit")?;

    Ok(output.status.success())
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

pub fn untrack(worktree: &Path, paths: &[String]) -> Result<()> {
    if paths.is_empty() {
        return Ok(());
    }

    let mut child = Command::new("git")
        .current_dir(worktree)
        .args(["update-index", "-z", "--force-remove", "--stdin"])
        .stdin(Stdio::piped())
        .spawn()
        .context("cannot run git to untrack paths")?;

    let mut input = child
        .stdin
        .take()
        .context("git accepted no list of paths to untrack")?;
    for path in paths {
        input
            .write_all(path.as_bytes())
            .and_then(|()| input.write_all(b"\0"))
            .context("cannot hand git the paths to untrack")?;
    }
    drop(input);

    let status = child
        .wait()
        .context("cannot wait for git to untrack paths")?;

    if !status.success() {
        bail!(
            "`git update-index --force-remove` failed for {}",
            paths.join(" ")
        );
    }

    Ok(())
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
