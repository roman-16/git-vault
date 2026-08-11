use std::fs;

use anyhow::{Context as _, Result};

use crate::exit::Code;
use crate::repo::session::Empty;
use crate::repo::{Repo, worktree};

pub fn lock() -> Result<Code> {
    let repo = Repo::discover()?;

    if !repo.is_unlocked() {
        println!("Already locked.");
        return Ok(Code::Ok);
    }

    let sealed = repo.seal_worktree(Empty::Refuse)?;
    let removed = worktree::remove_all(repo.worktree(), &repo.patterns()?)?;

    let key_path = repo.key_path();
    fs::remove_file(&key_path)
        .with_context(|| format!("cannot remove `{}`", key_path.display()))?;

    println!(
        "Locked. Removed {removed} plaintext file{}.",
        if removed == 1 { "" } else { "s" }
    );
    if sealed.changed {
        println!("`.vault/data` was resealed first, so nothing was lost.");
    }

    Ok(Code::Ok)
}
