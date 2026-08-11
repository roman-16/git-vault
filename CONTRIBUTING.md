# Contributing

Thanks for helping out. Issues, ideas and pull requests are all welcome.

## Getting set up

The repository uses [devbox](https://www.jetify.com/devbox) and [direnv](https://direnv.net/) to pin the toolchain:

```bash
git clone https://github.com/roman-16/git-vault
cd git-vault
direnv allow      # or: devbox shell
```

Without devbox you need a current Rust toolchain, plus `just`, `actionlint`, `cargo-insta`, `cargo-nextest`, `charm-freeze`, `goreleaser`, `hyperfine`, `jq`, `nixfmt`, `shellcheck` and `taplo` for the tasks below.

## Everyday commands

```bash
just lint         # format, lint, and every gate below
just test         # the whole suite, about a second
just test-one <pattern>
just test-git <nixpkgs-rev>   # the suite against another git version
just example      # a throwaway playground to try it by hand
just bench        # the hook that runs on every git command, against a budget
just golden       # accept new output snapshots
just update       # move every dependency and tool to the latest version
just demo         # re-record the README panel
```

`just lint` and `just test` both have to pass with nothing to report. `lint` also fails on any clippy warning, and runs `actionlint` over the workflows and `goreleaser check` over the release configuration.

## Tests

`tests/AGENTS.md` is the guide: what each suite is for, what the harness gives you, and the traps that have already caught somebody.

The short version: unit tests live beside the code, `tests/` drives the real binary against real repositories with real git, and the output of every command is pinned byte for byte with `insta`. Nothing needs credentials or a network, and the whole suite runs in about a second.

CI runs it against git 2.30, 2.34, 2.43, 2.51 and 2.55, because this tool depends on specific git behaviour rather than on documented API. You can do the same locally:

```bash
just test-git b4e193a23a1c5d8794794e65cabf1f1135d07fd9   # git 2.30.0
```

### The one performance number that matters

The `core.fsmonitor` hook runs at the start of every git command that reads the index, so its startup cost is paid constantly. `just bench` measures it and fails above 2000 µs. On a developer machine it is around 700 µs; the budget is loose because CI runners are shared and noisy, and it is there to catch a regression of the kind that comes from spawning another process or re-reading the whole vault, not to police microseconds.

## Trying it by hand

`just example` builds a throwaway playground under `example/`, which is ignored by git and rebuilt from scratch every time you run it:

```bash
just example
source example/activate
```

That puts the release binary on `PATH` as `git-vault`, scrubs the git environment so nothing touches your real config or your real identity in `~/.config/git-vault`, and gives you three people who share `example/origin.git`:

| Command | Who you become |
| --- | --- |
| `you` | The owner. Unlocked, secrets on disk, `origin` set up |
| `mate` | A second recipient. Also unlocked, so `push` and `pull` between the two work |
| `stranger` | A clone with no access, which is what somebody outside the team sees |

Each has their own identity in `example/identities`, so `share`, `revoke` and `rotate` can be exercised for real rather than imagined.

This is for hand testing and for reproducing a bug before writing the test that pins it. It asserts nothing: the suite in `tests/` is what proves behaviour.

## The README panel

`just demo` records a real session with `script`, strips the carriage returns, and renders `scripts/terminal-demo/demo.ansi` into `assets/demo-dark.svg` and `assets/demo-light.svg` with [freeze](https://github.com/charmbracelet/freeze). The recording is committed and CI re-records it, failing if a single byte differs, so the panel cannot drift from what the tool actually prints.

The recipe hands freeze its input with stdin redirected from `/dev/null`, because freeze reads stdin whenever stdin is a pipe and ignores the file it was given. Under a CI runner, which pipes stdin, it would otherwise render nothing and fail with `Language Unknown`.

## Style

**No comments.** Not few: none. If something needs explaining, rename it, split it, or restructure it until it explains itself; wanting a comment is a signal that the code is wrong. The tree has zero, in Rust and in configuration alike.

Where prose is genuinely part of the interface, it goes somewhere a tool can see: command help in `#[command(about = ...)]`, `just --list` descriptions in `[doc("...")]`, the reasons behind the git configuration in what `git vault doctor` prints, and everything else in `docs/`. `scripts/install.ps1` is the one exception, because PowerShell's `Get-Help` is comment-based and has no other mechanism.

The lints in `Cargo.toml` are strict on purpose: no `unwrap`, no `expect`, no panics, no indexing, no bare integer arithmetic, no `as` casts, and `clippy::pedantic` plus `clippy::nursery` are denied. Tests get `unwrap` and friends back through `clippy.toml`. Prototyping belongs in a test.

Those lints shape the design rather than just the style. All offset arithmetic lives in one checked reader, so a hostile `.vault/data` produces an error instead of a panic; exit codes are a typed enum because `std::process::exit` is denied; and error paths are values rather than assertions.

Escape hatches are `#[expect(lint, reason = "…")]`, never `#[allow]`, so a hatch that stops being needed fails the build.

## Pull requests

- Keep the change focused, and match the surrounding style.
- Run `just lint` and `just test`.
- Add tests for behaviour you change. If you change what a command prints, run `just golden` and let the snapshot diff be the review.
- Update `docs/` when the behaviour is user-facing, and the README when it is prominent.
- Commit messages follow [Conventional Commits](https://www.conventionalcommits.org/).

## Where things are

| Path | Contents |
| --- | --- |
| `src/main.rs` | Entry point, and the fast path for the hooks git calls on every command |
| `src/cli.rs` | The command surface |
| `src/cmd/` | One module per command |
| `src/filter/` | What git invokes: clean, smudge, textconv, merge, fsmonitor |
| `src/vault/` | The format, sealing, keys, recipients |
| `src/repo/` | Finding the repository, patterns, the worktree, wiring, hooks |
| `tests/` | Integration suites, driving the real binary |
| `docs/` | User documentation |
| `scripts/terminal-demo/` | The README panel, recorded from a real session |

## Releases

Releases are started by hand: run the **Release** workflow with a version like `1.0.0`. It bumps `Cargo.toml`, commits, tags, and pushes, then [GoReleaser](https://goreleaser.com) builds every target and publishes to the GitHub release, apk/deb/rpm, the APT repository, AUR, Homebrew and winget, with crates.io alongside. `.goreleaser.yaml` is the configuration and `just lint` validates it with `goreleaser check`.

Every channel after the GitHub release is `continue-on-error`, and the final job polls each registry and writes a status table, so one dead registry cannot fail a release and nothing fails silently. Re-running the workflow with the same version and `skip-tag-check` resumes from the existing tag.

## Security

Please do not file security issues publicly. [`SECURITY.md`](SECURITY.md) has the private channel.
