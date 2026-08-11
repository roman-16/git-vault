use std::fs;
use std::path::Path;

use anyhow::{Context as _, Result, bail};

use crate::paths;
use crate::vault::recipient::Recipient;

const HEADER: &str = "\
# Everyone who can open this vault, one public key per line, with an optional
# label. Managed by `git vault share` and `git vault revoke`.
";

pub fn read(worktree: &Path) -> Result<Vec<Recipient>> {
    let path = worktree.join(paths::RECIPIENTS);
    let contents = fs::read_to_string(&path).with_context(|| {
        format!(
            "cannot read `{}`. An age file does not record who it was wrapped for, so this list is the only way to re-wrap the vault key for everyone; `git checkout -- {}` puts it back",
            paths::RECIPIENTS,
            paths::RECIPIENTS
        )
    })?;

    contents
        .lines()
        .map(Recipient::from_line)
        .collect::<Result<Vec<_>>>()
        .with_context(|| format!("`{}` has a line git-vault cannot read", paths::RECIPIENTS))
        .map(|found| found.into_iter().flatten().collect())
}

pub fn write(worktree: &Path, recipients: &[Recipient]) -> Result<()> {
    if recipients.is_empty() {
        bail!("a vault needs at least one recipient");
    }

    let mut sorted = recipients.to_vec();
    sorted.sort_by(|left, right| left.key().cmp(right.key()));
    sorted.dedup_by(|left, right| left.key() == right.key());

    let mut contents = String::from(HEADER);
    for recipient in &sorted {
        contents.push_str(&recipient.to_string());
        contents.push('\n');
    }

    let path = worktree.join(paths::RECIPIENTS);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("cannot create `{}`", parent.display()))?;
    }

    fs::write(&path, contents).with_context(|| format!("cannot write `{}`", paths::RECIPIENTS))
}

#[cfg(test)]
mod tests {
    use tempfile::TempDir;

    use super::{read, write};
    use crate::vault::recipient::Recipient;

    const ONE: &str = "age1ql3z7hjy54pw3hyww5ayyfg7zqgvc7w3j2elw8zmrj2kg5sfn9aqmcac8p";
    const TWO: &str = "age1lggyhqrw2nlhcxprm67z43rta597azn8gknawjehu9d9dl0jq3yqqvfafg";

    #[test]
    fn a_list_round_trips() {
        let dir = TempDir::new().unwrap();
        let recipients = [
            Recipient::new(ONE, Some("roman@laptop")).unwrap(),
            Recipient::new(TWO, None).unwrap(),
        ];

        write(dir.path(), &recipients).unwrap();
        let read_back = read(dir.path()).unwrap();

        assert_eq!(read_back.len(), 2);
        let labelled = read_back
            .iter()
            .find(|recipient| recipient.key() == ONE)
            .unwrap();
        assert_eq!(labelled.label(), Some("roman@laptop"));
    }

    #[test]
    fn the_file_is_sorted_and_deduplicated() {
        let dir = TempDir::new().unwrap();
        let recipients = [
            Recipient::new(TWO, None).unwrap(),
            Recipient::new(ONE, None).unwrap(),
            Recipient::new(TWO, None).unwrap(),
        ];

        write(dir.path(), &recipients).unwrap();

        let keys: Vec<String> = read(dir.path())
            .unwrap()
            .iter()
            .map(|recipient| recipient.key().to_owned())
            .collect();
        assert_eq!(keys.len(), 2, "the duplicate is gone");
        assert!(
            keys.is_sorted(),
            "the file reads the same however it was edited"
        );
    }

    #[test]
    fn an_empty_list_is_refused() {
        let dir = TempDir::new().unwrap();

        assert!(write(dir.path(), &[]).is_err());
    }
}
