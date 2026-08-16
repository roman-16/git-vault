# Comparison

## Why nothing else hides names

Every transparent-encryption tool for git hooks into clean and smudge filters, which git invokes **per file**. A filter is a byte transformer bound to one path: git decides the path first, then asks the filter what bytes to store. The filter never gets to rename the path, remove it, or fold several paths into one.

The consequence is unavoidable. A rule like `secrets/** filter=x` produces one encrypted file per secret, with names, paths and sizes all visible. git-crypt's author says so directly: filenames will never be encrypted with git-crypt, and the limitation is inherent to how git filters work.

git-vault takes a different route. The secrets are **not tracked by git at all**; they are ordinary ignored files. One tracked file holds them, and git-vault keeps it in step at the three moments git offers a program to run: the fsmonitor hook, the pre-commit hook, and the smudge filter. → [How it works](how-it-works.md)

## The landscape

| Tool | Approach | Hides names? | Remote still browsable? |
| --- | --- | --- | --- |
| **git-vault** | one sealed file, kept in step by hooks | **yes** | **yes** |
| git-crypt | per-file clean/smudge, AES-256-CTR | no | yes |
| transcrypt | per-file clean/smudge, OpenSSL | no | yes |
| agebox, git-agecrypt | per-file, age | no | yes |
| sops | per-value, edit through the tool | no, and structure is visible | yes |
| git-secret | per-file, gpg, encrypted copies committed | no | yes |
| git-remote-gcrypt | encrypted transport | yes, but the whole repository | no |
| CryFS, gocryptfs | encrypted filesystem | yes | not a git tool |

## Against git-crypt

The closest comparison, and the one worth being precise about.

| | git-crypt | git-vault |
| --- | --- | --- |
| Names and paths of secrets | visible | hidden |
| Number and size of secrets | visible | count and rough size only |
| Files in the repository | one per secret | one, plus the key envelope |
| `git status` after editing a secret | names the file | names the vault |
| Which secret changed | `git status` | `git vault status` |
| Diffs | per file, plaintext | per secret, plaintext |
| Merges | git's own, per file | per secret, with a text merge inside one |
| Staging one secret at a time | yes | no, they commit together |
| Key sharing | GPG, or a symmetric key file | age, native keys or existing SSH keys |
| Clone without the key | works, files are opaque | works, the vault is opaque |
| Extra machinery per clone | `.git/config` | `.git/config` and a pre-commit hook |
| **Visible to anything reading the repo through git** | **yes, as ciphertext** | **no** |

That last row is the one place git-crypt wins outright, and it deserves to be stated plainly rather than buried. Nix, `cargo package`, `npm pack`, `git archive` and sparse CI checkouts all read the tracked tree. git-crypt tracks per-file ciphertext, so those tools get a file: the build fails on its contents, but the path resolves. git-vault's secrets are untracked by construction, so they are simply not there.

If a build of yours reads a secret from the worktree, that is the deciding fact, and you should know it before migrating. → [Limitations](limitations.md)

Otherwise: git-crypt is the better fit when you want each secret staged and reviewed on its own, and you do not mind the names being public. git-vault is the better fit when the names, the paths, the count and the structure are themselves worth hiding.

## Against sops

sops encrypts values inside a structured file, so the file, its keys and its shape stay readable, and you edit through the tool. That makes review pleasant, since a diff shows which value changed, and it makes hiding impossible: `secrets/production.yaml` and every key in it stays public.

They also solve different halves. sops is about *which* values are encrypted and by *which* KMS. git-vault is about the file itself not existing as far as the remote is concerned.

## Against an encrypted remote

`git-remote-gcrypt` and friends encrypt the whole repository, which hides everything, including your source. That also means the remote cannot be browsed, cannot run pull requests, cannot run CI, and cannot serve releases.

git-vault is deliberately narrower: the repository stays a normal repository, and only what you declared secret is sealed.

## Name

`git-vault` collides with a few small projects and with HashiCorp Vault in conversation. `git-sealed`, `git-lockbox` and `git-strongbox` were the alternatives considered. The name stayed because the tracked directory, the config namespace and the filter driver all read naturally as `vault`.
