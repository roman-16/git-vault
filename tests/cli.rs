mod harness;

use harness::Repo;

#[test]
fn both_invocation_forms_are_the_same_program() {
    let repo = Repo::bare_new();

    let direct = repo.vault(&["--version"]);
    let through_git = repo.git_vault(&["--version"]);

    assert_eq!(direct.stdout(), through_git.stdout());
    assert_eq!(direct.code(), through_git.code());
    assert!(direct.stdout().contains("git-vault"), "{}", direct.stdout());
}

#[test]
fn exit_codes_survive_gits_dispatch() {
    let repo = Repo::bare_new();

    assert_eq!(repo.vault(&["filter", "bogus"]).code(), 1);
    assert_eq!(repo.git_vault(&["filter", "bogus"]).code(), 1);
    assert_eq!(repo.vault(&["definitely-not-a-command"]).code(), 2);
    assert_eq!(repo.git_vault(&["definitely-not-a-command"]).code(), 2);
}

#[test]
fn the_short_help_flag_reaches_the_binary() {
    let repo = Repo::bare_new();

    let help = repo.git_vault(&["-h"]).ok();

    assert!(help.contains("git vault"), "{help}");
}

#[test]
fn git_intercepts_the_long_help_flag() {
    let repo = Repo::bare_new();

    let through_git = repo.git_vault(&["--help"]);
    let direct = repo.vault(&["--help"]).ok();

    assert!(
        !through_git.stdout().contains("Usage:"),
        "{}",
        through_git.stdout()
    );
    assert!(direct.contains("Usage: git vault"), "{direct}");
}

#[test]
fn a_filter_needs_a_mode() {
    let repo = Repo::bare_new();

    let stderr = repo.vault(&["filter"]).failed();

    assert!(stderr.contains("needs a mode"), "{stderr}");
}

#[test]
fn an_unknown_filter_mode_is_named() {
    let repo = Repo::bare_new();

    let stderr = repo.vault(&["filter", "bogus"]).failed();

    assert!(stderr.contains("unknown filter `bogus`"), "{stderr}");
}
