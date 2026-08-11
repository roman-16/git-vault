use std::collections::BTreeSet;
use std::ffi::OsString;
use std::fs;
use std::path::Path;

use anyhow::{Context as _, Result, bail};

use crate::exit::Code;
use crate::paths;
use crate::repo::Repo;
use crate::vault::format::{Entry, Vault};
use crate::vault::keys::VaultKey;
use crate::vault::seal::Secret;
use crate::vault::{seal_all, unseal_all};

pub fn merge(args: &[OsString]) -> Result<Code> {
    let (ancestor, ours, theirs) = match args {
        [ancestor, ours, theirs, ..] => (Path::new(ancestor), Path::new(ours), Path::new(theirs)),
        _too_few => bail!(
            "the vault merge driver needs `%O %A %B %L %P`; check `merge.vault.driver` in .git/config"
        ),
    };
    let marker_length = args
        .get(3)
        .and_then(|value| value.to_str())
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(7);

    let sides = Sides::read(ancestor, ours, theirs)?;
    let outcome = sides.merge(marker_length)?;

    fs::write(ours, &outcome.sealed)
        .with_context(|| format!("cannot write the merge result to `{}`", ours.display()))?;

    if outcome.conflicts.is_empty() {
        return Ok(Code::Ok);
    }

    eprintln!(
        "git-vault: {} of the sealed secrets conflict{}:",
        outcome.conflicts.len(),
        if outcome.conflicts.len() == 1 {
            "s"
        } else {
            ""
        }
    );
    for name in &outcome.conflicts {
        eprintln!("  {name}");
    }
    eprintln!("Resolve them as ordinary files, then:");
    eprintln!("  git vault seal && git add {}", paths::DATA);

    Ok(Code::Conflict)
}

struct Sides {
    ancestor: Vault,
    ours: Vault,
    theirs: Vault,
    raw: [Vec<u8>; 3],
}

struct Outcome {
    sealed: Vec<u8>,
    conflicts: Vec<String>,
}

impl Sides {
    fn read(ancestor: &Path, ours: &Path, theirs: &Path) -> Result<Self> {
        let raw = [read(ancestor)?, read(ours)?, read(theirs)?];
        let [base, mine, yours] = &raw;

        let sides = Self {
            ancestor: decode(base, "the common ancestor")?,
            ours: decode(mine, "our side")?,
            theirs: decode(yours, "their side")?,
            raw,
        };

        if sides.ours.key_id != sides.theirs.key_id {
            bail!(
                "the two sides were sealed with different vault keys, so they cannot be merged. Rotate one side onto the other's key first"
            );
        }

        Ok(sides)
    }

    fn merge(&self, marker_length: usize) -> Result<Outcome> {
        match cached_key() {
            Some(key) if key.id() == self.ours.key_id => self.merge_secrets(&key, marker_length),
            _no_usable_key => self.merge_entries(),
        }
    }

    fn merge_secrets(&self, key: &VaultKey, marker_length: usize) -> Result<Outcome> {
        let [base, mine, yours] = &self.raw;
        let ancestor = unseal_all(key, base)?;
        let ours = unseal_all(key, mine)?;
        let theirs = unseal_all(key, yours)?;

        let mut merged = Vec::new();
        let mut conflicts = Vec::new();

        for path in paths_of(&[&ancestor, &ours, &theirs]) {
            let base = find(&ancestor, &path);
            let mine = find(&ours, &path);
            let yours = find(&theirs, &path);

            match resolve(base, mine, yours) {
                Resolution::Take(secret) => merged.extend(secret.cloned()),
                Resolution::Conflict => match text_merge(base, mine, yours, marker_length) {
                    Some(Ok(text)) => merged.push(Secret {
                        content: text,
                        ..mine.or(yours).cloned().unwrap_or_else(|| blank(&path))
                    }),
                    Some(Err(text)) => {
                        conflicts.push(path.clone());
                        merged.push(Secret {
                            content: text,
                            ..mine.or(yours).cloned().unwrap_or_else(|| blank(&path))
                        });
                    }
                    None => {
                        conflicts.push(path.clone());
                        merged.extend(mine.or(yours).cloned());
                    }
                },
            }
        }

        Ok(Outcome {
            sealed: seal_all(key, &merged)?,
            conflicts,
        })
    }

    fn merge_entries(&self) -> Result<Outcome> {
        let mut merged = Vec::new();
        let mut conflicts = Vec::new();

        for id in ids_of(&[&self.ancestor, &self.ours, &self.theirs]) {
            let base = entry(&self.ancestor, id);
            let mine = entry(&self.ours, id);
            let yours = entry(&self.theirs, id);

            match resolve(base, mine, yours) {
                Resolution::Take(chosen) => merged.extend(chosen.cloned()),
                Resolution::Conflict => {
                    conflicts.push(format!("a sealed entry ({})", short(&id)));
                    merged.extend(mine.or(yours).cloned());
                }
            }
        }

        Ok(Outcome {
            sealed: Vault {
                key_id: self.ours.key_id,
                entries: merged,
            }
            .encode()?,
            conflicts,
        })
    }
}

fn cached_key() -> Option<VaultKey> {
    Repo::discover().ok()?.key().ok()
}

enum Resolution<'a, T> {
    Take(Option<&'a T>),
    Conflict,
}

fn resolve<'a, T: PartialEq>(
    base: Option<&'a T>,
    ours: Option<&'a T>,
    theirs: Option<&'a T>,
) -> Resolution<'a, T> {
    if ours == theirs {
        return Resolution::Take(ours);
    }
    if base == ours {
        return Resolution::Take(theirs);
    }
    if base == theirs {
        return Resolution::Take(ours);
    }

    Resolution::Conflict
}

fn text_merge(
    base: Option<&Secret>,
    ours: Option<&Secret>,
    theirs: Option<&Secret>,
    marker_length: usize,
) -> Option<Result<Vec<u8>, Vec<u8>>> {
    let (ours, theirs) = (ours?, theirs?);
    let base = base.map_or(&[][..], |secret| secret.content.as_slice());

    let mut options = diffy::MergeOptions::new();
    options.set_conflict_marker_length(marker_length);

    Some(options.merge_bytes(base, &ours.content, &theirs.content))
}

fn read(path: &Path) -> Result<Vec<u8>> {
    fs::read(path).with_context(|| format!("cannot read `{}`", path.display()))
}

fn decode(bytes: &[u8], side: &str) -> Result<Vault> {
    Vault::decode(bytes).with_context(|| format!("{side} is not a readable vault"))
}

fn find<'a>(secrets: &'a [Secret], path: &str) -> Option<&'a Secret> {
    secrets.iter().find(|secret| secret.path == path)
}

fn paths_of(sides: &[&[Secret]]) -> BTreeSet<String> {
    sides
        .iter()
        .flat_map(|secrets| secrets.iter().map(|secret| secret.path.clone()))
        .collect()
}

fn entry(vault: &Vault, id: [u8; 16]) -> Option<&Entry> {
    vault.entries.iter().find(|entry| entry.id == id)
}

fn ids_of(sides: &[&Vault]) -> BTreeSet<[u8; 16]> {
    sides
        .iter()
        .flat_map(|vault| vault.entries.iter().map(|entry| entry.id))
        .collect()
}

fn short(id: &[u8; 16]) -> String {
    use std::fmt::Write as _;

    let mut rendered = String::new();
    for byte in id.iter().take(4) {
        let _ignored = write!(rendered, "{byte:02x}");
    }
    rendered
}

fn blank(path: &str) -> Secret {
    Secret {
        path: path.to_owned(),
        kind: crate::vault::seal::Kind::File { executable: false },
        content: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::{Resolution, resolve, text_merge};
    use crate::vault::seal::{Kind, Secret};

    fn secret(content: &str) -> Secret {
        Secret {
            path: "secrets/prod.env".to_owned(),
            kind: Kind::File { executable: false },
            content: content.as_bytes().to_vec(),
        }
    }

    fn taken<T>(resolution: &Resolution<'_, T>) -> Option<T>
    where
        T: Clone,
    {
        match resolution {
            Resolution::Take(value) => value.cloned(),
            Resolution::Conflict => panic!("expected a clean resolution"),
        }
    }

    #[test]
    fn a_change_on_one_side_only_is_taken() {
        let base = secret("A=1\n");
        let changed = secret("A=2\n");

        assert_eq!(
            taken(&resolve(Some(&base), Some(&base), Some(&changed))),
            Some(changed.clone())
        );
        assert_eq!(
            taken(&resolve(Some(&base), Some(&changed), Some(&base))),
            Some(changed)
        );
    }

    #[test]
    fn the_same_change_on_both_sides_is_not_a_conflict() {
        let base = secret("A=1\n");
        let changed = secret("A=2\n");

        assert_eq!(
            taken(&resolve(Some(&base), Some(&changed), Some(&changed))),
            Some(changed)
        );
    }

    #[test]
    fn a_deletion_on_one_side_only_is_taken() {
        let base = secret("A=1\n");

        assert_eq!(taken(&resolve(Some(&base), Some(&base), None)), None);
        assert_eq!(taken(&resolve(Some(&base), None, Some(&base))), None);
    }

    #[test]
    fn an_addition_on_one_side_only_is_taken() {
        let added = secret("A=1\n");

        assert_eq!(
            taken(&resolve(None, None, Some(&added))),
            Some(added.clone())
        );
        assert_eq!(taken(&resolve(None, Some(&added), None)), Some(added));
    }

    #[test]
    fn both_sides_changing_the_same_thing_needs_more_work() {
        let base = secret("A=1\n");
        let mine = secret("A=2\n");
        let yours = secret("A=3\n");

        assert!(matches!(
            resolve(Some(&base), Some(&mine), Some(&yours)),
            Resolution::Conflict
        ));
    }

    #[test]
    fn edits_to_different_lines_of_one_secret_merge_cleanly() {
        let base = secret("A=1\nB=1\nC=1\n");
        let mine = secret("A=2\nB=1\nC=1\n");
        let yours = secret("A=1\nB=1\nC=2\n");

        let merged = text_merge(Some(&base), Some(&mine), Some(&yours), 7).unwrap();

        assert_eq!(merged.unwrap(), b"A=2\nB=1\nC=2\n");
    }

    #[test]
    fn edits_to_the_same_line_come_back_with_markers() {
        let base = secret("A=1\n");
        let mine = secret("A=2\n");
        let yours = secret("A=3\n");

        let merged = text_merge(Some(&base), Some(&mine), Some(&yours), 7)
            .unwrap()
            .unwrap_err();

        let text = String::from_utf8(merged).unwrap();
        assert!(text.contains("<<<<<<<"), "{text}");
        assert!(text.contains("A=2"), "{text}");
        assert!(text.contains("A=3"), "{text}");
    }

    #[test]
    fn the_marker_length_git_asks_for_is_honoured() {
        let merged = text_merge(
            Some(&secret("A=1\n")),
            Some(&secret("A=2\n")),
            Some(&secret("A=3\n")),
            12,
        )
        .unwrap()
        .unwrap_err();

        assert!(String::from_utf8(merged).unwrap().contains("<<<<<<<<<<<<"));
    }

    #[test]
    fn a_secret_that_only_one_side_has_cannot_be_text_merged() {
        assert!(text_merge(None, Some(&secret("A=1\n")), None, 7).is_none());
    }
}
