use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::{Context as _, Result, bail};

use crate::paths;

const BINARY: &str = "git-vault";

const SEALED_ATTRIBUTES: &str = "vault filter=vault-plaintext";

fn attributes_block() -> String {
    format!(
        "{data} filter=vault diff=vault merge=vault -text\n{keys} -text\n{recipients} -text\n",
        data = paths::DATA,
        keys = paths::KEYS,
        recipients = paths::RECIPIENTS,
    )
}

pub const fn sealed_attributes() -> &'static str {
    SEALED_ATTRIBUTES
}

pub fn fsmonitor_command() -> String {
    format!("{BINARY} hook fsmonitor")
}

fn settings() -> Vec<(String, String)> {
    vec![
        ("diff.vault.cachetextconv".to_owned(), "false".to_owned()),
        (
            "diff.vault.textconv".to_owned(),
            format!("{BINARY} filter textconv"),
        ),
        (
            "filter.vault-plaintext.clean".to_owned(),
            format!("{BINARY} filter refuse %f"),
        ),
        (
            "filter.vault-plaintext.required".to_owned(),
            "true".to_owned(),
        ),
        (
            "filter.vault.clean".to_owned(),
            format!("{BINARY} filter clean"),
        ),
        ("filter.vault.required".to_owned(), "true".to_owned()),
        (
            "filter.vault.smudge".to_owned(),
            format!("{BINARY} filter smudge"),
        ),
        (
            "merge.vault.driver".to_owned(),
            format!("{BINARY} filter merge %O %A %B %L %P"),
        ),
        (
            "merge.vault.name".to_owned(),
            "git-vault sealed merge".to_owned(),
        ),
    ]
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Watcher {
    Ours,
    Theirs(String),
}

pub fn configure(worktree: &Path) -> Result<Watcher> {
    for (key, value) in settings() {
        let status = Command::new("git")
            .current_dir(worktree)
            .args(["config", "--local", &key, &value])
            .status()
            .context("cannot run git to write the local configuration")?;

        if !status.success() {
            bail!("`git config --local {key}` failed");
        }
    }

    let wanted = fsmonitor_command();
    match crate::repo::index::config(worktree, "core.fsmonitor")? {
        Some(existing) if existing != wanted => Ok(Watcher::Theirs(existing)),
        Some(_ours) => Ok(Watcher::Ours),
        None => {
            set(worktree, "core.fsmonitor", &wanted)?;
            Ok(Watcher::Ours)
        }
    }
}

fn set(worktree: &Path, key: &str, value: &str) -> Result<()> {
    let status = Command::new("git")
        .current_dir(worktree)
        .args(["config", "--local", key, value])
        .status()
        .context("cannot run git to write the local configuration")?;

    if !status.success() {
        bail!("`git config --local {key}` failed");
    }

    Ok(())
}

pub fn ensure_attributes(worktree: &Path) -> Result<bool> {
    let path = worktree.join(paths::ATTRIBUTES);
    let existing = fs::read_to_string(&path).unwrap_or_default();

    if existing
        .lines()
        .any(|line| line.split_whitespace().next() == Some(paths::DATA))
    {
        return Ok(false);
    }

    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    updated.push_str(&attributes_block());

    fs::write(&path, updated).with_context(|| format!("cannot write `{}`", path.display()))?;

    Ok(true)
}

pub fn ensure_sealed(worktree: &Path, patterns: &[String]) -> Result<bool> {
    let path = worktree.join(paths::ATTRIBUTES);
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let missing: Vec<String> = patterns
        .iter()
        .map(|pattern| format!("{pattern} {SEALED_ATTRIBUTES}"))
        .filter(|line| {
            !existing
                .lines()
                .any(|existing| existing.trim() == line.as_str())
        })
        .collect();

    if missing.is_empty() {
        return Ok(false);
    }

    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    for line in missing {
        updated.push_str(&line);
        updated.push('\n');
    }

    fs::write(&path, updated).with_context(|| format!("cannot write `{}`", path.display()))?;

    Ok(true)
}

pub fn ensure_ignored(worktree: &Path, patterns: &[String]) -> Result<bool> {
    let path = worktree.join(paths::IGNORE);
    let existing = fs::read_to_string(&path).unwrap_or_default();
    let missing: Vec<&String> = patterns
        .iter()
        .filter(|pattern| !existing.lines().any(|line| line.trim() == pattern.as_str()))
        .collect();

    if missing.is_empty() {
        return Ok(false);
    }

    let mut updated = existing;
    if !updated.is_empty() && !updated.ends_with('\n') {
        updated.push('\n');
    }
    for pattern in missing {
        updated.push_str(pattern);
        updated.push('\n');
    }

    fs::write(&path, updated).with_context(|| format!("cannot write `{}`", path.display()))?;

    Ok(true)
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::ensure_attributes;

    #[test]
    fn a_fresh_repository_gets_the_declaration() {
        let dir = TempDir::new().unwrap();

        assert!(ensure_attributes(dir.path()).unwrap());

        let written = std::fs::read_to_string(dir.path().join(".gitattributes")).unwrap();
        assert!(
            written.contains(".vault/data filter=vault diff=vault merge=vault -text"),
            "{written}"
        );
        assert!(written.contains(".vault/keys -text"), "{written}");
        assert!(written.contains(".vault/recipients -text"), "{written}");
    }

    #[test]
    fn an_existing_declaration_is_left_alone() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(".gitattributes"),
            ".vault/data filter=vault\n",
        )
        .unwrap();

        assert!(!ensure_attributes(dir.path()).unwrap());
    }

    #[test]
    fn existing_attributes_are_appended_to_not_replaced() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join(".gitattributes"), "*.rs text\n").unwrap();

        ensure_attributes(dir.path()).unwrap();

        let written = std::fs::read_to_string(dir.path().join(".gitattributes")).unwrap();
        assert!(written.starts_with("*.rs text\n"), "{written}");
        assert!(written.contains(".vault/data"), "{written}");
    }
}
