# Test Guidelines

## Overview

Three layers, and the first two are the ones to reach for:

- **Unit** tests live beside the code they test, in a `#[cfg(test)] mod tests`.
- **Mechanism** tests in `mechanism.rs` pin what the design promises: that `git status` reports an edited secret, and that every git command still behaves normally.
- **Conformance** tests in `conformance.rs` pin the filter contract and what happens in a clone that does not have the tool.
- **Merge** tests in `merge.rs` cover the merge driver, with and without the key. They are what says whether this is a tool or a toy.
- **History** tests in `history.rs` cover the plaintext diffs git produces through textconv, and the per-secret `diff` and `log`.
- **Hardening** tests in `hardening.rs` cover the ways a worktree gets into trouble: a wiped worktree, a rename, an empty directory left behind, somebody else's fsmonitor, and a path that is not a file.
- **Access** tests in `access.rs` cover sharing, revoking and rotating, using two clones with different identities.
- **Output** tests in `output.rs` pin what every command prints, byte for byte, with `insta`. This output is what somebody reads while something has gone wrong, so the wording is part of the interface: change it, run `just golden`, and the diff is the review.
- **CLI** tests in `cli.rs` cover the command surface, including what git's external subcommand dispatch changes about it.

```bash
just test                        # everything
just test-one merging_a_branch   # one test, or a regex of names
just golden                      # accept new output snapshots
just test-git <nixpkgs-rev>      # the whole suite against another git version
just bench                       # the hook that runs on every git command
```

Nothing needs credentials, a network, or more than two seconds.

## The thing these tests exist to protect

`.vault/data` on disk always holds exactly what git stored, so git treats it as an ordinary file and never suspects it of holding unsaved changes. Nothing detects a secret edit at the moment it happens, because nothing of ours is running then. Git hands us three moments instead:

| Moment | What we do | Why it matters |
| --- | --- | --- |
| `core.fsmonitor`, asked "what changed?" at the start of every command that reads the index | seal the live secrets, then answer "check everything" | this is what lets plain `git status` report an edited secret |
| `pre-commit` | seal again, and report it when the commit will not carry the result | fsmonitor is best-effort and git carries on if it fails, but the hook must never change what you chose to commit |
| `smudge`, whenever git writes `.vault/data` | decrypt and reconcile the secrets, pass the bytes through unchanged | this is the reverse direction, and it is why a checkout materialises secrets |

The tempting shortcut is to keep the worktree copy **empty**, so git re-reads it on every command and needs no hooks to notice a secret edit. It is fatal. An empty worktree copy against a non-empty stored blob is indistinguishable, to git, from *"the user has unsaved work here"*, and git protects unsaved work by refusing to overwrite it: `merge`, `rebase` and `revert` all fail whenever the operation needs to change the vault. `merging_a_branch_that_changed_secrets_works`, `rebasing_across_a_secret_change_works` and `reverting_a_commit_that_changed_secrets_works` are what stand between that shortcut and the repository. They must never be deleted.

## The harness

`harness/mod.rs` builds a repository in a temporary directory with a scrubbed environment: no global or system git config, fixed author and committer identity and dates, no inherited `GIT_DIR`, and its own age identity so no test can reach the identity of whoever is running it.

| Helper | What it gives you |
| --- | --- |
| `Repo::bare_new()` | An initialised repository with no vault |
| `Repo::sealed(&["secrets/"])` | A repository with a vault, those declarations, and one commit, all done by the tool itself |
| `Repo::clone_of(&source)` | A clone carrying the same identity, so `git vault unlock` can open it |
| `Repo::keyless_clone_of(&source)` | A clone with no identity, which is what somebody without access has |
| `repo.git(&[…])` | Run git in the repository |
| `repo.vault(&[…])` | Run the binary as `git-vault …` |
| `repo.git_vault(&[…])` | Run the binary as `git vault …`, through git's dispatch |
| `repo.run_in(dir, …)` | Run anywhere, to test discovery from a subdirectory or outside a repository |
| `repo.disk_size(p)` / `blob_size(p)` / `blob_id(rev)` | Compare what is on disk against what git stored |
| `repo.modified(p)` | Prove that a no-op seal touches nothing |

A harness reports failure by panicking, which is the framework's error channel, so `harness/mod.rs` allows `clippy::panic` and `clippy::unwrap_used`. Test functions get the same allowance from `clippy.toml`. Everywhere else the strict lints apply, including in tests: no `as` casts, no bare integer arithmetic, no slicing.

## Things worth knowing before writing a test

- **`git checkout -- .vault/data` restores secrets only when something sealed first.** Reading the index runs the fsmonitor hook, which seals, which makes the file differ from the index, which is what gives checkout something to write and the smudge filter something to reconcile. That covers an edited or a single deleted secret. It cannot work on a wiped worktree, because the wipe guard refuses to seal and the file still matches the index: `git vault restore` is the only way back. `plain_git_restores_an_edited_secret`, `plain_git_restores_one_deleted_secret` and `plain_git_cannot_restore_a_wiped_worktree` pin all three.
- **Git sends a hook's stdout to stderr.** Anything the `pre-commit` hook prints arrives on `attempt.stderr()`, never `stdout()`, so assert there.
- **`git commit --all` uses a temporary index.** The pre-commit hook's staging lives in that index, so it is discarded if the commit then aborts, and it cannot rescue a commit git has already decided is empty. `without_fsmonitor_a_secret_only_commit_asks_to_be_repeated` pins exactly that.
- **The `pre-commit` hook never stages.** What a commit carries is what you staged, so `a_commit_of_named_files_leaves_secret_changes_out` and `a_commit_of_everything_carries_secret_changes` are a pair: breaking either one breaks the promise that `.vault/data` behaves like an ordinary file.
- **Nothing re-seals during a merge, rebase, cherry-pick, revert or bisect.** While one of those is unresolved the worktree is not the authority on what the secrets are, and sealing over it could discard the incoming side. `nothing_reseals_in_the_middle_of_a_merge` pins it.
- **Without a key, textconv still renders something that changes.** It lists each entry's id and nonce, both of which are already in the sealed file, so a keyless `git log -p` shows *which* entry a commit touched. A summary alone would be byte-identical on both sides of every diff, and git would show nothing at all.
- **Nothing seals a worktree with every secret missing.** That is what `git clean -xdf` looks like, and sealing it would record them all as deleted. `git vault seal --allow-empty` is the way to mean it, so a test that empties a vault has to say so.
- **A test name states the behaviour it pins**, so a failure reads as a sentence.
- **Keys are random per run**, so a snapshot must never contain one, and must not depend on the order of a list that is sorted by key. `keys_marks_which_recipient_is_you` normalises both away.
- **Labels come from `GIT_VAULT_LABEL`, then `GIT_AUTHOR_EMAIL`**, so output that mentions a recipient stays the same on every machine. The harness sets the address.
