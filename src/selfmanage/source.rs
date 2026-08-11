use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Source {
    Cargo,
    Homebrew,
    Nix,
    Standalone,
    Winget,
}

impl Source {
    pub fn of(executable: &Path) -> Self {
        let path = executable
            .to_string_lossy()
            .replace('\\', "/")
            .to_lowercase();

        if path.contains("/nix/store/") {
            Self::Nix
        } else if path.contains("/cellar/")
            || path.contains("/caskroom/")
            || path.contains("/.linuxbrew/")
        {
            Self::Homebrew
        } else if path.contains("/winget/") {
            Self::Winget
        } else if path.contains("/.cargo/bin/") {
            Self::Cargo
        } else {
            Self::Standalone
        }
    }

    pub const fn manager(self) -> &'static str {
        match self {
            Self::Cargo => "cargo",
            Self::Homebrew => "Homebrew",
            Self::Nix => "Nix",
            Self::Standalone => "",
            Self::Winget => "winget",
        }
    }

    pub const fn update_with(self) -> &'static str {
        match self {
            Self::Cargo => "run `cargo install git-vault-cli --force`",
            Self::Homebrew => "run `brew upgrade --cask git-vault`",
            Self::Nix => "update it through your flake or nixpkgs configuration",
            Self::Standalone => "",
            Self::Winget => "run `winget upgrade Roman-16.GitVault`",
        }
    }

    pub const fn remove_with(self) -> &'static str {
        match self {
            Self::Cargo => "run `cargo uninstall git-vault-cli`",
            Self::Homebrew => "run `brew uninstall --cask git-vault`",
            Self::Nix => "remove it through your flake or nixpkgs configuration",
            Self::Standalone => "",
            Self::Winget => "run `winget uninstall Roman-16.GitVault`",
        }
    }
}

pub fn running_executable() -> Result<PathBuf> {
    let executable = std::env::current_exe().context("cannot tell where this binary lives")?;

    Ok(std::fs::canonicalize(&executable)
        .map(crate::paths::without_verbatim_prefix)
        .unwrap_or(executable))
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::Source;

    #[test]
    fn a_package_managed_install_is_recognised_by_its_path() {
        for (path, expected) in [
            (
                "/nix/store/abc123-git-vault-1.0.0/bin/git-vault",
                Source::Nix,
            ),
            (
                "/opt/homebrew/Caskroom/git-vault/1.0.0/git-vault",
                Source::Homebrew,
            ),
            (
                "/usr/local/Cellar/git-vault/1.0.0/bin/git-vault",
                Source::Homebrew,
            ),
            ("/home/you/.linuxbrew/bin/git-vault", Source::Homebrew),
            ("/home/you/.cargo/bin/git-vault", Source::Cargo),
            (
                "C:/Users/You/AppData/Local/Microsoft/WinGet/Packages/Roman-16.GitVault/git-vault.exe",
                Source::Winget,
            ),
        ] {
            assert_eq!(Source::of(Path::new(path)), expected, "{path}");
        }
    }

    #[test]
    fn a_script_or_manual_install_may_manage_itself() {
        for path in [
            "/home/you/.local/bin/git-vault",
            "/usr/local/bin/git-vault",
            "/usr/bin/git-vault",
            "C:/Users/You/AppData/Local/Programs/git-vault/git-vault.exe",
        ] {
            assert_eq!(Source::of(Path::new(path)), Source::Standalone, "{path}");
        }
    }

    #[test]
    fn windows_separators_do_not_hide_a_managed_install() {
        let path = Path::new(r"C:\Users\You\.cargo\bin\git-vault.exe");

        assert_eq!(Source::of(path), Source::Cargo);
    }

    #[test]
    fn every_manager_names_a_way_to_update_and_remove() {
        for source in [Source::Cargo, Source::Homebrew, Source::Nix, Source::Winget] {
            assert!(!source.manager().is_empty());
            assert!(!source.update_with().is_empty());
            assert!(!source.remove_with().is_empty());
        }
    }
}
