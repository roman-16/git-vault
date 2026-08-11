mod harness;

use std::fs;

use harness::{BIN, Repo};

const DATA: &str = ".vault/data";

#[test]
fn a_clone_without_the_tool_leaves_the_vault_alone() {
    let source = Repo::sealed(&["secrets/"]);
    source.write("secrets/prod.env", "A=1\n");
    source.commit_all("add a secret");
    let sealed = source.blob_id(&format!(":{DATA}"));

    let clone = Repo::keyless_clone_of(&source);

    assert_eq!(clone.status(), "");
    assert!(!clone.exists("secrets/prod.env"));
    assert_eq!(
        clone.disk_size(DATA),
        clone.blob_size(DATA),
        "the sealed file is simply a file"
    );

    clone.write("README.md", "# project\n\nedited without the tool\n");
    clone.commit_all("a commit from someone who cannot read the vault");

    assert_eq!(clone.blob_id(&format!(":{DATA}")), sealed);
    assert_eq!(clone.blob_id(&format!("HEAD:{DATA}")), sealed);
}

#[test]
fn a_locked_clone_keeps_working_normally() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.commit_all("add a secret");
    let sealed = repo.blob_id(&format!(":{DATA}"));

    repo.vault(&["lock"]).ok();

    assert_eq!(repo.status(), "");
    repo.write("README.md", "# project\n\nstill working\n");
    repo.commit_all("a commit while locked");

    assert_eq!(repo.blob_id(&format!(":{DATA}")), sealed);
}

#[test]
fn a_corrupted_vault_is_refused_before_it_is_stored() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.commit_all("add a secret");
    let sealed = repo.blob_id(&format!(":{DATA}"));

    repo.git(&["config", "--unset", "core.fsmonitor"]).ok();
    repo.write_bytes(DATA, b"<<<<<<< HEAD\nnot a vault at all\n>>>>>>> other\n");
    let stderr = repo.git(&["add", "--", DATA]).failed();

    assert!(stderr.contains("not a valid vault"), "{stderr}");
    assert!(stderr.contains("git vault seal"), "{stderr}");
    assert_eq!(
        repo.blob_id(&format!(":{DATA}")),
        sealed,
        "nothing was stored"
    );
}

#[test]
fn an_empty_vault_file_is_refused_before_it_is_stored() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.commit_all("add a secret");

    repo.git(&["config", "--unset", "core.fsmonitor"]).ok();
    repo.write_bytes(DATA, b"");
    let stderr = repo.git(&["add", "--", DATA]).failed();

    assert!(stderr.contains("refusing to store an empty"), "{stderr}");
}

#[test]
fn required_true_aborts_the_operation_when_the_filter_fails() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.commit_all("add a secret");
    let sealed = repo.blob_id(&format!(":{DATA}"));

    repo.git(&["config", "--unset", "core.fsmonitor"]).ok();
    repo.git(&["config", "filter.vault.clean", "false"]).ok();
    repo.write_bytes(DATA, b"anything, as long as it differs\n");

    let stderr = repo.git(&["add", "--", DATA]).failed();

    assert!(stderr.contains("clean filter 'vault' failed"), "{stderr}");
    assert_eq!(repo.blob_id(&format!(":{DATA}")), sealed);
}

#[test]
fn sealing_works_from_a_subdirectory() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.commit_all("add a secret");
    let deep = repo.root().join("src/deep");
    fs::create_dir_all(&deep).unwrap();

    repo.write("secrets/prod.env", "A=2\n");
    let output = repo.run_in(&deep, BIN, &["seal"]).ok();

    assert!(output.contains("Sealed 1 secret"), "{output}");
    assert_eq!(repo.status(), " M .vault/data");
}

#[test]
fn outside_a_worktree_the_tool_says_so() {
    let outside = tempfile::Builder::new()
        .prefix("git-vault-outside-")
        .tempdir()
        .unwrap();
    let elsewhere = fs::canonicalize(outside.path()).unwrap();
    let repo = Repo::bare_new();

    let stderr = repo.run_in(&elsewhere, BIN, &["seal"]).failed();

    assert!(stderr.contains("not inside a git worktree"), "{stderr}");
}

#[test]
fn a_repository_without_a_vault_says_what_to_do() {
    let repo = Repo::bare_new();

    let stderr = repo.vault(&["unlock"]).failed();

    assert!(
        stderr.contains("is this a repository with a vault"),
        "{stderr}"
    );
}
