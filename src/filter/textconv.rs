use std::fs;
use std::path::Path;

use anyhow::{Context as _, Result};

use crate::repo::Repo;
use crate::vault::format::Vault;
use crate::vault::seal::{Kind, Secret};

pub fn render(path: &Path) -> Result<String> {
    let bytes = fs::read(path).with_context(|| format!("cannot read `{}`", path.display()))?;

    let Ok(vault) = Vault::decode(&bytes) else {
        return Ok("# git-vault: not a vault\n".to_owned());
    };

    let secrets = Repo::discover()
        .ok()
        .filter(Repo::is_unlocked)
        .and_then(|repo| repo.unseal(&bytes).ok());

    Ok(secrets.map_or_else(|| render_sealed(&vault), |secrets| render_secrets(&secrets)))
}

fn render_sealed(vault: &Vault) -> String {
    use std::fmt::Write as _;

    let mut rendered = format!(
        "# git-vault: {} sealed entr{}, and no key here to open them.\n# Run `git vault unlock` to see this as plaintext.\n",
        vault.entries.len(),
        if vault.entries.len() == 1 { "y" } else { "ies" }
    );

    for entry in &vault.entries {
        let _ignored = writeln!(
            rendered,
            "# sealed entry {} ({} bytes, contents {})",
            hex(&entry.id),
            entry.sealed.len(),
            hex(&entry.nonce)
        );
    }

    rendered
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    bytes
        .iter()
        .take(6)
        .fold(String::new(), |mut rendered, byte| {
            let _ignored = write!(rendered, "{byte:02x}");
            rendered
        })
}

fn render_secrets(secrets: &[Secret]) -> String {
    let mut rendered = String::new();

    for secret in secrets {
        rendered.push_str(&header(secret));

        match std::str::from_utf8(&secret.content) {
            Ok(text) => {
                rendered.push_str(text);
                if !text.ends_with('\n') {
                    rendered.push('\n');
                }
            }
            Err(_binary) => {}
        }

        rendered.push('\n');
    }

    if rendered.is_empty() {
        rendered.push_str("# git-vault: nothing is sealed\n");
    }

    rendered
}

fn header(secret: &Secret) -> String {
    let kind = match secret.kind {
        Kind::File { executable: true } => "executable",
        Kind::File { executable: false } => "file",
        Kind::Symlink => "symlink",
    };

    let mut header = format!("# {} ({kind}, {} bytes", secret.path, secret.content.len());

    if std::str::from_utf8(&secret.content).is_err() {
        header.push_str(", binary");
    } else if !secret.content.ends_with(b"\n") {
        header.push_str(", no trailing newline");
    }

    header.push_str(")\n");
    header
}

#[cfg(test)]
mod tests {
    use super::{Kind, Secret, render_secrets};

    fn secret(path: &str, content: &[u8]) -> Secret {
        Secret {
            path: path.to_owned(),
            kind: Kind::File { executable: false },
            content: content.to_vec(),
        }
    }

    #[test]
    fn every_secret_gets_a_header_that_names_it() {
        let rendered = render_secrets(&[
            secret("secrets/a.env", b"A=1\n"),
            secret("secrets/b.env", b"B=2\n"),
        ]);

        assert_eq!(
            rendered,
            "# secrets/a.env (file, 4 bytes)\nA=1\n\n# secrets/b.env (file, 4 bytes)\nB=2\n\n"
        );
    }

    #[test]
    fn a_missing_trailing_newline_is_stated_rather_than_invented() {
        let rendered = render_secrets(&[secret("a", b"no newline")]);

        assert!(rendered.contains("no trailing newline"), "{rendered}");
        assert!(rendered.contains("no newline\n"), "{rendered}");
    }

    #[test]
    fn binary_content_is_summarised_not_dumped() {
        let rendered = render_secrets(&[secret("a.bin", &[0xff, 0xfe, 0x00])]);

        assert_eq!(rendered, "# a.bin (file, 3 bytes, binary)\n\n");
    }

    #[test]
    fn modes_and_links_are_visible() {
        let rendered = render_secrets(&[
            Secret {
                path: "run.sh".to_owned(),
                kind: Kind::File { executable: true },
                content: b"#!/bin/sh\n".to_vec(),
            },
            Secret {
                path: "current".to_owned(),
                kind: Kind::Symlink,
                content: b"run.sh".to_vec(),
            },
        ]);

        assert!(
            rendered.contains("# run.sh (executable, 10 bytes)"),
            "{rendered}"
        );
        assert!(
            rendered.contains("# current (symlink, 6 bytes"),
            "{rendered}"
        );
    }

    #[test]
    fn an_empty_vault_says_so() {
        assert_eq!(render_secrets(&[]), "# git-vault: nothing is sealed\n");
    }
}
