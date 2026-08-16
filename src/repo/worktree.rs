use std::collections::BTreeSet;
use std::fs;
use std::io::Write as _;
use std::path::{Component, Path, PathBuf};

use anyhow::{Context as _, Result, bail};

use crate::repo::patterns::Patterns;
use crate::vault::seal::{Kind, Secret};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Applied {
    pub removed: usize,
    pub unchanged: usize,
    pub written: usize,
}

pub fn collect(worktree: &Path, patterns: &Patterns) -> Result<Vec<Secret>> {
    let mut secrets = Vec::new();

    for root in patterns.roots() {
        let start = if root.is_empty() {
            worktree.to_path_buf()
        } else {
            worktree.join(&root)
        };

        if !start.exists() {
            continue;
        }

        gather(worktree, &start, patterns, &mut secrets)?;
    }

    secrets.sort_by(|left, right| left.path.cmp(&right.path));
    secrets.dedup_by(|left, right| left.path == right.path);

    Ok(secrets)
}

pub fn apply(worktree: &Path, patterns: &Patterns, secrets: &[Secret]) -> Result<Applied> {
    let mut applied = Applied::default();

    for secret in secrets {
        if write_secret(worktree, secret)? {
            applied.written = applied.written.saturating_add(1);
        } else {
            applied.unchanged = applied.unchanged.saturating_add(1);
        }
    }

    let keep: BTreeSet<&str> = secrets.iter().map(|secret| secret.path.as_str()).collect();

    for existing in collect(worktree, patterns)? {
        if !keep.contains(existing.path.as_str()) {
            remove(worktree, &existing.path)?;
            applied.removed = applied.removed.saturating_add(1);
        }
    }

    Ok(applied)
}

pub fn apply_only(worktree: &Path, secrets: &[Secret]) -> Result<Applied> {
    let mut applied = Applied::default();

    for secret in secrets {
        if write_secret(worktree, secret)? {
            applied.written = applied.written.saturating_add(1);
        } else {
            applied.unchanged = applied.unchanged.saturating_add(1);
        }
    }

    Ok(applied)
}

pub fn remove_all(worktree: &Path, patterns: &Patterns) -> Result<usize> {
    let secrets = collect(worktree, patterns)?;

    for secret in &secrets {
        remove(worktree, &secret.path)?;
    }

    Ok(secrets.len())
}

fn gather(
    worktree: &Path,
    directory: &Path,
    patterns: &Patterns,
    secrets: &mut Vec<Secret>,
) -> Result<()> {
    if directory.is_file() || directory.is_symlink() {
        if let Some(rel) = relative(worktree, directory)
            && patterns.is_secret(&rel)
        {
            secrets.push(read_secret(directory, rel)?);
        }
        return Ok(());
    }

    let entries = fs::read_dir(directory)
        .with_context(|| format!("cannot read `{}`", directory.display()))?;

    for entry in entries {
        let entry = entry.with_context(|| format!("cannot read `{}`", directory.display()))?;
        let path = entry.path();

        let Some(rel) = relative(worktree, &path) else {
            continue;
        };

        if rel == ".git" || rel.starts_with(".git/") {
            continue;
        }

        let kind = entry
            .file_type()
            .with_context(|| format!("cannot inspect `{}`", path.display()))?;

        if kind.is_dir() {
            gather(worktree, &path, patterns, secrets)?;
        } else if patterns.is_secret(&rel) {
            secrets.push(read_secret(&path, rel)?);
        }
    }

    Ok(())
}

fn read_secret(path: &Path, rel: String) -> Result<Secret> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("cannot inspect `{}`", path.display()))?;

    if metadata.is_symlink() {
        let target = fs::read_link(path)
            .with_context(|| format!("cannot read the link `{}`", path.display()))?;
        let target = target
            .to_str()
            .with_context(|| format!("`{}` points somewhere unprintable", path.display()))?;

        return Ok(Secret {
            path: rel,
            kind: Kind::Symlink,
            content: target.as_bytes().to_vec(),
        });
    }

    if !metadata.is_file() {
        bail!(
            "`{}` is neither a regular file nor a symlink, so it cannot be sealed",
            path.display()
        );
    }

    Ok(Secret {
        path: rel,
        kind: Kind::File {
            executable: is_executable(&metadata),
        },
        content: fs::read(path).with_context(|| format!("cannot read `{}`", path.display()))?,
    })
}

fn destination(root: &Path, rel: &str) -> Result<PathBuf> {
    let relative = Path::new(rel);

    if relative
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("`{rel}` is not a path inside the target directory, so it will not be written");
    }

    let mut walked = root.to_path_buf();
    let mut components = relative.components().peekable();

    while let Some(component) = components.next() {
        walked.push(component);

        if components.peek().is_some()
            && fs::symlink_metadata(&walked).is_ok_and(|found| found.is_symlink())
        {
            bail!(
                "`{rel}` would be written through the symlink `{}`, which could put it outside the target directory",
                walked.display()
            );
        }
    }

    Ok(walked)
}

fn write_secret(worktree: &Path, secret: &Secret) -> Result<bool> {
    let path = destination(worktree, &secret.path)?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create `{}`", parent.display()))?;
    }

    match secret.kind {
        Kind::Symlink => {
            let target = String::from_utf8(secret.content.clone())
                .with_context(|| format!("`{}` has an unprintable link target", secret.path))?;

            if fs::read_link(&path).is_ok_and(|current| current == Path::new(&target)) {
                return Ok(false);
            }

            if path.exists() || path.is_symlink() {
                remove_path(&path)?;
            }
            symlink(&target, &path)?;
            Ok(true)
        }
        Kind::File { executable } => {
            let current = fs::symlink_metadata(&path).ok();
            let matches_content = current.as_ref().is_some_and(|metadata| {
                metadata.is_file()
                    && is_executable(metadata) == executable
                    && fs::read(&path).is_ok_and(|bytes| bytes == secret.content)
            });

            if matches_content {
                return Ok(false);
            }

            if current.is_some() {
                remove_path(&path)?;
            }

            write_atomically(&path, &secret.content, executable)?;
            Ok(true)
        }
    }
}

fn write_atomically(path: &Path, contents: &[u8], executable: bool) -> Result<()> {
    let parent = path
        .parent()
        .with_context(|| format!("`{}` has no parent directory", path.display()))?;

    let mut file = tempfile::Builder::new()
        .prefix(".vault-tmp-")
        .tempfile_in(parent)
        .with_context(|| format!("cannot create a temporary file in `{}`", parent.display()))?;

    file.write_all(contents)
        .with_context(|| format!("cannot write `{}`", path.display()))?;
    set_mode(file.path(), executable)?;

    file.persist(path)
        .map_err(|error| error.error)
        .with_context(|| format!("cannot place `{}`", path.display()))?;

    Ok(())
}

fn remove(worktree: &Path, rel: &str) -> Result<()> {
    let path = worktree.join(rel);
    remove_path(&path)?;

    let mut parent = path.parent().map(Path::to_path_buf);
    while let Some(directory) = parent {
        if directory == worktree || !directory.starts_with(worktree) {
            break;
        }
        if fs::remove_dir(&directory).is_err() {
            break;
        }
        parent = directory.parent().map(Path::to_path_buf);
    }

    Ok(())
}

fn remove_path(path: &Path) -> Result<()> {
    fs::remove_file(path).with_context(|| format!("cannot remove `{}`", path.display()))
}

fn relative(worktree: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(worktree).ok()?;
    let rel = rel.to_str()?;

    if rel.is_empty() {
        return None;
    }

    Some(rel.replace('\\', "/"))
}

#[cfg(unix)]
fn is_executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;

    metadata.permissions().mode() & 0o100 != 0
}

#[cfg(not(unix))]
const fn is_executable(_metadata: &fs::Metadata) -> bool {
    false
}

#[cfg(unix)]
fn set_mode(path: &Path, executable: bool) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    let mode = if executable { 0o755 } else { 0o644 };
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .with_context(|| format!("cannot set the mode of `{}`", path.display()))
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _executable: bool) -> Result<()> {
    Ok(())
}

#[cfg(unix)]
fn symlink(target: &str, path: &Path) -> Result<()> {
    std::os::unix::fs::symlink(target, path)
        .with_context(|| format!("cannot link `{}` to `{target}`", path.display()))
}

#[cfg(not(unix))]
fn symlink(target: &str, path: &Path) -> Result<()> {
    fs::write(path, target).with_context(|| format!("cannot write `{}`", path.display()))
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::{apply, apply_only, collect, remove_all};
    use crate::repo::patterns::Patterns;
    use crate::vault::seal::{Kind, Secret};

    fn worktree() -> TempDir {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join(".gitattributes"),
            "secrets/ vault\n*.key vault\n",
        )
        .unwrap();
        dir
    }

    fn secret(path: &str, content: &[u8]) -> Secret {
        Secret {
            path: path.to_owned(),
            kind: Kind::File { executable: false },
            content: content.to_vec(),
        }
    }

    #[test]
    fn collects_only_what_the_patterns_seal() {
        let dir = worktree();
        std::fs::create_dir_all(dir.path().join("secrets/deep")).unwrap();
        std::fs::write(dir.path().join("secrets/prod.env"), "A=1\n").unwrap();
        std::fs::write(dir.path().join("secrets/deep/nested.txt"), "inner\n").unwrap();
        std::fs::write(dir.path().join("top.key"), "k\n").unwrap();
        std::fs::write(dir.path().join("README.md"), "# hi\n").unwrap();

        let patterns = Patterns::load(dir.path()).unwrap();
        let secrets = collect(dir.path(), &patterns).unwrap();

        let paths: Vec<&str> = secrets.iter().map(|secret| secret.path.as_str()).collect();
        assert_eq!(
            paths,
            ["secrets/deep/nested.txt", "secrets/prod.env", "top.key"]
        );
    }

    #[test]
    fn applying_writes_creates_and_removes() {
        let dir = worktree();
        let patterns = Patterns::load(dir.path()).unwrap();
        std::fs::create_dir_all(dir.path().join("secrets")).unwrap();
        std::fs::write(dir.path().join("secrets/gone.env"), "old\n").unwrap();

        let applied = apply(
            dir.path(),
            &patterns,
            &[secret("secrets/prod.env", b"A=1\n")],
        )
        .unwrap();

        assert_eq!(applied.written, 1);
        assert_eq!(applied.removed, 1);
        assert!(!dir.path().join("secrets/gone.env").exists());
        assert_eq!(
            std::fs::read(dir.path().join("secrets/prod.env")).unwrap(),
            b"A=1\n"
        );
    }

    #[test]
    fn an_unchanged_secret_is_left_alone() {
        let dir = worktree();
        let patterns = Patterns::load(dir.path()).unwrap();
        let secrets = [secret("secrets/prod.env", b"A=1\n")];

        apply(dir.path(), &patterns, &secrets).unwrap();
        let before = std::fs::metadata(dir.path().join("secrets/prod.env"))
            .unwrap()
            .modified()
            .unwrap();
        let applied = apply(dir.path(), &patterns, &secrets).unwrap();

        assert_eq!(applied.unchanged, 1);
        assert_eq!(applied.written, 0);
        assert_eq!(
            std::fs::metadata(dir.path().join("secrets/prod.env"))
                .unwrap()
                .modified()
                .unwrap(),
            before,
            "an untouched secret must keep its mtime, or every watcher wakes up"
        );
    }

    #[cfg(unix)]
    #[test]
    fn modes_and_symlinks_round_trip_through_the_worktree() {
        use std::os::unix::fs::PermissionsExt as _;

        let dir = worktree();
        let patterns = Patterns::load(dir.path()).unwrap();
        let secrets = [
            Secret {
                path: "secrets/run.key".to_owned(),
                kind: Kind::File { executable: true },
                content: b"#!/bin/sh\n".to_vec(),
            },
            Secret {
                path: "secrets/link.key".to_owned(),
                kind: Kind::Symlink,
                content: b"run.key".to_vec(),
            },
        ];

        apply(dir.path(), &patterns, &secrets).unwrap();

        let mode = std::fs::metadata(dir.path().join("secrets/run.key"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o755);
        assert!(dir.path().join("secrets/link.key").is_symlink());

        let collected = collect(dir.path(), &patterns).unwrap();
        assert_eq!(collected, [secrets[1].clone(), secrets[0].clone()]);
    }

    #[test]
    fn a_path_that_climbs_out_of_the_directory_is_refused() {
        let dir = worktree();

        let error = apply_only(dir.path(), &[secret("../escaped.env", b"owned")]).unwrap_err();

        assert!(error.to_string().contains("not a path inside"), "{error}");
        assert!(!dir.path().parent().unwrap().join("escaped.env").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_secret_is_not_written_through_a_symlink() {
        let dir = worktree();
        let elsewhere = TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("secrets")).unwrap();
        std::os::unix::fs::symlink(elsewhere.path(), dir.path().join("secrets/out")).unwrap();

        let error =
            apply_only(dir.path(), &[secret("secrets/out/owned.env", b"owned")]).unwrap_err();

        assert!(error.to_string().contains("through the symlink"), "{error}");
        assert!(!elsewhere.path().join("owned.env").exists());
    }

    #[test]
    fn locking_removes_every_secret() {
        let dir = worktree();
        let patterns = Patterns::load(dir.path()).unwrap();
        apply(
            dir.path(),
            &patterns,
            &[secret("secrets/a.env", b"1"), secret("b.key", b"2")],
        )
        .unwrap();

        let removed = remove_all(dir.path(), &patterns).unwrap();

        assert_eq!(removed, 2);
        assert!(collect(dir.path(), &patterns).unwrap().is_empty());
    }
}
