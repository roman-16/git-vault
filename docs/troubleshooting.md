# Troubleshooting

## Every `doctor` finding

`git vault doctor` reports `ok`, `warning` or `problem`, and exits non-zero when there is a problem. Warnings do not fail.

| Finding | What it means | Fix |
| --- | --- | --- |
| `.vault/data is missing` | The vault was never committed, or the file was deleted | `git checkout -- .vault/data` |
| `.vault/data is not a vault` | The file was hand-edited, or a merge left markers in it | `git checkout -- .vault/data`, or resolve and `git vault seal` |
| `.vault/keys is missing, so nobody can open this vault` | Only `.vault/data` was committed | `git checkout -- .vault/keys` |
| `.vault/recipients lists nobody` | The file was emptied | `git checkout -- .vault/recipients` |
| `filter.vault.clean is not configured` | This clone was never wired up | `git vault unlock` |
| `filter.vault.required is not true` | A failing filter would let git store raw bytes | `git vault unlock` rewrites it |
| `core.fsmonitor is …` (warning) | Something else uses it, so `git status` will not report secret edits | Nothing needed; commits stay correct. Use `git vault status` |
| `core.fsmonitor is unset` (warning) | The same | `git vault unlock` sets it when the slot is free |
| `diff.vault.cachetextconv is not false` | Git would cache decrypted secrets under `.git` | `git vault unlock` rewrites it |
| `no pre-commit hook` | Nothing seals if `core.fsmonitor` also fails | `git vault unlock` |
| `another pre-commit hook is installed` (warning) | Yours was left alone | Add the line it prints to your hook |
| `.vault/data carries the index flag …` | `assume-unchanged` or `skip-worktree` is set, so git stops noticing it | Run the `git update-index` command it prints |
| `.vault/data is not tracked by git` | It was never added | `git add .vault && git commit` |
| `.gitattributes does not hand .vault/data to the filters` | The declaration line is missing | `git vault unlock` rewrites it |
| `… hands .vault/data to the clean and smudge filters but not to diff and merge` | The line is incomplete | Restore `filter=vault diff=vault merge=vault -text` |
| `… marks .vault/data as binary` | `binary` also switches off diff and merge | Use `-text` instead |
| `is sealed but not in .gitignore` | Git may commit the plaintext | `git vault add <path>` |
| `is sealed and also tracked in plaintext` | **A leak.** Git has the plaintext | `git vault add <path>`, then rewrite history if it was pushed |
| `a pattern has no leading directory` (warning) | Every git command walks the whole worktree | Anchor it, for example `secrets/**` instead of `*.key` |
| `.vault/data is … KiB` (warning) | It is sealed again on every git command | Keep large files out of the vault |
| `nothing is declared secret yet` (warning) | You have a vault but no secrets | `git vault add <path>` |

## Every refusal

**`this clone is locked: run git vault unlock`**
The command needs the vault key and this clone has none.

**`this identity cannot open the vault`**
Your key is not one of the recipients. The message prints your public key and the `git vault share` line to send to somebody who has access.

**`every sealed secret has disappeared from the worktree`**
Something removed your plaintext, most likely `git clean -xdf`. Sealing that would record every secret as deleted, so it is refused. `git vault restore` puts the committed ones back. If you meant to empty the vault, `git vault seal --allow-empty`.

**`there are no secrets on disk and .vault/data cannot be read`**
Both the plaintext and the sealed file are gone. `git checkout -- .vault/data` brings the sealed file back, then unlock.

**`.vault/data was sealed with a different vault key than this clone has cached`**
Somebody rotated the key. `git vault unlock` picks up the new one.

**`refusing to store an empty .vault/data`**
Git was about to store an empty vault, which would replace every secret with nothing. `git checkout -- .vault/data`, or `git vault seal` if your secrets are on disk.

**`refusing to store a .vault/data that is not a valid vault`**
Usually conflict markers from a merge that was resolved by hand in the wrong file. Resolve the plaintext instead, then `git vault seal`.

**`the two sides were sealed with different vault keys`**
A merge where one branch rotated the key and the other did not. Rotate one side onto the other's key first, then merge.

**`is neither a regular file nor a symlink, so it cannot be sealed`**
A socket, fifo or device turned up where a secret was expected. Move it out of the sealed paths.

**`git vault … is not implemented`**
You are on an older build than the documentation.

## Things that look wrong but are not

**`git status` shows ` M .vault/data` and I changed nothing.**
Something did change a secret: check `git vault status`. If it lists nothing, the vault was sealed with a different key, which `doctor` will tell you.

**`fatal: secrets/prod.env: clean filter 'vault-plaintext' failed`**
Git was about to store the plaintext of a declared secret. That is the guard working: the path is in `.gitattributes` as sealed, so its contents belong in `.vault/data`. Put it back in `.gitignore`, or run `git vault remove <path>` if you meant to publish it.

**`this commit would publish the plaintext of …`**
The plaintext was already in the index before the path became a secret, so no filter saw it. `git vault add <path>` takes it out of the index and starts sealing it.

**`git checkout -- .vault/data` did not bring my secrets back.**
It works while at least one secret is still on disk, because reading the index seals first and that gives git something to write. Once every secret is gone, the wipe guard refuses to seal, the file still matches what git stored, and git has nothing to do. `git vault restore` always works, and it is the one to reach for.

**My commit says "nothing to commit" after I edited a secret.**
`core.fsmonitor` is not ours, so nothing sealed before git looked. The pre-commit hook sealed it during that attempt and said so, which is why committing again works. `doctor` reports the cause as a warning.

**A colleague's `git pull` printed a git-vault warning.**
The vault key was rotated. Their pull succeeded; `git vault unlock` makes them current.
