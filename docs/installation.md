# Installation

git-vault is a single static binary for Linux and macOS on x86-64 and arm64, and for Windows on x86-64, and it needs `git` on your `PATH` to work. Nothing is published until the first release is tagged; until then, use Nix, cargo or a build from source.

## Linux

**Any distribution**, into `~/.local/bin`:

```bash
curl -fsSL https://raw.githubusercontent.com/roman-16/git-vault/main/scripts/install.sh | sh
```

It takes `--version X.Y.Z` and `--install-dir DIR`, or the same values from `GIT_VAULT_VERSION` and `GIT_VAULT_INSTALL_DIR`:

```bash
curl -fsSL https://raw.githubusercontent.com/roman-16/git-vault/main/scripts/install.sh | sh -s -- --install-dir /usr/local/bin
```

**Arch Linux**, from the AUR:

```bash
yay -S git-vault-bin      # or: paru -S git-vault-bin
```

**Debian, Ubuntu, Mint**, from the APT repository, so it updates with the rest of your system:

```bash
sudo install -d -m 0755 /etc/apt/keyrings
curl -fsSL https://roman-16.github.io/git-vault/gpg.key | sudo tee /etc/apt/keyrings/git-vault.asc >/dev/null
echo "deb [signed-by=/etc/apt/keyrings/git-vault.asc] https://roman-16.github.io/git-vault stable main" | sudo tee /etc/apt/sources.list.d/git-vault.list
sudo apt update && sudo apt install git-vault
```

**Fedora, RHEL, Alpine** - take the package from the [latest release](https://github.com/roman-16/git-vault/releases/latest):

```bash
sudo dnf install ./git-vault_*.rpm                  # Fedora, RHEL
sudo apk add --allow-untrusted ./git-vault_*.apk    # Alpine
```

**Nix** - the [`git-vault`](https://search.nixos.org/packages?query=git-vault) package is in nixpkgs:

```nix
environment.systemPackages = [ pkgs.git-vault ];
```

To track the latest release instead of your nixpkgs channel, use the flake:

```nix
inputs = {
  git-vault = {
    url = "github:roman-16/git-vault";
    inputs.nixpkgs.follows = "nixpkgs";
  };
};

# in a NixOS module
environment.systemPackages = [
  git-vault.packages.${pkgs.stdenv.hostPlatform.system}.default
];
```

## macOS

```bash
brew install --cask roman-16/tap/git-vault
```

Or the install script, which puts the binary in `~/.local/bin`:

```bash
curl -fsSL https://raw.githubusercontent.com/roman-16/git-vault/main/scripts/install.sh | sh
```

## Windows

```powershell
winget install Roman-16.GitVault
```

Or the PowerShell installer, which installs into `%LOCALAPPDATA%\Programs\git-vault`:

```powershell
irm https://raw.githubusercontent.com/roman-16/git-vault/main/scripts/install.ps1 | iex
```

It accepts `-Version` and `-InstallDir`:

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/roman-16/git-vault/main/scripts/install.ps1))) -InstallDir "C:\tools\git-vault"
```

On an arm64 Windows machine the x86-64 binary runs under emulation. Inside WSL, follow the Linux instructions instead.

## Cross-platform

**cargo** - the crate is `git-vault-cli`, and it installs a binary called `git-vault`:

```bash
cargo install git-vault-cli
```

**From source:**

```bash
git clone https://github.com/roman-16/git-vault
cd git-vault
devbox run just build      # or: cargo build --release
```

The binary lands in `target/release/git-vault`. Put it anywhere on your `PATH`.

## Manual download

Every release carries raw binaries, archives, and `checksums.txt` on the [releases page](https://github.com/roman-16/git-vault/releases/latest).

| Platform | Binary |
| --- | --- |
| Linux x86-64 | `git-vault_linux_amd64` |
| Linux arm64 | `git-vault_linux_arm64` |
| macOS Apple Silicon | `git-vault_darwin_arm64` |
| macOS Intel | `git-vault_darwin_amd64` |
| Windows x86-64 | `git-vault_windows_amd64.exe` |

```bash
curl -LO https://github.com/roman-16/git-vault/releases/latest/download/git-vault_linux_amd64
chmod +x git-vault_linux_amd64
sudo mv git-vault_linux_amd64 /usr/local/bin/git-vault
```

The `git-vault_<version>_<os>_<arch>.tar.gz` and `.zip` archives bundle the binary, the licence, the man pages, and completions for bash, zsh and fish.

### Verifying a download

```bash
curl -LO https://github.com/roman-16/git-vault/releases/latest/download/checksums.txt
sha256sum --check --ignore-missing checksums.txt
```

## Man pages and completions

Package installs (APT, AUR, Homebrew, RPM, APK) wire both up for you. After a script or manual install, the binary writes them itself:

```bash
git vault man ~/.local/share/man/man1
git vault completions bash > ~/.local/share/bash-completion/completions/git-vault
```

Man pages matter more here than usual: `git vault --help` is intercepted by git and turned into a man page lookup, so without them that one form fails. `git vault -h` and `git vault help <command>` always work.

## Checking it worked

```console
$ git-vault --version
git-vault 0.0.0

$ git vault --version
git-vault 0.0.0
```

Both forms are the same program: any executable named `git-vault` on your `PATH` is callable as `git vault`. The documentation uses `git vault` throughout.

## Per repository

Installing the binary is not enough for a given clone, because the wiring lives in `.git/config`, which is never committed:

```bash
git vault init     # a repository that does not have a vault yet
git vault unlock   # a clone of one that does
```

That is deliberate. A clone where nobody ran either command has no filters and no key, so it holds the sealed file and leaves it alone, which is exactly what somebody without access should get.

## Updating

If a package manager installed it, update with that: `apt upgrade`, `brew upgrade --cask git-vault`, `winget upgrade Roman-16.GitVault`, `yay -Syu`, `nix profile upgrade`, `cargo install git-vault-cli --force`.

After the install script or a manual download, git-vault updates itself:

```bash
git vault update             # install the latest release
git vault update --check     # only report whether an update exists
git vault update 1.2.0       # install a specific version
git vault update --reinstall # install again even if already current
```

It verifies the download against the release's `checksums.txt` before replacing anything, and refuses a package-managed install rather than fighting the package manager - naming the right command instead. `--check` answers whatever installed it, since asking is harmless.

This is the only command that touches the network. It honours `HTTPS_PROXY` and `NO_PROXY`, and `SSL_CERT_FILE` if your network terminates TLS with a private certificate authority.

The `.vault` format carries a version byte, and an older binary refuses a newer vault with a message telling you to update, rather than guessing.

## Uninstalling

If a package manager installed it, remove it with that (`apt remove git-vault`, `brew uninstall --cask git-vault`, `winget uninstall Roman-16.GitVault`).

After the install script or a manual download, the binary removes itself:

```bash
git vault uninstall --dry-run   # show what would go
git vault uninstall             # ask, then remove the binary
git vault uninstall --yes       # remove it without asking
git vault uninstall --yes --purge   # also delete your identity
```

**`--purge` cannot be undone.** It deletes `~/.config/git-vault/identity`, and every vault you are a recipient of becomes unreadable to you until somebody with access shares it with a new key.

Uninstalling does not touch your repositories, and their wiring lives in `.git/config`, which means `git add` will fail in them once the binary is gone. In each repository you used it in:

```bash
git config --remove-section filter.vault
git config --remove-section diff.vault
git config --remove-section merge.vault
git config --unset core.fsmonitor
rm .git/hooks/pre-commit
```

The sealed files stay where they are. To go back to plaintext secrets in git, run `git vault unlock`, then `git vault remove` the patterns and commit; to stop tracking them at all, delete `.vault/` and the two declaration lines.

Your identity at `~/.config/git-vault/identity` is yours and is not tied to any repository.
