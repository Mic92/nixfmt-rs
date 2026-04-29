# Test suite layout

This crate has four tiers of tests. Pick the lowest tier that catches your bug.

## 1. Smoke / unit tests (`src/**/*.rs` `#[cfg(test)]`)

Tiny, fast assertions colocated with the code they exercise (lexer, parser,
predoc layout helpers). Add here when testing a single function in isolation.

## 2. Regression tests (`src/regression_tests/`)

- `parser.rs` — inputs the parser used to reject or mis-parse.
- `ir.rs` — IR-construction regressions (pretty tree shape).
- `format.rs` — minimal end-to-end `format(input) == expected` reproducers,
  one per upstream-`nixfmt` divergence found in the wild (nixpkgs sweep).

Each case should be the *smallest* input that triggers the bug, with a doc
comment naming the Haskell function it mirrors. If the case is a minimisation
of something already in the fixture corpus, keep it (it runs in milliseconds)
but add a `/// Fixture:` line pointing at the corresponding
`tests/fixtures/nixfmt/diff/<name>/` directory.

## 3. Fixture corpus (`tests/fixtures/nixfmt/`)

Vendored verbatim from upstream `nixfmt`'s test suite:

- `correct/` — already-formatted snippets (format must be a no-op).
- `diff/*/in.nix` + `out.nix` (+ `out-pure.nix`) — input/golden pairs.
- `invalid/` — inputs the parser must reject.

Do **not** hand-edit these; re-vendor from upstream when bumping the target
`nixfmt` version.

## 4. Property tests (`src/regression_tests/properties.rs`)

Sweeps the entire fixture corpus and asserts, per file:

1. `idempotent_on_fixture_corpus` — `format(format(x)) == format(x)`.
2. `ast_preserved_on_fixture_corpus` — `parse(format(x)) ≡ parse(x)` modulo
   trivia.
3. `formats_to_golden_on_fixture_corpus` — `format(diff/*/in.nix) == out.nix`.
   Mismatches here are *logged, not failed*: they track remaining divergence
   from the reference formatter and are expected to shrink over time.

## When to add what

| You have…                                   | Add to…                              |
| ------------------------------------------- | ------------------------------------ |
| A new parser/lexer edge case                | `regression_tests/parser.rs`         |
| A formatting divergence vs. upstream nixfmt | `regression_tests/format.rs` (minimal repro) |
| A new invariant over *all* inputs           | `regression_tests/properties.rs`     |
| Upstream added test cases                   | re-vendor `tests/fixtures/nixfmt/`   |

Prefer a minimal regression test over a new fixture: fixtures come from
upstream, regressions are ours.
