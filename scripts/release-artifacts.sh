#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

artifacts=target/artifacts
rm --recursive --force "$artifacts"
mkdir --parents "$artifacts/completions" "$artifacts/man"

cargo build --quiet
binary=target/debug/git-vault

for shell in bash fish zsh; do
    "$binary" completions "$shell" > "$artifacts/completions/git-vault.$shell"
done

"$binary" man "$artifacts/man" > /dev/null

printf 'Generated %s completions and %s man pages in %s.\n' \
    "$(find "$artifacts/completions" -type f | wc --lines)" \
    "$(find "$artifacts/man" -type f | wc --lines)" \
    "$artifacts"
