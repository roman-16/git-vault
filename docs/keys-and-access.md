# Keys and access

## How access works

One key seals everything in the vault. For each person who has access, a copy of that key is wrapped so only their private key opens it, and all the copies live in `.vault/keys`.

Three files, three jobs:

| File | What it is |
| --- | --- |
| `.vault/keys` | The vault key, wrapped once per recipient. A stock age file |
| `.vault/recipients` | The public keys it was wrapped for, in the clear |
| `~/.config/git-vault/identity` | Your private key. Not in the repository, and not shared |

`.vault/recipients` exists because an age file does not record who it was wrapped for. For native `age1…` keys that is deliberate, and it is what makes them leave no fingerprint behind. The consequence is that re-wrapping the key for everybody, which sharing and rotating both do, needs the list kept separately. Keeping it in the clear also means a change of access shows up as a reviewable diff.

## Giving somebody access

They run `git vault unlock`, which makes them an identity and prints its public key:

```console
$ git vault unlock
git-vault: this identity cannot open the vault: No matching keys found

Your public key is:
  age1lggyhqrw2nlhcxprm67z43rta597azn8gknawjehu9d9dl0jq3yqqvfafg

Ask somebody with access to run:
  git vault share age1lggyhqrw2nlhcxprm67z43rta597azn8gknawjehu9d9dl0jq3yqqvfafg
```

You run exactly that, then commit:

```console
$ git vault share age1lggy… --label alice@work
age1lggyhqrw2nlh… can now open the vault.

$ git add .vault && git commit -m 'give alice access'
```

An SSH public key works just as well, which saves them making anything new:

```console
$ git vault share ~/keys/alice.pub --label alice@work
$ git vault share "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAI..." --label bob@laptop
```

`ssh-ed25519` and `ssh-rsa` are supported. Note that age cannot use `ssh-agent`, so a passphrase-protected SSH key is typed in once per `git vault unlock` on that machine, not once per git command.

Nothing but `git vault update` touches the network, so this works the same on GitHub, GitLab, Gitea or a bare repository on a NAS.

## Who has access

```console
$ git vault keys
age1x8x3zw8ul3zj…  you@laptop (you)
age1lggyhqrw2nlh…  alice@work

2 recipients in .vault/recipients
```

## Taking it away

```console
$ git vault revoke alice@work
age1lggyhqrw2nlh… can no longer open the vault.
The vault key was replaced, so everything is sealed anew.

They can still read every commit made while they had access. If a secret
was worth protecting from them, change the secret too.
```

`revoke` removes them and replaces the vault key, so nothing sealed from now on is readable by them. It cannot retract history: they had the old key, and every commit made while they had access stays readable to them. **If a secret was seen, change the secret.**

You can match a recipient by label, by key, or by any unambiguous part of either. Revoking the last recipient is refused, and so is revoking yourself.

## Rotating

```console
$ git vault rotate
The vault key was replaced and everything sealed anew.
2 recipients kept access.
```

Everybody keeps access, and every byte of `.vault/data` changes because every entry is sealed with a new key. Worth doing after somebody leaves the team by another route, or on a schedule if that is your policy.

Everyone else picks up the new key the next time they pull. Their `git pull` succeeds with a note, and one `git vault unlock` brings them current:

```console
$ git pull
git-vault: `.vault/data` was sealed with a newer vault key. The secrets on disk
were left alone; run `git vault unlock` to pick up the new key.

$ git vault unlock
Unlocked 2 secrets.
```

## CI

A runner has no age identity, so give it the vault key directly:

```console
$ git vault export-key ci.key
Wrote the vault key to ci.key.
```

Put that file in your CI secret store, write it out at the start of a job, and unlock with it:

```yaml
- run: |
    printf '%s' "$VAULT_KEY_BASE64" | base64 --decode > /tmp/vault.key
    git vault unlock --key-file /tmp/vault.key
    git vault doctor
```

Anything holding that file can read every secret in the vault, including its whole history. Treat it as the credential it is, and rotate after anybody with access to the runner leaves.

`git vault doctor` exits non-zero on real problems, so it is worth a line in every pipeline that touches the repository. It is what catches a sealed path that somebody committed in plaintext.

## If you lose your identity

There is no recovery from the tool. Somebody else with access runs `git vault share` for your new key. If nobody has access any more, the secrets are gone; that is what encryption means. This is why `~/.config/git-vault/identity` deserves the same treatment as an SSH key, and why teams should have more than one recipient.
