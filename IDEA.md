# git-vault

Transparent encryption for git that collapses everything it protects into a single opaque file, hiding names, paths and structure as well as contents.

Used like git-crypt. Locally your secrets are ordinary plaintext files and git behaves normally. In the repository they exist only as one encrypted blob that reveals nothing without the key.

## Goal

- Declare what is secret with gitattributes-style pattern rules.
- Merge everything secret, individual files at the repo root as well as whole directories buried deep in the tree, into **one** tracked file.
- Reveal nothing about file names, paths or structure to anyone without the key.
- Keep normal git working: `status`, `add`, `commit`, `push`, `checkout`, `merge`, `stash`, `rebase`.
- Keep commit hashes identical locally and on the remote.
- Ship as a single binary with no runtime dependencies.

## Why nothing existing does this

Every transparent-encryption tool for git hooks into clean/smudge filters, which git invokes **per blob**. A filter is a byte transformer bound to one path: git writes the index entry first, then asks the filter what bytes to store. The filter never sees the path as anything but information, and cannot rename it, remove it, or merge several paths into one.

The consequence is unavoidable. A rule like `secrets/** filter=x` produces N files and therefore N ciphertexts, with names, paths and sizes fully visible. git-crypt's author states it directly: filenames will never be encrypted with git-crypt, and the limitation is inherent to how git filters work.

| Tool | Approach | Hides names? |
| --- | --- | --- |
| agebox, git-agecrypt, git-conceal, git-secret, sops, transcrypt | per-file clean/smudge or CLI | no |
| git-crypt | per-file clean/smudge, AES-256-CTR | no |
| git-remote-gcrypt, git-remote-gitern, git-remote-encrypted, git-remote-grave | encrypted transport | yes, but the whole repo, and the remote is unusable for browsing, review or CI |
| zackiles/git-vault, yadm encrypt, gitstore | archive per item, manifest in the clear | no |
| CryFS, Cryptomator, gocryptfs | encrypted filesystems | CryFS yes, but it is not a git tool |

Nothing does selective sealing with a normal, browsable remote.

## The mechanism

Secret files are **plaintext on disk and ignored by git**. `.vault` is a normal tracked file that carries all of them, encrypted, and it is the only thing git knows about.

What makes this work without losing `git status` is a quirk of git's own laziness:

- The `.vault` file **in the worktree** is a 64-byte human-readable marker.
- The `.vault` blob **in git** is the real ciphertext, always larger than 250 bytes.
- Git decides whether to re-read a file by comparing its on-disk size against the size recorded in the index. Those two can never match, so git's skip-it shortcut never fires.
- Git therefore re-reads `.vault` on every operation, which means running the clean filter, which rebuilds the vault from the live secret files.

So `git status` reports the truth about secrets as a side effect of git trying to save itself work. No hooks, no daemon, no wrapper.

```
$ vim secrets/prod.env

$ git status
        modified:   src/main.rs
        modified:   .vault          <- the secret edit surfaced here

$ git vault status
     M  secrets/prod.env

$ git commit -am "rotate key"       <- plain git, no hook
```

Verified behaviour: clean status when nothing changed, `M .vault` when a secret changed, `git commit -a` stages it, and `rm -rf secrets .vault && git checkout -- .vault` restores everything.

## What the remote sees

```
my-project/
  README.md
  src/main.rs
  .gitattributes
  .gitignore
  .vault            binary, unreadable
```

No `secrets/` directory. Not the file names inside it, not how many there are, not their sizes, not their contents.

Everything else about the repository stays normal: browsable, searchable, pull requests, CI, releases, blame. Commit hashes match what you see locally, because there is only one history. Signed commits keep working.

What still leaks: that a vault exists, its rough size, which commits touched it, and the recipient list.

## The `.vault` format

```
+- header  (plaintext, opaque, copied verbatim on every seal) -+
|  magic "GITVAULT" + format version                           |
|  age stanza: vault key wrapped for recipient 1               |
|  age stanza: vault key wrapped for recipient 2               |
|  ...                                                         |
+- body  (deterministic, one independent entry per secret) ----+
|  entry: id     = BLAKE3-keyed(id_key, path)   16 bytes       |
|         sealed = DAE(content_key, path|mode|bytes)           |
|  ... sorted by id, padded to size classes                    |
+--------------------------------------------------------------+
```

Three properties this buys:

- **Deterministic.** The same secrets always produce byte-identical output. Without this, `.vault` would show as modified forever and the tool would be unusable.
- **Per-entry.** Changing one secret changes one small region, so git deltas the history normally instead of storing a full copy of the vault in every commit.
- **Order-blind.** Entries are sorted by keyed hash, so their position reveals nothing about their names.

### The header trap

age is deliberately non-deterministic, using a fresh ephemeral key per encryption. Regenerating the header on every seal would make `.vault` permanently dirty. This is the most likely way to get the design wrong.

The clean filter therefore never generates a header. It reads the existing header verbatim out of the `.vault` blob already in the index and copies the bytes through. Only `init`, `revoke`, `rotate` and `share` write a new header, and those write `.vault` directly.

### Crypto

- **Key wrapping**: age, with SSH ed25519/RSA recipients and native `age1...` keys.
- **Content**: XChaCha20-Poly1305 with the nonce derived as a keyed BLAKE3 hash over the entry's path, mode and content. Deterministic and authenticated.
- **Key hygiene**: the vault key lives in `secrecy::Secret` and is wiped on drop via `zeroize`.

## The filter contract

This is the whole tool. Everything else is convenience around it.

**clean**, answering "what should `.vault` contain?"

1. Read the header verbatim from the index copy of `.vault`.
2. Resolve the sealed patterns from `.gitattributes` using git's real matching rules.
3. Seal each matching file deterministically.
4. Emit header + body. Ignore stdin entirely.

**smudge**, applying a `.vault` blob to the worktree

1. Decrypt.
2. **Reconcile, do not extract.** Write changed files, restore modes, and delete secrets that no longer exist in this commit.
3. Refuse if a secret has uncommitted edits, the way git refuses to check out over dirty files.
4. Write the marker file to `.vault`.

**textconv**, for `git diff`, `git log -p`, `git show`

Render the vault as a canonical text listing so git diffs it natively and produces real per-file plaintext diffs. `diff.vault.cachetextconv` must be forced off, otherwise git caches decrypted secrets in `.git/objects/info/cache` where they survive `git vault lock`.

**merge**

- With the key: decrypt all three sides, three-way merge per path, re-seal. Conflicts only on genuinely conflicting files.
- Without the key: structural merge by entry id. Anything added, removed or changed on one side only resolves cleanly. Only the same entry changed on both sides is a real conflict.

## Locked and unlocked

| State | Worktree `.vault` | Secrets on disk | Filter |
| --- | --- | --- | --- |
| Unlocked | 64-byte marker | present, plaintext | active |
| Locked, no key, or tool absent | the real ciphertext | absent | inactive |

When locked, `.vault` on disk is byte-identical to the blob, so git's shortcut does fire and the filter is never called. A keyless clone lands in this state naturally with no configuration, which gives graceful degradation for free.

Safety rule: if the filter is invoked while locked it emits the index blob unchanged. Never stdin, which would be the marker, and which would destroy the vault.

## Declaring secrets

Two ordinary committed files:

```
.gitattributes
    .vault       filter=vault diff=vault merge=vault
    secrets/**   vault
    *.key        vault

.gitignore
    secrets/
    *.key
```

They are the only source of truth, so hand-editing them works. `git vault add` and `git vault remove` are shortcuts that also do three things hand-editing would not:

- Untrack a path git is already tracking, so plaintext stops being committed.
- Refuse patterns that would seal `.gitattributes`, `.gitignore` or `.vault` itself.
- Warn that sealing something today does not unsay what was pushed yesterday.

Committing the patterns publishes the *label on the box*, meaning a folder called `secrets` exists. It does not publish the inventory: the file names inside, their count, their sizes and their contents all stay hidden. A generic pattern such as `*.sealed` leaks nothing at all.

Because the tool resolves the patterns itself rather than relying on git's filter machinery, a bare `secrets/` works, unlike git-crypt which requires `secrets/**`.

## Keys and access

One vault key encrypts everything. For each recipient, a copy of that key is wrapped so only their private key opens it, and the wrapped copies live in the `.vault` header.

```
$ git vault share ~/Downloads/alice.pub
$ git vault share "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI..."
```

Public keys are text. The tool never touches the network, so it works the same on GitHub, GitLab, Gitea, or a bare repo on a NAS.

`git vault revoke` removes a recipient and rotates the vault key, but cannot retract history. Anyone who had access can still read every commit made while they had it. If a secret was seen, change the secret.

The recipient list reveals how many people have access, and with SSH recipients a short fingerprint identifies which. git-crypt leaks the same thing through GPG fingerprints. Native age keys leave no fingerprint at all.

## Commands

```
git vault init                    generate key, create .vault, wire .git/config
git vault unlock [--key-file F]   materialise secrets on this machine
git vault lock                    remove plaintext and local key

git vault add <path>...           edit .gitattributes/.gitignore, untrack, warn
git vault remove <path>...        reverse
git vault ls                      what is sealed

git vault status                  which secrets changed
git vault diff [<path>...]        plaintext diff
git vault log [<path>]            history of a secret
git vault restore [<path>...]     discard local secret edits

git vault share <key|file>        wrap the vault key for a recipient
git vault revoke <key|name>       remove and rotate
git vault keys                    who has access
git vault rotate                  new vault key, re-seal

git vault export-key <file>       for CI
git vault doctor                  verify wiring, non-zero exit on problems

git vault filter <clean|smudge|textconv|merge>    internal, called by git
```

`git vault doctor` belongs in CI. It fails if a sealed path is tracked in plaintext, if the two declaration files disagree, or if `.vault` is stale.

Deliberately not wrapped: `git add`, `git checkout`, `git commit`, `git log`, `git merge`, `git pull`, `git push`, `git stash`, `git status`.

## Implementation

Rust, chosen for four reasons specific to this tool:

- **Startup latency.** The clean filter runs on every `git status`, `git diff`, `git add` and `git commit`, and `git status` fires on every prompt render for anyone using starship or powerlevel10k. Measured on the target machine: Rust binaries start at roughly 1 ms against a 0.75 ms fork/exec baseline, while typical Go CLIs land at 11-22 ms and Bun at 13 ms. Git's long-running filter protocol does not help, because there is exactly one sealed file and therefore exactly one call per git command. Raw startup is the whole cost.
- **Key hygiene.** Go cannot reliably erase memory, since the garbage collector may copy values. For a tool whose job is holding a decryption key, `zeroize` plus move semantics is the right property.
- **Deterministic encryption** is available off the shelf.
- **gitoxide** implements git's real attribute and exclude matching natively, so patterns behave exactly as git would resolve them instead of being approximated, and without extra process spawns on the hot path.

```
git-vault/
  src/
    main.rs          argv dispatch, fast path for `filter` before clap loads
    cli.rs           clap definitions
    vault/
      format.rs      binary layout, encode and decode
      seal.rs        deterministic entry encryption
      keys.rs        age wrapping, recipients, rotation
    repo/
      wiring.rs      .git/config, filter/diff/merge registration
      patterns.rs    .gitattributes and .gitignore via gix
      worktree.rs    reconcile, modes, atomic writes, dirty checks
    filter/
      clean.rs  smudge.rs  textconv.rs  merge.rs
    cmd/             one module per command
  tests/             integration tests against real repositories
```

| Crate | Purpose |
| --- | --- |
| `age` | key wrapping, SSH and native recipients |
| `anyhow`, `thiserror` | errors |
| `blake3` | entry ids, nonce derivation |
| `chacha20poly1305` | content encryption |
| `clap` | CLI |
| `diffy` | three-way merge |
| `gix` | index, attributes, exclude, object access |
| `secrecy`, `zeroize` | key handling |
| `tempfile`, `rustix` | atomic writes, modes |

Startup budget: under 2 ms end to end for a small vault, benchmarked in CI with the build failing on regression. `main.rs` dispatches `filter` before clap initialises.

## Milestones

**M0, prototype in bash.** Pin down the binary format and the exact filter behaviour, which is where the surprises live.

**M1, the core.** Format, deterministic sealing, clean and smudge, `init`, `unlock`, `lock`.

**M2, usable.** `add`, `remove`, `ls`, `status`, `restore`, `doctor`. Usable solo at this point.

**M3, people.** `share`, `revoke`, `keys`, `rotate`, `export-key`. Usable by a team.

**M4, integration.** textconv diff and the merge driver. The merge driver is the largest single piece and decides whether this is a toy or a tool.

**M5, hardening.** Reconciliation edge cases covering deletions, renames, modes and symlinks. Dirty-check refusals, a local journal so `git clean -xdf` is not fatal, Windows.

**M6, shipping.** Static musl, macOS and Windows binaries. Nix flake, Homebrew, cargo, man page, shell completions.

## Tests that must exist

- **Determinism**: seal the same tree a hundred times, assert byte-identical output every time.
- **The status contract**: no edit gives a clean `git status`, one secret edited gives exactly `M .vault`.
- **Round trip**: `rm -rf secrets .vault && git checkout` restores everything including modes.
- **Reconciliation**: switching to a branch where a secret was deleted removes it from disk.
- **Keyless passthrough**: clone without the tool, commit, push, and the `.vault` blob comes back byte-identical.
- **Dirty refusal**: an uncommitted secret edit plus `git checkout other-branch` refuses rather than overwriting.
- **Plumbing**: `cherry-pick`, `rebase`, `reset --hard`, `stash`, `worktree add`.
- **Merge**: both sides add different secrets, both edit the same secret, and keyless structural merge.
- **Leak guard**: `doctor` catches a sealed path committed in plaintext.

## Risks, ranked

1. **Non-deterministic output.** Any drift leaves `.vault` permanently dirty and the tool unusable. Mitigated by the header-copy rule and a brutal determinism suite.
2. **The locked/unlocked invariant.** Getting it wrong writes the marker over the ciphertext and destroys the vault. Needs explicit state detection and a refuse-by-default posture.
3. **The merge driver.** The largest chunk of work.
4. **Uncommitted secret edits.** They live only in ignored files, so `git clean -xdf` destroys them. Needs a journal and dirty checks. This is the one failure mode git-crypt does not have.
5. **Startup regression.** A dependency adding 15 ms makes every prompt sluggish. CI benchmark gate.
6. **Windows.** Modes, symlinks, atomic rename, line endings.
7. **Plaintext already in history.** `add` warns, but cannot unsay a past push.
8. **Large files.** The whole vault is processed in memory. Soft limit with a warning.

## Open decisions

**Partial staging.** All secret changes currently commit together, because they live in one file. Adding `git vault stage <path>` means maintaining a staging list and having the clean filter mix worktree state for staged paths with committed state for the rest. It is the only piece that adds real complexity. Ship without it, and add it in M5 if it turns out to bite.

**Name.** `git-vault` collides with three small existing projects (`zackiles/git-vault`, `aheissenberger/gitvault`, `DraconDev/git-seal`), none with traction, and with HashiCorp Vault in the ecosystem's vocabulary. `git-sealed`, `git-lockbox` and `git-strongbox` are unused if the collision matters later.
