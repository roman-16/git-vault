use std::cmp::Reverse;
use std::collections::BTreeSet;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::slice;

use anyhow::{Context as _, Result, bail};

use crate::exit::Code;
use crate::paths;
use crate::repo::session::Empty;
use crate::repo::{Repo, index, wiring};
use crate::size::{self, LOUD_ENOUGH_TO_MENTION};

pub fn add(arguments: &[PathBuf]) -> Result<Code> {
    let repo = Repo::discover()?;
    let worktree = repo.worktree().to_path_buf();
    let patterns = arguments
        .iter()
        .map(|argument| pattern_for(&worktree, argument))
        .collect::<Result<Vec<_>>>()?;

    let matched = patterns
        .iter()
        .map(|pattern| {
            index::tracked(&worktree, slice::from_ref(pattern))
                .map(|paths| (pattern.clone(), paths))
        })
        .collect::<Result<Vec<_>>>()?;
    let declared_already = repo.tracked_secrets()?;

    let leaked: BTreeSet<&String> = matched
        .iter()
        .flat_map(|(_pattern, paths)| paths)
        .chain(&declared_already)
        .collect();
    index::untrack(
        &worktree,
        &leaked
            .iter()
            .map(|path| (*path).clone())
            .collect::<Vec<_>>(),
    )?;

    let attributes_changed = wiring::ensure_sealed(&worktree, &patterns)?;
    let ignore_changed = wiring::ensure_ignored(&worktree, &patterns)?;

    for (pattern, untracked) in &matched {
        println!("Sealing {pattern}");

        for path in untracked {
            println!("  untracked {path}, so its plaintext stops being committed");
        }
        if !untracked.is_empty() {
            println!("  its earlier contents are still in this repository's history");
        }

        if nothing_there_yet(&worktree, pattern) {
            println!("  nothing there yet, which is fine: it will be sealed when it appears");
        }
    }

    let repaired: Vec<&String> = declared_already
        .iter()
        .filter(|path| {
            !matched
                .iter()
                .any(|(_pattern, untracked)| untracked.contains(path))
        })
        .collect();
    for path in repaired {
        println!(
            "Untracked {path}, which {} already declared secret",
            paths::ATTRIBUTES
        );
    }

    if !attributes_changed && !ignore_changed && leaked.is_empty() {
        println!("Nothing to change.");
        return Ok(Code::Ok);
    }

    if repo.is_unlocked() {
        repo.seal_worktree(Empty::Refuse)?;
        report_bulk(&repo)?;
    }

    println!();
    println!("Commit the declarations along with the vault:");
    println!("  git add .gitattributes .gitignore .vault");

    Ok(Code::Ok)
}

pub fn remove(arguments: &[PathBuf]) -> Result<Code> {
    let repo = Repo::discover()?;
    let worktree = repo.worktree().to_path_buf();
    let patterns = arguments
        .iter()
        .map(|argument| pattern_for(&worktree, argument))
        .collect::<Result<Vec<_>>>()?;

    let mut removed = Vec::new();
    for pattern in &patterns {
        let undeclared = drop_line(
            &worktree.join(paths::ATTRIBUTES),
            &format!("{pattern} {}", wiring::sealed_attributes()),
        )?;
        let unignored = drop_line(&worktree.join(paths::IGNORE), pattern)?;

        if undeclared || unignored {
            removed.push(pattern.clone());
        }
    }

    if removed.is_empty() {
        println!("Nothing was sealed by those patterns.");
        return Ok(Code::Ok);
    }

    if repo.is_unlocked() {
        repo.seal_worktree(Empty::Allow)?;
    }

    for pattern in &removed {
        println!("No longer sealing {pattern}");
    }
    println!();
    println!("Those files are ordinary files again. Add them if you want them committed");
    println!("in the clear, and remember that the vault's history still holds what they");
    println!("used to contain.");

    let still_tracked = index::tracked(&worktree, &removed)?;
    if !still_tracked.is_empty() {
        println!();
        for path in &still_tracked {
            println!("{path} is already tracked, so the next commit publishes it.");
        }
    }

    Ok(Code::Ok)
}

fn report_bulk(repo: &Repo) -> Result<()> {
    let mut secrets = repo.secrets()?;
    let total = secrets.iter().fold(0_usize, |running, secret| {
        running.saturating_add(secret.content.len())
    });

    println!();
    println!(
        "{} secret{}, {} in total.",
        secrets.len(),
        if secrets.len() == 1 { "" } else { "s" },
        size::human(total)
    );

    if total <= LOUD_ENOUGH_TO_MENTION {
        return Ok(());
    }

    secrets.sort_by_key(|secret| Reverse(secret.content.len()));

    println!();
    println!("That is a lot to seal again on every git command. The largest are:");
    for secret in secrets.iter().take(3) {
        println!(
            "  {:>9}  {}",
            size::human(secret.content.len()),
            secret.path
        );
    }

    if let Some(biggest) = secrets.first()
        && let Some(parent) = Path::new(&biggest.path).parent().and_then(Path::to_str)
        && !parent.is_empty()
    {
        println!();
        println!(
            "Build output belongs outside the vault. Exclude it in {}:",
            paths::ATTRIBUTES
        );
        println!("  {parent}/** -vault");
    }

    Ok(())
}

fn nothing_there_yet(worktree: &Path, pattern: &str) -> bool {
    let root = root_of(pattern);

    !root.contains('*') && !worktree.join(root).exists()
}

fn root_of(pattern: &str) -> &str {
    pattern
        .split_once("/**")
        .map_or(pattern, |(root, _rest)| root)
        .trim_end_matches('/')
}

fn pattern_for(worktree: &Path, argument: &Path) -> Result<String> {
    let cwd = env::current_dir().context("cannot read the current directory")?;
    let absolute = if argument.is_absolute() {
        argument.to_path_buf()
    } else {
        cwd.join(argument)
    };

    let relative = absolute
        .strip_prefix(worktree)
        .with_context(|| format!("`{}` is outside this repository", argument.display()))?
        .to_str()
        .with_context(|| format!("`{}` is not valid UTF-8", argument.display()))?
        .replace('\\', "/");

    if relative.is_empty() {
        bail!("sealing the whole repository would seal the vault itself");
    }

    if paths::is_never_sealed(&relative) {
        bail!("`{relative}` cannot be sealed: the repository needs it to open the vault at all");
    }

    let asked_for_a_directory = argument.to_string_lossy().ends_with('/');
    let is_directory = absolute.is_dir();
    let trimmed = relative.trim_end_matches('/').to_owned();

    Ok(if is_directory || asked_for_a_directory {
        format!("{trimmed}/**")
    } else {
        trimmed
    })
}

fn drop_line(path: &Path, line: &str) -> Result<bool> {
    let Ok(existing) = fs::read_to_string(path) else {
        return Ok(false);
    };

    let kept: Vec<&str> = existing
        .lines()
        .filter(|candidate| candidate.trim() != line)
        .collect();

    if kept.len() == existing.lines().count() {
        return Ok(false);
    }

    let mut updated = kept.join("\n");
    if !updated.is_empty() {
        updated.push('\n');
    }
    fs::write(path, updated).with_context(|| format!("cannot write `{}`", path.display()))?;

    Ok(true)
}
