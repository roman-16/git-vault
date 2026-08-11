#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

rustup toolchain install stable --profile minimal --no-self-update
rustup default stable

for target in aarch64-apple-darwin aarch64-unknown-linux-musl x86_64-apple-darwin \
    x86_64-pc-windows-gnu x86_64-unknown-linux-musl; do
    rustup target add "$target"
done

goreleaser release --snapshot --clean --skip=publish
