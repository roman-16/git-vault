use std::sync::Arc;
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use sha2::{Digest as _, Sha256};
use ureq::ResponseExt as _;

const RELEASES: &str = "https://github.com/roman-16/git-vault/releases";
const TIMEOUT: Duration = Duration::from_mins(1);

pub const DEVELOPMENT: &str = "0.0.0";

pub fn asset_name() -> Result<String> {
    let system = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "darwin"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        bail!("there are no published binaries for this operating system")
    };

    let architecture = if cfg!(target_arch = "x86_64") {
        "amd64"
    } else if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        bail!("there are no published binaries for this processor")
    };

    let suffix = if cfg!(target_os = "windows") {
        ".exe"
    } else {
        ""
    };

    Ok(format!("git-vault_{system}_{architecture}{suffix}"))
}

fn agent() -> ureq::Agent {
    let roots = std::env::var_os("SSL_CERT_FILE")
        .and_then(|path| std::fs::read(path).ok())
        .map_or(ureq::tls::RootCerts::WebPki, |pem| {
            ureq::tls::RootCerts::Specific(Arc::new(
                ureq::tls::parse_pem(&pem)
                    .filter_map(|item| match item {
                        Ok(ureq::tls::PemItem::Certificate(certificate)) => Some(certificate),
                        _other => None,
                    })
                    .collect(),
            ))
        });

    ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .tls_config(ureq::tls::TlsConfig::builder().root_certs(roots).build())
        .build()
        .into()
}

pub fn latest_version() -> Result<String> {
    let response = agent()
        .get(format!("{RELEASES}/latest"))
        .call()
        .with_context(|| format!("cannot reach {RELEASES}/latest"))?;

    let landed = response.get_uri().to_string();
    let Some((_before, tag)) = landed.rsplit_once("/tag/") else {
        bail!("there are no published releases yet, so there is nothing to update to");
    };

    Ok(tag.trim_start_matches('v').to_owned())
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    numbers(candidate) > numbers(current)
}

fn numbers(version: &str) -> Vec<u64> {
    version
        .split('-')
        .next()
        .unwrap_or_default()
        .split('.')
        .map(|part| part.parse().unwrap_or_default())
        .collect()
}

pub fn expected_checksum(checksums: &str, asset: &str) -> Result<String> {
    checksums
        .lines()
        .filter_map(|line| line.split_once(char::is_whitespace))
        .find(|(_sum, name)| name.trim().trim_start_matches('*') == asset)
        .map(|(sum, _name)| sum.to_owned())
        .with_context(|| format!("checksums.txt does not mention {asset}"))
}

pub fn download(version: &str) -> Result<Vec<u8>> {
    let asset = asset_name()?;
    let base = format!("{RELEASES}/download/v{}", version.trim_start_matches('v'));
    let agent = agent();

    let checksums = get(&agent, &format!("{base}/checksums.txt"))?;
    let expected = expected_checksum(
        std::str::from_utf8(&checksums).context("checksums.txt is not text")?,
        &asset,
    )?;

    let binary = get(&agent, &format!("{base}/{asset}"))?;
    let actual = hex(&Sha256::digest(&binary));

    if actual != expected {
        bail!(
            "{asset} does not match its published checksum, so it was not installed (expected {expected}, got {actual})"
        );
    }

    Ok(binary)
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;

    bytes.iter().fold(String::new(), |mut text, byte| {
        let _written = write!(text, "{byte:02x}");
        text
    })
}

fn get(agent: &ureq::Agent, url: &str) -> Result<Vec<u8>> {
    let mut response = agent
        .get(url)
        .call()
        .with_context(|| format!("cannot download {url}"))?;

    if response.status() != 200 {
        bail!("{url} answered {}", response.status());
    }

    response
        .body_mut()
        .with_config()
        .limit(64 * 1024 * 1024)
        .read_to_vec()
        .with_context(|| format!("cannot read the response from {url}"))
}

#[cfg(test)]
mod tests {
    use super::{asset_name, expected_checksum, hex, is_newer};

    #[test]
    fn the_asset_matches_what_a_release_publishes() {
        let name = asset_name().unwrap();

        assert!(name.starts_with("git-vault_"), "{name}");
        assert!(
            ["linux", "darwin", "windows"]
                .iter()
                .any(|system| name.contains(system)),
            "{name}"
        );
        assert!(
            ["amd64", "arm64"]
                .iter()
                .any(|architecture| name.contains(architecture)),
            "{name}"
        );
    }

    #[test]
    fn a_checksum_is_found_by_asset_name() {
        let checksums = "\
aaaa  git-vault_darwin_arm64
bbbb  git-vault_linux_amd64
cccc *git-vault_windows_amd64.exe
";

        assert_eq!(
            expected_checksum(checksums, "git-vault_linux_amd64").unwrap(),
            "bbbb"
        );
        assert_eq!(
            expected_checksum(checksums, "git-vault_windows_amd64.exe").unwrap(),
            "cccc"
        );
        assert!(expected_checksum(checksums, "git-vault_linux_arm64").is_err());
    }

    #[test]
    fn versions_compare_by_number_not_by_text() {
        assert!(is_newer("1.10.0", "1.9.0"));
        assert!(is_newer("2.0.0", "1.99.99"));
        assert!(is_newer("1.0.1", "1.0.0"));
        assert!(!is_newer("1.0.0", "1.0.0"));
        assert!(!is_newer("1.0.0", "1.0.1"));
        assert!(is_newer("1.0.0", "0.0.0"));
    }

    #[test]
    fn a_digest_renders_as_lowercase_hex() {
        assert_eq!(hex(&[0x00, 0x0f, 0xab, 0xff]), "000fabff");
    }

    #[test]
    fn a_prerelease_compares_on_its_numbers() {
        assert!(is_newer("1.2.0-rc.1", "1.1.0"));
        assert!(!is_newer("1.2.0-rc.1", "1.2.0"));
    }
}
