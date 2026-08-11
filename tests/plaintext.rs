mod harness;

use harness::Repo;

fn with_a_secret() -> Repo {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "STRIPE_KEY=sk_live_example\n");
    repo.commit_all("add a secret");
    repo
}

#[test]
fn an_ignored_secret_is_never_in_the_filters_way() {
    let repo = with_a_secret();
    repo.write("secrets/another.env", "A=1\n");

    let run = repo.git(&["add", "--all"]);

    assert!(run.succeeded(), "{}", run.stderr());
    assert!(
        !repo.status().contains("secrets/"),
        "a secret reached the index: {}",
        repo.status()
    );
}

#[test]
fn forcing_a_secret_past_gitignore_is_refused() {
    let repo = with_a_secret();

    let run = repo.git(&["add", "--force", "--", "secrets/prod.env"]);

    assert!(!run.succeeded());
    assert!(
        run.stderr().contains("refusing to put the plaintext"),
        "{}",
        run.stderr()
    );
    assert!(!repo.status().contains("secrets/prod.env"));
}

#[test]
fn losing_the_gitignore_entry_does_not_publish_the_secret() {
    let repo = with_a_secret();
    repo.write(".gitignore", "");

    let run = repo.git(&["add", "."]);

    assert!(!run.succeeded(), "{}", run.stderr());
    assert!(
        !repo.status().contains("A  secrets/prod.env"),
        "the plaintext was staged: {}",
        repo.status()
    );
}

#[test]
fn stashing_everything_cannot_smuggle_a_secret_into_the_object_store() {
    let repo = with_a_secret();
    repo.write("README.md", "# changed\n");

    let run = repo.git(&["stash", "--all"]);

    assert!(!run.succeeded(), "{}", run.stderr());
    assert_eq!(
        repo.read("secrets/prod.env"),
        b"STRIPE_KEY=sk_live_example\n",
        "the refusal must not cost the secret"
    );
}

#[test]
fn a_commit_of_plaintext_staged_without_the_filter_is_refused() {
    let repo = with_a_secret();

    repo.git(&["config", "--unset", "filter.vault-plaintext.clean"])
        .ok();
    repo.git(&["config", "--unset", "filter.vault-plaintext.required"])
        .ok();
    repo.git(&["add", "--force", "--", "secrets/prod.env"]).ok();
    repo.git(&[
        "config",
        "filter.vault-plaintext.clean",
        "git-vault filter refuse %f",
    ])
    .ok();
    repo.git(&["config", "filter.vault-plaintext.required", "true"])
        .ok();
    repo.remove("secrets/prod.env");

    let run = repo.git(&["commit", "--message", "would leak"]);

    assert!(!run.succeeded());
    assert!(
        run.stderr().contains("would publish the plaintext"),
        "{}",
        run.stderr()
    );
}

#[test]
fn undeclaring_a_path_is_how_you_publish_it_in_the_clear() {
    let repo = with_a_secret();

    repo.vault(&["remove", "secrets/"]).ok();
    repo.write("secrets/prod.env", "PUBLIC=yes\n");
    let run = repo.git(&["add", "secrets/prod.env"]);

    assert!(run.succeeded(), "{}", run.stderr());
    assert!(
        repo.status().contains("secrets/prod.env"),
        "{}",
        repo.status()
    );
}

#[test]
fn declaring_a_path_takes_its_plaintext_out_of_the_index() {
    let repo = Repo::bare_new();
    repo.vault(&["init"]).ok();
    repo.write("secrets/prod.env", "STRIPE_KEY=sk_live_example\n");
    repo.git(&["add", "--all"]).ok();
    repo.commit("committed in the clear, before anybody thought about it");

    let run = repo.vault(&["add", "secrets/"]);

    assert!(run.succeeded(), "{}", run.stderr());
    assert!(
        run.stdout().contains("untracked secrets/prod.env"),
        "{}",
        run.stdout()
    );
    assert!(
        repo.git(&["ls-files", "--cached", "--", "secrets/prod.env"])
            .ok()
            .is_empty(),
        "the plaintext is still in the index"
    );
}
