mod harness;

use harness::Repo;

fn with_one_secret() -> Repo {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "STRIPE_KEY=sk_live_example\n");
    repo.write("secrets/deep/nested.key", "inner\n");
    repo.commit_all("add secrets");
    repo
}

#[test]
fn ls_lists_what_is_sealed() {
    let repo = with_one_secret();

    insta::assert_snapshot!(repo.vault(&["ls"]).ok());
}

#[test]
fn ls_says_so_when_nothing_is_sealed_yet() {
    let repo = Repo::sealed(&[]);

    insta::assert_snapshot!(repo.vault(&["ls"]).ok());
}

#[test]
fn ls_when_locked() {
    let repo = with_one_secret();
    repo.vault(&["lock"]).ok();

    let run = repo.vault(&["ls"]);

    assert_eq!(run.code(), 3, "locked is its own exit code");
    insta::assert_snapshot!(run.stdout());
}

#[test]
fn status_with_nothing_changed() {
    let repo = with_one_secret();

    insta::assert_snapshot!(repo.vault(&["status"]).ok());
}

#[test]
fn status_shows_what_changed() {
    let repo = with_one_secret();
    repo.write("secrets/prod.env", "STRIPE_KEY=sk_live_rotated\n");
    repo.write("secrets/added.env", "brand new\n");
    repo.remove("secrets/deep/nested.key");

    insta::assert_snapshot!(repo.vault(&["status"]).ok());
}

#[test]
fn doctor_on_a_healthy_repository() {
    let repo = with_one_secret();

    let run = repo.vault(&["doctor"]);

    assert_eq!(run.code(), 0);
    insta::assert_snapshot!(run.stdout());
}

#[test]
fn doctor_finds_every_kind_of_problem() {
    let repo = with_one_secret();

    repo.write("secrets/leaky.env", "should have been sealed\n");
    repo.git(&["config", "--unset", "filter.vault-plaintext.clean"])
        .ok();
    repo.git(&["config", "--unset", "filter.vault-plaintext.required"])
        .ok();
    repo.git(&["add", "--force", "--", "secrets/leaky.env"])
        .ok();
    repo.git(&[
        "config",
        "filter.vault-plaintext.clean",
        "git-vault filter refuse %f",
    ])
    .ok();
    repo.git(&["config", "filter.vault-plaintext.required", "true"])
        .ok();
    repo.git(&["config", "--unset", "filter.vault.required"])
        .ok();
    repo.git(&["config", "diff.vault.cachetextconv", "true"])
        .ok();
    repo.git(&["config", "--unset", "core.fsmonitor"]).ok();
    repo.write(".gitignore", "");

    let run = repo.vault(&["doctor"]);

    assert_eq!(run.code(), 4, "problems exit non-zero, for CI");
    insta::assert_snapshot!(run.stdout());
}

#[test]
fn doctor_warns_about_an_unanchored_pattern() {
    let repo = Repo::sealed(&["*.key"]);
    repo.write("top.key", "one\n");
    repo.commit_all("a secret at the top");

    let run = repo.vault(&["doctor"]);

    assert_eq!(run.code(), 0, "a warning is not a failure");
    insta::assert_snapshot!(run.stdout());
}

#[test]
fn add_explains_what_it_did() {
    let repo = Repo::sealed(&[]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.git(&["add", "--", "secrets/prod.env"]).ok();
    repo.commit("commit a secret in the clear, before sealing it");

    insta::assert_snapshot!(repo.vault(&["add", "secrets/"]).ok());
}

#[test]
fn add_refuses_to_seal_the_vault_shut() {
    let repo = Repo::sealed(&[]);

    insta::assert_snapshot!(repo.vault(&["add", ".gitattributes"]).failed());
}

#[test]
fn remove_says_what_stopped_being_secret() {
    let repo = with_one_secret();

    insta::assert_snapshot!(repo.vault(&["remove", "secrets/"]).ok());
}

#[test]
fn restore_reports_what_it_put_back() {
    let repo = with_one_secret();
    repo.write("secrets/prod.env", "edited\n");
    repo.write("secrets/stray.env", "never sealed\n");

    insta::assert_snapshot!(repo.vault(&["restore"]).ok());
}

#[test]
fn unlocking_a_clone_reports_what_it_found() {
    let source = with_one_secret();
    let clone = Repo::clone_of(&source);

    insta::assert_snapshot!(clone.vault(&["unlock"]).ok());
}

#[test]
fn locking_reports_what_it_removed() {
    let repo = with_one_secret();

    insta::assert_snapshot!(repo.vault(&["lock"]).ok());
}

#[test]
fn keys_marks_which_recipient_is_you() {
    let source = with_one_secret();
    let stranger = Repo::stranger_clone_of(&source);
    source
        .vault(&["share", &stranger.public_key(), "--label", "alice@work"])
        .ok();

    let listing = source.vault(&["keys"]).ok();
    let mut people: Vec<String> = listing
        .lines()
        .filter(|line| line.starts_with("age1"))
        .map(|line| {
            line.split_whitespace()
                .map(|word| {
                    if word.starts_with("age1") {
                        "age1…"
                    } else {
                        word
                    }
                })
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    people.sort();
    let summary = listing.lines().next_back().unwrap();

    insta::assert_snapshot!(format!("{}\n\n{summary}", people.join("\n")));
}

#[test]
fn revoking_says_what_cannot_be_undone() {
    let source = with_one_secret();
    let stranger = Repo::stranger_clone_of(&source);
    source
        .vault(&["share", &stranger.public_key(), "--label", "alice@work"])
        .ok();

    let output = source.vault(&["revoke", "alice@work"]).ok();

    let shortened: String = stranger.public_key().chars().take(16).collect();
    insta::assert_snapshot!(output.replace(&shortened, "age1"));
}

#[test]
fn diff_renders_one_hunk_per_secret() {
    let repo = with_one_secret();
    repo.write("secrets/prod.env", "STRIPE_KEY=sk_live_rotated\n");
    repo.write("secrets/added.env", "brand new\n");
    repo.remove("secrets/deep/nested.key");

    insta::assert_snapshot!(repo.vault(&["diff"]).ok());
}

#[test]
fn log_shows_the_history_of_a_secret() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "STRIPE=one\n");
    repo.commit_all("add the stripe key");
    repo.write("secrets/prod.env", "STRIPE=two\n");
    repo.commit_all("rotate the stripe key");

    let log = repo.vault(&["log", "secrets/prod.env"]).ok();
    let shape: Vec<String> = log
        .lines()
        .map(|line| match line.split_once(' ') {
            Some((first, rest)) if first.len() == 8 && !line.starts_with(' ') => {
                format!("<commit> {rest}")
            }
            _other => line.to_owned(),
        })
        .collect();

    insta::assert_snapshot!(shape.join("\n"));
}
