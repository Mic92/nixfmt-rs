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

Use the dedicated dev shell, which exports `RUSTC_BOOTSTRAP=1` so the nixpkgs
rustc accepts cargo-fuzz's unstable flags:

```sh
nix develop .#fuzz
cargo fuzz run -s none fuzz_parse      -- -max_total_time=300 -timeout=10
cargo fuzz run -s none fuzz_roundtrip  -- -max_total_time=300 -timeout=10
cargo fuzz run -s none fuzz_idempotent -- -max_total_time=300 -timeout=10
```

`-s none` is required: nixpkgs rustc does not ship the sanitizer runtimes, so
the default `-Zsanitizer=address` cannot link. libFuzzer's coverage-guided
engine still works; only AddressSanitizer's extra UB detection is lost.

## Coverage

The `fuzz` dev shell also provides version-matched `llvm-profdata` / `llvm-cov`.
`fuzz/coverage.sh` runs a target over its corpus and prints a per-file
line/region report:

```sh
nix develop .#fuzz -c ./fuzz/coverage.sh fuzz_roundtrip
```

The merged profile lands in `fuzz/coverage/<target>/coverage.profdata`; the
script also prints the `llvm-cov show --format=html` invocation for a browsable
report.

## Triage

```sh
# reproduce a crash
cargo fuzz run -s none <target> fuzz/artifacts/<target>/crash-<hash>

# minimise it
cargo fuzz tmin -s none <target> fuzz/artifacts/<target>/crash-<hash>
```

Minimised reproducers belong in `src/regression_tests/fuzz.rs`.

## Known upstream divergences

`fuzz_idempotent` checks `f² == f³` rather than `f¹ == f²` because upstream
Haskell nixfmt (which this project mirrors) has inputs that only stabilise on
the second pass, e.g. a trailing line comment immediately after a multi-line
string literal. Oscillation or unbounded growth is still caught.

Bugs found and fixed so far are listed in `src/regression_tests/fuzz.rs`.
