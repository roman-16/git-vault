use anyhow::{Context as _, Result, bail};

use crate::vault::keys::KEY_ID_LEN;
use crate::vault::reader::Reader;

pub const MAGIC: &[u8; 8] = b"GITVAULT";
pub const VERSION: u8 = 1;
pub const ID_LEN: usize = 16;
pub const NONCE_LEN: usize = 24;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Entry {
    pub id: [u8; ID_LEN],
    pub nonce: [u8; NONCE_LEN],
    pub sealed: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Vault {
    pub key_id: [u8; KEY_ID_LEN],
    pub entries: Vec<Entry>,
}

impl Vault {
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut entries = self.entries.clone();
        entries.sort_by_key(|entry| entry.id);

        if let Some(duplicate) = first_duplicate(&entries) {
            bail!("two secrets share the entry id {duplicate:x?}");
        }

        let mut bytes = Vec::new();
        bytes.extend_from_slice(MAGIC);
        bytes.push(VERSION);
        bytes.extend_from_slice(&self.key_id);

        for entry in &entries {
            let length =
                u32::try_from(entry.sealed.len()).context("a sealed entry is too large")?;
            bytes.extend_from_slice(&entry.id);
            bytes.extend_from_slice(&entry.nonce);
            bytes.extend_from_slice(&length.to_le_bytes());
            bytes.extend_from_slice(&entry.sealed);
        }

        Ok(bytes)
    }

    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut reader = Reader::new(bytes);

        if &reader.array::<8>()? != MAGIC {
            bail!("this is not a git-vault file");
        }

        let version = reader.u8()?;
        if version != VERSION {
            bail!(
                "`.vault` is version {version}, but this git-vault only understands version {VERSION}. Update git-vault"
            );
        }

        let key_id = reader.array::<KEY_ID_LEN>()?;
        let mut entries = Vec::new();

        while !reader.is_empty() {
            let id = reader.array::<ID_LEN>()?;
            let nonce = reader.array::<NONCE_LEN>()?;
            let length = reader.u32()?;
            let sealed = reader.sized(length)?.to_vec();
            entries.push(Entry { id, nonce, sealed });
        }

        reader.finish()?;

        if entries.windows(2).any(|pair| {
            pair.first()
                .zip(pair.last())
                .is_some_and(|(left, right)| left.id >= right.id)
        }) {
            bail!("`.vault` entries are not in order, so it was not written by git-vault");
        }

        Ok(Self { key_id, entries })
    }
}

fn first_duplicate(entries: &[Entry]) -> Option<[u8; ID_LEN]> {
    entries
        .windows(2)
        .filter_map(|pair| pair.first().zip(pair.last()))
        .find(|(left, right)| left.id == right.id)
        .map(|(left, _)| left.id)
}

#[cfg(test)]
mod tests {
    use super::{Entry, MAGIC, VERSION, Vault};

    fn entry(id: u8) -> Entry {
        Entry {
            id: [id; 16],
            nonce: [id; 24],
            sealed: vec![id; 300],
        }
    }

    #[test]
    fn a_vault_survives_a_round_trip() {
        let vault = Vault {
            key_id: [1; 8],
            entries: vec![entry(9), entry(2), entry(5)],
        };

        let decoded = Vault::decode(&vault.encode().unwrap()).unwrap();

        assert_eq!(decoded.key_id, vault.key_id);
        assert_eq!(decoded.entries.len(), 3);
    }

    #[test]
    fn entries_are_sorted_so_collection_order_cannot_leak() {
        let forwards = Vault {
            key_id: [1; 8],
            entries: vec![entry(1), entry(2), entry(3)],
        };
        let backwards = Vault {
            key_id: [1; 8],
            entries: vec![entry(3), entry(2), entry(1)],
        };

        assert_eq!(forwards.encode().unwrap(), backwards.encode().unwrap());
    }

    #[test]
    fn an_empty_vault_is_not_an_empty_blob() {
        let bytes = Vault {
            key_id: [0; 8],
            entries: Vec::new(),
        }
        .encode()
        .unwrap();

        assert!(!bytes.is_empty());
        assert!(bytes.starts_with(MAGIC));
        assert_eq!(Vault::decode(&bytes).unwrap().entries, Vec::new());
    }

    #[test]
    fn foreign_bytes_are_refused() {
        let error = Vault::decode(b"not a vault at all").unwrap_err();

        assert!(
            error.to_string().contains("not a git-vault file"),
            "{error}"
        );
    }

    #[test]
    fn a_future_version_is_refused_with_advice() {
        let mut bytes = Vault {
            key_id: [0; 8],
            entries: Vec::new(),
        }
        .encode()
        .unwrap();
        let version = bytes.get_mut(8).unwrap();
        *version = VERSION.wrapping_add(1);

        let error = Vault::decode(&bytes).unwrap_err();

        assert!(error.to_string().contains("Update git-vault"), "{error}");
    }

    #[test]
    fn truncation_is_refused() {
        let bytes = Vault {
            key_id: [0; 8],
            entries: vec![entry(1)],
        }
        .encode()
        .unwrap();

        let (head, _) = bytes.split_at(bytes.len() - 10);

        assert!(Vault::decode(head).is_err());
    }

    #[test]
    fn duplicate_ids_are_refused() {
        let vault = Vault {
            key_id: [0; 8],
            entries: vec![entry(1), entry(1)],
        };

        assert!(vault.encode().is_err());
    }

    #[test]
    fn unsorted_input_is_refused() {
        let mut bytes = Vault {
            key_id: [0; 8],
            entries: vec![entry(1), entry(2)],
        }
        .encode()
        .unwrap();
        for offset in 17..33 {
            bytes.swap(offset, offset + 344);
        }

        assert!(Vault::decode(&bytes).is_err());
    }
}
