use std::fmt;
use std::fs;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};

use age::x25519::Identity;
use anyhow::{Context as _, Result, bail};
use secrecy::{ExposeSecret as _, SecretBox};

use crate::vault::recipient::Recipient;

pub const KEY_LEN: usize = 32;

pub const KEY_ID_LEN: usize = 8;

const CONTEXT_ID: &str = "git-vault 2026-01 entry id";
const CONTEXT_NONCE: &str = "git-vault 2026-01 entry nonce";
const CONTEXT_CONTENT: &str = "git-vault 2026-01 entry content";
const CONTEXT_KEY_ID: &str = "git-vault 2026-01 key id";

pub struct VaultKey(SecretBox<[u8; KEY_LEN]>);

impl fmt::Debug for VaultKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "VaultKey({:02x?})", self.id())
    }
}

impl VaultKey {
    pub fn generate() -> Result<Self> {
        let mut bytes = [0_u8; KEY_LEN];
        getrandom::fill(&mut bytes)
            .context("cannot read random bytes from the operating system")?;
        Ok(Self(SecretBox::new(Box::new(bytes))))
    }

    pub fn from_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(SecretBox::new(Box::new(bytes)))
    }

    pub fn try_from_slice(bytes: &[u8]) -> Result<Self> {
        let bytes: [u8; KEY_LEN] = bytes
            .try_into()
            .ok()
            .with_context(|| format!("a vault key is {KEY_LEN} bytes, got {}", bytes.len()))?;
        Ok(Self::from_bytes(bytes))
    }

    fn material(&self) -> &[u8; KEY_LEN] {
        self.0.expose_secret()
    }

    pub fn id(&self) -> [u8; KEY_ID_LEN] {
        let derived = blake3::derive_key(CONTEXT_KEY_ID, self.material());
        derived
            .first_chunk::<KEY_ID_LEN>()
            .copied()
            .unwrap_or_default()
    }

    pub fn id_key(&self) -> [u8; KEY_LEN] {
        blake3::derive_key(CONTEXT_ID, self.material())
    }

    pub fn nonce_key(&self) -> [u8; KEY_LEN] {
        blake3::derive_key(CONTEXT_NONCE, self.material())
    }

    pub fn content_key(&self) -> [u8; KEY_LEN] {
        blake3::derive_key(CONTEXT_CONTENT, self.material())
    }

    pub fn cache(&self, path: &Path) -> Result<()> {
        let parent = path
            .parent()
            .with_context(|| format!("`{}` has no parent directory", path.display()))?;
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create `{}`", parent.display()))?;
        write_private(path, self.material())
    }

    pub fn from_cache(path: &Path) -> Result<Self> {
        let bytes = fs::read(path).with_context(|| format!("cannot read `{}`", path.display()))?;
        Self::try_from_slice(&bytes)
            .with_context(|| format!("`{}` is not a vault key", path.display()))
    }
}

pub fn identity_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("GIT_VAULT_IDENTITY") {
        return Ok(PathBuf::from(path));
    }

    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
        .context(
            "neither XDG_CONFIG_HOME nor HOME is set, so there is nowhere to keep the identity",
        )?;

    Ok(base.join("git-vault/identity"))
}

pub fn load_or_create_identity(path: &Path) -> Result<Identity> {
    if path.exists() {
        return load_identity(path);
    }

    let parent = path
        .parent()
        .with_context(|| format!("`{}` has no parent directory", path.display()))?;
    fs::create_dir_all(parent).with_context(|| format!("cannot create `{}`", parent.display()))?;

    let identity = Identity::generate();
    let contents = format!(
        "# git-vault identity. Keep it: without it the vault cannot be opened.\n\
         # public key: {}\n{}\n",
        identity.to_public(),
        identity.to_string().expose_secret(),
    );
    write_private(path, contents.as_bytes())?;

    Ok(identity)
}

pub fn load_identity(path: &Path) -> Result<Identity> {
    let contents =
        fs::read_to_string(path).with_context(|| format!("cannot read `{}`", path.display()))?;

    contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .find_map(|line| line.parse::<Identity>().ok())
        .with_context(|| format!("`{}` holds no age identity", path.display()))
}

pub fn wrap(key: &VaultKey, recipients: &[Recipient]) -> Result<String> {
    if recipients.is_empty() {
        bail!("a vault needs at least one recipient");
    }

    let addressed = recipients
        .iter()
        .map(Recipient::to_age)
        .collect::<Result<Vec<_>>>()?;

    let encryptor = age::Encryptor::with_recipients(
        addressed
            .iter()
            .map(|recipient| -> &dyn age::Recipient { recipient.as_ref() }),
    )
    .map_err(|error| anyhow::anyhow!("cannot wrap the vault key: {error}"))?;

    let mut armoured = Vec::new();
    let armor =
        age::armor::ArmoredWriter::wrap_output(&mut armoured, age::armor::Format::AsciiArmor)
            .context("cannot start the key envelope")?;
    let mut writer = encryptor
        .wrap_output(armor)
        .map_err(|error| anyhow::anyhow!("cannot write the key envelope: {error}"))?;
    writer
        .write_all(key.material())
        .context("cannot write the wrapped key")?;
    writer
        .finish()
        .and_then(age::armor::ArmoredWriter::finish)
        .context("cannot finish the key envelope")?;

    String::from_utf8(armoured).context("the key envelope is not text")
}

pub fn unwrap(envelope: &[u8], identity: &Identity) -> Result<VaultKey> {
    let armor = age::armor::ArmoredReader::new(envelope);
    let decryptor = age::Decryptor::new_buffered(armor)
        .map_err(|error| anyhow::anyhow!("`.vault.keys` is not a usable age file: {error}"))?;

    let identity: &dyn age::Identity = identity;
    let mut reader = decryptor
        .decrypt(std::iter::once(identity))
        .map_err(|error| {
            anyhow::anyhow!("this identity cannot open the vault: {error}. Ask someone with access to run `git vault share`")
        })?;

    let mut bytes = Vec::new();
    reader
        .read_to_end(&mut bytes)
        .context("cannot read the wrapped key")?;

    VaultKey::try_from_slice(&bytes)
}

#[cfg(unix)]
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    use std::os::unix::fs::OpenOptionsExt as _;

    let mut file = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .mode(0o600)
        .open(path)
        .with_context(|| format!("cannot write `{}`", path.display()))?;
    file.write_all(bytes)
        .with_context(|| format!("cannot write `{}`", path.display()))
}

#[cfg(windows)]
fn write_private(path: &Path, bytes: &[u8]) -> Result<()> {
    fs::write(path, bytes).with_context(|| format!("cannot write `{}`", path.display()))?;

    let user = std::env::var("USERNAME")
        .context("USERNAME is unset, so the key file cannot be restricted to you")?;
    let status = std::process::Command::new("icacls")
        .arg(path)
        .arg("/inheritance:r")
        .arg("/grant:r")
        .arg(format!("{user}:(R,W)"))
        .stdout(std::process::Stdio::null())
        .status()
        .with_context(|| format!("cannot run icacls to restrict `{}`", path.display()))?;

    if !status.success() {
        anyhow::bail!(
            "icacls could not restrict `{}` to you, so the key would stay readable by others",
            path.display()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::{VaultKey, load_or_create_identity, unwrap, wrap};
    use crate::vault::recipient::Recipient;

    #[test]
    fn subkeys_differ_from_each_other_and_from_the_key() {
        let key = VaultKey::from_bytes([7; 32]);

        let derived = [key.id_key(), key.nonce_key(), key.content_key()];

        assert_ne!(derived[0], derived[1]);
        assert_ne!(derived[1], derived[2]);
        assert_ne!(derived[0], derived[2]);
        assert!(derived.iter().all(|subkey| subkey != &[7; 32]));
    }

    #[test]
    fn the_key_id_is_stable_and_key_specific() {
        assert_eq!(
            VaultKey::from_bytes([1; 32]).id(),
            VaultKey::from_bytes([1; 32]).id()
        );
        assert_ne!(
            VaultKey::from_bytes([1; 32]).id(),
            VaultKey::from_bytes([2; 32]).id()
        );
    }

    #[test]
    fn a_wrapped_key_comes_back_unchanged() {
        let dir = TempDir::new().unwrap();
        let identity = load_or_create_identity(&dir.path().join("identity")).unwrap();
        let key = VaultKey::generate().unwrap();

        let mine = Recipient::new(&identity.to_public().to_string(), None).unwrap();
        let envelope = wrap(&key, &[mine]).unwrap();
        let recovered = unwrap(envelope.as_bytes(), &identity).unwrap();

        assert_eq!(recovered.id(), key.id());
        assert!(envelope.starts_with("-----BEGIN AGE ENCRYPTED FILE-----"));
    }

    #[test]
    fn a_foreign_identity_cannot_open_the_vault() {
        let dir = TempDir::new().unwrap();
        let mine = load_or_create_identity(&dir.path().join("mine")).unwrap();
        let theirs = load_or_create_identity(&dir.path().join("theirs")).unwrap();
        let owner = Recipient::new(&mine.to_public().to_string(), None).unwrap();
        let envelope = wrap(&VaultKey::generate().unwrap(), &[owner]).unwrap();

        let error = unwrap(envelope.as_bytes(), &theirs).unwrap_err();

        assert!(
            error.to_string().contains("cannot open the vault"),
            "{error}"
        );
    }

    #[test]
    fn an_identity_is_created_once_and_then_reused() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("identity");

        let first = load_or_create_identity(&path).unwrap();
        let second = load_or_create_identity(&path).unwrap();

        assert_eq!(
            first.to_public().to_string(),
            second.to_public().to_string()
        );
    }
}
