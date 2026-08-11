use std::fmt;
use std::str::FromStr;

use anyhow::{Context as _, Result, bail};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Recipient {
    key: String,
    label: Option<String>,
}

impl Recipient {
    pub fn new(key: &str, label: Option<&str>) -> Result<Self> {
        let recipient = Self {
            key: key.trim().to_owned(),
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
        let body = self.key.rsplit(' ').next().unwrap_or(&self.key);
        let head: String = body.chars().take(16).collect();
        format!("{head}…")
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
