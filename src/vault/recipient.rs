use std::fmt;
use std::str::FromStr;

use anyhow::{Context as _, Result, bail};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Recipient {
    key: String,
    label: Option<String>,
}

fn normalise(key: &str) -> String {
    let key = key.trim();

    if !key.starts_with("ssh-") {
        return key.to_owned();
    }

    let mut fields = key.split_whitespace();
    match (fields.next(), fields.next()) {
        (Some(kind), Some(body)) => format!("{kind} {body}"),
        _incomplete => key.to_owned(),
    }
}

fn tail(text: &str, count: usize) -> String {
    text.chars()
        .rev()
        .take(count)
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

impl Recipient {
    pub fn new(key: &str, label: Option<&str>) -> Result<Self> {
        let recipient = Self {
            key: normalise(key),
            label: label
                .map(str::trim)
                .filter(|label| !label.is_empty())
                .map(str::to_owned),
        };

        recipient.to_age()?;

        Ok(recipient)
    }

    pub fn from_line(line: &str) -> Result<Option<Self>> {
        let line = line.trim();

        if line.is_empty() || line.starts_with('#') {
            return Ok(None);
        }

        let mut fields = line.split_whitespace();
        let first = fields.next().unwrap_or_default();

        let (key, label) = if first.starts_with("ssh-") {
            let body = fields.next().unwrap_or_default();
            (format!("{first} {body}"), fields.next())
        } else {
            (first.to_owned(), fields.next())
        };

        Self::new(&key, label).map(Some)
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    pub fn short(&self) -> String {
        match self.key.split_once(' ') {
            Some((kind, body)) => format!("{kind} …{}", tail(body, 10)),
            None => format!("{}…", self.key.chars().take(16).collect::<String>()),
        }
    }

    pub fn to_age(&self) -> Result<Box<dyn age::Recipient + Send>> {
        if let Ok(native) = age::x25519::Recipient::from_str(&self.key) {
            return Ok(Box::new(native));
        }

        if self.key.starts_with("ssh-") {
            return age::ssh::Recipient::from_str(&self.key)
                .map(|recipient| -> Box<dyn age::Recipient + Send> { Box::new(recipient) })
                .ok()
                .with_context(|| format!("`{}` is not a usable SSH public key", self.key));
        }

        bail!(
            "`{}` is neither an age recipient (`age1…`) nor an SSH public key (`ssh-ed25519 …`)",
            self.key
        )
    }
}

impl fmt::Display for Recipient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.label {
            Some(label) => write!(formatter, "{} {label}", self.key),
            None => write!(formatter, "{}", self.key),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Recipient;

    const AGE: &str = "age1ql3z7hjy54pw3hyww5ayyfg7zqgvc7w3j2elw8zmrj2kg5sfn9aqmcac8p";
    const SSH: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIGkAKvGGMdI6zXd7lYlB4Z8mCLLoxOQNyxLQ0eLcMwWL";

    #[test]
    fn an_age_recipient_is_accepted() {
        let recipient = Recipient::new(AGE, None).unwrap();

        assert_eq!(recipient.key(), AGE);
        assert!(recipient.to_age().is_ok());
    }

    #[test]
    fn an_ssh_recipient_is_accepted() {
        let recipient = Recipient::new(SSH, Some("alice@laptop")).unwrap();

        assert_eq!(recipient.label(), Some("alice@laptop"));
        assert!(recipient.to_age().is_ok());
    }

    #[test]
    fn an_ssh_key_keeps_the_label_you_gave_rather_than_its_own_comment() {
        let with_comment = format!("{SSH} roman@roman-nixos");

        let recipient = Recipient::new(&with_comment, Some("homelab")).unwrap();

        assert_eq!(recipient.key(), SSH, "the comment is not part of the key");
        assert_eq!(recipient.label(), Some("homelab"));

        let reread = Recipient::from_line(&recipient.to_string())
            .unwrap()
            .unwrap();
        assert_eq!(
            reread.label(),
            Some("homelab"),
            "a recipient must survive a round trip through .vault/recipients"
        );
        assert_eq!(reread.key(), SSH);
    }

    #[test]
    fn an_ssh_recipient_renders_as_something_you_can_tell_apart() {
        let one = Recipient::new(SSH, None).unwrap().short();
        let two = Recipient::new(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIH2K3nBFtPqLQvJ8Rb0lLPCwPYVAJZLhdVLbLBEHVQdE",
            None,
        )
        .unwrap()
        .short();

        assert!(one.starts_with("ssh-ed25519 "), "{one}");
        assert_ne!(one, two, "every ed25519 key starts with the same bytes");
    }

    #[test]
    fn nonsense_is_refused_immediately() {
        let error = Recipient::new("not-a-key", None).unwrap_err();

        assert!(
            error.to_string().contains("neither an age recipient"),
            "{error}"
        );
    }

    #[test]
    fn a_line_carries_an_optional_label() {
        let native = Recipient::from_line(&format!("{AGE}  roman@laptop"))
            .unwrap()
            .unwrap();
        assert_eq!(native.key(), AGE);
        assert_eq!(native.label(), Some("roman@laptop"));

        let ssh = Recipient::from_line(&format!("{SSH} alice@work"))
            .unwrap()
            .unwrap();
        assert_eq!(ssh.key(), SSH);
        assert_eq!(ssh.label(), Some("alice@work"));
    }

    #[test]
    fn blanks_and_comments_are_skipped() {
        assert_eq!(Recipient::from_line("").unwrap(), None);
        assert_eq!(Recipient::from_line("   ").unwrap(), None);
        assert_eq!(Recipient::from_line("# a note").unwrap(), None);
    }

    #[test]
    fn a_line_round_trips_through_display() {
        let line = format!("{AGE} roman@laptop");
        let recipient = Recipient::from_line(&line).unwrap().unwrap();

        assert_eq!(recipient.to_string(), line);
    }
}
