mod harness;

use harness::Repo;

fn committed_in_the_clear(paths: &[(&str, &str)]) -> Repo {
    let repo = Repo::bare_new();
    repo.vault(&["init"]).ok();
    for (path, contents) in paths {
        repo.write(path, contents);
    }
    repo.write("README.md", "# project\n");
    repo.git(&["add", "--all"]).ok();
    repo.commit("committed in the clear, before anybody thought about it");
    repo
}

fn leak_into_the_index(repo: &Repo, path: &str) {
    repo.git(&["config", "--unset", "filter.vault-plaintext.clean"])
        .ok();
    repo.git(&["config", "--unset", "filter.vault-plaintext.required"])
        .ok();
    repo.git(&["add", "--force", "--", path]).ok();
    repo.git(&[
        "config",
        "filter.vault-plaintext.clean",
        "git-vault filter refuse %f",
    ])
    .ok();
    repo.git(&["config", "filter.vault-plaintext.required", "true"])
        .ok();
}

#[test]
fn sealing_a_pattern_that_matches_several_tracked_files_leaves_git_working() {
    let repo = committed_in_the_clear(&[
        ("a/secrets.json", "{\"k\":\"a\"}\n"),
        ("b/secrets.json", "{\"k\":\"b\"}\n"),
    ]);

    let sealing = repo.vault(&["add", "**/secrets.json"]);
    assert!(sealing.succeeded(), "{}", sealing.stderr());

    let status = repo.git(&["status", "--porcelain"]);
    assert!(
        status.succeeded(),
        "plain git stopped working: {}",
        status.stderr()
    );
    assert!(
        repo.git(&["ls-files", "--cached", "--", "a/", "b/"])
            .ok()
            .is_empty(),
        "the plaintext is still in the index"
    );

    let listed = repo.vault(&["ls"]).ok();
    assert!(listed.contains("a/secrets.json"), "{listed}");
    assert!(listed.contains("b/secrets.json"), "{listed}");
}

#[test]
fn the_first_commit_after_sealing_a_tracked_file_is_not_refused() {
    let repo = committed_in_the_clear(&[("s.env", "TOKEN=1\n")]);
    repo.vault(&["add", "s.env"]).ok();

    repo.git(&["add", ".gitattributes", ".gitignore", ".vault"])
        .ok();
    let commit = repo.git(&["commit", "--message", "add a vault"]);

    assert!(commit.succeeded(), "{}", commit.stderr());
    assert_eq!(repo.status(), "");

    repo.remove("s.env");
    repo.vault(&["restore"]).ok();
    assert_eq!(repo.read("s.env"), b"TOKEN=1\n");
}

#[test]
fn a_whole_git_crypt_migration_survives_a_clone() {
    let source = committed_in_the_clear(&[
        ("hosts/one/secrets.json", "{\"token\":\"one\"}\n"),
        ("hosts/two/secrets.json", "{\"token\":\"two\"}\n"),
    ]);

    source.vault(&["add", "**/secrets.json"]).ok();
    source.git(&["add", "--all"]).ok();
    source.commit("seal the secrets");

    let clone = Repo::clone_of(&source);
    clone.vault(&["unlock"]).ok();

    assert_eq!(
        clone.read("hosts/one/secrets.json"),
        b"{\"token\":\"one\"}\n"
    );
    assert_eq!(
        clone.read("hosts/two/secrets.json"),
        b"{\"token\":\"two\"}\n"
    );
    assert_eq!(clone.status(), "");
    assert!(
        clone
            .git(&["ls-tree", "-r", "--name-only", "HEAD"])
            .ok()
            .lines()
            .all(|path| !path.ends_with("secrets.json")),
        "the remote can still see the secret files"
    );
}

#[test]
fn add_repairs_declared_secrets_that_are_still_tracked() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.write("secrets/deploy.token", "B=2\n");
    repo.commit_all("add secrets");
    leak_into_the_index(&repo, "secrets/prod.env");
    leak_into_the_index(&repo, "secrets/deploy.token");

    assert_eq!(repo.vault(&["doctor"]).code(), 4);

    let repair = repo.vault(&["add", "secrets/"]);

    assert!(repair.succeeded(), "{}", repair.stderr());
    assert!(
        repo.git(&["ls-files", "--cached", "--", "secrets/"])
            .ok()
            .is_empty()
    );
    assert_eq!(
        repo.vault(&["doctor"]).code(),
        0,
        "{}",
        repo.vault(&["doctor"]).stdout()
    );
}

#[test]
fn doctor_reports_a_tracked_secret_that_is_gone_from_the_worktree() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.write("secrets/kept.env", "B=2\n");
    repo.commit_all("add secrets");
    leak_into_the_index(&repo, "secrets/prod.env");
    repo.remove("secrets/prod.env");

    let run = repo.vault(&["doctor"]);

    assert_eq!(run.code(), 4, "{}", run.stdout());
    assert!(
        run.stdout()
            .contains("`secrets/prod.env` is sealed and also tracked in plaintext"),
        "a plaintext secret sits in the index and doctor missed it: {}",
        run.stdout()
    );
}

#[test]
fn a_commit_is_refused_while_a_declared_secret_sits_in_the_index() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.commit_all("add a secret");
    leak_into_the_index(&repo, "secrets/prod.env");
    repo.remove("secrets/prod.env");

    let attempt = repo.git(&["commit", "--message", "would leak"]);

    assert!(!attempt.succeeded());
    assert!(
        attempt.stderr().contains("would publish the plaintext"),
        "{}",
        attempt.stderr()
    );
}

#[test]
fn doctor_does_not_fail_a_vault_that_is_not_committed_yet() {
    let repo = Repo::bare_new();
    repo.write("README.md", "# project\n");
    repo.git(&["add", "--all"]).ok();
    repo.commit("init");
    repo.vault(&["init"]).ok();
    repo.vault(&["add", "secrets/"]).ok();
    repo.write("secrets/prod.env", "A=1\n");
    repo.vault(&["seal"]).ok();

    let run = repo.vault(&["doctor"]);

    assert_eq!(run.code(), 0, "{}", run.stdout());
    assert!(
        run.stdout().contains("not committed yet"),
        "{}",
        run.stdout()
    );
}
