mod harness;

use std::thread::sleep;
use std::time::Duration;

use harness::Repo;

const DATA: &str = ".vault/data";

fn past_the_racy_window() {
    sleep(Duration::from_millis(1100));
}

#[test]
fn editing_a_secret_shows_up_in_plain_git_status() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.commit_all("add a secret");

    repo.write("secrets/prod.env", "A=2\n");

    assert_eq!(repo.status(), " M .vault/data");
}

#[test]
fn an_untouched_repository_is_clean() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.commit_all("add a secret");

    past_the_racy_window();

    assert_eq!(repo.status(), "");
}

#[test]
fn the_sealed_file_on_disk_always_equals_what_git_stored() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.commit_all("add a secret");

    for step in ["initial", "after checkout", "after reset", "after stash"] {
        assert_eq!(
            repo.disk_size(DATA),
            repo.blob_size(DATA),
            "sizes disagree {step}"
        );
        assert_eq!(repo.status(), "", "status is not clean {step}");

        match step {
            "initial" => repo.git(&["checkout", "--quiet", "-b", "other"]).ok(),
            "after checkout" => repo.git(&["reset", "--quiet", "--hard"]).ok(),
            _ => {
                repo.write("secrets/prod.env", "A=stash\n");
                repo.git(&["stash", "--quiet"]).ok()
            }
        };
    }
}

#[test]
fn nothing_is_written_when_nothing_changed() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.commit_all("add a secret");

    let before = repo.modified(DATA);
    let sealed = repo.blob_id(&format!(":{DATA}"));

    for _ in 0..20 {
        assert_eq!(repo.status(), "");
    }

    assert_eq!(repo.modified(DATA), before, "`{DATA}` was rewritten");
    assert_eq!(repo.blob_id(&format!(":{DATA}")), sealed);
}

#[test]
fn merging_a_branch_that_changed_secrets_works() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=main\n");
    repo.commit_all("main's secret");

    repo.git(&["checkout", "--quiet", "-b", "feature"]).ok();
    repo.write("secrets/only-here.env", "from the feature branch\n");
    repo.commit_all("a secret on the feature branch");

    repo.git(&["checkout", "--quiet", "main"]).ok();
    repo.write("README.md", "# project\n\nmoved on\n");
    repo.commit_all("unrelated work on main");

    repo.git(&["merge", "--no-edit", "feature"]).ok();

    assert_eq!(
        repo.read("secrets/only-here.env"),
        b"from the feature branch\n"
    );
    assert_eq!(repo.read("secrets/prod.env"), b"A=main\n");
    assert_eq!(repo.status(), "");
}

#[test]
fn rebasing_across_a_secret_change_works() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=base\n");
    repo.commit_all("base");

    repo.git(&["checkout", "--quiet", "-b", "topic"]).ok();
    repo.write("secrets/topic.env", "topic\n");
    repo.commit_all("a secret on the topic branch");

    repo.git(&["checkout", "--quiet", "main"]).ok();
    repo.write("README.md", "# project\n\nmoved on\n");
    repo.commit_all("unrelated work on main");

    repo.git(&["checkout", "--quiet", "topic"]).ok();
    repo.git(&["rebase", "main"]).ok();

    assert_eq!(repo.read("secrets/topic.env"), b"topic\n");
    assert_eq!(repo.read("secrets/prod.env"), b"A=base\n");
    assert_eq!(repo.status(), "");
}

#[test]
fn reverting_a_commit_that_changed_secrets_works() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.commit_all("add a secret");
    repo.write("secrets/gone.env", "temporary\n");
    repo.commit_all("add another");

    repo.git(&["revert", "--no-edit", "HEAD"]).ok();

    assert!(!repo.exists("secrets/gone.env"), "the revert removed it");
    assert_eq!(repo.read("secrets/prod.env"), b"A=1\n");
    assert_eq!(repo.status(), "");
}

#[test]
fn cherry_picking_a_secret_change_works() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=base\n");
    repo.commit_all("base");
    repo.git(&["checkout", "--quiet", "-b", "topic"]).ok();
    repo.write("secrets/topic.env", "topic\n");
    repo.commit_all("a secret to pick");
    let pick = repo.blob_id("HEAD");

    repo.git(&["checkout", "--quiet", "main"]).ok();
    repo.git(&["cherry-pick", &pick]).ok();

    assert_eq!(repo.read("secrets/topic.env"), b"topic\n");
    assert_eq!(repo.status(), "");
}

#[test]
fn stashing_a_secret_edit_round_trips() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=committed\n");
    repo.commit_all("add a secret");

    repo.write("secrets/prod.env", "A=work in progress\n");
    repo.git(&["stash", "--quiet"]).ok();

    assert_eq!(repo.read("secrets/prod.env"), b"A=committed\n");
    assert_eq!(repo.status(), "");

    repo.git(&["stash", "pop", "--quiet"]).ok();

    assert_eq!(repo.read("secrets/prod.env"), b"A=work in progress\n");
}

#[test]
fn a_commit_of_everything_carries_the_secret_change() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.commit_all("add a secret");

    repo.write("secrets/prod.env", "A=2\n");
    repo.commit_all("rotate");

    repo.remove("secrets");
    repo.remove(DATA);
    repo.git(&["checkout", "--", DATA]).ok();

    assert_eq!(repo.read("secrets/prod.env"), b"A=2\n");
}

#[test]
fn a_commit_of_named_files_leaves_secret_changes_out() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.commit_all("add a secret");
    let sealed_before = repo.blob_id(&format!("HEAD:{DATA}"));

    repo.write("secrets/prod.env", "A=2\n");
    repo.write("notes.md", "# notes\n");
    repo.git(&["add", "notes.md"]).ok();
    repo.git(&["commit", "--quiet", "--message", "notes"]).ok();

    assert_eq!(
        repo.blob_id(&format!("HEAD:{DATA}")),
        sealed_before,
        "a commit of named files must not drag the vault in"
    );
    assert!(
        repo.status().contains(&format!(" M {DATA}")),
        "the secret edit is still waiting, and git says so: {}",
        repo.status()
    );

    repo.git(&["add", DATA]).ok();
    repo.git(&["commit", "--quiet", "--message", "rotate"]).ok();

    repo.remove("secrets");
    repo.remove(DATA);
    repo.git(&["checkout", "--", DATA]).ok();

    assert_eq!(repo.read("secrets/prod.env"), b"A=2\n");
}

#[test]
fn a_commit_of_everything_carries_secret_changes() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.commit_all("add a secret");

    repo.write("secrets/prod.env", "A=2\n");
    repo.write("README.md", "# project\n\nordinary work\n");
    repo.commit_all("ordinary work, with a secret edit alongside");

    repo.remove("secrets");
    repo.remove(DATA);
    repo.git(&["checkout", "--", DATA]).ok();

    assert_eq!(
        repo.read("secrets/prod.env"),
        b"A=2\n",
        "`git commit --all` means all, and the vault is a tracked file like any other"
    );
}

#[test]
fn without_fsmonitor_a_commit_reports_the_secret_edit_it_left_out() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.commit_all("add a secret");

    repo.git(&["config", "--unset", "core.fsmonitor"]).ok();
    repo.write("secrets/prod.env", "A=2\n");

    assert_eq!(repo.status(), "", "without fsmonitor the edit is invisible");

    repo.write("README.md", "# project\n\nordinary work\n");
    let attempt = repo.git(&["commit", "--all", "--message", "ordinary work"]);

    assert!(attempt.succeeded());
    assert!(
        attempt.stderr().contains("does not include"),
        "{}",
        attempt.stderr()
    );

    repo.commit_all("rotate");

    repo.remove("secrets");
    repo.remove(DATA);
    repo.git(&["checkout", "--", DATA]).ok();

    assert_eq!(
        repo.read("secrets/prod.env"),
        b"A=2\n",
        "the hook sealed it, so the next commit could pick it up"
    );
}

#[test]
fn without_fsmonitor_a_secret_only_commit_asks_to_be_repeated() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.commit_all("add a secret");

    repo.git(&["config", "--unset", "core.fsmonitor"]).ok();
    repo.write("secrets/prod.env", "A=2\n");

    let attempt = repo.git(&["commit", "--all", "--message", "rotate"]);

    assert!(!attempt.succeeded());
    assert!(
        attempt.stdout().contains("nothing to commit"),
        "{}",
        attempt.stdout()
    );
    assert!(
        attempt.stderr().contains("sealed your secret changes"),
        "{}",
        attempt.stderr()
    );

    repo.commit_all("rotate");

    repo.remove("secrets");
    repo.remove(DATA);
    repo.git(&["checkout", "--", DATA]).ok();
    assert_eq!(repo.read("secrets/prod.env"), b"A=2\n");
}

#[test]
fn nothing_reseals_in_the_middle_of_a_merge() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("conflict.txt", "base\n");
    repo.write("secrets/prod.env", "A=base\n");
    repo.git(&["add", "--all"]).ok();
    repo.commit("base");

    repo.git(&["checkout", "--quiet", "-b", "other"]).ok();
    repo.write("conflict.txt", "theirs\n");
    repo.commit_all("their change");

    repo.git(&["checkout", "--quiet", "main"]).ok();
    repo.write("conflict.txt", "ours\n");
    repo.commit_all("our change");

    let merge = repo.git(&["merge", "--no-edit", "other"]);
    assert!(!merge.succeeded(), "the text file should conflict");

    let sealed_before = repo.read(DATA);
    repo.write("secrets/prod.env", "A=edited mid-merge\n");
    repo.status();

    assert_eq!(
        repo.read(DATA),
        sealed_before,
        "the sealed file must not move while a merge is unresolved"
    );
}

#[test]
fn switching_branches_reconciles_rather_than_extracts() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=main\n");
    repo.commit_all("main's secret");

    repo.git(&["checkout", "--quiet", "-b", "feature"]).ok();
    repo.write("secrets/prod.env", "A=feature\n");
    repo.write("secrets/only-here.env", "temporary\n");
    repo.commit_all("feature's secrets");

    repo.git(&["checkout", "--quiet", "main"]).ok();

    assert_eq!(repo.read("secrets/prod.env"), b"A=main\n", "edits reverted");
    assert!(
        !repo.exists("secrets/only-here.env"),
        "a secret this branch does not have must be removed, not left behind"
    );
    assert_eq!(repo.status(), "");

    repo.git(&["checkout", "--quiet", "feature"]).ok();

    assert_eq!(repo.read("secrets/prod.env"), b"A=feature\n");
    assert_eq!(repo.read("secrets/only-here.env"), b"temporary\n");
}

#[test]
fn an_uncommitted_secret_edit_blocks_a_branch_switch() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=main\n");
    repo.commit_all("main's secret");
    repo.git(&["checkout", "--quiet", "-b", "feature"]).ok();
    repo.write("secrets/prod.env", "A=feature\n");
    repo.commit_all("feature's secret");
    repo.git(&["checkout", "--quiet", "main"]).ok();

    repo.write("secrets/prod.env", "A=work in progress\n");
    let stderr = repo.git(&["checkout", "feature"]).failed();

    assert!(stderr.contains("would be overwritten"), "{stderr}");
    assert_eq!(
        repo.read("secrets/prod.env"),
        b"A=work in progress\n",
        "the uncommitted edit is still there"
    );
}

#[test]
fn removing_everything_and_checking_out_restores_it() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.write("secrets/deep/nested.key", "inner\n");
    repo.commit_all("add secrets");

    repo.remove("secrets");
    repo.remove(DATA);
    repo.git(&["checkout", "--", DATA]).ok();

    assert_eq!(repo.read("secrets/prod.env"), b"A=1\n");
    assert_eq!(repo.read("secrets/deep/nested.key"), b"inner\n");
    assert_eq!(repo.status(), "");
}

#[cfg(unix)]
#[test]
fn modes_and_symlinks_survive_a_round_trip() {
    use std::os::unix::fs::PermissionsExt as _;

    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/deploy.sh", "#!/bin/sh\necho deploying\n");
    std::fs::set_permissions(
        repo.root().join("secrets/deploy.sh"),
        std::fs::Permissions::from_mode(0o755),
    )
    .unwrap();
    std::os::unix::fs::symlink("deploy.sh", repo.root().join("secrets/current")).unwrap();
    repo.commit_all("add an executable and a link");

    repo.remove("secrets");
    repo.vault(&["restore"]).ok();

    let mode = std::fs::metadata(repo.root().join("secrets/deploy.sh"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o755, "the executable bit came back");
    assert!(
        repo.root().join("secrets/current").is_symlink(),
        "the link came back"
    );
    assert_eq!(repo.status(), "");
}

#[test]
fn locking_removes_the_plaintext_and_unlocking_brings_it_back() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.commit_all("add a secret");
    let sealed = repo.blob_id(&format!(":{DATA}"));

    repo.vault(&["lock"]).ok();

    assert!(!repo.exists("secrets/prod.env"), "plaintext is gone");
    assert!(!repo.exists(".git/vault/key"), "the local key is gone");
    assert_eq!(repo.status(), "", "locking leaves nothing to commit");

    repo.vault(&["unlock"]).ok();

    assert_eq!(repo.read("secrets/prod.env"), b"A=1\n");
    assert_eq!(repo.blob_id(&format!(":{DATA}")), sealed);
    assert_eq!(repo.status(), "");
}

#[test]
fn a_clone_unlocks_with_the_same_identity() {
    let source = Repo::sealed(&["secrets/"]);
    source.write("secrets/prod.env", "A=1\n");
    source.write("secrets/deep/nested.key", "inner\n");
    source.commit_all("add secrets");

    let clone = Repo::clone_of(&source);

    assert!(!clone.exists("secrets/prod.env"));
    assert_eq!(clone.status(), "");

    clone.vault(&["unlock"]).ok();

    assert_eq!(clone.read("secrets/prod.env"), b"A=1\n");
    assert_eq!(clone.read("secrets/deep/nested.key"), b"inner\n");
    assert_eq!(clone.status(), "");
}

#[test]
fn a_clone_without_an_identity_is_told_what_to_do() {
    let source = Repo::sealed(&["secrets/"]);
    source.write("secrets/prod.env", "A=1\n");
    source.commit_all("add a secret");

    let clone = Repo::keyless_clone_of(&source);
    let stderr = clone.vault(&["unlock"]).failed();

    assert!(stderr.contains("git vault share"), "{stderr}");
}

#[test]
fn a_linked_worktree_comes_up_unlocked() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.commit_all("add a secret");

    let linked = repo.root().join("../linked");
    repo.git(&[
        "worktree",
        "add",
        "--quiet",
        linked.to_str().unwrap(),
        "-b",
        "linked",
    ])
    .ok();

    assert_eq!(
        std::fs::read(linked.join("secrets/prod.env")).unwrap(),
        b"A=1\n"
    );

    std::fs::remove_dir_all(&linked).ok();
}

#[test]
fn an_unanchored_pattern_seals_files_anywhere() {
    let repo = Repo::sealed(&["*.key"]);
    repo.write("top.key", "one\n");
    repo.write("config/deep/prod.key", "two\n");
    repo.commit_all("secrets scattered around");

    repo.remove("top.key");
    repo.remove("config/deep/prod.key");
    repo.vault(&["restore"]).ok();

    assert_eq!(repo.read("top.key"), b"one\n");
    assert_eq!(repo.read("config/deep/prod.key"), b"two\n");
    assert_eq!(repo.status(), "");
}

#[test]
fn plain_git_restores_an_edited_secret() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.commit_all("add a secret");

    repo.write("secrets/prod.env", "A=ruined\n");
    repo.git(&["checkout", "--", DATA]).ok();

    assert_eq!(
        repo.read("secrets/prod.env"),
        b"A=1\n",
        "sealing on the way in made the file differ, so checkout had something to write"
    );
}

#[test]
fn plain_git_restores_one_deleted_secret() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.write("secrets/deploy.token", "token\n");
    repo.commit_all("add secrets");

    repo.remove("secrets/prod.env");
    repo.git(&["checkout", "--", DATA]).ok();

    assert_eq!(repo.read("secrets/prod.env"), b"A=1\n");
}

#[test]
fn plain_git_cannot_restore_a_wiped_worktree() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.commit_all("add a secret");

    repo.remove("secrets");
    repo.git(&["checkout", "--", DATA]).ok();

    assert!(
        !repo.root().join("secrets/prod.env").exists(),
        "the wipe guard refuses to seal, so git sees nothing to do"
    );

    repo.vault(&["restore"]).ok();

    assert_eq!(repo.read("secrets/prod.env"), b"A=1\n");
}

#[test]
fn restoring_one_secret_leaves_the_others_edited() {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "A=1\n");
    repo.write("secrets/deploy.token", "token\n");
    repo.commit_all("add secrets");

    repo.write("secrets/prod.env", "A=ruined\n");
    repo.write("secrets/deploy.token", "deliberate\n");
    repo.vault(&["restore", "secrets/prod.env"]).ok();

    assert_eq!(repo.read("secrets/prod.env"), b"A=1\n");
    assert_eq!(
        repo.read("secrets/deploy.token"),
        b"deliberate\n",
        "git alone cannot do this: to it the vault is one blob"
    );
}
