use std::fmt::Write as _;
use std::path::Path;

use anyhow::Result;

use crate::exit::Code;
use crate::paths;
use crate::repo::hooks;
use crate::repo::{Repo, index, recipients};
use crate::vault::format::Vault;

const LOUD_ENOUGH_TO_MENTION: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum Level {
    Ok,
    Warning,
    Problem,
}

impl Level {
    const fn marker(self) -> &'static str {
        match self {
            Self::Ok => "ok     ",
            Self::Warning => "warning",
            Self::Problem => "problem",
        }
    }
}

struct Report {
    findings: Vec<(Level, String)>,
}

impl Report {
    const fn new() -> Self {
        Self {
            findings: Vec::new(),
        }
    }

    fn ok(&mut self, message: impl Into<String>) {
        self.findings.push((Level::Ok, message.into()));
    }

    fn warn(&mut self, message: impl Into<String>) {
        self.findings.push((Level::Warning, message.into()));
    }

    fn problem(&mut self, message: impl Into<String>) {
        self.findings.push((Level::Problem, message.into()));
    }

    fn worst(&self) -> Level {
        self.findings
            .iter()
            .map(|(level, _message)| *level)
            .max()
            .unwrap_or(Level::Ok)
    }
}

pub fn doctor() -> Result<Code> {
    let repo = Repo::discover()?;
    let mut report = Report::new();

    check_files(&repo, &mut report);
    check_wiring(&repo, &mut report)?;
    check_hooks(&repo, &mut report);
    check_index(&repo, &mut report)?;
    check_declarations(&repo, &mut report)?;

    let mut rendered = String::new();
    for (level, message) in &report.findings {
        let _ignored = writeln!(rendered, "{} {message}", level.marker());
    }
    print!("{rendered}");

    match report.worst() {
        Level::Problem => Ok(Code::Findings),
        Level::Warning | Level::Ok => Ok(Code::Ok),
    }
}

fn check_files(repo: &Repo, report: &mut Report) {
    match repo.read_data() {
        Ok(bytes) => match Vault::decode(&bytes) {
            Ok(vault) => {
                report.ok(format!(
                    "{} holds {} sealed entr{}",
                    paths::DATA,
                    vault.entries.len(),
                    if vault.entries.len() == 1 { "y" } else { "ies" }
                ));

                if bytes.len() > LOUD_ENOUGH_TO_MENTION {
                    report.warn(format!(
                        "{} is {} KiB, and it is sealed again on every git command. Large files belong outside the vault",
                        paths::DATA,
                        bytes.len() / 1024
                    ));
                }
            }
            Err(error) => report.problem(format!("{} is not a vault: {error}", paths::DATA)),
        },
        Err(_missing) => report.problem(format!("{} is missing", paths::DATA)),
    }

    if repo.keys_path().is_file() {
        report.ok(format!("{} is present", paths::KEYS));
    } else {
        report.problem(format!(
            "{} is missing, so nobody can open this vault",
            paths::KEYS
        ));
    }

    match recipients::read(repo.worktree()) {
        Ok(listed) if listed.is_empty() => report.problem(format!(
            "{} lists nobody, so nobody can open this vault",
            paths::RECIPIENTS
        )),
        Ok(listed) => report.ok(format!(
            "{} lists {} recipient{}",
            paths::RECIPIENTS,
            listed.len(),
            if listed.len() == 1 { "" } else { "s" }
        )),
        Err(error) => report.problem(format!("{error:#}")),
    }

    if repo.is_unlocked() {
        report.ok("this clone is unlocked");
    } else {
        report.warn("this clone is locked, so the secrets are not on disk");
    }
}

fn check_wiring(repo: &Repo, report: &mut Report) -> Result<()> {
    let worktree = repo.worktree();

    for key in ["filter.vault.clean", "filter.vault.smudge"] {
        if index::config(worktree, key)?.is_some() {
            report.ok(format!("{key} is configured"));
        } else {
            report.problem(format!(
                "{key} is not configured, so git leaves the vault alone. Run `git vault unlock`"
            ));
        }
    }

    match index::config(worktree, "filter.vault.required")?.as_deref() {
        Some("true") => report.ok("filter.vault.required is true"),
        _other => report.problem(
            "filter.vault.required is not true, so a failing filter would let git store the raw bytes",
        ),
    }

    match index::config(worktree, "filter.vault-plaintext.clean")?.as_deref() {
        Some(value) if value.contains("filter refuse") => {
            report.ok("filter.vault-plaintext.clean refuses plaintext secrets");
        }
        _other => report.problem(
            "filter.vault-plaintext.clean is not configured, so `git add` would store a declared secret in the clear",
        ),
    }

    match index::config(worktree, "filter.vault-plaintext.required")?.as_deref() {
        Some("true") => report.ok("filter.vault-plaintext.required is true"),
        _other => report.problem(
            "filter.vault-plaintext.required is not true, so git would stage a declared secret even after the filter refused it",
        ),
    }

    match index::config(worktree, "core.fsmonitor")?.as_deref() {
        Some(value) if value.contains("git-vault") => {
            report.ok("core.fsmonitor runs git-vault, so git status sees secret edits");
        }
        Some(other) => report.warn(format!(
            "core.fsmonitor is `{other}`, so nothing seals a secret edit until the next commit attempt, which then reports that it left the edit out"
        )),
        None => report.warn(
            "core.fsmonitor is unset, so nothing seals a secret edit until the next commit attempt, which then reports that it left the edit out",
        ),
    }

    match index::config(worktree, "diff.vault.cachetextconv")?.as_deref() {
        Some("false") => report.ok("diff.vault.cachetextconv is false"),
        _other => report.problem(
            "diff.vault.cachetextconv is not false, so decrypted secrets would be cached under .git and outlive `git vault lock`",
        ),
    }

    Ok(())
}

fn check_hooks(repo: &Repo, report: &mut Report) {
    let hook = repo.common_dir().join("hooks/pre-commit");

    match std::fs::read_to_string(&hook) {
        Ok(contents) if contents.contains("git-vault hook pre-commit") => {
            report.ok("the pre-commit hook is installed");
        }
        Ok(_foreign) => report.warn(format!(
            "another pre-commit hook is installed, so add this line to it: {}",
            hooks::pre_commit_line()
        )),
        Err(_missing) => report.problem(
            "no pre-commit hook, so a commit could carry secrets that were never sealed. Run `git vault unlock`",
        ),
    }
}

fn check_index(repo: &Repo, report: &mut Report) -> Result<()> {
    match index::index_flag(repo.worktree(), paths::DATA)? {
        Some('H') => report.ok(format!("{} is tracked normally", paths::DATA)),
        Some(flag) => report.problem(format!(
            "{} carries the index flag `{flag}`, so git would stop noticing it. Clear it with `git update-index --no-assume-unchanged --no-skip-worktree -- {}`",
            paths::DATA,
            paths::DATA
        )),
        None => report.problem(format!("{} is not tracked by git", paths::DATA)),
    }

    Ok(())
}

fn check_declarations(repo: &Repo, report: &mut Report) -> Result<()> {
    let worktree = repo.worktree();
    let patterns = repo.patterns()?;

    match attribute_line(worktree) {
        None => report.problem(format!(
            "{} does not hand {} to the filters",
            paths::ATTRIBUTES,
            paths::DATA
        )),
        Some(line) if !line.contains("diff=vault") || !line.contains("merge=vault") => report
            .problem(format!(
                "{} hands {} to the clean and smudge filters but not to diff and merge, so diffs stay opaque and merges conflict on every change",
                paths::ATTRIBUTES,
                paths::DATA
            )),
        Some(line) if line.split_whitespace().any(|word| word == "binary") => report.problem(
            format!(
                "{} marks {} as `binary`, which also switches off diff and merge. Use `-text` instead",
                paths::ATTRIBUTES,
                paths::DATA
            ),
        ),
        Some(_wired) => report.ok(format!("{} hands {} to the filters", paths::ATTRIBUTES, paths::DATA)),
    }

    let declared = patterns.declared();
    if declared.is_empty() {
        report.warn("nothing is declared secret yet");
        return Ok(());
    }

    let ignore = std::fs::read_to_string(worktree.join(paths::IGNORE)).unwrap_or_default();
    for pattern in &declared {
        if ignore
            .lines()
            .any(|line| covered_directory(line.trim()) == covered_directory(pattern))
        {
            continue;
        }
        report.problem(format!(
            "`{pattern}` is sealed but not in {}, so git may commit its plaintext",
            paths::IGNORE
        ));
    }

    for root in patterns.roots() {
        if root.is_empty() {
            report.warn(
                "a pattern has no leading directory, so every git command has to walk the whole worktree. An anchored pattern such as `secrets/**` is much cheaper",
            );
        }
    }

    let sealed: Vec<String> = repo
        .secrets()
        .unwrap_or_default()
        .into_iter()
        .map(|secret| secret.path)
        .collect();
    let leaked = index::tracked(worktree, &sealed)?;

    if leaked.is_empty() {
        report.ok("no sealed path is tracked in plaintext");
    } else {
        for path in leaked {
            report.problem(format!(
                "`{path}` is sealed and also tracked in plaintext. Run `git vault add {path}`"
            ));
        }
    }

    Ok(())
}

fn covered_directory(pattern: &str) -> &str {
    pattern
        .split_once("/**")
        .map_or(pattern, |(root, _rest)| root)
        .trim_end_matches('/')
}

fn attribute_line(worktree: &Path) -> Option<String> {
    std::fs::read_to_string(worktree.join(paths::ATTRIBUTES))
        .unwrap_or_default()
        .lines()
        .find(|line| line.split_whitespace().next() == Some(paths::DATA))
        .map(str::to_owned)
}
