#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/../.."
binary="$PWD/${GIT_VAULT:-target/release/git-vault}"

work=$(mktemp --directory)
trap 'rm -rf "$work"' EXIT
mkdir -p "$work/bin"
ln --symbolic "$binary" "$work/bin/git-vault"

export PATH="$work/bin:$PATH"
export GIT_VAULT_IDENTITY="$work/identity"
export GIT_VAULT_LABEL="you@laptop"
export GIT_CONFIG_GLOBAL=/dev/null
export GIT_CONFIG_SYSTEM=/dev/null
export GIT_AUTHOR_NAME="You"
export GIT_AUTHOR_EMAIL="you@example.com"
export GIT_AUTHOR_DATE="2026-04-16T10:00:00+02:00"
export GIT_COMMITTER_NAME="You"
export GIT_COMMITTER_EMAIL="you@example.com"
export GIT_COMMITTER_DATE="2026-04-16T10:00:00+02:00"
export TERM=xterm-256color

stty columns 84 rows 40 2>/dev/null || true

cd "$work"
mkdir -p project/src project/secrets
cd project
git init --quiet --initial-branch main
printf '# my-project\n' >README.md
printf 'fn main() {\n    println!("hello");\n}\n' >src/main.rs
git-vault init >/dev/null
git-vault add secrets/ >/dev/null
printf 'STRIPE_KEY=sk_live_51HxYqL\nDATABASE_URL=postgres://prod\n' >secrets/prod.env
printf 'deploy-token-9f2b1c\n' >secrets/deploy.token
git add --all >/dev/null
git commit --quiet --message 'seal the secrets'

prompt() { printf '\033[38;2;240;60;46m$\033[0m %s\n' "$*"; }

prompt "vim secrets/prod.env"
printf 'STRIPE_KEY=sk_live_9zQt4M\nDATABASE_URL=postgres://prod\n' >secrets/prod.env
printf '\n'

prompt "git status --short"
git status --short
printf '\n'

prompt "git vault status"
git-vault status
printf '\n'

prompt "git commit --quiet --all --message 'rotate the stripe key'"
git commit --quiet --all --message 'rotate the stripe key'
printf '\n'

prompt "git ls-tree -r --name-only HEAD"
git ls-tree -r --name-only HEAD
printf '\n'

prompt "git vault ls"
git-vault ls
