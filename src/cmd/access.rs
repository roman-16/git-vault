use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};

use crate::exit::Code;
use crate::paths;
use crate::repo::{Repo, recipients};
use crate::vault::keys::{self, VaultKey};
use crate::vault::recipient::Recipient;

pub fn keys() -> Result<Code> {
    let repo = Repo::discover()?;
    let listed = recipients::read(repo.worktree())?;
    let mine = my_public_key();

    for recipient in &listed {
        let marker = if mine.as_deref() == Some(recipient.key()) {
            " (you)"
        } else {
            ""
        };
        match recipient.label() {
            Some(label) => println!("{}  {label}{marker}", recipient.short()),
            None => println!("{}{marker}", recipient.short()),
        }
    }

    println!();
    println!(
        "{} recipient{} in {}",
        listed.len(),
        if listed.len() == 1 { "" } else { "s" },
        paths::RECIPIENTS
    );

    Ok(Code::Ok)
}

pub fn share(argument: &str, label: Option<&str>) -> Result<Code> {
    let repo = Repo::discover()?;
    let key = repo.key()?;
    let worktree = repo.worktree().to_path_buf();

    let added = Recipient::new(&read_key(argument)?, label)?;
    let mut listed = recipients::read(&worktree)?;

    if listed.iter().any(|existing| existing.key() == added.key()) {
        println!("{} already has access.", added.short());
        return Ok(Code::Ok);
    }

    listed.push(added.clone());
    recipients::write(&worktree, &listed)?;
    rewrap(&repo, &key, &listed)?;

    println!("{} can now open the vault.", added.short());
    println!();
    println!("Commit the change so everyone else gets it:");
    println!("  git add {} {}", paths::KEYS, paths::RECIPIENTS);

    Ok(Code::Ok)
}

pub fn revoke(needle: &str) -> Result<Code> {
    let repo = Repo::discover()?;
    let worktree = repo.worktree().to_path_buf();
    let listed = recipients::read(&worktree)?;

    let matched = find(&listed, needle)?;
    let remaining: Vec<Recipient> = listed
        .iter()
        .filter(|recipient| recipient.key() != matched.key())
        .cloned()
        .collect();

    if remaining.is_empty() {
        bail!(
            "{} is the only recipient left, and a vault nobody can open is a vault nobody can fix",
            matched.short()
        );
    }

    if my_public_key().as_deref() == Some(matched.key()) {
        bail!(
            "that recipient is you. Revoking your own access would leave you unable to open this vault"
        );
    }

    recipients::write(&worktree, &remaining)?;
    rotate_to_new_key(&repo, &remaining)?;

    println!("{} can no longer open the vault.", matched.short());
    println!("The vault key was replaced, so everything is sealed anew.");
    println!();
    println!("They can still read every commit made while they had access. If a secret");
    println!("was worth protecting from them, change the secret too.");
    println!();
    println!("Commit the change:");
    println!(
        "  git add {} {} {}",
        paths::DATA,
        paths::KEYS,
        paths::RECIPIENTS
    );

    Ok(Code::Ok)
}

pub fn rotate() -> Result<Code> {
    let repo = Repo::discover()?;
    let listed = recipients::read(repo.worktree())?;

    rotate_to_new_key(&repo, &listed)?;

    println!("The vault key was replaced and everything sealed anew.");
    println!(
        "{} recipient{} kept access.",
        listed.len(),
        if listed.len() == 1 { "" } else { "s" }
    );
    println!();
    println!("Commit the change:");
    println!("  git add {} {}", paths::DATA, paths::KEYS);

    Ok(Code::Ok)
}

pub fn export_key(path: &Path) -> Result<Code> {
    let repo = Repo::discover()?;
    let key = repo.key()?;

    key.cache(path)?;

    println!("Wrote the vault key to {}.", path.display());
    println!("Anything holding this file can read every secret. On a runner:");
    println!("  git vault unlock --key-file {}", path.display());

    Ok(Code::Ok)
}

fn rewrap(repo: &Repo, key: &VaultKey, listed: &[Recipient]) -> Result<()> {
    fs::write(repo.keys_path(), keys::wrap(key, listed)?)
        .with_context(|| format!("cannot write `{}`", paths::KEYS))
}

fn rotate_to_new_key(repo: &Repo, listed: &[Recipient]) -> Result<()> {
    let _old = repo.key()?;
    let fresh = VaultKey::generate()?;

    rewrap(repo, &fresh, listed)?;
    fresh.cache(&repo.key_path())?;
    repo.reseal_from_scratch()?;

    Ok(())
}

fn my_public_key() -> Option<String> {
    let path = keys::identity_path().ok()?;
    let identity = keys::load_identity(&path).ok()?;

    Some(identity.to_public().to_string())
}

fn read_key(argument: &str) -> Result<String> {
    let candidate = PathBuf::from(argument);

    if candidate.is_file() {
        let contents =
            fs::read_to_string(&candidate).with_context(|| format!("cannot read `{argument}`"))?;
        return contents
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with('#'))
            .map(str::to_owned)
            .with_context(|| format!("`{argument}` holds no public key"));
    }

    Ok(argument.to_owned())
}

fn find(listed: &[Recipient], needle: &str) -> Result<Recipient> {
    let matches: Vec<&Recipient> = listed
        .iter()
        .filter(|recipient| {
            recipient.key() == needle
                || recipient.label() == Some(needle)
                || recipient.key().contains(needle)
                || recipient
                    .label()
                    .is_some_and(|label| label.contains(needle))
        })
        .collect();

    match matches.as_slice() {
        [] => bail!("no recipient matches `{needle}`. `git vault keys` lists them"),
        [single] => Ok((*single).clone()),
        many => {
            let names: Vec<String> = many.iter().map(|recipient| recipient.short()).collect();
            bail!(
                "`{needle}` matches {} recipients: {}",
                many.len(),
                names.join(", ")
            )
        }
    }
}
