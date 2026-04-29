# nixfmt-rs

A from-scratch Rust reimplementation of [nixfmt] that produces
byte-identical output to the Haskell original, with no runtime
dependencies and a single static binary.

[nixfmt]: https://github.com/NixOS/nixfmt

## Status

**Parity reached** with upstream `nixfmt` v1.2.0 (2026-04-29):

- 0 / 2000 divergences on the nixpkgs `pkgs/` differential sweep
  (`scripts/diff_sweep.sh`).
- 211 / 211 tests green (unit, regression, vendored upstream fixture
  corpus, property tests).
- `~/git/nixpkgs/pkgs/top-level/all-packages.nix` formats in ≈70 ms
  (release build, M-series mac).

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for how the pieces fit
together.

## Build & run

```bash
nix develop          # or: cargo build --release
cargo build --release

# Format (stdin → stdout)
echo '{a=1;}' | ./target/release/nixfmt_rs

# Debugging modes (match `nixfmt --ast` / `nixfmt --ir` exactly)
echo '{a=1;}' | ./target/release/nixfmt_rs --ast
echo '{a=1;}' | ./target/release/nixfmt_rs --ir
```

## Testing

```bash
cargo test                       # full suite (211 tests)
cargo llvm-cov --html            # coverage report → target/llvm-cov/html/
scripts/diff_sweep.sh            # differential check vs. `nixfmt` over nixpkgs
```

The test suite is layered (unit → regression → vendored fixtures →
properties); see [`tests/README.md`](tests/README.md) for where to add
new cases.

## Design goals

- **Exact behavioural parity.** `--ast`, `--ir` and formatted output are
  diffable byte-for-byte against the Haskell implementation, so any
  divergence can be bisected mechanically.
- **Hand-written recursive-descent parser.** No parser-combinator or
  grammar generator; the structure mirrors `Nixfmt/Parser.hs` directly,
  which keeps error messages and trivia handling under our control.
- **Zero dependencies** in the `[dependencies]` section.
