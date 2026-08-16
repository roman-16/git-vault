# Declaring secrets

## The two files

Two ordinary committed files decide everything, and they are the only source of truth:

```gitattributes
# .gitattributes
.vault/data  filter=vault diff=vault merge=vault -text
secrets/     vault
*.key        vault
```

```gitignore
# .gitignore
secrets/
*.key
```

`.gitattributes` says what is sealed. `.gitignore` keeps the plaintext out of git's hands. Both are needed: without the attribute a file is not sealed, and without the ignore git would commit it in the clear. `git vault doctor` fails when they disagree.

Hand-editing them works and always will. `git vault add` and `git vault remove` are shortcuts that also do the things hand-editing cannot:

- stop git tracking a path it already had, so plaintext stops being committed,
- refuse patterns that would seal `.gitattributes`, `.gitignore` or the vault itself,
- say out loud that sealing something now does not unsay what was pushed before.

## Patterns

Matching uses git's own glob implementation, so patterns behave exactly as git resolves them elsewhere.

| Pattern | Seals |
| --- | --- |
| `secrets/` | Everything under `secrets/`, at any depth |
| `secrets/**` | The same |
| `secrets/*.env` | `secrets/prod.env`, but not `secrets/deep/prod.env` |
| `*.key` | Any file ending in `.key`, anywhere in the repository |
| `config/prod.key` | Exactly that file |
| `secrets/README.md -vault` | Nothing: this un-seals a path that an earlier line sealed |

The last matching line wins, as it does in git.

One deliberate difference from git: a pattern that names a directory seals everything **inside** it. Git's own attribute lookup would match the directory and nothing under it, which is why git-crypt makes you write `secrets/**`. Here a bare `secrets/` does what you meant.

Four paths can never be sealed, whatever the patterns say, because the repository needs them to open the vault at all: `.gitattributes`, `.gitignore`, `.vault/…` and anything inside `.git/`.

## Anchored patterns are faster

Every git command re-seals the vault, and to do that git-vault has to find the secrets. It walks the directories your patterns point at, so the shape of a pattern is a performance decision:

| Pattern | What gets walked |
| --- | --- |
| `secrets/**` | just `secrets/` |
| `config/*/prod.env` | just `config/` |
| `*.key` | **the whole worktree, on every git command** |

On a small repository nobody notices. On a large one, prefer `secrets/**` or `config/**/*.key` over a bare `*.key`. `git vault doctor` warns when a pattern has no leading directory.

`git vault add secrets/` writes the anchored form for you.

## Sealing a source tree without its build output

A pattern that names a directory seals **everything** inside it, at any depth, except `.git`. That includes `node_modules/`, `.venv/`, `__pycache__/`, `dist/` and every database file your tools have left lying about.

Nested `.gitignore` files are deliberately not consulted. Git itself never reads them here either, because it never descends into an ignored directory, and honouring them would be actively dangerous: `.env` is both the most commonly ignored filename and the most common secret, so a nested rule would silently leave your secret out of the vault.

So exclude build output explicitly, with a `-vault` line after the one that seals the tree:

```gitattributes
src/service/**             vault filter=vault-plaintext
src/service/.venv/**       -vault
src/service/data/**        -vault
src/service/__pycache__/** -vault
```

The last matching line wins, as it does in git.

`git vault add` tells you what it just sealed, and names the largest entries when the total is big enough to be a problem, so you can act while it is still one command to undo:

```console
$ git vault add src/service/
Sealing src/service/**

3 secrets, 18.1 MiB in total.

That is a lot to seal again on every git command. The largest are:
   13.3 MiB  src/service/data/market.duckdb
    4.7 MiB  src/service/.venv/lib/blob
       12 B  src/service/src/strategy.py

Build output belongs outside the vault. Exclude it in .gitattributes:
  src/service/data/** -vault
```

This matters more than the disk space suggests: the whole vault is held in memory and sealed again on every git command. → [Limitations](limitations.md)

## Removing something

```console
$ git vault remove secrets/old/
No longer sealing secrets/old/
```

Those files stay on disk, and they are now ordinary untracked files. Add them if you want them committed in the clear, and remember that the vault's history still holds what they used to contain.

## What a pattern publishes

`.gitattributes` is committed, so the patterns are public. That publishes the label on the box, meaning a directory called `secrets` exists. It does not publish the inventory: the names inside, their number, their sizes and their contents all stay sealed. A generic pattern such as `*.sealed` leaks nothing at all.
