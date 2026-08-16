use std::fs;
use std::path::Path;

use anyhow::{Context as _, Result};

use crate::cmd::source::Source;
use crate::exit::Code;
use crate::repo::{patterns, worktree};
use crate::vault::seal::Secret;
use crate::vault::unseal_all;

pub struct Destination<'a> {
    pub directory: &'a Path,
    pub entries: &'a [String],
    pub mode: Option<&'a str>,
    pub verbose: bool,
}

pub fn unseal(source: &Source, target: &Destination<'_>) -> Result<Code> {
    let opened = source.open()?;
    let secrets = unseal_all(&opened.key, &opened.sealed)?;

    let wanted: Vec<Secret> = if target.entries.is_empty() {
        secrets
    } else {
        secrets
            .into_iter()
            .filter(|secret| patterns::matches_any(target.entries, &secret.path))
            .collect()
    };

    let mode = match target.mode {
        Some(given) => Some(
            u32::from_str_radix(given.trim_start_matches("0o"), 8)
                .with_context(|| format!("`{given}` is not an octal file mode, such as 0400"))?,
        ),
        None => None,
    };

    create_private_directory(target.directory)?;
    worktree::apply_only(target.directory, &wanted)?;
    for secret in &wanted {
        if let Some(mode) = mode {
            set_mode(&target.directory.join(&secret.path), mode)?;
        }
    }

    println!(
        "Unsealed {} secret{} into {}.",
        wanted.len(),
        if wanted.len() == 1 { "" } else { "s" },
        target.directory.display()
    );

    if target.verbose {
        for secret in &wanted {
            println!("  {}", secret.path);
        }
    }

    Ok(Code::Ok)
}

fn create_private_directory(directory: &Path) -> Result<()> {
    let fresh = !directory.exists();

    fs::create_dir_all(directory)
        .with_context(|| format!("cannot create `{}`", directory.display()))?;

    if fresh {
        set_mode(directory, 0o700)?;
    }

    Ok(())
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .with_context(|| format!("cannot set the mode of `{}`", path.display()))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}
