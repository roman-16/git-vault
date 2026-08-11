mod harness;

use harness::Repo;

const DATA: &str = ".vault/data";

fn diverging() -> Repo {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "STRIPE=one\nDB=one\nCACHE=one\n");
    repo.commit_all("the shared secret");
    repo.git(&["checkout", "--quiet", "-b", "theirs"]).ok();
    repo
}

#[test]
fn both_sides_adding_different_secrets_merges_cleanly() {
    let repo = diverging();
    repo.write("secrets/theirs.env", "from them\n");
    repo.commit_all("their new secret");

    repo.git(&["checkout", "--quiet", "main"]).ok();
    repo.write("secrets/ours.env", "from us\n");
    repo.commit_all("our new secret");

    repo.git(&["merge", "--no-edit", "theirs"]).ok();

    assert_eq!(repo.read("secrets/ours.env"), b"from us\n");
    assert_eq!(repo.read("secrets/theirs.env"), b"from them\n");
    assert_eq!(
        repo.read("secrets/prod.env"),
        b"STRIPE=one\nDB=one\nCACHE=one\n"
    );
    assert_eq!(repo.status(), "");
}

#[test]
fn both_sides_editing_different_lines_of_one_secret_merges_cleanly() {
    let repo = diverging();
    repo.write("secrets/prod.env", "STRIPE=one\nDB=one\nCACHE=theirs\n");
    repo.commit_all("they changed the cache");

    repo.git(&["checkout", "--quiet", "main"]).ok();
    repo.write("secrets/prod.env", "STRIPE=ours\nDB=one\nCACHE=one\n");
    repo.commit_all("we changed stripe");

    repo.git(&["merge", "--no-edit", "theirs"]).ok();

    assert_eq!(
        repo.read("secrets/prod.env"),
        b"STRIPE=ours\nDB=one\nCACHE=theirs\n"
    );
    assert_eq!(repo.status(), "");
}

#[test]
fn a_deletion_on_one_side_only_is_taken() {
    let repo = diverging();
    repo.write("secrets/other.env", "kept\n");
    repo.commit_all("a second secret");
    repo.remove("secrets/prod.env");
    repo.commit_all("they removed one");

    repo.git(&["checkout", "--quiet", "main"]).ok();
    repo.write("secrets/unrelated.env", "ours\n");
    repo.commit_all("we added something else");

    repo.git(&["merge", "--no-edit", "theirs"]).ok();

    assert!(
        !repo.exists("secrets/prod.env"),
        "the deletion carried over"
    );
    assert_eq!(repo.read("secrets/unrelated.env"), b"ours\n");
    assert_eq!(repo.status(), "");
}

#[test]
fn both_sides_editing_the_same_line_conflicts_in_the_plaintext() {
    let repo = diverging();
    repo.write("secrets/prod.env", "STRIPE=theirs\nDB=one\nCACHE=one\n");
    repo.commit_all("they changed stripe");

    repo.git(&["checkout", "--quiet", "main"]).ok();
    repo.write("secrets/prod.env", "STRIPE=ours\nDB=one\nCACHE=one\n");
    repo.commit_all("we changed stripe too");

    let merge = repo.git(&["merge", "--no-edit", "theirs"]);

    assert!(!merge.succeeded(), "this genuinely conflicts");
    assert!(
        merge.stderr().contains("secrets/prod.env"),
        "{}",
        merge.stderr()
    );
    assert!(
        merge.stderr().contains("git vault seal"),
        "{}",
        merge.stderr()
    );

    let conflicted = String::from_utf8(repo.read("secrets/prod.env")).unwrap();
    assert!(conflicted.contains("<<<<<<<"), "{conflicted}");
    assert!(conflicted.contains("STRIPE=ours"), "{conflicted}");
    assert!(conflicted.contains("STRIPE=theirs"), "{conflicted}");
    assert!(
        conflicted.contains("DB=one"),
        "the rest merged: {conflicted}"
    );
}

#[test]
fn resolving_a_conflict_is_editing_an_ordinary_file() {
    let repo = diverging();
    repo.write("secrets/prod.env", "STRIPE=theirs\nDB=one\nCACHE=one\n");
    repo.commit_all("they changed stripe");
    repo.git(&["checkout", "--quiet", "main"]).ok();
    repo.write("secrets/prod.env", "STRIPE=ours\nDB=one\nCACHE=one\n");
    repo.commit_all("we changed stripe too");
    repo.git(&["merge", "--no-edit", "theirs"]);

    repo.write("secrets/prod.env", "STRIPE=agreed\nDB=one\nCACHE=one\n");
    repo.vault(&["seal"]).ok();
    repo.git(&["add", "--", DATA]).ok();
    repo.git(&["commit", "--quiet", "--no-edit"]).ok();

    assert_eq!(repo.status(), "");
    assert_eq!(
        repo.read("secrets/prod.env"),
        b"STRIPE=agreed\nDB=one\nCACHE=one\n"
    );

    repo.remove("secrets");
    repo.remove(DATA);
    repo.git(&["checkout", "--", DATA]).ok();
    assert_eq!(
        repo.read("secrets/prod.env"),
        b"STRIPE=agreed\nDB=one\nCACHE=one\n"
    );
}

#[test]
fn somebody_without_the_key_can_still_merge() {
    let repo = diverging();
    repo.write("secrets/theirs.env", "from them\n");
    repo.commit_all("their new secret");
    repo.git(&["checkout", "--quiet", "main"]).ok();
    repo.write("secrets/ours.env", "from us\n");
    repo.commit_all("our new secret");

    repo.vault(&["lock"]).ok();
    repo.git(&["merge", "--no-edit", "theirs"]).ok();

    assert!(
        !repo.exists("secrets/ours.env"),
        "still locked, so no plaintext"
    );
    assert_eq!(repo.status(), "");

    repo.vault(&["unlock"]).ok();
    assert_eq!(repo.read("secrets/ours.env"), b"from us\n");
    assert_eq!(repo.read("secrets/theirs.env"), b"from them\n");
    assert_eq!(
        repo.read("secrets/prod.env"),
        b"STRIPE=one\nDB=one\nCACHE=one\n"
    );
}

#[test]
fn without_the_key_one_entry_changed_on_both_sides_conflicts() {
    let repo = diverging();
    repo.write("secrets/prod.env", "STRIPE=theirs\nDB=one\nCACHE=one\n");
    repo.commit_all("they changed it");
    repo.git(&["checkout", "--quiet", "main"]).ok();
    repo.write("secrets/prod.env", "STRIPE=ours\nDB=one\nCACHE=one\n");
    repo.commit_all("we changed it");

    repo.vault(&["lock"]).ok();
    let merge = repo.git(&["merge", "--no-edit", "theirs"]);

    assert!(!merge.succeeded());
    assert!(
        merge.stderr().contains("a sealed entry"),
        "without the key it can only name the entry: {}",
        merge.stderr()
    );
}

#[test]
fn a_rebase_across_diverging_secrets_works() {
    let repo = diverging();
    repo.write("secrets/prod.env", "STRIPE=one\nDB=one\nCACHE=theirs\n");
    repo.commit_all("their edit");

    repo.git(&["checkout", "--quiet", "main"]).ok();
    repo.write("secrets/prod.env", "STRIPE=ours\nDB=one\nCACHE=one\n");
    repo.commit_all("our edit");

    repo.git(&["rebase", "theirs"]).ok();

    assert_eq!(
        repo.read("secrets/prod.env"),
        b"STRIPE=ours\nDB=one\nCACHE=theirs\n"
    );
    assert_eq!(repo.status(), "");
}
