pub mod format;
pub mod identity;
pub mod keys;
mod reader;
pub mod recipient;
pub mod seal;

use anyhow::Result;

use crate::vault::format::Vault;
use crate::vault::keys::VaultKey;
use crate::vault::seal::Secret;

pub fn seal_all(key: &VaultKey, secrets: &[Secret]) -> Result<Vec<u8>> {
    let entries = secrets
        .iter()
        .map(|secret| secret.seal(key))
        .collect::<Result<Vec<_>>>()?;

    Vault {
        key_id: key.id(),
        entries,
    }
    .encode()
}

pub fn unseal_all(key: &VaultKey, bytes: &[u8]) -> Result<Vec<Secret>> {
    let vault = Vault::decode(bytes)?;

    if vault.key_id != key.id() {
        anyhow::bail!(
            "`.vault` was sealed with a different vault key than this clone has cached. Run `git vault unlock` to pick up the new key"
        );
    }

    let mut secrets = vault
        .entries
        .iter()
        .map(|entry| seal::unseal(entry, key))
        .collect::<Result<Vec<_>>>()?;
    secrets.sort_by(|left, right| left.path.cmp(&right.path));

    Ok(secrets)
}

#[cfg(test)]
mod tests {
    use super::{seal_all, unseal_all};
    use crate::vault::keys::VaultKey;
    use crate::vault::seal::{Kind, Secret};

    fn secret(path: &str, content: &[u8]) -> Secret {
        Secret {
            path: path.to_owned(),
            kind: Kind::File { executable: false },
            content: content.to_vec(),
        }
    }

    #[test]
    fn every_secret_comes_back() {
        let key = VaultKey::from_bytes([1; 32]);
        let secrets = vec![
            secret("secrets/deep/nested.key", b"inner\n"),
            secret("secrets/prod.env", b"A=1\n"),
            secret("top.key", b"k\n"),
        ];

        let recovered = unseal_all(&key, &seal_all(&key, &secrets).unwrap()).unwrap();

        assert_eq!(recovered, secrets);
    }

    #[test]
    fn the_whole_file_is_deterministic() {
        let key = VaultKey::from_bytes([2; 32]);
        let secrets = vec![secret("b", b"two"), secret("a", b"one")];

        let first = seal_all(&key, &secrets).unwrap();

        for _ in 0..100 {
            assert_eq!(seal_all(&key, &secrets).unwrap(), first);
        }
    }

    #[test]
    fn collection_order_does_not_change_the_bytes() {
        let key = VaultKey::from_bytes([3; 32]);
        let forwards = vec![secret("a", b"one"), secret("b", b"two")];
        let backwards = vec![secret("b", b"two"), secret("a", b"one")];

        assert_eq!(
            seal_all(&key, &forwards).unwrap(),
            seal_all(&key, &backwards).unwrap()
        );
    }

    #[test]
    fn a_vault_from_another_key_is_named_as_such() {
        let sealed = seal_all(&VaultKey::from_bytes([4; 32]), &[secret("a", b"one")]).unwrap();

        let error = unseal_all(&VaultKey::from_bytes([5; 32]), &sealed).unwrap_err();

        assert!(error.to_string().contains("git vault unlock"), "{error}");
    }
}
