# How it works

## The problem

Your secrets are ordinary files on disk, and git does not know they exist: they are listed in `.gitignore`. The only thing git tracks is `.vault/data`, which holds all of them, sealed.

So the whole difficulty is this: **how does git find out that you edited a secret**, when the file you edited is invisible to it? The answer has to be that `.vault/data` changes, and the only thing that can change it is git-vault.

Which raises the real question: **when does git-vault run?** It is a program, not a service. Nothing of it is running while you edit a file. Your editor does not tell it, and neither does the operating system.

This is not an implementation detail, it is a consequence of hiding names. git-crypt does not have this problem, because there the secret file *is* the tracked file: you edit `prod.env`, git sees `prod.env` change, git asks git-crypt to encrypt it. That is also exactly why git-crypt can never hide the name `prod.env`. The moment many secrets collapse into one file with a different name, "the file changed" stops being a usable trigger.

## The three moments

Git offers exactly three moments where it will run somebody else's program on the way past. git-vault takes all three, and needs nothing else.

| Moment | What it is | What git-vault does |
| --- | --- | --- |
| `core.fsmonitor` | Git asks it "what changed?" at the start of every command that reads the index, which is `status`, `diff`, `add`, `commit` | Seals the live secrets, then answers "check everything" |
| `pre-commit` | A hook git runs before making a commit | Seals again, and says so if the commit does not carry the result |
| `smudge` | A filter git runs whenever it writes `.vault/data` into the worktree | Decrypts, and reconciles the secret files |
| `clean`, on a declared secret | A filter git runs when it is about to store a file | Refuses, so plaintext cannot reach the index |

**fsmonitor** is what makes plain `git status` report ` M .vault/data` a moment after you save a secret. Answering "check everything" is deliberate: git-vault cannot know what else changed on disk, and claiming otherwise would make git skip files that really did change.

**pre-commit** is the backstop, for when the fsmonitor hook did not run. Git carries on when an fsmonitor hook fails, which is correct of it, and then nothing has sealed your edits by the time git assembles the commit. So the hook seals again.

It does not stage anything. `.vault/data` goes into a commit exactly when you put it there, like any other file: `git commit -a` takes it because it is a modified tracked file, and `git add src/main.rs && git commit` leaves it out because you left it out. A hook that quietly added a file to your commit would be the one piece of this design that surprised you, and with secrets a surprise is expensive. When sealing finds changes the commit will not carry, it says so and tells you the command that adds them.

**smudge** is the other direction, and it is the one git guarantees: git knows when it is writing `.vault/data`, so it always asks. This is why a checkout, a merge or a pull turns sealed bytes back into ordinary files without any hooks at all.

## The invariant everything rests on

> `.vault/data` on disk always holds exactly the bytes git has stored.

Because of that, git treats it as a perfectly ordinary binary file. It has no reason to think it contains unsaved changes, so `merge`, `rebase`, `revert`, `stash` and `cherry-pick` all behave exactly as they do in any other repository.

This is worth stating because the obvious alternative fails. An earlier design kept the worktree copy **empty**, which exploits a rule in git's index: when the size git recorded is zero but the content it stored is not, git distrusts its own bookkeeping and re-reads the file through the clean filter. That worked, `git status` needed no hooks at all, and it was magic.

It was also fatal. To git, an empty worktree copy against a non-empty stored blob is indistinguishable from *"the user has unsaved work here"*, and git protects unsaved work by refusing to overwrite it. So `git merge`, `git rebase` and `git revert` all failed, with a confusing message about local changes, whenever the operation needed to change the vault. Worse, the refusal happens before any driver of ours runs, so it also made a merge driver impossible. The two signals, "ask git-vault every time" and "the user may have unsaved work", are the same signal to git, and there is no way to have one without the other.

Keeping the sealed bytes on disk gives up the magic and buys every git command back.

## Sealing has to be deterministic

The vault is sealed again on every git command. If sealing the same secrets twice produced different bytes, `.vault/data` would be permanently modified, `git status` would never be clean, and the tool would be unusable.

So the sealed form is a pure function of the secrets and the key: entries are sorted by a keyed hash of their path, and each one is encrypted with a nonce derived from its own contents. Nothing is timestamped and nothing is random. → [The `.vault` format](format.md)

Two consequences fall out of it. Sealing is a no-op when nothing changed, so a `git status` on an untouched repository writes nothing and wakes no file watchers. And a change to one secret changes one small region of the file, so git deltas the history normally instead of storing a whole new vault in every commit.

## Where things live

```
.vault/data         the sealed secrets, tracked, always equal to what git stored
.vault/keys         the vault key, wrapped for each recipient, a stock age file
.vault/recipients   the public keys it was wrapped for
.gitattributes      hands .vault/data to the filters, and declares what is secret
.gitignore          keeps the plaintext out of git's hands
```

Everything local and untracked lives in git's own directory:

```
.git/vault/key      the unwrapped vault key; its presence is what "unlocked" means
.git/config         the filters, the merge driver, and core.fsmonitor
.git/hooks/pre-commit
```

Because all of that is inside `.git`, none of it travels. A clone has no key, no filters and no hooks, which is exactly why a clone without access is harmless rather than broken.

The key sits in the **common** git directory, so `git worktree add` produces a worktree that is already unlocked instead of one that silently looks locked.

## Why the key is cached

`git vault unlock` writes the unwrapped vault key to `.git/vault/key` rather than unwrapping it each time it is needed.

That is not laziness. age has no `ssh-agent` support, so a passphrase-protected SSH key would have to be typed in again on every single git command. The cached key is a real trade: a 32-byte file readable only by you, next to the plaintext secrets that are already sitting in your worktree. `git vault lock` removes both.

## Why the filter never generates anything

There is a clean filter, and all it does is check that what git is about to store really is a vault, then pass it through. It never produces the sealed bytes; the hooks and the commands do that.

It earns its place by catching corruption at `git add` time rather than at decryption time much later: an empty `.vault/data` left by a half-finished operation, or one with conflict markers in it from a merge that went sideways.

`filter.vault.required = true` matters for the same reason. Without it, git falls back to storing the raw file when a filter fails, which is how a broken or missing binary would quietly wreck a vault.
