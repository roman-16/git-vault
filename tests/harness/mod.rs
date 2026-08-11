#![allow(clippy::panic, clippy::unwrap_used, dead_code)]

use std::fmt::Write as _;
use std::fs;
use std::iter;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use tempfile::TempDir;

pub const BIN: &str = env!("CARGO_BIN_EXE_git-vault");

pub fn git_binary() -> String {
    std::env::var("GIT_VAULT_TEST_GIT").unwrap_or_else(|_| "git".to_owned())
}

pub fn size_of(bytes: &[u8]) -> u64 {
    u64::try_from(bytes.len()).unwrap()
}

pub struct Run {
    pub what: String,
    pub output: Output,
}

impl Run {
    pub fn stdout(&self) -> String {
        String::from_utf8_lossy(&self.output.stdout).into_owned()
    }

    pub fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.output.stderr).into_owned()
    }

    pub fn code(&self) -> i32 {
        self.output.status.code().unwrap_or(-1)
    }

    pub fn succeeded(&self) -> bool {
        self.output.status.success()
    }

    pub fn ok(&self) -> String {
        assert!(
            self.succeeded(),
            "{} failed with {}\nstdout: {}\nstderr: {}",
            self.what,
            self.code(),
            self.stdout(),
            self.stderr(),
        );
        self.stdout().trim_end().to_owned()
    }

    pub fn failed(&self) -> String {
        assert!(
            !self.succeeded(),
            "{} unexpectedly succeeded\nstdout: {}",
            self.what,
            self.stdout(),
        );
        self.stderr()
    }
}

pub struct Repo {
    bin_dir: PathBuf,
    dir: TempDir,
    identity: PathBuf,
    no_config: PathBuf,
    root: PathBuf,
}

impl Repo {
    fn scaffold() -> Self {
        let dir = tempfile::Builder::new()
            .prefix("git-vault-test-")
            .tempdir()
            .unwrap();
        let base = fs::canonicalize(dir.path()).unwrap();

        let bin_dir = base.join("bin");
        fs::create_dir(&bin_dir).unwrap();
        place_binary(&bin_dir);

        let root = base.join("work");
        fs::create_dir(&root).unwrap();

        let no_config = base.join("empty-gitconfig");
        fs::write(&no_config, "").unwrap();

        Self {
            bin_dir,
            dir,
            identity: base.join("identity"),
            no_config,
            root,
        }
    }

    pub fn bare_new() -> Self {
        let repo = Self::scaffold();
        repo.git(&["init", "--quiet", "--initial-branch", "main"])
            .ok();
        repo
    }

    pub fn sealed(declarations: &[&str]) -> Self {
        let repo = Self::bare_new();
        repo.vault(&["init"]).ok();
        repo.declare(declarations);
        repo.write("README.md", "# project\n");
        repo.git(&["add", "--all"]).ok();
        repo.commit("add a vault");
        repo
    }

    pub fn declare(&self, patterns: &[&str]) {
        let attributes = self.root.join(".gitattributes");
        let mut text = fs::read_to_string(&attributes).unwrap_or_default();
        let mut ignores = fs::read_to_string(self.root.join(".gitignore")).unwrap_or_default();

        for pattern in patterns {
            let declared = pattern.strip_suffix('/').map_or_else(
                || (*pattern).to_owned(),
                |directory| format!("{directory}/**"),
            );
            writeln!(text, "{declared} vault filter=vault-plaintext").unwrap();
            writeln!(ignores, "{declared}").unwrap();
        }

        fs::write(attributes, text).unwrap();
        fs::write(self.root.join(".gitignore"), ignores).unwrap();
    }

    pub fn clone_of(source: &Self) -> Self {
        let clone = Self::scaffold();
        let source_path = source.root.to_str().unwrap().to_owned();
        clone
            .run_in(
                &clone.root.clone(),
                &git_binary(),
                &["clone", "--quiet", &source_path, "."],
            )
            .ok();
        fs::copy(&source.identity, &clone.identity).unwrap();
        clone
    }

    pub fn keyless_clone_of(source: &Self) -> Self {
        let clone = Self::clone_of(source);
        fs::remove_file(&clone.identity).unwrap();
        clone
    }

    pub fn stranger_clone_of(source: &Self) -> Self {
        let clone = Self::clone_of(source);
        fs::remove_file(&clone.identity).unwrap();
        let _refused = clone.vault(&["unlock"]);
        clone
    }

    pub fn public_key(&self) -> String {
        let contents = fs::read_to_string(&self.identity).unwrap();
        contents
            .lines()
            .find_map(|line| line.trim().strip_prefix("# public key: "))
            .unwrap()
            .to_owned()
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn modified(&self, rel: &str) -> std::time::SystemTime {
        fs::metadata(self.root.join(rel))
            .unwrap()
            .modified()
            .unwrap()
    }

    pub fn git(&self, args: &[&str]) -> Run {
        self.run(&git_binary(), args)
    }

    pub fn vault(&self, args: &[&str]) -> Run {
        self.run(BIN, args)
    }

    pub fn git_vault(&self, args: &[&str]) -> Run {
        let mut dispatched = vec!["vault"];
        dispatched.extend_from_slice(args);
        self.git(&dispatched)
    }

    fn run(&self, program: &str, args: &[&str]) -> Run {
        self.run_in(&self.root, program, args)
    }

    pub fn run_in(&self, cwd: &Path, program: &str, args: &[&str]) -> Run {
        let output = self
            .command(program, cwd)
            .args(args)
            .output()
            .unwrap_or_else(|error| panic!("cannot run {program}: {error}"));

        Run {
            what: format!("`{program} {}`", args.join(" ")),
            output,
        }
    }

    fn command(&self, program: &str, cwd: &Path) -> Command {
        let path = std::env::var_os("PATH").map_or_else(
            || self.bin_dir.clone().into_os_string(),
            |existing| {
                let entries =
                    iter::once(self.bin_dir.clone()).chain(std::env::split_paths(&existing));
                std::env::join_paths(entries).unwrap()
            },
        );

        let mut command = Command::new(program);
        command
            .current_dir(cwd)
            .env("PATH", path)
            .env("GIT_VAULT_IDENTITY", &self.identity)
            .env("GIT_CONFIG_GLOBAL", &self.no_config)
            .env("GIT_CONFIG_SYSTEM", &self.no_config)
            .env("GIT_AUTHOR_NAME", "Vault Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_AUTHOR_DATE", "2026-01-15T10:00:00+01:00")
            .env("GIT_COMMITTER_NAME", "Vault Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_DATE", "2026-01-15T10:00:00+01:00")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env_remove("GIT_DIR")
            .env_remove("GIT_WORK_TREE")
            .env_remove("GIT_INDEX_FILE")
            .env_remove("GIT_PREFIX");
        command
    }

    pub fn write(&self, rel: &str, contents: &str) {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    pub fn write_bytes(&self, rel: &str, contents: &[u8]) {
        let path = self.root.join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, contents).unwrap();
    }

    pub fn read(&self, rel: &str) -> Vec<u8> {
        fs::read(self.root.join(rel)).unwrap()
    }

    pub fn exists(&self, rel: &str) -> bool {
        self.root.join(rel).exists()
    }

    pub fn remove(&self, rel: &str) {
        let path = self.root.join(rel);
        if path.is_dir() {
            fs::remove_dir_all(path).unwrap();
        } else {
            fs::remove_file(path).unwrap();
        }
    }

    pub fn disk_size(&self, rel: &str) -> u64 {
        fs::metadata(self.root.join(rel)).unwrap().len()
    }

    pub fn index_size(&self, rel: &str) -> u64 {
        let debug = self.git(&["ls-files", "--debug", "--", rel]).ok();
        debug
            .lines()
            .filter_map(|line| line.trim().strip_prefix("size:"))
            .find_map(|rest| rest.split_whitespace().next()?.parse::<u64>().ok())
            .unwrap_or_else(|| panic!("no size recorded for `{rel}`:\n{debug}"))
    }

    pub fn blob_size(&self, rel: &str) -> u64 {
        let spec = format!(":{rel}");
        self.git(&["cat-file", "-s", &spec])
            .ok()
            .parse()
            .unwrap_or_else(|error| panic!("cannot read the blob size of `{rel}`: {error}"))
    }

    pub fn blob_id(&self, rev: &str) -> String {
        self.git(&["rev-parse", rev]).ok()
    }

    pub fn status(&self) -> String {
        self.git(&["status", "--porcelain"]).ok()
    }

    pub fn commit(&self, message: &str) {
        self.git(&["commit", "--quiet", "--message", message]).ok();
    }

    pub fn commit_all(&self, message: &str) {
        self.git(&["commit", "--quiet", "--all", "--message", message])
            .ok();
    }

    pub fn add(&self, rel: &str) {
        self.git(&["add", "--", rel]).ok();
    }

    pub fn keep(&self) -> &Path {
        self.dir.path()
    }
}

#[cfg(unix)]
fn place_binary(directory: &Path) {
    std::os::unix::fs::symlink(BIN, directory.join("git-vault")).unwrap();
}

#[cfg(windows)]
fn place_binary(directory: &Path) {
    fs::copy(BIN, directory.join("git-vault.exe")).unwrap();
}
