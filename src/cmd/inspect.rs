use std::collections::BTreeSet;
use std::path::PathBuf;

use anyhow::Result;

use crate::exit::Code;
use crate::paths;
use crate::repo::session::Empty;
use crate::repo::{Repo, index};
use crate::size;
use crate::vault::format::Vault;
use crate::vault::seal::{Kind, Secret};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Change {
    Added,
    Modified,
    Deleted,
}

impl Change {
    const fn letter(self) -> char {
        match self {
            Self::Added => 'A',
            Self::Modified => 'M',
            Self::Deleted => 'D',
        }
    }
}

pub fn ls() -> Result<Code> {
    let repo = Repo::discover()?;

    if !repo.is_unlocked() {
        println!("Locked, so the inventory cannot be read. Run `git vault unlock`.");
        return Ok(Code::Locked);
    }

    let secrets = repo.secrets()?;

    if secrets.is_empty() {
        println!("Nothing is sealed yet. Declare something with `git vault add`.");
        return Ok(Code::Ok);
    }

    for secret in &secrets {
        println!(
            "{:>9}  {}  {}",
            size::human(secret.content.len()),
            kind_of(secret),
            secret.path
        );
    }

    let total: usize = secrets.iter().fold(0_usize, |running, secret| {
        running.saturating_add(secret.content.len())
    });

    println!();
    println!(
        "{} secret{}, {} in total.",
        secrets.len(),
        if secrets.len() == 1 { "" } else { "s" },
        size::human(total)
    );

    match sealed_entries(&repo) {
        Some(entries) if entries != secrets.len() => println!(
            "{} still holds {entries}. The next git command seals what is listed above.",
            paths::DATA
        ),
        _agreed => println!("That is what {} holds.", paths::DATA),
    }

    Ok(Code::Ok)
}

fn sealed_entries(repo: &Repo) -> Option<usize> {
    let sealed = repo.read_data().ok()?;

    Vault::decode(&sealed).ok().map(|vault| vault.entries.len())
}

pub fn status() -> Result<Code> {
    let repo = Repo::discover()?;

    if !repo.is_unlocked() {
        println!("Locked, so secret changes cannot be seen. Run `git vault unlock`.");
        return Ok(Code::Locked);
    }

    let changes = compare(&repo)?;

    if changes.is_empty() {
        println!("No secret changes.");
        return Ok(Code::Ok);
    }

    for (change, path) in &changes {
        println!("{} {path}", change.letter());
    }
    println!();
    println!("Stage them with `git add {}`.", paths::DATA);

    Ok(Code::Ok)
}

pub fn restore(arguments: &[PathBuf]) -> Result<Code> {
    let repo = Repo::discover()?;

    if !repo.is_unlocked() {
        println!("Locked, so there is nothing to restore. Run `git vault unlock`.");
        return Ok(Code::Locked);
    }

    let staged = staged_secrets(&repo)?;
    let wanted: BTreeSet<String> = arguments
        .iter()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .collect();

    let subset: Vec<Secret> = if wanted.is_empty() {
        staged
    } else {
        staged
            .into_iter()
            .filter(|secret| wanted.contains(&secret.path))
            .collect()
    };

    let applied = if wanted.is_empty() {
        repo.apply(&subset)?
    } else {
        repo.apply_only(&subset)?
    };

    println!(
        "Restored {} secret{}.",
        applied.written,
        if applied.written == 1 { "" } else { "s" }
    );
    if applied.removed > 0 {
        println!("Removed {} that were not sealed.", applied.removed);
    }
    if applied.written == 0 && applied.removed == 0 {
        println!("Nothing to restore.");
    }

    repo.seal_worktree(Empty::Refuse)?;

    Ok(Code::Ok)
}

fn staged_secrets(repo: &Repo) -> Result<Vec<Secret>> {
    let Some(blob) = index::staged_blob(repo.worktree(), paths::DATA)? else {
        return Ok(Vec::new());
    };

    repo.unseal(&blob)
}

fn compare(repo: &Repo) -> Result<Vec<(Change, String)>> {
    let staged = staged_secrets(repo)?;
    let live = repo.secrets()?;

    let mut changes = Vec::new();

    for secret in &live {
        match staged.iter().find(|other| other.path == secret.path) {
            None => changes.push((Change::Added, secret.path.clone())),
            Some(other) if other != secret => {
                changes.push((Change::Modified, secret.path.clone()));
            }
            Some(_unchanged) => {}
        }
    }

    for secret in &staged {
        if !live.iter().any(|other| other.path == secret.path) {
            changes.push((Change::Deleted, secret.path.clone()));
        }
    }

    changes.sort_by(|left, right| left.1.cmp(&right.1));

    Ok(changes)
}

const fn kind_of(secret: &Secret) -> &'static str {
    match secret.kind {
        Kind::File { executable: true } => "exec",
        Kind::File { executable: false } => "file",
        Kind::Symlink => "link",
    }
}
