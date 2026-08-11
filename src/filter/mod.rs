mod merge;
mod textconv;

use std::ffi::OsString;
use std::io::{Read as _, Write as _, stdin, stdout};
use std::path::Path;

use anyhow::{Context as _, Result, bail};

use crate::exit::Code;

const EVERYTHING_MAY_HAVE_CHANGED: &[u8] = b"git-vault\0/\0";
use crate::repo::Repo;
use crate::repo::session::Empty;
use crate::vault::format::Vault;

pub fn dispatch(args: &[OsString]) -> Result<Code> {
    match args.first().and_then(|arg| arg.to_str()) {
        Some("clean") => clean(),
        Some("fsmonitor") => fsmonitor(),
        Some("merge") => merge::merge(args.get(1..).unwrap_or_default()),
        Some("refuse") => refuse(args.get(1)),
        Some("smudge") => smudge(),
        Some("textconv") => textconv(args.get(1..).unwrap_or_default()),
        Some(other) => {
            bail!(
                "unknown filter `{other}`: expected `clean`, `fsmonitor`, `merge`, `refuse`, `smudge` or `textconv`"
            )
        }
        None => {
            bail!(
                "`git vault filter` needs a mode: `clean`, `fsmonitor`, `merge`, `refuse`, `smudge` or `textconv`"
            )
        }
    }
}

fn clean() -> Result<Code> {
    let input = read_input()?;

    if input.is_empty() {
        bail!(
            "refusing to store an empty `.vault/data`: run `git vault seal`, or `git checkout -- .vault/data` to put the sealed file back"
        );
    }

    Vault::decode(&input).context(
        "refusing to store a `.vault/data` that is not a valid vault. If a merge left conflict markers in it, resolve them with `git vault seal`",
    )?;

    write_output(&input)
}

fn refuse(path: Option<&OsString>) -> Result<Code> {
    let path = path.map_or_else(
        || "that file".to_owned(),
        |path| path.to_string_lossy().into_owned(),
    );

    bail!(
        "refusing to put the plaintext of `{path}` into the index: it is declared secret, so its contents belong in `{}`, and `{}` is what normally keeps it out of git. To publish it in the clear, run `git vault remove {path}` first",
        crate::paths::DATA,
        crate::paths::IGNORE
    )
}

fn smudge() -> Result<Code> {
    let repo = Repo::discover()?;
    let input = read_input()?;

    if input.is_empty() {
        bail!("git handed the vault filter an empty blob, which cannot be a sealed vault");
    }

    if repo.is_unlocked() {
        let key = repo.key()?;

        if Vault::decode(&input)?.key_id == key.id() {
            let secrets = repo.unseal(&input)?;
            repo.apply(&secrets)?;
        } else {
            eprintln!(
                "git-vault: `{}` was sealed with a newer vault key. The secrets on disk were left alone; run `git vault unlock` to pick up the new key.",
                crate::paths::DATA
            );
        }
    }

    write_output(&input)
}

pub fn fsmonitor() -> Result<Code> {
    if let Err(error) = reseal() {
        eprintln!("git-vault: {error:#}");
    }

    write_output(EVERYTHING_MAY_HAVE_CHANGED)
}

fn reseal() -> Result<()> {
    let repo = Repo::discover()?;

    if !repo.is_unlocked() || repo.operation_in_progress() {
        return Ok(());
    }

    repo.seal_worktree(Empty::Refuse).map(|_outcome| ())
}

fn textconv(args: &[OsString]) -> Result<Code> {
    let path = args
        .first()
        .context("`git vault filter textconv` needs the file to render")?;

    let rendered = textconv::render(Path::new(path))?;

    write_output(rendered.as_bytes())
}

fn read_input() -> Result<Vec<u8>> {
    let mut buffer = Vec::new();
    stdin()
        .lock()
        .read_to_end(&mut buffer)
        .context("cannot read the filter input")?;
    Ok(buffer)
}

fn write_output(bytes: &[u8]) -> Result<Code> {
    let mut output = stdout().lock();
    output
        .write_all(bytes)
        .context("cannot write the filter output")?;
    output.flush().context("cannot flush the filter output")?;
    Ok(Code::Ok)
}
