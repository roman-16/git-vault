use std::collections::BTreeSet;
use std::fs;
use std::io::ErrorKind;
use std::path::Path;

use anyhow::{Context as _, Result};

use crate::paths;
use bstr::{BStr, ByteSlice as _};
use gix_attributes::StateRef;
use gix_glob::Pattern;
use gix_glob::pattern::{Case, Mode as PatternMode};
use gix_glob::wildmatch::Mode;

const MATCH: Mode = Mode::NO_MATCH_SLASH_LITERAL;

struct Rule {
    pattern: Pattern,
    seals: bool,
}

pub struct Patterns {
    rules: Vec<Rule>,
}

impl Patterns {
    pub fn load(worktree: &Path) -> Result<Self> {
        let path = worktree.join(".gitattributes");

        match fs::read(&path) {
            Ok(bytes) => Ok(Self::parse(&bytes)),
            Err(error) if error.kind() == ErrorKind::NotFound => Ok(Self { rules: Vec::new() }),
            Err(error) => Err(error).with_context(|| format!("cannot read `{}`", path.display())),
        }
    }

    fn parse(bytes: &[u8]) -> Self {
        let rules = gix_attributes::parse(bytes)
            .filter_map(Result::ok)
            .filter_map(|(kind, assignments, _line)| match kind {
                gix_attributes::parse::Kind::Pattern(pattern) => Some((pattern, assignments)),
                gix_attributes::parse::Kind::Macro(_) => None,
            })
            .filter_map(|(pattern, assignments)| {
                assignments
                    .filter_map(Result::ok)
                    .filter(|assignment| assignment.name.as_str() == "vault")
                    .filter_map(|assignment| match assignment.state {
                        StateRef::Set => Some(true),
                        StateRef::Unset => Some(false),
                        StateRef::Value(_) | StateRef::Unspecified => None,
                    })
                    .last()
                    .map(|seals| Rule { pattern, seals })
            })
            .collect();

        Self { rules }
    }

    pub fn is_secret(&self, rel: &str) -> bool {
        if paths::is_never_sealed(rel) {
            return false;
        }

        self.rules
            .iter()
            .rfind(|rule| matches_path_or_parent(&rule.pattern, rel))
            .is_some_and(|rule| rule.seals)
    }

    pub fn declared(&self) -> Vec<String> {
        self.rules
            .iter()
            .filter(|rule| rule.seals)
            .map(|rule| {
                let text = rule.pattern.text.to_string();
                if rule.pattern.mode.contains(PatternMode::MUST_BE_DIR) {
                    format!("{text}/")
                } else {
                    text
                }
            })
            .collect()
    }

    pub fn roots(&self) -> Vec<String> {
        let mut roots = BTreeSet::new();

        for rule in self.rules.iter().filter(|rule| rule.seals) {
            let text = rule.pattern.text.to_str_lossy();
            let literal = rule
                .pattern
                .first_wildcard_pos
                .and_then(|position| text.get(..position));

            let root = literal.map_or_else(
                || text.as_ref(),
                |prefix| prefix.rsplit_once('/').map_or("", |(head, _)| head),
            );

            roots.insert(root.trim_matches('/').to_owned());
        }

        if roots.contains("") {
            return vec![String::new()];
        }

        roots
            .iter()
            .filter(|root| !roots.iter().any(|other| is_inside(root, other)))
            .cloned()
            .collect()
    }
}

fn is_inside(candidate: &str, other: &str) -> bool {
    candidate != other
        && candidate
            .strip_prefix(other)
            .is_some_and(|rest| rest.starts_with('/'))
}

fn matches_path_or_parent(pattern: &Pattern, rel: &str) -> bool {
    if matches(pattern, rel, false) {
        return true;
    }

    rel.char_indices()
        .filter(|(_, character)| *character == '/')
        .filter_map(|(position, _)| rel.get(..position))
        .any(|parent| matches(pattern, parent, true))
}

fn matches(pattern: &Pattern, path: &str, is_dir: bool) -> bool {
    let basename_start = path.rfind('/').and_then(|position| position.checked_add(1));

    pattern.matches_repo_relative_path(
        BStr::new(path),
        basename_start,
        Some(is_dir),
        Case::Sensitive,
        MATCH,
    )
}

#[cfg(test)]
mod tests {
    use super::Patterns;

    fn patterns(text: &str) -> Patterns {
        Patterns::parse(text.as_bytes())
    }

    #[test]
    fn a_bare_directory_seals_everything_inside_it() {
        let sealed = patterns("secrets/ vault\n");

        assert!(sealed.is_secret("secrets/prod.env"));
        assert!(sealed.is_secret("secrets/deep/nested.key"));
        assert!(!sealed.is_secret("src/main.rs"));
    }

    #[test]
    fn a_directory_without_a_slash_also_seals_its_contents() {
        let sealed = patterns("secrets vault\n");

        assert!(sealed.is_secret("secrets/prod.env"));
    }

    #[test]
    fn a_double_star_pattern_works_the_way_git_crypt_users_expect() {
        let sealed = patterns("secrets/** vault\n");

        assert!(sealed.is_secret("secrets/prod.env"));
        assert!(sealed.is_secret("secrets/deep/nested.key"));
    }

    #[test]
    fn an_extension_pattern_matches_anywhere() {
        let sealed = patterns("*.key vault\n");

        assert!(sealed.is_secret("top.key"));
        assert!(sealed.is_secret("config/deep/prod.key"));
        assert!(!sealed.is_secret("config/deep/prod.pub"));
    }

    #[test]
    fn a_single_star_does_not_cross_a_slash() {
        let sealed = patterns("secrets/*.env vault\n");

        assert!(sealed.is_secret("secrets/prod.env"));
        assert!(!sealed.is_secret("secrets/deep/prod.env"));
    }

    #[test]
    fn the_last_matching_rule_wins() {
        let sealed = patterns("secrets/ vault\nsecrets/README.md -vault\n");

        assert!(sealed.is_secret("secrets/prod.env"));
        assert!(!sealed.is_secret("secrets/README.md"));
    }

    #[test]
    fn the_files_that_make_the_repository_work_are_never_sealed() {
        let sealed = patterns("* vault\n");

        assert!(!sealed.is_secret(".gitattributes"));
        assert!(!sealed.is_secret(".gitignore"));
        assert!(!sealed.is_secret(".vault/data"));
        assert!(!sealed.is_secret(".vault/keys"));
        assert!(!sealed.is_secret(".git/config"));
        assert!(sealed.is_secret("anything-else"));
    }

    #[test]
    fn the_vault_declaration_line_itself_seals_nothing() {
        let sealed = patterns(".vault filter=vault diff=vault merge=vault -text\n");

        assert!(sealed.rules.is_empty());
        assert!(!sealed.is_secret(".vault/data"));
    }

    #[test]
    fn declared_patterns_keep_the_slash_that_makes_them_directories() {
        assert_eq!(patterns("secrets/ vault\n").declared(), ["secrets/"]);
        assert_eq!(patterns("*.key vault\n").declared(), ["*.key"]);
        assert_eq!(
            patterns("secrets/ vault\nvendor/ -vault\n").declared(),
            ["secrets/"]
        );
    }

    #[test]
    fn walk_roots_follow_the_literal_prefix() {
        assert_eq!(patterns("secrets/ vault\n").roots(), ["secrets"]);
        assert_eq!(patterns("secrets/** vault\n").roots(), ["secrets"]);
        assert_eq!(
            patterns("config/prod.key vault\n").roots(),
            ["config/prod.key"]
        );
        assert_eq!(patterns("deploy/*/prod.env vault\n").roots(), ["deploy"]);
    }

    #[test]
    fn an_unanchored_pattern_has_to_walk_everything() {
        assert_eq!(patterns("*.key vault\n").roots(), [String::new()]);
    }

    #[test]
    fn nested_roots_collapse_into_their_parent() {
        let roots = patterns("secrets/ vault\nsecrets/deep/ vault\nother/ vault\n").roots();

        assert_eq!(roots, ["other", "secrets"]);
    }

    #[test]
    fn unsealing_rules_do_not_widen_the_walk() {
        let roots = patterns("secrets/ vault\nvendor/** -vault\n").roots();

        assert_eq!(roots, ["secrets"]);
    }
}
