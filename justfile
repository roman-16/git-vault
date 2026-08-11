[doc("Measure the hook git runs on every command that reads the index, and fail if it got slow")]
bench budget_us="2000": build
    #!/usr/bin/env bash
    set -euo pipefail
    work=$(mktemp --directory)
    trap 'rm -rf "$work"' EXIT
    mkdir -p "$work/bin" "$work/repo"
    ln --symbolic "$PWD/target/release/git-vault" "$work/bin/git-vault"
    export PATH="$work/bin:$PATH" GIT_VAULT_IDENTITY="$work/identity"
    export GIT_CONFIG_GLOBAL=/dev/null GIT_CONFIG_SYSTEM=/dev/null
    export GIT_AUTHOR_NAME=bench GIT_AUTHOR_EMAIL=bench@example.com
    export GIT_COMMITTER_NAME=bench GIT_COMMITTER_EMAIL=bench@example.com
    cd "$work/repo"
    git init --quiet --initial-branch main
    git-vault init >/dev/null
    printf 'secrets/ vault\n' >> .gitattributes
    printf 'secrets/\n' > .gitignore
    mkdir -p secrets
    for i in $(seq 12); do printf 'KEY_%s=value\n' "$i" > "secrets/s$i.env"; done
    git add --all >/dev/null
    git commit --quiet --message bench
    hyperfine --warmup 20 --runs 200 --shell=none --export-json "$work/measured.json" \
        'git-vault hook fsmonitor'
    mean=$(jq '.results[0].mean * 1000000 | round' "$work/measured.json")
    if [ "$mean" -gt '{{ budget_us }}' ]; then
        printf 'The hook averaged %s µs, over the %s µs budget.\n' "$mean" '{{ budget_us }}' >&2
        exit 1
    fi
    printf 'The hook averaged %s µs, inside the %s µs budget.\n' "$mean" '{{ budget_us }}'

[doc("Build the release binary")]
build:
    cargo build --release

[doc("Re-record the README panel by running a real session, then render it")]
demo: build
    #!/usr/bin/env bash
    set -euo pipefail
    ansi=scripts/terminal-demo/demo.ansi
    script --quiet --command "bash scripts/terminal-demo/record.sh" --return /dev/null \
        | sed --expression 's/\r$//' --expression 's/.*\r//' > "$ansi"
    recorded=$(wc --bytes < "$ansi")
    printf 'Recorded %s bytes of session.\n' "$recorded"
    if [ "$recorded" -lt 200 ]; then
        printf 'The recording is too short to be the demo, so `script` produced nothing usable here.\n' >&2
        cat --show-nonprinting "$ansi" >&2
        exit 1
    fi
    render() {
        freeze "$ansi" --config "scripts/terminal-demo/$1.json" --output "assets/demo-$1.svg"
        sed --in-place "s|\(<g font-family=[^>]*\)fill=\"[^\"]*\"|\1fill=\"$2\"|" "assets/demo-$1.svg"
    }
    render dark "#E6E8EC"
    render light "#14161C"

[doc("Build a throwaway multi-repo playground in example/ for testing by hand")]
example: build
    bash scripts/example.sh

[doc("Build the nix package from the working tree, which a flake only sees once tracked")]
flake:
    #!/usr/bin/env bash
    set -euo pipefail
    work=$(mktemp --directory)
    trap 'rm -rf "$work"' EXIT
    git ls-files --cached --others --exclude-standard -z 2>/dev/null | tar --create --null --files-from=- 2>/dev/null | tar --extract --directory="$work" ||
        tar --create --exclude=./target --exclude=./.devbox --exclude=./.direnv --exclude=./.git --exclude=./example . | tar --extract --directory="$work"
    cd "$work"
    git init --quiet
    git add --all
    nix build --print-build-logs

[doc("Update the snapshots that pin what every command prints")]
golden:
    cargo insta test --accept --unreferenced=delete

[doc("Fix and format everything fixable, then lint with no findings allowed")]
lint:
    cargo clippy --all-targets --all-features --fix --allow-dirty --allow-staged
    cargo fmt --all
    taplo fmt
    nixfmt flake.nix
    actionlint
    goreleaser check
    shellcheck --exclude=SC2016 scripts/*.sh scripts/terminal-demo/*.sh
    cargo clippy --all-targets --all-features -- --deny warnings

[doc("Build every release artifact without publishing, the way a tag would")]
snapshot:
    #!/usr/bin/env bash
    set -euo pipefail
    export CARGO_HOME="$PWD/target/release-toolchain/cargo"
    export RUSTUP_HOME="$PWD/target/release-toolchain/rustup"
    export AUR_KEY="${AUR_KEY:-unused}"
    export TAP_WINGET_TOKEN="${TAP_WINGET_TOKEN:-unused}"
    nix shell nixpkgs#cargo-zigbuild nixpkgs#gcc nixpkgs#goreleaser nixpkgs#rustup nixpkgs#zig \
        --command bash scripts/release-snapshot.sh

[doc("The whole suite, then the startup budget")]
test:
    cargo nextest run --all-features
    just bench

[doc("Run the suite against another git, by nixpkgs revision or branch (see .github/workflows/ci.yml)")]
test-git ref:
    nix shell github:NixOS/nixpkgs/{{ ref }}#git --command \
        sh -c 'git --version && cargo nextest run --all-features'

[doc("Run a single test (or a regex of test names)")]
test-one pattern:
    cargo nextest run --all-features -E 'test(/{{ pattern }}/)'

[doc("Move every dependency and tool to the latest version")]
update:
    cargo update
    devbox update
    just lint
    just test
