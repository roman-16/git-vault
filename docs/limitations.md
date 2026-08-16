# Limitations

## Your build cannot read a sealed secret

Sealed secrets are untracked and ignored files. That is not an implementation detail, it is the whole mechanism: a file git does not track has no name for anyone to see. The cost is that **anything reading your repository through git sees nothing at all**, not even an unreadable placeholder:

| What you might be doing | What it sees |
| --- | --- |
| A Nix flake (`builtins.readFile ./secrets.json`) | the tracked tree only, so the file does not exist |
| `cargo package`, `npm pack` | the same, so the file is missing from the artifact |
| A Docker build context fed by `git archive` | the same |
| Bazel, or a CI job with a sparse or tracked-file-only checkout | the same |

This is the one axis where git-crypt is better, and it is worth knowing before you migrate rather than after. git-crypt tracks per-file ciphertext, so those tools are handed a file. Your build fails on its contents, but the file is there and the path resolves. git-vault hands them nothing.

The escape hatch people reach for first does not work either. Nix's `path:` bypasses git entirely and copies **everything**, including `.git` and every ignored build artifact, which on a real repository means hundreds of megabytes hashed into the store on every evaluation.

The shape that does work:

> Seal only what your build does not read. Unseal the rest at deploy time.

`git vault unseal` is the deploy-time half, and it needs no repository and no git. → [Deploying secrets](deploying.md)

If a build step genuinely has to read a secret, no transparent-encryption tool can help, because the build needs plaintext and the repository must not have it. That code belongs in a private repository instead.

## By design

**All secret changes commit together.** They live in one file, so there is no `git add secrets/one.env`. `git commit -a` takes every secret change at once, and staging one secret while leaving another unstaged is not possible.

**`git status` reports the vault, not the secret.** It can say that something changed, not what. `git vault status` and `git vault diff` answer that.

**`core.fsmonitor` is a single slot.** It is how `git status` learns that a secret changed. If watchman or another monitor already uses it, git-vault leaves it alone: commits stay correct through the pre-commit hook, but `git status` will not surface secret edits in that repository.

**Sealing is not a substitute for rewriting history.** Anything committed in plaintext before it was sealed is still in history.

**Revocation is not retroactive.** Somebody who had access can read every commit made while they had it. → [Threat model](threat-model.md)

## Practical

**`git clean -xdf` removes your secrets**, because they are ignored files, exactly as it removes any other ignored file. Committed secrets come back with `git vault restore`. Uncommitted edits are gone. git-vault refuses to seal a worktree where every secret has vanished, so the loss cannot be committed by accident, but it cannot bring the files back.

**The whole vault is held in memory** and sealed again on every git command. That is fine for configuration, keys and tokens. It is the wrong tool for large binaries: `doctor` warns past 16 MiB, and the honest advice is to keep big files out of the vault entirely.

**Unanchored patterns cost a full worktree walk** on every git command. `*.key` works but `config/**/*.key` is much cheaper. → [Declaring secrets](declaring-secrets.md#anchored-patterns-are-faster)

**No `ssh-agent`.** age does not support it, so a passphrase-protected SSH key is typed in once per `git vault unlock`, on each machine.

**Hardware keys are not supported.** No YubiKey, no PIV, no FIDO2, because age's own support for them is not there.

**Nested `.gitattributes` files are not read.** Patterns are taken from the one at the top of the worktree. A `vault` attribute in a subdirectory is ignored, silently, which is a gap worth closing.

## Platforms

**Linux and macOS** are the proven ones: CI runs the whole suite against git 2.30 through 2.55, and the released binaries are static, so any distribution works, Alpine included.

**Windows runs the suite in CI too**, but with two caveats that follow from the platform rather than from the code. File modes and symlinks are not preserved, exactly as git itself behaves there with `core.fileMode=false` and `core.symlinks=false`; a sealed symlink comes back as a regular file holding its target. And replacing a secret fails while another program holds that file open, because Windows locks open files where Unix does not.

Inside WSL, the Linux build applies and neither caveat does.
