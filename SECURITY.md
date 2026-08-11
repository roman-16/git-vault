# Security

## Reporting a vulnerability

Please do not open a public issue.

Use [GitHub's private vulnerability reporting](https://github.com/roman-16/git-vault/security/advisories/new) for anything that could put somebody's secrets at risk.

Include what you did, what happened, and what you expected. A failing test against a throwaway repository is the most useful thing you can send.

## What is in scope

- Recovering a secret's contents, name, path or exact size from a repository without a key.
- A sealed secret being written to git in plaintext, or a path that should be sealed not being sealed.
- A commit that appears to contain a secret change but does not, or the reverse.
- Plaintext, or the vault key, being left anywhere it outlives `git vault lock`.
- A crafted `.vault/data` causing anything worse than a clean error.

## What is not

- The recipient list being visible. It is in `.vault/recipients` on purpose. → [Threat model](docs/threat-model.md)
- Somebody who had access still being able to read commits made while they had it. Revocation cannot retract history, and the tool says so when you revoke.
- Plaintext on an unlocked machine. Unlocked means the secrets are ordinary files, and anything that can read your worktree can read them.
- `git clean -xdf` removing uncommitted secret edits. They are ignored files, and git treats them as such.
- Anything already committed in plaintext before it was sealed.

## What this project assumes

The threat model, the cryptography and the exact leakage are written down in [`docs/threat-model.md`](docs/threat-model.md). The short version: contents are XChaCha20-Poly1305 with a nonce derived from the contents, keys are wrapped with [age](https://age-encryption.org), and the vault key is cached on disk while a clone is unlocked.

**This project is unaudited.** It has had no external review.
