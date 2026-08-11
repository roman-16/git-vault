#!/bin/sh
set -eu

REPO="roman-16/git-vault"
BIN="git-vault"
INSTALL_DIR="${GIT_VAULT_INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${GIT_VAULT_VERSION:-latest}"

die() {
    printf '%s: %s\n' "$BIN" "$1" >&2
    exit 1
}

warn() {
    printf '%s: %s\n' "$BIN" "$1" >&2
}

need() {
    command -v "$1" >/dev/null 2>&1 || die "$1 is required but not installed"
}

usage() {
    cat <<USAGE
Usage: install.sh [--version X.Y.Z] [--install-dir DIR]

  --version      release to install, or "latest" (default: latest)
  --install-dir  where to put the binary (default: ~/.local/bin)

The same values can come from GIT_VAULT_VERSION and GIT_VAULT_INSTALL_DIR.
USAGE
}

while [ $# -gt 0 ]; do
    case "$1" in
    -h | --help)
        usage
        exit 0
        ;;
    --version)
        [ $# -ge 2 ] || die "--version needs a value"
        VERSION="$2"
        shift 2
        ;;
    --version=*)
        VERSION="${1#*=}"
        shift
        ;;
    --install-dir)
        [ $# -ge 2 ] || die "--install-dir needs a value"
        INSTALL_DIR="$2"
        shift 2
        ;;
    --install-dir=*)
        INSTALL_DIR="${1#*=}"
        shift
        ;;
    *) die "unknown option: $1" ;;
    esac
done

detect_os() {
    case "$(uname -s)" in
    Linux) echo linux ;;
    Darwin) echo darwin ;;
    *) die "unsupported operating system: $(uname -s). On Windows use the PowerShell installer or winget" ;;
    esac
}

detect_arch() {
    case "$(uname -m)" in
    x86_64 | amd64) echo amd64 ;;
    aarch64 | arm64) echo arm64 ;;
    *) die "unsupported architecture: $(uname -m)" ;;
    esac
}

need uname
need curl
need install
command -v git >/dev/null 2>&1 || warn "git is not installed, and git-vault cannot work without it"

asset="${BIN}_$(detect_os)_$(detect_arch)"

if [ "$VERSION" = latest ]; then
    base="https://github.com/$REPO/releases/latest/download"
else
    base="https://github.com/$REPO/releases/download/v${VERSION#v}"
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT INT TERM

curl --fail --location --silent --show-error --output "$work/$asset" "$base/$asset" ||
    die "could not download $base/$asset"

if curl --fail --location --silent --show-error --output "$work/checksums.txt" "$base/checksums.txt"; then
    expected=$(awk -v name="$asset" '$2 == name || $2 == "*" name { print $1 }' "$work/checksums.txt")
    [ -n "$expected" ] || die "checksums.txt does not mention $asset"

    if command -v sha256sum >/dev/null 2>&1; then
        actual=$(sha256sum "$work/$asset" | cut -d' ' -f1)
    elif command -v shasum >/dev/null 2>&1; then
        actual=$(shasum --algorithm 256 "$work/$asset" | cut -d' ' -f1)
    else
        actual=""
        warn "no sha256sum or shasum, so the download was not verified"
    fi

    if [ -n "$actual" ] && [ "$actual" != "$expected" ]; then
        die "checksum mismatch for $asset (expected $expected, got $actual)"
    fi
else
    warn "could not fetch checksums.txt, so the download was not verified"
fi

mkdir -p "$INSTALL_DIR"
install -m 0755 "$work/$asset" "$INSTALL_DIR/$BIN" ||
    die "could not install into $INSTALL_DIR (pass --install-dir to choose somewhere writable)"

installed=$("$INSTALL_DIR/$BIN" --version 2>/dev/null || echo "$BIN")
printf '%s installed to %s/%s\n' "$installed" "$INSTALL_DIR" "$BIN"

case ":$PATH:" in
*":$INSTALL_DIR:"*) ;;
*)
    warn "$INSTALL_DIR is not on your PATH, so git will not find \`git vault\`. Add this to your shell profile:"
    printf '\n    export PATH="%s:$PATH"\n\n' "$INSTALL_DIR" >&2
    ;;
esac
