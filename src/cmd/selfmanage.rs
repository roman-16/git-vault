use std::io::{IsTerminal as _, Write as _, stdin, stdout};

use anyhow::{Result, bail};

use crate::exit::Code;
use crate::selfmanage::release::{self, DEVELOPMENT};
use crate::selfmanage::replace;
use crate::selfmanage::source::{Source, running_executable};
use crate::vault::keys;

const CURRENT: &str = env!("CARGO_PKG_VERSION");

pub fn update(wanted: Option<&str>, check: bool, reinstall: bool) -> Result<Code> {
    let executable = running_executable()?;

    if !check {
        refuse_managed(&executable, Managed::Update)?;
    }

    let latest = match wanted {
        Some(version) => version.trim_start_matches('v').to_owned(),
        None => release::latest_version()?,
    };
    let newer = release::is_newer(&latest, CURRENT);

    if check {
        if newer {
            println!(
                "An update is available: {CURRENT} to {latest}. Install it with `git vault update`."
            );
        } else {
            println!("git-vault is up to date ({CURRENT}).");
        }
        return Ok(Code::Ok);
    }

    if CURRENT == DEVELOPMENT && wanted.is_none() && !reinstall {
        bail!(
            "this is a development build with no version to compare against. Name the version you want, as in `git vault update 1.0.0`"
        );
    }

    if !newer && wanted.is_none() && !reinstall {
        println!("git-vault is already up to date ({CURRENT}).");
        return Ok(Code::Ok);
    }

    println!("Downloading git-vault {latest}...");
    let binary = release::download(&latest)?;

    println!("The checksum matches. Installing...");
    replace::install(&binary, &executable)
        .map_err(|error| managed_by_permission(error, Managed::Update))?;

    println!("Updated git-vault {CURRENT} to {latest}.");

    Ok(Code::Ok)
}

pub fn uninstall(dry_run: bool, purge: bool, yes: bool) -> Result<Code> {
    let executable = running_executable()?;
    refuse_managed(&executable, Managed::Remove)?;

    let identity = if purge {
        Some(keys::identity_path()?)
    } else {
        None
    };

    println!("This would remove:");
    println!("  {}", executable.display());
    if let Some(path) = &identity {
        println!("  {} (your identity)", path.display());
    }

    if dry_run {
        println!();
        println!("Nothing was removed, because this was a dry run.");
        return Ok(Code::Ok);
    }

    if !yes {
        if identity.is_some() {
            println!();
            println!(
                "Deleting your identity cannot be undone. Every vault you are a recipient of becomes unreadable to you, unless somebody with access shares it with a new key."
            );
        }
        if !confirmed()? {
            println!("Left everything in place.");
            return Ok(Code::Ok);
        }
    }

    if let Some(path) = &identity
        && let Err(error) = std::fs::remove_file(path)
    {
        eprintln!("git-vault: could not remove `{}`: {error}", path.display());
    }

    replace::remove(&executable).map_err(|error| managed_by_permission(error, Managed::Remove))?;

    println!("Removed git-vault.");
    println!();
    println!(
        "Repositories you used it in still have its wiring in `.git/config`, which makes `git add` fail there. In each one:"
    );
    println!("  git config --remove-section filter.vault");
    println!("  git config --remove-section diff.vault");
    println!("  git config --remove-section merge.vault");
    println!("  git config --unset core.fsmonitor");
    println!("  rm .git/hooks/pre-commit");

    Ok(Code::Ok)
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Managed {
    Remove,
    Update,
}

fn refuse_managed(executable: &std::path::Path, action: Managed) -> Result<()> {
    let source = Source::of(executable);

    if source == Source::Standalone {
        return Ok(());
    }

    let instead = match action {
        Managed::Remove => source.remove_with(),
        Managed::Update => source.update_with(),
    };

    bail!(
        "git-vault was installed with {} (`{}`), so {instead}",
        source.manager(),
        executable.display()
    )
}

fn managed_by_permission(error: anyhow::Error, action: Managed) -> anyhow::Error {
    let denied = error
        .chain()
        .filter_map(|cause| cause.downcast_ref::<std::io::Error>())
        .any(|cause| cause.kind() == std::io::ErrorKind::PermissionDenied);

    if !denied {
        return error;
    }

    let verb = match action {
        Managed::Remove => "remove",
        Managed::Update => "replace",
    };

    error.context(format!(
        "cannot {verb} it without permission, which is what a system package looks like. Use the package manager that installed it: apt, dnf, apk, pacman or your distribution's equivalent"
    ))
}

fn confirmed() -> Result<bool> {
    if !stdin().is_terminal() {
        bail!("nothing is asking, because this is not a terminal. Pass `--yes` to mean it");
    }

    print!("Remove it? [y/N] ");
    stdout().flush()?;

    let mut answer = String::new();
    stdin().read_line(&mut answer)?;

    Ok(matches!(answer.trim(), "y" | "Y" | "yes"))
}
