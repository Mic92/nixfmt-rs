# Fuzzing

Coverage-guided fuzzing for the parser and formatter via
[`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz) / libFuzzer.

## Targets

| Target             | Property                                                                  |
| ------------------ | ------------------------------------------------------------------------- |
| `fuzz_parse`       | `parse()` never panics on arbitrary bytes; on `Err`, `format_error()` rendering also never panics. |
| `fuzz_roundtrip`   | `parse → format → parse` succeeds and yields the same AST modulo trivia.  |
| `fuzz_idempotent`  | `format` is idempotent: `format(format(x)) == format(x)` (and `format(x)` reparses). |
| `fuzz_debug_dumps` | `format_ast()` / `format_ir()` (the `--ast`/`--ir` debug renderers) never panic on parseable input. |

All targets are seeded from `tests/fixtures/nixfmt/` plus `fuzz/seeds/`. Run
`./fuzz/seed-corpus.sh` once to populate `fuzz/corpus/<target>/`. The files in
`fuzz/seeds/` are hand-written to exercise parser/printer branches the upstream
fixtures miss (bare URI literals, `~` paths, the legacy `let { }` form,
single-line indented strings, etc.); add more there when coverage shows a gap.

## Running

Use the dedicated dev shell, which exports `RUSTC_BOOTSTRAP=1` so the nixpkgs
rustc accepts cargo-fuzz's unstable flags:

```sh
nix develop .#fuzz
cargo fuzz run -s none fuzz_parse       -- -max_total_time=300 -timeout=10
cargo fuzz run -s none fuzz_roundtrip   -- -max_total_time=300 -timeout=10
cargo fuzz run -s none fuzz_idempotent  -- -max_total_time=300 -timeout=10
cargo fuzz run -s none fuzz_debug_dumps -- -max_total_time=300 -timeout=10
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
nix develop .#fuzz -c ./fuzz/coverage.sh all   # combined across every target
```

The merged profile lands in `fuzz/coverage/<target>/coverage.profdata` (or
`fuzz/coverage/combined/` for multiple targets); the script also prints the
`llvm-cov show --format=html` invocation for a browsable report.

`fuzz_roundtrip` alone caps at ≈ 83 % region coverage because it never reaches
the diagnostic renderer or the `--ast`/`--ir` debug printers; `all` is the
meaningful number for whole-crate coverage.

## Triage

```sh
# reproduce a crash
cargo fuzz run -s none <target> fuzz/artifacts/<target>/crash-<hash>

# minimise it
cargo fuzz tmin -s none <target> fuzz/artifacts/<target>/crash-<hash>
```

Minimised reproducers belong in `src/regression_tests/fuzz.rs`.

Bugs found and fixed so far are listed in `src/regression_tests/fuzz.rs`.
