mod harness;

use harness::Repo;

const DATA: &str = ".vault/data";

fn shared_repo() -> Repo {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.commit_all("add a secret");
    repo
}

#[test]
fn a_stranger_is_told_exactly_what_to_ask_for() {
    let source = shared_repo();
    let clone = Repo::clone_of(&source);
    std::fs::remove_file(clone.root().join("../identity")).unwrap();

    let stderr = clone.vault(&["unlock"]).failed();

    assert!(stderr.contains("Your public key is"), "{stderr}");
    assert!(stderr.contains("git vault share age1"), "{stderr}");
    assert!(!clone.exists("secrets/prod.env"));
}

#[test]
fn sharing_lets_somebody_else_unlock() {
    let source = shared_repo();
    let stranger = Repo::stranger_clone_of(&source);

    source
        .vault(&["share", &stranger.public_key(), "--label", "alice@work"])
        .ok();
    source.git(&["add", "--", ".vault"]).ok();
    source.commit("share with alice");

    stranger.git(&["pull", "--quiet", "origin", "main"]).ok();
    stranger.vault(&["unlock"]).ok();

    assert_eq!(stranger.read("secrets/prod.env"), b"A=1\n");
    assert_eq!(stranger.status(), "");
}

#[test]
fn sharing_with_somebody_who_already_has_access_changes_nothing() {
    let repo = shared_repo();
    let before = std::fs::read(repo.root().join(".vault/keys")).unwrap();

    let output = repo.vault(&["share", &repo.public_key()]).ok();

    assert!(output.contains("already has access"), "{output}");
    assert_eq!(
        std::fs::read(repo.root().join(".vault/keys")).unwrap(),
        before
    );
}

#[test]
fn a_key_that_is_not_a_key_is_refused_before_anything_changes() {
    let repo = shared_repo();
    let before = std::fs::read(repo.root().join(".vault/recipients")).unwrap();

    let stderr = repo.vault(&["share", "hunter2"]).failed();

    assert!(stderr.contains("neither an age recipient"), "{stderr}");
    assert_eq!(
        std::fs::read(repo.root().join(".vault/recipients")).unwrap(),
        before
    );
}

#[test]
fn revoking_replaces_the_key_and_seals_everything_anew() {
    let source = shared_repo();
    let stranger = Repo::stranger_clone_of(&source);
    source
        .vault(&["share", &stranger.public_key(), "--label", "alice"])
        .ok();
    source.git(&["add", "--", ".vault"]).ok();
    source.commit("share");
    let before = source.read(DATA);

    let output = source.vault(&["revoke", "alice"]).ok();

    assert!(output.contains("can no longer open the vault"), "{output}");
    assert!(output.contains("still read every commit"), "{output}");
    assert_ne!(source.read(DATA), before, "a new key means new bytes");
    assert!(source.vault(&["ls"]).ok().contains("secrets/prod.env"));
    assert_eq!(source.read("secrets/prod.env"), b"A=1\n");
}

#[test]
fn revoking_the_only_recipient_is_refused() {
    let repo = shared_repo();

    let stderr = repo.vault(&["revoke", &repo.public_key()]).failed();

    assert!(stderr.contains("only recipient left"), "{stderr}");
}

#[test]
fn revoking_yourself_is_refused() {
    let source = shared_repo();
    let stranger = Repo::stranger_clone_of(&source);
    source
        .vault(&["share", &stranger.public_key(), "--label", "alice"])
        .ok();

    let stderr = source.vault(&["revoke", &source.public_key()]).failed();

    assert!(stderr.contains("that recipient is you"), "{stderr}");
}

#[test]
fn an_ambiguous_name_is_reported_rather_than_guessed() {
    let source = shared_repo();
    let first = Repo::stranger_clone_of(&source);
    let second = Repo::stranger_clone_of(&source);
    source
        .vault(&["share", &first.public_key(), "--label", "alice@home"])
        .ok();
    source
        .vault(&["share", &second.public_key(), "--label", "alice@work"])
        .ok();

    let stderr = source.vault(&["revoke", "alice"]).failed();

    assert!(stderr.contains("matches 2 recipients"), "{stderr}");
}

#[test]
fn rotating_keeps_everyone_and_changes_every_byte() {
    let source = shared_repo();
    let stranger = Repo::stranger_clone_of(&source);
    source
        .vault(&["share", &stranger.public_key(), "--label", "alice"])
        .ok();
    source.git(&["add", "--", ".vault"]).ok();
    source.commit("share");
    let before = source.read(DATA);

    source.vault(&["rotate"]).ok();

    assert_ne!(source.read(DATA), before);
    assert_eq!(source.read("secrets/prod.env"), b"A=1\n");
    assert_eq!(source.vault(&["keys"]).ok().lines().count(), 4);

    source.git(&["add", "--", ".vault"]).ok();
    source.commit("rotate");
    stranger.git(&["pull", "--quiet", "origin", "main"]).ok();
    stranger.vault(&["unlock"]).ok();
    assert_eq!(stranger.read("secrets/prod.env"), b"A=1\n");
}

#[test]
fn a_clone_holding_a_stale_key_is_told_to_unlock_rather_than_broken() {
    let source = shared_repo();
    let other = Repo::clone_of(&source);
    other.vault(&["unlock"]).ok();

    source.vault(&["rotate"]).ok();
    source.git(&["add", "--", ".vault"]).ok();
    source.commit("rotate");

    let pull = other.git(&["pull", "origin", "main"]);

    assert!(pull.succeeded(), "{}", pull.stderr());
    assert!(
        pull.stderr().contains("sealed with a newer vault key"),
        "{}",
        pull.stderr()
    );
    assert_eq!(
        other.read("secrets/prod.env"),
        b"A=1\n",
        "the secrets on disk were left alone"
    );

    other.vault(&["unlock"]).ok();
    assert_eq!(other.status(), "");
}

#[test]
fn an_exported_key_unlocks_without_an_identity() {
    let source = shared_repo();
    let key_file = source.root().join("../ci.key");
    source
        .vault(&["export-key", key_file.to_str().unwrap()])
        .ok();

    let runner = Repo::keyless_clone_of(&source);
    let copied = runner.root().join("../ci.key");
    std::fs::copy(&key_file, &copied).unwrap();

    runner
        .vault(&["unlock", "--key-file", copied.to_str().unwrap()])
        .ok();

    assert_eq!(runner.read("secrets/prod.env"), b"A=1\n");
    assert_eq!(runner.status(), "");
}
