use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context as _, Result, bail};

use crate::exit::Code;
use crate::paths;
use crate::repo::{Repo, index};
use crate::vault::seal::Secret;

pub fn diff(wanted: &[PathBuf]) -> Result<Code> {
    let repo = Repo::discover()?;

    if !repo.is_unlocked() {
        println!("Locked, so there is nothing to compare. Run `git vault unlock`.");
        return Ok(Code::Locked);
    }

    let staged = match index::staged_blob(repo.worktree(), paths::DATA)? {
        Some(blob) => repo.unseal(&blob)?,
        None => Vec::new(),
    };
    let live = repo.secrets()?;
    let filter = Filter::new(wanted);

    let mut printed = false;
    for path in union(&staged, &live) {
        if !filter.wants(&path) {
            continue;
        }

        let before = find(&staged, &path);
        let after = find(&live, &path);

        if before.map(|secret| &secret.content) == after.map(|secret| &secret.content) {
            continue;
        }

        printed = true;
        print!("{}", render(&path, before, after));
    }

    if !printed {
        println!("No secret changes.");
    }

    Ok(Code::Ok)
}

pub fn log(wanted: Option<&Path>) -> Result<Code> {
    let repo = Repo::discover()?;

    if !repo.is_unlocked() {
        println!("Locked, so the history cannot be read. Run `git vault unlock`.");
        return Ok(Code::Locked);
    }

    let commits = commits_touching_the_vault(repo.worktree())?;
    if commits.is_empty() {
        println!("The vault has no history yet.");
        return Ok(Code::Ok);
    }

    let filter = wanted.map(|path| path.to_string_lossy().replace('\\', "/"));
    let mut previous: Option<Vec<Secret>> = None;
    let mut printed = false;

    for commit in commits.iter().rev() {
        let secrets = repo.unseal(&blob_at(repo.worktree(), &commit.id)?)?;
        let before = previous.take().unwrap_or_default();

        let changed: Vec<String> = union(&before, &secrets)
            .into_iter()
            .filter(|path| filter.as_ref().is_none_or(|wanted| wanted == path))
            .filter(|path| {
                find(&before, path).map(|secret| &secret.content)
                    != find(&secrets, path).map(|secret| &secret.content)
            })
            .collect();

        if !changed.is_empty() {
            printed = true;
            println!("{} {}", commit.short(), commit.subject);
            for path in changed {
                print!(
                    "{}",
                    render(&path, find(&before, &path), find(&secrets, &path))
                );
            }
            println!();
        }

        previous = Some(secrets);
    }

    if !printed {
        match filter {
            Some(path) => println!("`{path}` never changed in the vault's history."),
            None => println!("The vault never changed."),
        }
    }

    Ok(Code::Ok)
}

fn render(path: &str, before: Option<&Secret>, after: Option<&Secret>) -> String {
    let empty = Vec::new();
    let original = before.map_or(&empty, |secret| &secret.content);
    let modified = after.map_or(&empty, |secret| &secret.content);

    let marker = match (before, after) {
        (None, Some(_added)) => "added",
        (Some(_removed), None) => "removed",
        _changed => "changed",
    };

    let mut rendered = format!("{path} ({marker})\n");

    match (std::str::from_utf8(original), std::str::from_utf8(modified)) {
        (Ok(was), Ok(is_now)) => {
            let unified = diffy::create_patch(was, is_now);
            for line in unified.to_string().lines().skip(2) {
                rendered.push_str("  ");
                rendered.push_str(line);
                rendered.push('\n');
            }
        }
        _binary => {
            rendered.push_str("  binary contents differ\n");
        }
    }

    rendered
}

struct Filter {
    wanted: BTreeSet<String>,
}

impl Filter {
    fn new(paths: &[PathBuf]) -> Self {
        Self {
            wanted: paths
                .iter()
                .map(|path| path.to_string_lossy().replace('\\', "/"))
                .collect(),
        }
    }

    fn wants(&self, path: &str) -> bool {
        self.wanted.is_empty() || self.wanted.contains(path)
    }
}

struct Commit {
    id: String,
    subject: String,
}

impl Commit {
    fn short(&self) -> String {
        self.id.chars().take(8).collect()
    }
}

fn commits_touching_the_vault(worktree: &Path) -> Result<Vec<Commit>> {
    let output = Command::new("git")
        .current_dir(worktree)
        .args(["log", "--format=%H%x00%s", "--", paths::DATA])
        .output()
        .context("cannot run git to read the vault's history")?;

    if !output.status.success() {
        bail!("`git log -- {}` failed", paths::DATA);
    }

    Ok(String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| line.split_once('\0'))
        .map(|(id, subject)| Commit {
            id: id.to_owned(),
            subject: subject.to_owned(),
        })
        .collect())
}

fn blob_at(worktree: &Path, commit: &str) -> Result<Vec<u8>> {
    let spec = format!("{commit}:{}", paths::DATA);
    let output = Command::new("git")
        .current_dir(worktree)
        .args(["cat-file", "blob", &spec])
        .output()
        .context("cannot run git to read a past vault")?;

    if !output.status.success() {
        bail!("`{spec}` cannot be read");
    }

    Ok(output.stdout)
}

fn union(before: &[Secret], after: &[Secret]) -> BTreeSet<String> {
    before
        .iter()
        .chain(after)
        .map(|secret| secret.path.clone())
        .collect()
}

fn find<'a>(secrets: &'a [Secret], path: &str) -> Option<&'a Secret> {
    secrets.iter().find(|secret| secret.path == path)
}
