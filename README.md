# nixfmt-rs

A from-scratch Rust reimplementation of [nixfmt] that produces byte-identical output to the Haskell original.

Try it in your browser: <https://mic92.github.io/nixfmt-rs/> (WebAssembly build, formats locally — no upload).

[nixfmt]: https://github.com/NixOS/nixfmt

## Status

**Parity reached** with upstream `nixfmt` v1.2.0:

- Byte-identical output across the entire nixpkgs tree
  (`LIMIT=0 cargo run --release --features sweep --example diff_sweep`).
- Formats all of nixpkgs in <2 s — see [Benchmarks](#benchmarks).

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

# differential check vs. reference `nixfmt` over a nixpkgs checkout
# modes: format | ir | ast; env: NIXPKGS, LIMIT, JOBS, MAX_BYTES, REF, OUT
LIMIT=0 cargo run --release --features sweep --example diff_sweep -- format
```

The test suite is layered (unit → regression → vendored fixtures →
properties); see [`tests/README.md`](tests/README.md) for where to add
new cases.

## Error messages

Parse errors come with source snippets, related spans and fix-it hints:

```
Error[E001]: expected ';', found '='
   ┌─ config.nix:2:27
   │
 1 │ {
 2 │   services.nginx.enable = true
   │                           ^^^^
 3 │   networking.firewall.enable = false;
   = note: missing semicolon after definition
   = help: add a semicolon at the end of the previous line
```

```
Error[E002]: unclosed delimiter '{'
   ┌─ config.nix:5:1
   │
 3 │   bar = {
   │         - unclosed delimiter opened here
 4 │     baz = 2;
 5 │ }
   │ ^
   = help: add closing '}'
```

```
Error[E005]: commas are not used to separate list elements in Nix
   ┌─ config.nix:1:4
   │
 1 │ [ 1, 2, 3 ]
   │    ^
   = help: use spaces to separate list elements: [1 2 3]
```

Run `cargo run --example error_visualization` to see the full catalogue
of diagnostics on intentionally broken inputs.

## Benchmarks

`--check` over a full nixpkgs checkout (42 942 `.nix` files), AMD EPYC
7713P 64-core, nixfmt 1.2.0, treefmt 2.5.0. treefmt runs use `--no-cache`
so every file is actually processed:

| command                           | wall time  | user time | vs nixfmt-rs |
| --------------------------------- | ---------- | --------- | ------------ |
| `nixfmt-rs --check .`             | **1.68 s** | 9.34 s    | 1.00×        |
| `treefmt` driving nixfmt-rs       | 3.35 s     | 10.14 s   | 1.99×        |
| `nixfmt-tree` (treefmt + Haskell) | 38.89 s    | 216.2 s   | 23.2×        |
| `nixfmt --check .` (Haskell)      | 220.67 s   | 214.4 s   | 131×         |

Single large file (`all-packages.nix`, ~12 k lines): 36.8 ms vs 762.7 ms
(20.7×).

Reproduce with [`scripts/bench.sh`](scripts/bench.sh); the dev shell
provides `hyperfine` and the script defaults to the nixpkgs revision
pinned in `flake.lock`, so `nix develop -c scripts/bench.sh` is
self-contained.

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
