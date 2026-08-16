use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use clap::Args;

use crate::paths;
use crate::repo::Repo;
use crate::vault::identity::Identity;
use crate::vault::keys::{self, VaultKey};

#[derive(Args, Debug)]
pub struct Source {
    #[arg(
        long,
        value_name = "FILE",
        help = "The sealed file to open, instead of this repository's"
    )]
    data: Option<PathBuf>,

    #[arg(
        long,
        value_name = "FILE",
        help = "The wrapped vault key, if it does not sit beside the sealed file"
    )]
    keys: Option<PathBuf>,

    #[arg(
        long,
        value_name = "FILE",
        help = "The age or SSH private key that opens it"
    )]
    identity: Option<PathBuf>,

    #[arg(
        long,
        value_name = "FILE",
        conflicts_with = "identity",
        help = "A raw vault key, as written by `git vault export-key`"
    )]
    key_file: Option<PathBuf>,
}

pub struct Opened {
    pub key: VaultKey,
    pub sealed: Vec<u8>,
}

impl Source {
    pub fn open(&self) -> Result<Opened> {
        let repo = Repo::discover().ok();
        let data = self.data_path(repo.as_ref())?;
        let sealed = fs::read(&data)
            .with_context(|| format!("cannot read the sealed file `{}`", data.display()))?;

        Ok(Opened {
            key: self.key(repo.as_ref(), &data)?,
            sealed,
        })
    }

    fn data_path(&self, repo: Option<&Repo>) -> Result<PathBuf> {
        if let Some(given) = &self.data {
            return Ok(given.clone());
        }

        repo.map(Repo::data_path).context(
            "not inside a repository with a vault, so there is nothing to open. Name the sealed file with `--data`",
        )
    }

    fn keys_path(&self, data: &Path) -> PathBuf {
        if let Some(given) = &self.keys {
            return given.clone();
        }

        data.parent()
            .unwrap_or_else(|| Path::new("."))
            .join(paths::KEYS_FILE)
    }

    fn key(&self, repo: Option<&Repo>, data: &Path) -> Result<VaultKey> {
        if let Some(path) = &self.key_file {
            let bytes = fs::read(path)
                .with_context(|| format!("cannot read the key file `{}`", path.display()))?;
            return VaultKey::try_from_slice(&bytes)
                .with_context(|| format!("`{}` is not a vault key", path.display()));
        }

        if self.identity.is_none()
            && let Some(repo) = repo
            && repo.is_unlocked()
            && self.data.is_none()
        {
            return repo.key();
        }

        let identity = match &self.identity {
            Some(path) => Identity::load(path)?,
            None => Identity::load_or_create(&keys::identity_path()?)?,
        };

        let path = self.keys_path(data);
        let envelope = fs::read(&path).with_context(|| {
            format!(
                "cannot read `{}`, which holds the vault key wrapped for each recipient",
                path.display()
            )
        })?;

        keys::unwrap(&envelope, &identity)
            .map_err(|error| anyhow::anyhow!("{error}\n\n{}", identity.how_to_publish()))
    }
}
