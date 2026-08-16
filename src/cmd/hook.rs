use std::fs;

use anyhow::{Context as _, Result, bail};

use crate::exit::Code;
use crate::paths;
use crate::repo::session::Empty;
use crate::repo::{Repo, index};

pub fn hook(name: &str) -> Result<Code> {
    match name {
        "pre-commit" => pre_commit(),
        other => bail!("unknown hook `{other}`: expected `pre-commit` or `fsmonitor`"),
    }
}

fn pre_commit() -> Result<Code> {
    let repo = Repo::discover()?;

    refuse_tracked_plaintext(&repo)?;

    if !repo.is_unlocked() || repo.operation_in_progress() {
        return Ok(Code::Ok);
    }

    if !repo.seal_worktree(Empty::Refuse)?.changed || commit_carries_the_secrets(&repo)? {
        return Ok(Code::Ok);
    }

    eprintln!(
        "git-vault: sealed your secret changes into `{data}`, which this commit does not include. Run `git add {data}` and commit again to add them.",
        data = paths::DATA
    );

    Ok(Code::Ok)
}

fn refuse_tracked_plaintext(repo: &Repo) -> Result<()> {
    let tracked = repo.tracked_secrets()?;

    if tracked.is_empty() {
        return Ok(());
    }

    bail!(
        "this commit would publish the plaintext of {names}, which {} says is secret. Run `git vault add {names}` to take it out of the index, or `git vault remove {names}` to stop sealing it",
        paths::ATTRIBUTES,
        names = tracked.join(" ")
    )
}

fn commit_carries_the_secrets(repo: &Repo) -> Result<bool> {
    let path = repo.data_path();
    let sealed = fs::read(&path).with_context(|| format!("cannot read `{}`", path.display()))?;

    Ok(index::staged_blob(repo.worktree(), paths::DATA)?.is_some_and(|staged| staged == sealed))
}
