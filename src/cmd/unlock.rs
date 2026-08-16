use std::fs;
use std::path::Path;

use anyhow::{Context as _, Result};

use crate::exit::Code;
use crate::paths;
use crate::repo::hooks::{self, Installed};
use crate::repo::{Repo, wiring};
use crate::vault::identity::Identity;
use crate::vault::keys::{self, VaultKey};

pub fn unlock(key_file: Option<&Path>, identity: Option<&Path>) -> Result<Code> {
    let repo = Repo::discover()?;
    let worktree = repo.worktree().to_path_buf();
    let sealed = repo.read_data()?;

    let key = load_key(&repo, key_file, identity)?;

    let watcher = wiring::configure(&worktree)?;
    let hook = hooks::install_pre_commit(repo.common_dir())?;
    key.cache(&repo.key_path())?;

    let secrets = repo.unseal(&sealed)?;
    let applied = repo.apply(&secrets)?;

    println!(
        "Unlocked {} secret{}.",
        secrets.len(),
        if secrets.len() == 1 { "" } else { "s" }
    );
    if applied.removed > 0 {
        println!("Removed {} stale file(s).", applied.removed);
    }
    if let wiring::Watcher::Theirs(existing) = &watcher {
        println!();
        println!("`core.fsmonitor` is already `{existing}`, so it was left alone. Without it");
        println!("`git status` will not report edited secrets; commits stay correct either way.");
    }
    if hook == Installed::Foreign {
        println!();
        println!("You already have a pre-commit hook, so it was left alone. Add this line to it:");
        println!("  {}", hooks::pre_commit_line());
    }

    Ok(Code::Ok)
}

fn load_key(repo: &Repo, key_file: Option<&Path>, identity: Option<&Path>) -> Result<VaultKey> {
    if let Some(path) = key_file {
        let bytes = fs::read(path).with_context(|| format!("cannot read `{}`", path.display()))?;
        return VaultKey::try_from_slice(&bytes)
            .with_context(|| format!("`{}` is not a vault key", path.display()));
    }

    let envelope_path = repo.keys_path();
    let envelope = fs::read(&envelope_path).with_context(|| {
        format!(
            "cannot read `{}`: this repository has no vault, or it was not committed",
            paths::KEYS
        )
    })?;

    let identity = match identity {
        Some(path) => Identity::load(path)?,
        None => Identity::load_or_create(&keys::identity_path()?)?,
    };

    keys::unwrap(&envelope, &identity)
        .map_err(|error| anyhow::anyhow!("{error}\n\n{}", identity.how_to_publish()))
}
