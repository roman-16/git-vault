# Getting started

## Your first vault

```console
$ cd my-project
$ git vault init
Vault created.
  identity   /home/you/.config/git-vault/identity
  recipient  age1x8x3zw8ul3zjw5xn0dxlh0q6nx85t5uvjduleu9dgepgq94n50j

Next:
  git vault add secrets/           declare what is secret
  git add .gitattributes .gitignore .vault
  git commit -m 'add a vault'
```

`init` does four things: it makes an age identity for this machine if you do not have one, generates a vault key and wraps it for you, wires this clone up in `.git/config`, and creates `.vault/`.

Your identity is the important file. It lives outside the repository, at `~/.config/git-vault/identity`, it is the same one for every repository you use, and **without it, or a copy of the vault key, the vault cannot be opened**. Back it up the way you back up an SSH key.

## Declaring what is secret

```console
$ git vault add secrets/ config/prod.key
Sealing secrets/
Sealing config/prod.key
  untracked config/prod.key, so its plaintext stops being committed
  its earlier contents are still in this repository's history
```

`add` writes to `.gitattributes` and `.gitignore`, and stops git from tracking anything it already had. If a file was committed in the clear before, that history is still there; sealing it now protects only what comes next. → [Declaring secrets](declaring-secrets.md)

## Day to day

Nothing to remember. Edit the files, commit with git:

```console
$ vim secrets/prod.env

$ git status --short
 M .vault/data

$ git vault status
M secrets/prod.env

$ git commit -am 'rotate the stripe key'
```

`git status` tells you that the vault changed. `git vault status` tells you which secret it was. Because every secret lives in one file, they all commit together: `git add secrets/prod.env` is not a thing, and `git commit -a` takes all of them.

```console
$ git vault diff                  # what changed, in plaintext
$ git vault log secrets/prod.env  # the history of one secret
$ git vault restore               # throw local secret edits away
$ git vault ls                    # what is sealed
```

## On a second machine

```console
$ git clone git@github.com:you/my-project.git
$ cd my-project
$ git vault unlock
Unlocked 2 secrets.
```

`unlock` needs your identity, so copy `~/.config/git-vault/identity` across, or give the new machine its own and share the vault with it:

```console
$ git vault unlock
git-vault: this identity cannot open the vault: No matching keys found

Your public key is:
  age1lggyhqrw2nlhcxprm67z43rta597azn8gknawjehu9d9dl0jq3yqqvfafg

Ask somebody with access to run:
  git vault share age1lggyhqrw2nlhcxprm67z43rta597azn8gknawjehu9d9dl0jq3yqqvfafg
```

That is also the flow for a new colleague. → [Keys and access](keys-and-access.md)

## Locking up

```console
$ git vault lock
Locked. Removed 2 plaintext files.
```

The plaintext is gone and so is the local key. The repository still works: git commands behave normally, and `git log -p` shows which entry a commit touched without revealing anything. `git vault unlock` brings it all back.

## Checking the setup

```console
$ git vault doctor
ok      .vault/data holds 2 sealed entries
ok      .vault/keys is present
ok      .vault/recipients lists 1 recipient
ok      this clone is unlocked
...
```

`doctor` exits non-zero when something is actually wrong, which makes it worth a line in CI. → [Troubleshooting](troubleshooting.md)
