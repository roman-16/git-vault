# Threat model

## What an observer sees

Somebody who can read the repository, on the remote or in a clone, but has no key.

**Hidden from them:** the contents of every secret, their file names, their paths, the directory structure they live in, their exact sizes, and their modes.

**Visible to them:**

| What | Why |
| --- | --- |
| That a vault exists | `.vault/` is committed, and `.gitattributes` names it |
| Its rough size, and each secret's size to within a factor of two | Sealed entries are padded to power-of-two size classes |
| How many secrets there are | Entries are independent regions, so they can be counted |
| Which commits touched secrets | `.vault/data` appears in those commits |
| How many people have access, and who they are | `.vault/recipients` holds their public keys |
| Which entry a commit changed | Each entry has a stable id, and its nonce changes with its contents |
| Whether a secret went back to an earlier value | Sealing is deterministic, so the same plaintext seals to the same bytes |
| Whatever your patterns say | `secrets/** vault` in `.gitattributes` publishes the label on the box, but not its inventory. A pattern like `*.sealed` says nothing at all |

## What the tool assumes

- **Your machine is trusted.** Unlocked, the plaintext is on disk and the vault key is in `.git/vault/key`. Anything that can read your worktree can read your secrets, and `git vault lock` is what takes that back.
- **`git status` is not a security boundary.** The tool decides what is secret from `.gitattributes`, which is committed and reviewable. Somebody who can change that file in a merge can stop a path from being sealed, and the change is visible in the diff. `git vault doctor` fails when a sealed path is tracked in plaintext, which is the check worth running in CI.
- **The remote is untrusted for reading, trusted for integrity.** Every entry is authenticated, so a tampered vault fails to open rather than opening wrongly. Nothing stops a remote from serving an older vault, which is what signed commits are for.
- **`git vault update` trusts GitHub.** It is the only command that goes to the network. It fetches over TLS, and verifies the binary against the `checksums.txt` published beside it - which proves the download is intact, not that GitHub served what we built, because the checksums come from the same place as the binary. Anyone who can publish to the repository's releases can publish a matching pair. Package managers that sign their own indexes (APT, AUR, Homebrew, nixpkgs) do not have that property, and updating through them is the stronger path.
- **The plaintext guard is local, like every other piece of wiring.** `.gitattributes` travels with the repository, but the filter that reads it lives in `.git/config`, which does not. A clone where nobody ran `git vault init` or `git vault unlock` will happily commit a secret in the clear. `git vault doctor` fails on exactly that, which is why it belongs in CI: it is the only check that runs where the wiring does not.

## Revocation cannot retract

`git vault revoke` removes somebody from the recipient list and replaces the vault key, so they cannot read anything sealed afterwards. They can still read **every commit made while they had access**, because they had the key and history does not change.

If a secret was worth protecting from that person, change the secret. The tool says exactly this when you revoke.

## Sealing does not unsay

Sealing a path today does nothing about what was pushed yesterday. If plaintext was ever committed, it is in history and it stays there until the history is rewritten. `git vault add` warns when it untracks a file git already had, which is the only moment the tool can tell.

## Cryptography

- **Contents:** XChaCha20-Poly1305, with a 24-byte nonce derived as a keyed BLAKE3 hash of the record it protects. Deterministic and authenticated. Nonce reuse can only happen for a byte-identical record under the same key, where the ciphertext is identical anyway.
- **Keys:** one 32-byte vault key from the operating system's randomness, with per-purpose subkeys derived through BLAKE3.
- **Key wrapping:** [age](https://age-encryption.org), with native `age1…` recipients or `ssh-ed25519` and `ssh-rsa` public keys. `.vault/keys` is a stock age file.
- **Key hygiene:** the vault key lives in `secrecy::SecretBox` and is wiped on drop through `zeroize`. It is written to `.git/vault/key` with mode `0600`.
- **Unaudited.** This has had no external review.

## Reporting a problem

Please do not open a public issue for a security problem. [`SECURITY.md`](../SECURITY.md) has the private channel.
