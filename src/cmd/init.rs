use std::fs;

use anyhow::{Context as _, Result, bail};

use crate::exit::Code;
use crate::paths;
use crate::repo::hooks::{self, Installed};
use crate::repo::recipients;
use crate::repo::session::Empty;
use crate::repo::{Repo, wiring};
use crate::vault::keys::{self, VaultKey};
use crate::vault::recipient::Recipient;

pub fn init() -> Result<Code> {
    let repo = Repo::discover()?;
    let worktree = repo.worktree().to_path_buf();

    if repo.keys_path().exists() {
        bail!(
            "this repository already has a vault. Run `git vault unlock` to open it on this machine"
        );
    }

    let identity_path = keys::identity_path()?;
    let identity = keys::load_or_create_identity(&identity_path)?;
    let key = VaultKey::generate()?;

    fs::create_dir_all(worktree.join(paths::VAULT_DIR))
        .with_context(|| format!("cannot create `{}`", paths::VAULT_DIR))?;
    let mine = [Recipient::new(
        &identity.to_public().to_string(),
        label().as_deref(),
    )?];
    recipients::write(&worktree, &mine)?;
    fs::write(repo.keys_path(), keys::wrap(&key, &mine)?)
        .with_context(|| format!("cannot write `{}`", paths::KEYS))?;
    key.cache(&repo.key_path())?;

    let watcher = wiring::configure(&worktree)?;
    wiring::ensure_attributes(&worktree)?;
    let hook = hooks::install_pre_commit(repo.common_dir())?;

    repo.seal_worktree(Empty::Allow)?;

    println!("Vault created.");
    println!("  identity   {}", identity_path.display());
    println!("  recipient  {}", identity.to_public());
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
    println!();
    println!("Next:");
    println!("  git vault add secrets/           declare what is secret");
    println!("  git add .gitattributes .gitignore .vault");
    println!("  git commit -m 'add a vault'");

    Ok(Code::Ok)
}

fn label() -> Option<String> {
    if let Ok(given) = std::env::var("GIT_VAULT_LABEL") {
        return Some(given);
    }

    for name in ["GIT_AUTHOR_EMAIL", "EMAIL"] {
        if let Ok(address) = std::env::var(name) {
            return Some(address);
        }
    }

    let user = std::env::var("USER").ok()?;
    let host = std::fs::read_to_string("/etc/hostname").ok()?;

    Some(format!("{user}@{}", host.trim()))
}
