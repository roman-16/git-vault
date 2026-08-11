# Git integration

## What every command does

Nothing here is wrapped, aliased or intercepted. This is plain git with a filter, a merge driver and two hooks configured in `.git/config`.

| Command | What happens |
| --- | --- |
| `git status` | Seals first, so an edited secret shows as ` M .vault/data` |
| `git add`, `git commit` | The same. The commit carries `.vault/data` only if you staged it |
| `git commit -a` | Takes every secret change at once, because they are one file |
| `git checkout`, `git switch` | Writes the sealed file, and the smudge filter reconciles your secret files to match |
| `git merge`, `git rebase`, `git revert`, `git cherry-pick` | Merge per secret through the merge driver |
| `git stash`, `git stash pop` | Round trip, secrets and all |
| `git diff`, `git log -p`, `git show` | Plaintext, through textconv |
| `git clone`, `git push`, `git pull` | Unchanged. Commit hashes are the same locally and on the remote |
| `git worktree add` | The new worktree is already unlocked, because the key lives in the common git directory |
| `git clean -xdf` | **Removes your secrets**, because they are ignored files |

## Reconciliation, not extraction

When git writes `.vault/data`, git-vault does not simply unpack it. It makes your worktree match:

- a secret whose contents already match is left alone, so mtimes stay put and file watchers stay quiet,
- a secret this commit does not have is **removed**,
- a directory that held nothing but secrets goes with them.

Without that last part, switching from a branch that has `secrets/staging.env` to one that does not would leave the file behind, and you would be running with a secret from another branch.

## Merging

To git, `.vault/data` is opaque binary, so any change on both sides would be a conflict. The merge driver makes it behave as if the secrets were ordinary files.

**With the key**, all three sides are opened and merged per secret:

| Situation | Result |
| --- | --- |
| Each side adds different secrets | clean |
| One side changes a secret, the other does not | clean |
| One side deletes it, the other does not | clean, the deletion wins |
| Both change different lines of the same secret | clean, through a real three-way text merge |
| Both change the same line | conflict |

**Without the key**, the merge is by entry: entries are independent and their ids are stable, so anything added, removed or changed on one side only still resolves cleanly. Only the same entry changed on both sides conflicts. This is what lets somebody without access rebase or merge a branch that touches secrets.

## Resolving a conflict

The conflict markers land inside the plaintext secret, not in the sealed file:

```console
$ git merge feature
git-vault: 1 of the sealed secrets conflicts:
  secrets/prod.env
Resolve them as ordinary files, then:
  git vault seal && git add .vault/data

$ cat secrets/prod.env
<<<<<<< ours
STRIPE=ours
=======
STRIPE=theirs
>>>>>>> theirs
DATABASE_URL=postgres://prod
```

So you resolve it the way you resolve any conflict, then seal and continue:

```console
$ vim secrets/prod.env
$ git vault seal && git add .vault/data
$ git commit
```

Nothing re-seals by itself while a merge, rebase, cherry-pick, revert or bisect is unresolved. During those the worktree is not the authority on what the secrets are, and sealing over it could throw away the side being merged in. That is why the last step is explicit.

## Dirty secrets are protected by git itself

An uncommitted secret edit is a modification to `.vault/data`, so git refuses to check out over it, with its own message:

```console
$ git checkout other-branch
error: Your local changes to the following files would be overwritten by checkout:
        .vault/data
Please commit your changes or stash them before you switch branches.
```

No dirty-check code of ours is involved. It is the same protection git gives every other file, and `git stash` gets you past it the same way.

## What `git status` cannot do

`git status` reports that the vault changed. It cannot report *which* secret, because to git the vault is one file. `git vault status` answers that, and `git vault diff` shows the change itself.

The other gap: if something else already uses `core.fsmonitor`, git-vault leaves it alone, and then nothing seals until you commit. The pre-commit hook seals at that point and reports that the commit does not carry your secret changes, so committing again picks them up. `git vault status` always tells the truth, since it reads your files on the spot.

## Plaintext cannot be committed

A declared secret carries two things: `.gitignore` keeps it out of git's way, and `.gitattributes` hands it to a filter whose only job is to refuse:

```
secrets/** vault filter=vault-plaintext
```

`.gitignore` is a convention, and conventions lose. Delete the line, resolve a merge carelessly, or reach for `git add --force`, and the plaintext is one command from being permanent. The filter is not a convention:

```console
$ git add --force secrets/prod.env
git-vault: refusing to put the plaintext of `secrets/prod.env` into the index: it is
declared secret, so its contents belong in `.vault/data`, and `.gitignore` is what
normally keeps it out of git. To publish it in the clear, run
`git vault remove secrets/prod.env` first
fatal: secrets/prod.env: clean filter 'vault-plaintext' failed
```

Nothing is staged, not even the other paths in that `git add` - git stops the whole operation. The same refusal covers `git stash --all`, which would otherwise copy ignored files into the object store.

If plaintext reached the index before the path was declared, the `pre-commit` hook stops the commit instead:

```
git-vault: this commit would publish the plaintext of secrets/prod.env, which
.gitattributes says is secret. Run `git vault add secrets/prod.env` to take it out
of the index, or `git vault remove secrets/prod.env` to stop sealing it
```

To publish a secret in the clear on purpose, undeclare it first. That is what `git vault remove` is for, and afterwards the path is an ordinary file again.

## Staging

`.vault/data` is staged like any other file, and nothing stages it behind your back:

```bash
git add src/main.rs && git commit    # your code only, secrets left for later
git commit --all                     # everything, secrets included
git vault status                     # which secrets are waiting
```

A secret change you leave out stays in your working copy, and `git status` keeps showing ` M .vault/data` until you commit it.
