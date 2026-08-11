#![allow(clippy::panic, clippy::unwrap_used)]

mod harness;

use std::fs;
use std::io::ErrorKind;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::thread;
use std::time::Duration;

use harness::BIN;
use tempfile::TempDir;

struct Installed {
    dir: TempDir,
    binary: PathBuf,
    identity: PathBuf,
}

fn installed_at(relative: &str) -> Installed {
    let dir = TempDir::new().unwrap();
    let binary = dir.path().join(relative);
    fs::create_dir_all(binary.parent().unwrap()).unwrap();
    fs::copy(BIN, &binary).unwrap();

    let identity = dir.path().join("identity");
    fs::write(&identity, "# git-vault identity\nAGE-SECRET-KEY-1\n").unwrap();

    Installed {
        dir,
        binary,
        identity,
    }
}

impl Installed {
    fn run(&self, args: &[&str]) -> (bool, String) {
        let mut command = Command::new(&self.binary);
        command
            .args(args)
            .current_dir(self.dir.path())
            .env("GIT_VAULT_IDENTITY", &self.identity)
            .env("HOME", self.dir.path());

        let output = spawn_once_the_copy_settles(&mut command);

        let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
        text.push_str(&String::from_utf8_lossy(&output.stderr));

        (output.status.success(), text)
    }

    fn exists(&self) -> bool {
        self.binary.exists()
    }
}

fn spawn_once_the_copy_settles(command: &mut Command) -> Output {
    for _attempt in 0..200 {
        match command.output() {
            Ok(output) => return output,
            Err(error)
                if error.kind() == ErrorKind::ExecutableFileBusy
                    || error.kind() == ErrorKind::PermissionDenied =>
            {
                thread::sleep(Duration::from_millis(10));
            }
            Err(error) => panic!("cannot run the copied binary: {error}"),
        }
    }

    panic!("the copied binary stayed busy, which another thread's fork can cause")
}

fn standalone() -> Installed {
    installed_at("local/bin/git-vault")
}

#[test]
fn a_standalone_install_removes_itself() {
    let install = standalone();

    let (ok, output) = install.run(&["uninstall", "--yes"]);

    assert!(ok, "{output}");
    assert!(!install.exists(), "the binary is still there: {output}");
}

#[test]
fn a_dry_run_removes_nothing() {
    let install = standalone();

    let (ok, output) = install.run(&["uninstall", "--dry-run"]);

    assert!(ok, "{output}");
    assert!(install.exists(), "the binary was removed anyway");
    assert!(output.contains("dry run"), "{output}");
}

#[test]
fn purging_deletes_the_identity_and_says_what_that_costs() {
    let install = standalone();

    let (ok, output) = install.run(&["uninstall", "--yes", "--purge"]);

    assert!(ok, "{output}");
    assert!(!install.exists());
    assert!(
        !install.identity.exists(),
        "the identity survived a purge: {output}"
    );
}

#[test]
fn a_dry_run_names_the_identity_without_deleting_it() {
    let install = standalone();

    let (ok, output) = install.run(&["uninstall", "--dry-run", "--purge"]);

    assert!(ok, "{output}");
    assert!(install.identity.exists(), "{output}");
    assert!(output.contains("your identity"), "{output}");
}

#[test]
fn removing_without_an_answer_refuses_rather_than_hanging() {
    let install = standalone();

    let (ok, output) = install.run(&["uninstall"]);

    assert!(!ok, "{output}");
    assert!(output.contains("--yes"), "{output}");
    assert!(install.exists());
}

#[test]
fn uninstall_explains_that_repositories_keep_their_wiring() {
    let install = standalone();

    let (_ok, output) = install.run(&["uninstall", "--yes"]);

    assert!(output.contains("filter.vault"), "{output}");
    assert!(output.contains("pre-commit"), "{output}");
}

#[test]
fn a_package_managed_install_is_refused_before_anything_happens() {
    for (path, manager, instead) in [
        (
            ".cargo/bin/git-vault",
            "cargo",
            "cargo uninstall git-vault-cli",
        ),
        (
            "nix/store/abc-git-vault-1.0.0/bin/git-vault",
            "Nix",
            "flake",
        ),
        (
            "opt/homebrew/Caskroom/git-vault/1.0.0/git-vault",
            "Homebrew",
            "brew uninstall --cask git-vault",
        ),
    ] {
        let install = installed_at(path);

        let (ok, output) = install.run(&["uninstall", "--yes"]);

        assert!(!ok, "{path}: {output}");
        assert!(output.contains(manager), "{path}: {output}");
        assert!(output.contains(instead), "{path}: {output}");
        assert!(install.exists(), "{path} was removed anyway");
    }
}

#[test]
fn updating_a_package_managed_install_is_refused_without_reaching_the_network() {
    let install = installed_at(".cargo/bin/git-vault");

    let (ok, output) = install.run(&["update"]);

    assert!(!ok, "{output}");
    assert!(
        output.contains("cargo install git-vault-cli --force"),
        "{output}"
    );
    assert!(
        !output.contains("release"),
        "it should refuse before looking one up: {output}"
    );
}

#[test]
fn a_symlink_does_not_hide_where_the_binary_really_lives() {
    let install = installed_at("nix/store/abc-git-vault-1.0.0/bin/git-vault");
    let link = install.dir.path().join("bin/git-vault");
    fs::create_dir_all(link.parent().unwrap()).unwrap();
    symlink(&install.binary, &link);

    let output = Command::new(&link)
        .args(["uninstall", "--yes"])
        .env("HOME", install.dir.path())
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Nix"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(install.exists());
}

#[cfg(unix)]
fn symlink(target: &Path, link: &Path) {
    std::os::unix::fs::symlink(target, link).unwrap();
}

#[cfg(windows)]
fn symlink(target: &Path, link: &Path) {
    fs::copy(target, link).unwrap();
}
