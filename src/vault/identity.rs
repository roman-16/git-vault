use std::fmt;
use std::fs;
use std::io::BufReader;
use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use secrecy::ExposeSecret as _;

use crate::vault::keys::write_private;

enum Kind {
    Native(age::x25519::Identity),
    Ssh(Box<age::ssh::Identity>),
}

pub struct Identity {
    kind: Kind,
    path: PathBuf,
}

impl fmt::Debug for Identity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let kind = match self.kind {
            Kind::Native(_) => "age",
            Kind::Ssh(_) => "ssh",
        };

        write!(formatter, "Identity({kind} at {})", self.path.display())
    }
}

impl Identity {
    pub fn load(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path)
            .with_context(|| format!("cannot read `{}`", path.display()))?;

        if let Some(native) = text
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .find_map(|line| line.parse::<age::x25519::Identity>().ok())
        {
            return Ok(Self {
                kind: Kind::Native(native),
                path: path.to_owned(),
            });
        }

        let ssh = age::ssh::Identity::from_buffer(
            BufReader::new(text.as_bytes()),
            Some(path.display().to_string()),
        )
        .ok()
        .with_context(|| {
            format!(
                "`{}` holds neither an age identity nor an SSH private key",
                path.display()
            )
        })?;

        if matches!(ssh, age::ssh::Identity::Encrypted(_)) {
            bail!(
                "`{}` is a passphrase-protected SSH key, and git-vault cannot ask for the passphrase. Use a key without one, or an age identity",
                path.display()
            );
        }

        if matches!(ssh, age::ssh::Identity::Unsupported(_)) {
            bail!(
                "`{}` is an SSH key type age cannot use. `ssh-ed25519` and `ssh-rsa` work",
                path.display()
            );
        }

        Ok(Self {
            kind: Kind::Ssh(Box::new(ssh)),
            path: path.to_owned(),
        })
    }

    pub fn load_or_create(path: &Path) -> Result<Self> {
        if path.exists() {
            return Self::load(path);
        }

        let parent = path
            .parent()
            .with_context(|| format!("`{}` has no parent directory", path.display()))?;
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create `{}`", parent.display()))?;

        let native = age::x25519::Identity::generate();
        let contents = format!(
            "# git-vault identity. Keep it: without it the vault cannot be opened.\n\
             # public key: {}\n{}\n",
            native.to_public(),
            native.to_string().expose_secret(),
        );
        write_private(path, contents.as_bytes())?;

        Ok(Self {
            kind: Kind::Native(native),
            path: path.to_owned(),
        })
    }

    pub fn public(&self) -> Option<String> {
        match &self.kind {
            Kind::Native(native) => Some(native.to_public().to_string()),
            Kind::Ssh(_) => {
                let mut sibling = self.path.clone().into_os_string();
                sibling.push(".pub");

                fs::read_to_string(PathBuf::from(sibling))
                    .ok()?
                    .lines()
                    .map(str::trim)
                    .find(|line| line.starts_with("ssh-"))
                    .map(str::to_owned)
            }
        }
    }

    pub fn how_to_publish(&self) -> String {
        self.public().map_or_else(
            || {
                format!(
                    "Print your public key with:\n  ssh-keygen -y -f {}\n\nThen ask somebody with access to run `git vault share` with it.",
                    self.path.display()
                )
            },
            |public| {
                format!(
                    "Your public key is:\n  {public}\n\nAsk somebody with access to run:\n  git vault share {public}"
                )
            },
        )
    }

    pub fn as_age(&self) -> &dyn age::Identity {
        match &self.kind {
            Kind::Native(native) => native,
            Kind::Ssh(ssh) => ssh.as_ref(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::Identity;

    #[test]
    fn an_identity_is_created_once_and_then_reused() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("identity");

        let first = Identity::load_or_create(&path).unwrap();
        let second = Identity::load_or_create(&path).unwrap();

        assert_eq!(first.public(), second.public());
        assert!(first.public().unwrap().starts_with("age1"));
    }

    #[test]
    fn an_ssh_private_key_is_an_identity() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("host_key");
        fs::write(&path, ED25519).unwrap();

        let identity = Identity::load(&path).unwrap();

        assert!(identity.public().is_none(), "no sibling .pub yet");

        fs::write(dir.path().join("host_key.pub"), format!("{ED25519_PUB}\n")).unwrap();
        assert_eq!(
            Identity::load(&path).unwrap().public().as_deref(),
            Some(ED25519_PUB)
        );
    }

    #[test]
    fn a_passphrase_protected_key_says_so_instead_of_failing_to_match() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("locked_key");
        fs::write(&path, ENCRYPTED).unwrap();

        let error = Identity::load(&path).unwrap_err().to_string();

        assert!(error.contains("passphrase-protected"), "{error}");
    }

    #[test]
    fn nonsense_is_refused_with_both_possibilities_named() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("junk");
        fs::write(&path, "hunter2\n").unwrap();

        let error = Identity::load(&path).unwrap_err().to_string();

        assert!(error.contains("neither an age identity"), "{error}");
    }

    const ED25519_PUB: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIM2nKNxvClKZar0f9XV19vnQJwAGB5z/RX0cu6dRLf11";

    const ED25519: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACDNpyjcbwpSmWq9H/V1dfb50CcABgec/0V9HLunUS39dQAAAJgfqB7rH6ge
6wAAAAtzc2gtZWQyNTUxOQAAACDNpyjcbwpSmWq9H/V1dfb50CcABgec/0V9HLunUS39dQ
AAAEAWryDLluZObv9al2OsnfGkU1oxCYMxkGq8Z2UdpO1qNM2nKNxvClKZar0f9XV19vnQ
JwAGB5z/RX0cu6dRLf11AAAAEHRlc3RAZXhhbXBsZS5jb20BAgMEBQ==
-----END OPENSSH PRIVATE KEY-----
";

    const ENCRYPTED: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAACmFlczI1Ni1jdHIAAAAGYmNyeXB0AAAAGAAAABBjNDh5oV
RylMEFtSvcef4gAAAAGAAAAAEAAAAzAAAAC3NzaC1lZDI1NTE5AAAAIH1XyNmsl7G10fyH
1MR4uhbRqe932bIc3Yv7d5BrQHUiAAAAoEdDAX+wK3/8OMp7uU65CD6ctgFni0cIL8UYy/
ekBo6b7dD7T2zcjDqMaa3fvWnS0iVr64Z2rWHahpxiIXNLiXYehEFGldxsbrk5IeReeQT5
9EVJPFjvRmkQEohyhXX5Wkw+ryvuir08xIa6ke74YUJJYgvBCxcl/v7Zkn+HlnKreEsKNb
o9CTYaBKILPVUSYncgfNIBna4iy/ZhGHtu6ZY=
-----END OPENSSH PRIVATE KEY-----
";
}
