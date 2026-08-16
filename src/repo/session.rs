use std::fs;
use std::path::PathBuf;

use anyhow::{Context as _, Result, bail};

use crate::paths;
use crate::repo::patterns::Patterns;
use crate::repo::{Repo, index, worktree};
use crate::vault::format::Vault;
use crate::vault::keys::VaultKey;
use crate::vault::seal::Secret;
use crate::vault::{seal_all, unseal_all};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Empty {
    Allow,
    Refuse,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Sealed {
    pub changed: bool,
    pub secrets: usize,
}

impl Repo {
    pub fn data_path(&self) -> PathBuf {
        self.worktree().join(paths::DATA)
    }

    pub fn keys_path(&self) -> PathBuf {
        self.worktree().join(paths::KEYS)
    }

    pub fn key(&self) -> Result<VaultKey> {
        if !self.is_unlocked() {
            bail!("this clone is locked: run `git vault unlock` to open the vault on this machine");
        }

        VaultKey::from_cache(&self.key_path())
    }

    pub fn patterns(&self) -> Result<Patterns> {
        Patterns::load(self.worktree())
    }

    pub fn secrets(&self) -> Result<Vec<Secret>> {
        worktree::collect(self.worktree(), &self.patterns()?)
    }

    pub fn tracked_secrets(&self) -> Result<Vec<String>> {
        let patterns = self.patterns()?;

        Ok(index::tracked(self.worktree(), &patterns.declared())?
            .into_iter()
            .filter(|path| patterns.is_secret(path))
            .collect())
    }

    pub fn read_data(&self) -> Result<Vec<u8>> {
        let path = self.data_path();
        fs::read(&path).with_context(|| {
            format!(
                "cannot read `{}`: is this a repository with a vault?",
                path.display()
            )
        })
    }

    pub fn unseal(&self, sealed: &[u8]) -> Result<Vec<Secret>> {
        unseal_all(&self.key()?, sealed)
    }

    pub fn apply(&self, secrets: &[Secret]) -> Result<worktree::Applied> {
        worktree::apply(self.worktree(), &self.patterns()?, secrets)
    }

    pub fn reseal_from_scratch(&self) -> Result<Sealed> {
        let key = self.key()?;
        let secrets = self.secrets()?;
        self.refuse_a_wipe(&secrets)?;
        let sealed = seal_all(&key, &secrets)?;

        fs::write(self.data_path(), &sealed)
            .with_context(|| format!("cannot write `{}`", paths::DATA))?;

        Ok(Sealed {
            changed: true,
            secrets: secrets.len(),
        })
    }

    fn refuse_a_wipe(&self, live: &[Secret]) -> Result<()> {
        if !live.is_empty() {
            return Ok(());
        }

        let sealed = fs::read(self.data_path())
            .ok()
            .as_deref()
            .and_then(|bytes| Vault::decode(bytes).ok())
            .map(|vault| vault.entries.len());

        match sealed {
            Some(0) => Ok(()),
            Some(entries) => bail!(
                "every sealed secret has disappeared from the worktree, and sealing that would record all {entries} of them as deleted. If something removed them, `git vault restore` puts them back. If you meant it, `git vault seal --allow-empty`"
            ),
            None => bail!(
                "there are no secrets on disk and `{}` cannot be read, so there is nothing to seal. `git checkout -- {}` puts the sealed file back",
                paths::DATA,
                paths::DATA
            ),
        }
    }

    fn refuse_foreign_vault(&self, key: &VaultKey) -> Result<()> {
        let Ok(existing) = fs::read(self.data_path()) else {
            return Ok(());
        };
        let Ok(vault) = Vault::decode(&existing) else {
            return Ok(());
        };

        if vault.key_id != key.id() {
            bail!(
                "`{}` was sealed with a different vault key than this clone has cached, so sealing would replace somebody else's work. Run `git vault unlock`",
                paths::DATA
            );
        }

        Ok(())
    }

    pub fn apply_only(&self, secrets: &[Secret]) -> Result<worktree::Applied> {
        worktree::apply_only(self.worktree(), secrets)
    }

    pub fn seal_worktree(&self, empty: Empty) -> Result<Sealed> {
        let key = self.key()?;
        self.refuse_foreign_vault(&key)?;
        let secrets = self.secrets()?;

        if empty == Empty::Refuse {
            self.refuse_a_wipe(&secrets)?;
        }
        let sealed = seal_all(&key, &secrets)?;

        if sealed.is_empty() {
            bail!("refusing to write an empty vault");
        }

        let path = self.data_path();
        if fs::read(&path).is_ok_and(|existing| existing == sealed) {
            return Ok(Sealed {
                changed: false,
                secrets: secrets.len(),
            });
        }

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("cannot create `{}`", parent.display()))?;
        }
        fs::write(&path, &sealed).with_context(|| format!("cannot write `{}`", path.display()))?;

        Ok(Sealed {
            changed: true,
            secrets: secrets.len(),
        })
    }
}
