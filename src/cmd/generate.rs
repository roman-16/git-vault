use std::io::Write as _;
use std::path::Path;

use anyhow::{Context as _, Result};
use clap::CommandFactory as _;
use clap_complete::Shell;

use crate::cli::Args;
use crate::exit::Code;

pub fn completions(shell: Shell) -> Result<Code> {
    let mut command = Args::command();
    let name = command.get_name().to_owned();
    let mut out = std::io::stdout().lock();

    clap_complete::generate(shell, &mut command, name, &mut out);

    let dispatch = match shell {
        Shell::Bash => Some("_git_vault() { _git-vault \"$@\"; }"),
        Shell::Zsh => Some("compdef _git-vault git-vault"),
        _elsewhere => None,
    };

    if let Some(dispatch) = dispatch {
        writeln!(out, "{dispatch}").context("cannot write the completion script")?;
    }

    out.flush().context("cannot write the completion script")?;

    Ok(Code::Ok)
}

pub fn man(directory: &Path) -> Result<Code> {
    std::fs::create_dir_all(directory)
        .with_context(|| format!("cannot create `{}`", directory.display()))?;

    let root = Args::command();
    write_page(&root, root.get_name(), directory)?;

    for command in root.get_subcommands() {
        if command.is_hide_set() {
            continue;
        }
        let name = format!("git-vault-{}", command.get_name());
        write_page(command, &name, directory)?;
    }

    println!("Wrote man pages to {}.", directory.display());

    Ok(Code::Ok)
}

fn write_page(command: &clap::Command, name: &str, directory: &Path) -> Result<()> {
    let mut rendered = Vec::new();
    clap_mangen::Man::new(command.clone().name(name.to_owned()))
        .render(&mut rendered)
        .with_context(|| format!("cannot render the manual page for `{name}`"))?;

    let path = directory.join(format!("{name}.1"));
    std::fs::write(&path, rendered).with_context(|| format!("cannot write `{}`", path.display()))
}
