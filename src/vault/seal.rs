use anyhow::{Context as _, Result, bail};
use chacha20poly1305::aead::{Aead as _, KeyInit as _};
use chacha20poly1305::{Key, XChaCha20Poly1305, XNonce};

use crate::vault::format::{Entry, ID_LEN, NONCE_LEN};
use crate::vault::keys::VaultKey;
use crate::vault::reader::Reader;

const MIN_CLASS: usize = 256;

const KIND_FILE: u8 = 1;
const KIND_SYMLINK: u8 = 2;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Kind {
    File { executable: bool },
    Symlink,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Secret {
    pub path: String,
    pub kind: Kind,
    pub content: Vec<u8>,
}

impl Secret {
    pub fn id(&self, key: &VaultKey) -> [u8; ID_LEN] {
        blake3::keyed_hash(&key.id_key(), self.path.as_bytes())
            .as_bytes()
            .first_chunk::<ID_LEN>()
            .copied()
            .unwrap_or_default()
    }

    pub fn seal(&self, key: &VaultKey) -> Result<Entry> {
        let record = self.encode()?;
        let nonce = synthetic_nonce(key, &record);

        let cipher = XChaCha20Poly1305::new(&Key::from(key.content_key()));
        let sealed = cipher
            .encrypt(&XNonce::from(nonce), record.as_slice())
            .ok()
            .with_context(|| format!("cannot seal `{}`", self.path))?;

        Ok(Entry {
            id: self.id(key),
            nonce,
            sealed,
        })
    }

    fn encode(&self) -> Result<Vec<u8>> {
        let path = self.path.as_bytes();
        let path_len = u16::try_from(path.len())
            .with_context(|| format!("`{}` has too long a path to seal", self.path))?;
        let content_len = u32::try_from(self.content.len())
            .with_context(|| format!("`{}` is too large to seal", self.path))?;

        let mut record = Vec::new();
        record.push(match self.kind {
            Kind::File { .. } => KIND_FILE,
            Kind::Symlink => KIND_SYMLINK,
        });
        record.push(u8::from(matches!(
            self.kind,
            Kind::File { executable: true }
        )));
        record.extend_from_slice(&path_len.to_le_bytes());
        record.extend_from_slice(&content_len.to_le_bytes());
        record.extend_from_slice(path);
        record.extend_from_slice(&self.content);

        record.resize(size_class(record.len())?, 0);

        Ok(record)
    }

    fn decode(record: &[u8]) -> Result<Self> {
        let mut reader = Reader::new(record);

        let kind = reader.u8()?;
        let executable = reader.u8()? == 1;
        let path_len = reader.u16()?;
        let content_len = reader.u32()?;
        let path = reader.take(usize::from(path_len))?;
        let content = reader.sized(content_len)?;

        let kind = match kind {
            KIND_FILE => Kind::File { executable },
            KIND_SYMLINK => Kind::Symlink,
            other => bail!("unknown entry kind {other}"),
        };

        Ok(Self {
            path: String::from_utf8(path.to_vec()).context("an entry path is not valid UTF-8")?,
            kind,
            content: content.to_vec(),
        })
    }
}

pub fn unseal(entry: &Entry, key: &VaultKey) -> Result<Secret> {
    let cipher = XChaCha20Poly1305::new(&Key::from(key.content_key()));
    let record = cipher
        .decrypt(&XNonce::from(entry.nonce), entry.sealed.as_slice())
        .ok()
        .context("an entry does not open with this key, or has been tampered with")?;

    if synthetic_nonce(key, &record) != entry.nonce {
        bail!("an entry's nonce does not match its contents");
    }

    let secret = Secret::decode(&record)?;

    if secret.id(key) != entry.id {
        bail!("`{}` is stored under the wrong entry id", secret.path);
    }

    Ok(secret)
}

fn synthetic_nonce(key: &VaultKey, record: &[u8]) -> [u8; NONCE_LEN] {
    blake3::keyed_hash(&key.nonce_key(), record)
        .as_bytes()
        .first_chunk::<NONCE_LEN>()
        .copied()
        .unwrap_or_default()
}

fn size_class(len: usize) -> Result<usize> {
    if len <= MIN_CLASS {
        return Ok(MIN_CLASS);
    }
    len.checked_next_power_of_two()
        .context("a secret is too large to pad")
}

#[cfg(test)]
mod tests {
    use super::{Kind, MIN_CLASS, Secret, size_class, unseal};
    use crate::vault::keys::VaultKey;

    fn secret(path: &str, content: &[u8]) -> Secret {
        Secret {
            path: path.to_owned(),
            kind: Kind::File { executable: false },
            content: content.to_vec(),
        }
    }

    #[test]
    fn a_sealed_secret_comes_back_unchanged() {
        let key = VaultKey::from_bytes([3; 32]);
        let original = secret("secrets/prod.env", b"STRIPE_KEY=sk_live_1\n");

        let recovered = unseal(&original.seal(&key).unwrap(), &key).unwrap();

        assert_eq!(recovered, original);
    }

    #[test]
    fn modes_and_symlinks_survive() {
        let key = VaultKey::from_bytes([4; 32]);
        for kind in [
            Kind::File { executable: true },
            Kind::File { executable: false },
            Kind::Symlink,
        ] {
            let original = Secret {
                path: "secrets/thing".to_owned(),
                kind,
                content: b"../elsewhere".to_vec(),
            };

            let recovered = unseal(&original.seal(&key).unwrap(), &key).unwrap();

            assert_eq!(recovered.kind, kind);
        }
    }

    #[test]
    fn sealing_is_deterministic() {
        let key = VaultKey::from_bytes([5; 32]);
        let original = secret("secrets/prod.env", b"A=1\n");

        let first = original.seal(&key).unwrap();

        for _ in 0..100 {
            assert_eq!(original.seal(&key).unwrap(), first);
        }
    }

    #[test]
    fn a_different_key_produces_a_different_entry() {
        let original = secret("secrets/prod.env", b"A=1\n");

        let mine = original.seal(&VaultKey::from_bytes([6; 32])).unwrap();
        let theirs = original.seal(&VaultKey::from_bytes([7; 32])).unwrap();

        assert_ne!(mine.id, theirs.id);
        assert_ne!(mine.sealed, theirs.sealed);
    }

    #[test]
    fn the_wrong_key_cannot_unseal() {
        let entry = secret("secrets/prod.env", b"A=1\n")
            .seal(&VaultKey::from_bytes([8; 32]))
            .unwrap();

        assert!(unseal(&entry, &VaultKey::from_bytes([9; 32])).is_err());
    }

    #[test]
    fn tampering_is_detected() {
        let key = VaultKey::from_bytes([10; 32]);
        let mut entry = secret("secrets/prod.env", b"A=1\n").seal(&key).unwrap();

        entry.sealed.reverse();

        assert!(unseal(&entry, &key).is_err());
    }

    #[test]
    fn small_secrets_share_one_size_class() {
        let key = VaultKey::from_bytes([11; 32]);

        let short = secret("a", b"x").seal(&key).unwrap();
        let longer = secret("b", b"a hundred times longer, but still small")
            .seal(&key)
            .unwrap();

        assert_eq!(short.sealed.len(), longer.sealed.len());
    }

    #[test]
    fn size_classes_are_powers_of_two_above_the_floor() {
        assert_eq!(size_class(0).unwrap(), MIN_CLASS);
        assert_eq!(size_class(MIN_CLASS).unwrap(), MIN_CLASS);
        assert_eq!(size_class(MIN_CLASS + 1).unwrap(), MIN_CLASS * 2);
        assert_eq!(size_class(5000).unwrap(), 8192);
    }
}
