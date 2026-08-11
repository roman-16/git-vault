use std::ffi::OsString;
use std::iter;
use std::path::PathBuf;

use anyhow::Result;
use clap::{Parser, Subcommand};
use clap_complete::Shell;

use crate::cmd;
use crate::exit::Code;
use crate::filter;

pub fn run() -> Result<Code> {
    let args = match Args::try_parse() {
        Ok(args) => args,
        Err(report) => {
            let misused = report.use_stderr();
            let _ignored = report.print();
            return Ok(if misused { Code::Misuse } else { Code::Ok });
        }
    };

    match args.command {
        Command::Add { paths } => cmd::add(&paths),
        Command::Completions { shell } => cmd::completions(shell),
        Command::Diff { paths } => cmd::diff(&paths),
        Command::Doctor => cmd::doctor(),
        Command::ExportKey { file } => cmd::export_key(&file),
        Command::Filter { mode, args } => {
            let forwarded: Vec<OsString> =
                iter::once(mode).chain(args).map(OsString::from).collect();
            filter::dispatch(&forwarded)
        }
        Command::Hook { name } => cmd::hook(&name),
        Command::Init => cmd::init(),
        Command::Keys => cmd::keys(),
        Command::Lock => cmd::lock(),
        Command::Log { path } => cmd::log(path.as_deref()),
        Command::Ls => cmd::ls(),
        Command::Man { directory } => cmd::man(&directory),
        Command::Remove { paths } => cmd::remove(&paths),
        Command::Restore { paths } => cmd::restore(&paths),
        Command::Revoke { recipient } => cmd::revoke(&recipient),
        Command::Rotate => cmd::rotate(),
        Command::Seal { allow_empty } => cmd::seal(allow_empty),
        Command::Share { recipient, label } => cmd::share(&recipient, label.as_deref()),
        Command::Status => cmd::status(),
        Command::Uninstall {
            dry_run,
            purge,
            yes,
        } => cmd::uninstall(dry_run, purge, yes),
        Command::Unlock { key_file } => cmd::unlock(key_file.as_deref()),
        Command::Update {
            version,
            check,
            reinstall,
        } => cmd::update(version.as_deref(), check, reinstall),
    }
}

#[derive(Debug, Parser)]
#[command(
    name = "git-vault",
    bin_name = "git vault",
    version,
    about = "Transparent encryption for git that collapses everything it protects into one opaque file",
    max_term_width = 100
)]
pub struct Args {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Generate a vault key, wire this clone, and create the vault files")]
    Init,

    #[command(about = "Materialise the sealed secrets on this machine")]
    Unlock {
        #[arg(
            long,
            value_name = "FILE",
            help = "Read the vault key from a file instead of unwrapping it with an age identity"
        )]
        key_file: Option<PathBuf>,
    },

    #[command(about = "Remove the plaintext secrets and the local vault key")]
    Lock,

    #[command(about = "Bring the sealed file up to date with the secrets on disk")]
    Seal {
        #[arg(
            long,
            help = "Seal even when every secret has disappeared from the worktree"
        )]
        allow_empty: bool,
    },

    #[command(about = "Declare paths secret: seal them, stop tracking them, ignore them")]
    Add {
        #[arg(required = true, value_name = "PATH")]
        paths: Vec<PathBuf>,
    },

    #[command(about = "Replace this binary with a published release")]
    Update {
        #[arg(
            value_name = "VERSION",
            help = "A version to install instead of the latest"
        )]
        version: Option<String>,

        #[arg(long, help = "Only report whether an update is available")]
        check: bool,

        #[arg(long, help = "Install again even when already up to date")]
        reinstall: bool,
    },

    #[command(about = "Remove this binary")]
    Uninstall {
        #[arg(long, help = "Show what would be removed and stop")]
        dry_run: bool,

        #[arg(long, help = "Also delete your identity, losing access to every vault")]
        purge: bool,

        #[arg(long, help = "Do not ask first")]
        yes: bool,
    },

    #[command(about = "Stop sealing paths")]
    Remove {
        #[arg(required = true, value_name = "PATH")]
        paths: Vec<PathBuf>,
    },

    #[command(about = "List what is sealed")]
    Ls,

    #[command(about = "Show which secrets changed")]
    Status,

    #[command(about = "Show a plaintext diff of the secrets")]
    Diff {
        #[arg(value_name = "PATH")]
        paths: Vec<PathBuf>,
    },

    #[command(about = "Show the history of a secret")]
    Log {
        #[arg(value_name = "PATH")]
        path: Option<PathBuf>,
    },

    #[command(about = "Discard local edits to secrets")]
    Restore {
        #[arg(value_name = "PATH")]
        paths: Vec<PathBuf>,
    },

    #[command(about = "Wrap the vault key for another recipient")]
    Share {
        #[arg(
            value_name = "KEY",
            help = "An age recipient, an SSH public key, or a file holding one"
        )]
        recipient: String,

        #[arg(long, value_name = "NAME", help = "A name to remember them by")]
        label: Option<String>,
    },

    #[command(about = "Remove a recipient and rotate the vault key")]
    Revoke {
        #[arg(value_name = "KEY")]
        recipient: String,
    },

    #[command(about = "List who has access")]
    Keys,

    #[command(about = "Replace the vault key and re-seal")]
    Rotate,

    #[command(about = "Write the vault key to a file, for CI")]
    ExportKey {
        #[arg(value_name = "FILE")]
        file: PathBuf,
    },

    #[command(about = "Verify the wiring of this clone, exiting non-zero on problems")]
    Doctor,

    #[command(about = "Print a shell completion script")]
    Completions {
        #[arg(value_name = "SHELL")]
        shell: Shell,
    },

    #[command(hide = true, about = "Write the manual pages to a directory")]
    Man {
        #[arg(value_name = "DIR")]
        directory: PathBuf,
    },

    #[command(hide = true, about = "Internal: the hooks git runs for us")]
    Hook {
        #[arg(value_name = "NAME")]
        name: String,
    },

    #[command(hide = true, about = "Internal: the filter driver git invokes")]
    Filter {
        #[arg(
            value_name = "MODE",
            help = "clean, smudge, textconv, merge or fsmonitor"
        )]
        mode: String,

        #[arg(
            trailing_var_arg = true,
            allow_hyphen_values = true,
            value_name = "ARG",
            help = "Whatever git passes after the mode"
        )]
        args: Vec<String>,
    },
}

#[cfg(test)]
mod tests {
    use clap::CommandFactory as _;

    use super::Args;

    #[test]
    fn the_command_tree_is_well_formed() {
        Args::command().debug_assert();
    }

    #[test]
    fn help_names_the_canonical_invocation() {
        let help = Args::command().render_help().to_string();

        assert!(help.contains("git vault"), "{help}");
    }

    #[test]
    fn every_command_carries_its_own_help() {
        for command in Args::command().get_subcommands() {
            assert!(
                command.get_about().is_some(),
                "`{}` has no help text",
                command.get_name()
            );
        }
    }
}
