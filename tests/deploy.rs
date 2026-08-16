#![allow(clippy::panic, clippy::unwrap_used)]

mod harness;

use std::fs;
use std::path::{Path, PathBuf};

use harness::{BIN, Repo};
use tempfile::TempDir;

const SSH_KEY: &str = "-----BEGIN OPENSSH PRIVATE KEY-----
b3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZW
QyNTUxOQAAACDNpyjcbwpSmWq9H/V1dfb50CcABgec/0V9HLunUS39dQAAAJgfqB7rH6ge
6wAAAAtzc2gtZWQyNTUxOQAAACDNpyjcbwpSmWq9H/V1dfb50CcABgec/0V9HLunUS39dQ
AAAEAWryDLluZObv9al2OsnfGkU1oxCYMxkGq8Z2UdpO1qNM2nKNxvClKZar0f9XV19vnQ
JwAGB5z/RX0cu6dRLf11AAAAEHRlc3RAZXhhbXBsZS5jb20BAgMEBQ==
-----END OPENSSH PRIVATE KEY-----
";

const SSH_PUB: &str = "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIM2nKNxvClKZar0f9XV19vnQJwAGB5z/RX0cu6dRLf11 test@example.com\n";

struct Machine {
    dir: TempDir,
}

impl Machine {
    fn new() -> Self {
        Self {
            dir: tempfile::Builder::new()
                .prefix("git-vault-machine-")
                .tempdir()
                .unwrap(),
        }
    }

    fn root(&self) -> PathBuf {
        fs::canonicalize(self.dir.path()).unwrap()
    }

    fn path(&self, rel: &str) -> PathBuf {
        self.root().join(rel)
    }

    fn write(&self, rel: &str, contents: &str) -> PathBuf {
        let path = self.path(rel);
        fs::write(&path, contents).unwrap();
        path
    }
}

fn with_secrets() -> Repo {
    let repo = Repo::sealed(&["secrets/"]);
    repo.write("secrets/prod.env", "STRIPE=live\n");
    repo.write("secrets/hosts/homelab.json", "{\"token\":\"homelab\"}\n");
    repo.commit_all("add secrets");
    repo
}

fn copy_vault(repo: &Repo, machine: &Machine) -> (PathBuf, PathBuf) {
    let data = machine.path("data");
    let keys = machine.path("keys");
    fs::copy(repo.root().join(".vault/data"), &data).unwrap();
    fs::copy(repo.root().join(".vault/keys"), &keys).unwrap();
    (data, keys)
}

fn unseal(repo: &Repo, at: &Path, args: &[&str]) -> harness::Run {
    repo.run_in(at, BIN, &[&["unseal"], args].concat())
}

#[test]
fn a_vault_opens_on_a_machine_with_no_repository_and_no_key_of_its_own() {
    let repo = with_secrets();
    let key_file = repo.root().join("../ci.key");
    repo.vault(&["export-key", key_file.to_str().unwrap()]).ok();

    let machine = Machine::new();
    let (data, _keys) = copy_vault(&repo, &machine);
    let carried = machine.path("ci.key");
    fs::copy(&key_file, &carried).unwrap();
    let into = machine.path("run");

    let run = unseal(
        &repo,
        &machine.root(),
        &[
            "--data",
            data.to_str().unwrap(),
            "--key-file",
            carried.to_str().unwrap(),
            "--into",
            into.to_str().unwrap(),
        ],
    );

    assert!(run.succeeded(), "{}", run.stderr());
    assert!(
        run.stdout().contains("Unsealed 2 secrets"),
        "{}",
        run.stdout()
    );
    assert_eq!(
        fs::read(into.join("secrets/prod.env")).unwrap(),
        b"STRIPE=live\n"
    );
    assert_eq!(
        fs::read(into.join("secrets/hosts/homelab.json")).unwrap(),
        b"{\"token\":\"homelab\"}\n"
    );
}

#[test]
fn a_host_opens_the_vault_with_its_own_ssh_key() {
    let repo = with_secrets();
    let machine = Machine::new();
    let private = machine.write("host_key", SSH_KEY);
    let public = machine.write("host_key.pub", SSH_PUB);

    repo.vault(&["share", public.to_str().unwrap(), "--label", "homelab"])
        .ok();
    repo.git(&["add", "--", ".vault"]).ok();
    repo.commit("give the host access");

    let (data, keys) = copy_vault(&repo, &machine);
    let into = machine.path("run");

    let run = unseal(
        &repo,
        &machine.root(),
        &[
            "--data",
            data.to_str().unwrap(),
            "--keys",
            keys.to_str().unwrap(),
            "--identity",
            private.to_str().unwrap(),
            "--into",
            into.to_str().unwrap(),
        ],
    );

    assert!(run.succeeded(), "{}", run.stderr());
    assert_eq!(
        fs::read(into.join("secrets/prod.env")).unwrap(),
        b"STRIPE=live\n"
    );
}

#[test]
fn a_host_can_be_given_only_the_secrets_it_needs() {
    let repo = with_secrets();
    let key_file = repo.root().join("../ci.key");
    repo.vault(&["export-key", key_file.to_str().unwrap()]).ok();

    let machine = Machine::new();
    let (data, _keys) = copy_vault(&repo, &machine);
    let carried = machine.path("ci.key");
    fs::copy(&key_file, &carried).unwrap();
    let into = machine.path("run");

    let run = unseal(
        &repo,
        &machine.root(),
        &[
            "--data",
            data.to_str().unwrap(),
            "--key-file",
            carried.to_str().unwrap(),
            "--entries",
            "secrets/hosts/**",
            "--into",
            into.to_str().unwrap(),
        ],
    );

    assert!(run.succeeded(), "{}", run.stderr());
    assert!(into.join("secrets/hosts/homelab.json").exists());
    assert!(
        !into.join("secrets/prod.env").exists(),
        "a host received a secret it did not ask for"
    );
}

#[test]
fn it_says_nothing_about_the_names_unless_asked() {
    let repo = with_secrets();
    let key_file = repo.root().join("../ci.key");
    repo.vault(&["export-key", key_file.to_str().unwrap()]).ok();

    let machine = Machine::new();
    let (data, _keys) = copy_vault(&repo, &machine);
    let carried = machine.path("ci.key");
    fs::copy(&key_file, &carried).unwrap();
    let into = machine.path("run");

    let quiet = unseal(
        &repo,
        &machine.root(),
        &[
            "--data",
            data.to_str().unwrap(),
            "--key-file",
            carried.to_str().unwrap(),
            "--into",
            into.to_str().unwrap(),
        ],
    );
    assert!(
        !quiet.stdout().contains("prod.env"),
        "a secret's name reached the log: {}",
        quiet.stdout()
    );

    let loud = unseal(
        &repo,
        &machine.root(),
        &[
            "--data",
            data.to_str().unwrap(),
            "--key-file",
            carried.to_str().unwrap(),
            "--into",
            into.to_str().unwrap(),
            "--verbose",
        ],
    );
    assert!(
        loud.stdout().contains("secrets/prod.env"),
        "{}",
        loud.stdout()
    );
}

#[test]
fn outside_a_repository_it_says_which_file_to_name() {
    let repo = Repo::bare_new();
    let machine = Machine::new();

    let stderr = unseal(
        &repo,
        &machine.root(),
        &["--into", machine.path("run").to_str().unwrap()],
    )
    .failed();

    assert!(stderr.contains("--data"), "{stderr}");
}

#[test]
fn a_key_that_cannot_open_it_says_how_to_be_given_access() {
    let repo = with_secrets();
    let machine = Machine::new();
    let private = machine.write("host_key", SSH_KEY);
    let (data, keys) = copy_vault(&repo, &machine);

    let stderr = unseal(
        &repo,
        &machine.root(),
        &[
            "--data",
            data.to_str().unwrap(),
            "--keys",
            keys.to_str().unwrap(),
            "--identity",
            private.to_str().unwrap(),
            "--into",
            machine.path("run").to_str().unwrap(),
        ],
    )
    .failed();

    assert!(stderr.contains("cannot open the vault"), "{stderr}");
    assert!(stderr.contains("ssh-keygen -y"), "{stderr}");
}

#[cfg(unix)]
#[test]
fn what_it_writes_is_not_readable_by_everyone() {
    use std::os::unix::fs::PermissionsExt as _;

    let repo = with_secrets();
    let key_file = repo.root().join("../ci.key");
    repo.vault(&["export-key", key_file.to_str().unwrap()]).ok();

    let machine = Machine::new();
    let (data, _keys) = copy_vault(&repo, &machine);
    let carried = machine.path("ci.key");
    fs::copy(&key_file, &carried).unwrap();
    let into = machine.path("run");

    unseal(
        &repo,
        &machine.root(),
        &[
            "--data",
            data.to_str().unwrap(),
            "--key-file",
            carried.to_str().unwrap(),
            "--mode",
            "0400",
            "--into",
            into.to_str().unwrap(),
        ],
    )
    .ok();

    let directory = fs::metadata(&into).unwrap().permissions().mode();
    assert_eq!(directory & 0o777, 0o700, "the directory is not private");

    let secret = fs::metadata(into.join("secrets/prod.env"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(secret & 0o777, 0o400, "the secret is not private");
}

#[test]
fn an_ssh_key_keeps_the_label_it_was_shared_under() {
    let repo = with_secrets();
    let machine = Machine::new();
    let public = machine.write("host_key.pub", SSH_PUB);

    repo.vault(&["share", public.to_str().unwrap(), "--label", "homelab"])
        .ok();

    let listed = repo.vault(&["keys"]).ok();
    let ssh_line = listed
        .lines()
        .find(|line| line.starts_with("ssh-"))
        .unwrap_or_default();

    assert!(
        ssh_line.ends_with("homelab"),
        "the key's own comment replaced the label: {listed}"
    );
    assert!(
        !ssh_line.contains("test@example.com"),
        "the key's own comment is being shown as a label: {listed}"
    );

    let revoked = repo.vault(&["revoke", "homelab"]);
    assert!(revoked.succeeded(), "{}", revoked.stderr());
}
