# Fuzzing

Coverage-guided fuzzing for the parser and formatter via
[`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz) / libFuzzer.

## Targets

| Target            | Property                                                                  |
| ----------------- | ------------------------------------------------------------------------- |
| `fuzz_parse`      | `parse()` never panics/hangs/OOMs on arbitrary bytes (errors are fine).   |
| `fuzz_roundtrip`  | `parse → format → parse` succeeds and yields the same AST modulo trivia.  |
| `fuzz_idempotent` | `format` converges: `format²(x) == format³(x)` (and `format(x)` reparses).|

All targets are seeded from `tests/fixtures/nixfmt/`. Run
`./fuzz/seed-corpus.sh` once to populate `fuzz/corpus/<target>/`.

## Running

`cargo-fuzz` requires a nightly toolchain. From the repo root:

```sh
# one-time
rustup toolchain install nightly

# run a target for 5 minutes
cargo +nightly fuzz run fuzz_parse      -- -max_total_time=300 -timeout=10
cargo +nightly fuzz run fuzz_roundtrip  -- -max_total_time=300 -timeout=10
cargo +nightly fuzz run fuzz_idempotent -- -max_total_time=300 -timeout=10
```

With Nix (no rustup):

```sh
nix run nixpkgs#cargo-fuzz -- run fuzz_parse -- -max_total_time=300
```

On macOS with a Nix-provided toolchain you may need
`LIBRARY_PATH=$(nix build --print-out-paths nixpkgs#libiconv)/lib` for the
libfuzzer build script to link.

## Triage

```sh
# reproduce a crash
cargo +nightly fuzz run <target> fuzz/artifacts/<target>/crash-<hash>

# minimise it
cargo +nightly fuzz tmin <target> fuzz/artifacts/<target>/crash-<hash>
```

Minimised reproducers belong in `src/regression_tests/fuzz.rs`.

## Known upstream divergences

`fuzz_idempotent` checks `f² == f³` rather than `f¹ == f²` because upstream
Haskell nixfmt (which this project mirrors) has inputs that only stabilise on
the second pass, e.g. a trailing line comment immediately after a multi-line
string literal. Oscillation or unbounded growth is still caught.

Bugs found and fixed so far are listed in `src/regression_tests/fuzz.rs`.
