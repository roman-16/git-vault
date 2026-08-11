# Agent Guidelines

## Quality Gates

After changing anything, run these in order. Stop at the first failure and fix it.

| | Gate | Run it when |
| --- | --- | --- |
| 1 | `just test` - the whole suite, then the startup budget. Hermetic: no network, no credentials, nothing to set up | always |
| 2 | `just lint` - fixes what is fixable, then denies every clippy warning, checks the workflows, the release config and the scripts | always |
| 3 | `just test-git nixpkgs-unstable` - the suite against the newest packaged git. Any pinned revision from `.github/workflows/ci.yml` works too | anything that talks to git |
| 4 | `just snapshot` - builds every release artifact for all five targets without publishing | `.goreleaser.yaml`, dependencies, install scripts |
| 5 | `just flake` - builds the nix package from the working tree, which a flake only sees once tracked | `flake.nix`, dependencies |
| 6 | `just demo` - re-records the README panel; `scripts/terminal-demo/demo.ansi` must come out byte-identical | any command's output changed |

**Read the exit code, not the output.** `just lint` can fail while printing nothing you grepped for, and `cmd && echo ok` prints nothing at all on failure. Check `$?`.

**Never report a gate as passing without having run it.** A stale binary, a cached clippy result, or a text replacement that silently matched nothing all produce green-looking output. To claim a fix works, break it deliberately, watch the test fail, restore it, watch it pass.
