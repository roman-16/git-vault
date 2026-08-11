mod harness;

use harness::Repo;

fn with_history() -> Repo {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "STRIPE=one\nDB=one\n");
    repo.commit_all("the first secret");
    repo.write("secrets/prod.env", "STRIPE=two\nDB=one\n");
    repo.commit_all("rotate stripe");
    repo
}

#[test]
fn git_log_p_shows_the_plaintext_of_every_change() {
    let repo = with_history();

    let log = repo.git(&["log", "--patch"]).ok();

    assert!(log.contains("-STRIPE=one"), "{log}");
    assert!(log.contains("+STRIPE=two"), "{log}");
    assert!(log.contains("# secrets/prod.env (file"), "{log}");
}

#[test]
fn git_show_shows_the_plaintext_of_one_commit() {
    let repo = with_history();

    let shown = repo.git(&["show"]).ok();

    assert!(shown.contains("+STRIPE=two"), "{shown}");
}

#[test]
fn without_a_key_the_rendering_says_so_instead_of_failing() {
    let repo = with_history();
    repo.vault(&["lock"]).ok();

    let shown = repo.git(&["show"]).ok();

    assert!(shown.contains("sealed entry"), "{shown}");
    assert!(
        !shown.contains("STRIPE"),
        "no plaintext without a key: {shown}"
    );
}

#[test]
fn decrypted_secrets_are_never_cached_under_git() {
    let repo = with_history();

    repo.git(&["log", "--patch"]).ok();
    repo.git(&["diff", "HEAD~1", "HEAD"]).ok();

    let cache = repo.root().join(".git/objects/info/cache");
    assert!(!cache.exists(), "git cached the decrypted rendering");
}

#[test]
fn the_history_of_one_secret_can_be_singled_out() {
    let repo = with_history();
    repo.write("secrets/other.env", "unrelated\n");
    repo.commit_all("an unrelated secret");

    let all = repo.vault(&["log"]).ok();
    let one = repo.vault(&["log", "secrets/prod.env"]).ok();

    assert!(all.contains("secrets/other.env"), "{all}");
    assert!(!one.contains("secrets/other.env"), "{one}");
    assert!(one.contains("+STRIPE=two"), "{one}");
}

#[test]
fn a_diff_can_be_narrowed_to_one_secret() {
    let repo = with_history();
    repo.write("secrets/prod.env", "STRIPE=three\nDB=one\n");
    repo.write("secrets/other.env", "changed too\n");

    let all = repo.vault(&["diff"]).ok();
    let one = repo.vault(&["diff", "secrets/prod.env"]).ok();

    assert!(all.contains("secrets/other.env"), "{all}");
    assert!(!one.contains("secrets/other.env"), "{one}");
    assert!(one.contains("+STRIPE=three"), "{one}");
}

#[test]
fn a_binary_secret_is_summarised_rather_than_dumped() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write_bytes("secrets/key.bin", &[0xff, 0xfe, 0x00, 0x01]);
    repo.commit_all("a binary secret");

    let shown = repo.git(&["show"]).ok();
    assert!(shown.contains("binary)"), "{shown}");

    repo.write_bytes("secrets/key.bin", &[0x00, 0x01, 0x02, 0x03]);
    let diff = repo.vault(&["diff"]).ok();
    assert!(diff.contains("binary contents differ"), "{diff}");
}
