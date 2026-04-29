# nixfmt-rs

A from-scratch Rust reimplementation of [nixfmt] that produces byte-identical output to the Haskell original.

[nixfmt]: https://github.com/NixOS/nixfmt

## Status

**Parity reached** with upstream `nixfmt` v1.2.0:

- Byte-identical output across the entire nixpkgs tree
  (`LIMIT=0 cargo run --release --example diff_sweep`).
- Formats all of nixpkgs (≈43 k `.nix` files) in ≈2.8 s versus
  ≈71 s for the Haskell original — about 25× (Apple M3, 8‑core,
  16 GB).

See [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md) for how the pieces fit
together.

## Build & run

```bash
nix develop          # or: cargo build --release
cargo build --release

# Format (stdin → stdout)
echo '{a=1;}' | ./target/release/nixfmt_rs

# Format files / directories in place (recurses for *.nix, parallel)
./target/release/nixfmt_rs path/to/file.nix path/to/dir

# Check only (exit 1 if any file would change)
./target/release/nixfmt_rs -c path/to/dir

# Debugging modes (match `nixfmt --ast` / `nixfmt --ir` exactly)
echo '{a=1;}' | ./target/release/nixfmt_rs --ast
echo '{a=1;}' | ./target/release/nixfmt_rs --ir
```

## Testing

```bash
cargo test                       # full suite
cargo llvm-cov --html            # coverage report → target/llvm-cov/html/

# differential check vs. reference `nixfmt` over a nixpkgs checkout
# modes: format | ir | ast; env: NIXPKGS, LIMIT, JOBS, MAX_BYTES, REF, OUT
LIMIT=0 cargo run --release --example diff_sweep -- format
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
- **Minimal dependencies.** The library itself uses only `memchr` and
  `compact_str`; the binary adds `rayon` and `walkdir` for parallel
  directory formatting.
