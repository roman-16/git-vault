mod harness;

use harness::Repo;

const DATA: &str = ".vault/data";
const KEYS: &str = ".vault/keys";

fn with_secrets() -> Repo {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.write("secrets/deep/nested.key", "inner\n");
    repo.commit_all("add secrets");
    repo
}

#[test]
fn wiping_the_worktree_is_refused_rather_than_published() {
    let repo = with_secrets();

    repo.git(&["clean", "-xdf"]).ok();
    assert!(
        !repo.exists("secrets/prod.env"),
        "clean removed the plaintext"
    );

    let attempt = repo.git(&["commit", "--all", "--message", "oops"]);

    assert!(!attempt.succeeded(), "the wipe must not become a commit");
    assert!(
        attempt.stderr().contains("git vault restore"),
        "{}",
        attempt.stderr()
    );

    repo.vault(&["restore"]).ok();
    assert_eq!(repo.read("secrets/prod.env"), b"A=1\n");
    assert_eq!(repo.status(), "");
}

#[test]
fn emptying_the_vault_on_purpose_only_takes_saying_so() {
    let repo = with_secrets();
    repo.remove("secrets");

    repo.vault(&["seal", "--allow-empty"]).ok();
    repo.commit_all("all secrets are gone, deliberately");

    assert!(repo.vault(&["ls"]).ok().contains("Nothing is sealed"));
    assert_eq!(repo.status(), "");
}

#[test]
fn renaming_a_secret_carries_the_contents_across() {
    let repo = with_secrets();

    std::fs::rename(
        repo.root().join("secrets/prod.env"),
        repo.root().join("secrets/production.env"),
    )
    .unwrap();

    let status = repo.vault(&["status"]).ok();
    assert!(status.contains("A secrets/production.env"), "{status}");
    assert!(status.contains("D secrets/prod.env"), "{status}");

    repo.commit_all("rename the secret");
    repo.remove("secrets");
    repo.vault(&["restore"]).ok();

    assert_eq!(repo.read("secrets/production.env"), b"A=1\n");
    assert!(!repo.exists("secrets/prod.env"));
}

#[test]
fn a_directory_that_held_only_secrets_does_not_linger() {
    let repo = with_secrets();
    repo.git(&["checkout", "--quiet", "-b", "without"]).ok();
    repo.remove("secrets/deep/nested.key");
    repo.commit_all("drop the nested secret");
    repo.git(&["checkout", "--quiet", "main"]).ok();
    assert_eq!(repo.read("secrets/deep/nested.key"), b"inner\n");

    repo.git(&["checkout", "--quiet", "without"]).ok();

    assert!(
        !repo.root().join("secrets/deep").exists(),
        "an empty directory was left behind"
    );
    assert_eq!(repo.status(), "");
}

#[test]
fn somebody_elses_fsmonitor_is_left_alone() {
    let repo = Repo::bare_new();
    repo.git(&["config", "core.fsmonitor", "/usr/bin/watchman-hook"])
        .ok();

    let output = repo.vault(&["init"]).ok();

    assert!(
        output.contains("already `/usr/bin/watchman-hook`"),
        "{output}"
    );
    assert_eq!(
        repo.git(&["config", "--get", "core.fsmonitor"]).ok(),
        "/usr/bin/watchman-hook"
    );

    repo.declare(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.write("notes.md", "# notes\n");
    repo.git(&["add", "--all"]).ok();
    let attempt = repo.git(&["commit", "--message", "with somebody else's watcher"]);

    assert!(attempt.succeeded());
    assert!(
        attempt.stderr().contains("does not include"),
        "nothing sealed before git looked, so the commit has to say so: {}",
        attempt.stderr()
    );

    repo.commit_all("seal what the watcher missed");

    repo.remove("secrets");
    repo.remove(DATA);
    repo.git(&["checkout", "--", DATA]).ok();
    assert_eq!(repo.read("secrets/prod.env"), b"A=1\n");
}

#[test]
fn a_secret_that_is_neither_a_file_nor_a_link_is_named() {
    let repo = with_secrets();
    let fifo = repo.root().join("secrets/pipe");

    let made = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .is_ok_and(|status| status.success());
    if !made {
        return;
    }

    let stderr = repo.vault(&["seal"]).failed();

    assert!(
        stderr.contains("neither a regular file nor a symlink"),
        "{stderr}"
    );
}

#[test]
fn line_ending_translation_cannot_corrupt_the_vault() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.git(&["config", "core.autocrlf", "true"]).ok();
    repo.write("secrets/prod.env", "A=1\nB=2\n");
    repo.commit_all("a secret with several lines");

    repo.remove("secrets");
    repo.remove(DATA);
    repo.remove(KEYS);
    repo.git(&["checkout", "--", "."]).ok();

    assert_eq!(
        repo.read("secrets/prod.env"),
        b"A=1\nB=2\n",
        "git rewrote the line endings of a secret"
    );
    assert!(
        !repo.read(KEYS).contains(&b'\r'),
        "`{KEYS}` came back with CRLF, which age cannot read"
    );
    assert!(
        repo.vault(&["ls"]).ok().contains("secrets/prod.env"),
        "the key file no longer parses"
    );
}
