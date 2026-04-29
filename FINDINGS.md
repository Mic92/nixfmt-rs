# Bug Findings Summary

## ✅ FIXED: Operator Associativity (3 bugs)
- **Fixed in:** `src/parser.rs:713-766`
- **Bugs:** `++`, `//`, `+` operators were left-associative, now correctly right-associative
- **Solution:** Added `is_right_associative()` helper and updated precedence climbing algorithm
- **Tests:** 6 regression tests in `tests/operator_associativity_test.rs` - all passing
- **Note:** `-` correctly remains left-associative (unlike `+`)

## Current nixpkgs Corpus Scan (`test_nixpkgs_compare.py`)
- **Command:** `./test_nixpkgs_compare.py`
- **Summary:** 23 files under `../nixpkgs` currently diverge from nixfmt.
  - Parser accepts -> nixfmt rejects (7): `shell.nix`, `flake.nix`, `default.nix`, `ci/parse.nix`, `ci/default.nix`, `lib/trivial.nix`, `lib/flake.nix`
- Both parse but AST differs (11): `ci/nixpkgs-vet.nix`, `lib/strings-with-deps.nix`, `lib/filesystem.nix`, `lib/debug.nix`, `lib/cli.nix`, `lib/meta.nix`, `lib/source-types.nix`, `lib/flake-version-info.nix`, `lib/versions.nix`, `lib/sources.nix`, `lib/gvariant.nix`
- Additional AST drift (multiline strings) in NixOS networking modules such as `nixos/modules/services/networking/tailscale.nix`, `skydns.nix`, `radvd.nix`, `vwifi.nix`, `soju.nix`, `syncthing-relay.nix`, `inadyn.nix`, `frr.nix`, `wireguard-networkd.nix`, `centrifugo.nix`, `matterbridge.nix`, `aria2.nix`, `wgautomesh.nix`, `ivpn.nix`, `firewall-nftables.nix`, `minidlna.nix`, `hickory-dns.nix`, `tailscale.nix`, `whoogle-search.nix`, `legit.nix`, `tailscale-derper.nix`, `dnscache.nix`, `quicktun.nix`, `zeronsd.nix`, `https-dns-proxy.nix` — traced to multiline string indentation handling (see BUG #15).
  - nixfmt timeouts (5): `maintainers/team-list.nix`, `maintainers/maintainer-list.nix`, `lib/attrsets.nix`, `lib/generators.nix`, `lib/types.nix`
- **Next steps:** Investigate parse failures first (string selector fallout), then focus on AST drift within `lib/` utilities; timeouts may require nixfmt invocation tweaks or sample reduction.

## ✅ NOT A BUG #1: Double Negation (`- -1`, `--1`)
- **Status:** NOT A BUG - nixfmt is overly restrictive (verified 2025-10-25)
- **File:** Parser (`src/parser.rs:618-665`)
- **Severity:** N/A (valid Nix syntax)
- **Issue:** Our parser correctly accepts stacked unary negation operators like `- -1` and `--1`, creating nested `Negation` nodes. While nixfmt rejects these, **Nix itself accepts and evaluates them correctly** (`nix eval --expr "- -1"` returns `1`).
- **Minimal reproducers:** `- -1` (→ 1), `--1` (→ 1), `---1` (→ -1)
- **Notes:** This is a **nixfmt bug**, not ours. Our behavior matches the actual Nix evaluator.

## ✅ NOT A BUG #2: Double Inversion (`!!true`)
- **Status:** NOT A BUG - nixfmt is overly restrictive (verified 2025-10-25)
- **File:** Parser (`src/parser.rs:618-665`)
- **Severity:** N/A (valid Nix syntax)
- **Issue:** Our parser correctly accepts stacked boolean inversion operators like `!!true`, creating nested `Inversion` nodes. While nixfmt rejects these, **Nix itself accepts and evaluates them correctly** (`nix eval --expr "!!true"` returns `true`).
- **Minimal reproducers:** `!!true` (→ true), `!!!true` (→ false)
- **Notes:** This is a **nixfmt bug**, not ours. Our behavior matches the actual Nix evaluator.

## ✅ BUG #3: Comparison Chain (`1 < 2 < 3`)
- **Status:** FIXED
- **File:** Parser (operator precedence) — handled in `src/parser.rs`
- **Severity:** Medium (regression test guards against regressions)
- **Fix:** Comparison operators now short-circuit on the first comparison instead of allowing chains.
- **Verification:** `tests/regression_tests.rs:48` (`regression_comparison_chain_should_fail`) asserts `parse("a == b == c")` returns an error; passes against nixfmt.
- **Residual risk:** Mixed comparison/operator combinations still need broader fuzzing.

## ❌ BUG #4: Mixed Addition/Subtraction Associativity (`1 + 2 - 3`)
- **Status:** STILL UNFIXED (verified 2025-10-25)
- **File:** Parser (`src/parser.rs:650-720`)
- **Severity:** Low (AST mismatch)
- **Issue:** Our AST nests as `(1 + (2 - 3))`, while nixfmt keeps `( (1 + 2) - 3 )`. The custom right-associative handling for `+` diverges once `-` appears.
- **Minimal reproducer:** `1 + 2 - 3`
- **Notes:** Confirmed via `nixfmt --ast` diff; no behavioural regression yet but affects AST comparisons.

## ❌ BUG #5: Empty String (`""`)
- **Status:** STILL UNFIXED (verified 2025-10-25)
- **File:** Pretty printer (`tests/common/mod.rs` diff display)
- **Severity:** Low (cosmetic AST formatting difference)
- **Issue:** We pretty-print the nested brackets across two lines, whereas nixfmt keeps `[ [] ]` on one line. Fonctionally identical but breaks string comparisons.
- **Minimal reproducer:** `""`
- **Notes:** Parser structures align; only pretty_simple output needs tightening.

## ✅ BUG #6: `or` Keyword as Identifier
- **Status:** FIXED
- **File:** Lexer/Parser keyword handling (`src/lexer/token.rs`, `src/parser.rs`)
- **Severity:** Medium (previously rejected valid identifier)
- **Fix:** Context-sensitive treatment of `or` so bare usages lex as identifiers unless in `attr or default` position.
- **Verification:** `tests/regression_tests.rs:27` (`regression_or_as_identifier`) compares AST with nixfmt; the test passes.
- **Residual risk:** Complex `inherit (foo) or ...` combinations still worth fuzzing.

## ✅ BUG #7: String & Interpolated Attribute Selectors
- **Status:** FIXED
- **File:** Parser (`src/parser.rs` — selector parsing)
- **Severity:** High (previously rejected valid syntax)
- **Fix:** Selector parsing now accepts quoted and interpolated keys across attribute sets, `inherit`, and selectors.
- **Verification:** Regression suite in `tests/regression_tests.rs`:
  - `regression_string_selector` / `regression_string_selector_interpolation_*`
  - `regression_attrset_*` and `regression_let_*`
  All compare our AST with nixfmt and pass.
- **Residual risk:** Keep an eye on complex nested interpolations and attrset rewrites uncovered by the nixpkgs corpus.

## ✅ BUG #8: Numeric Literal Lexing
- **Status:** FIXED
- **File:** Lexer (number parsing)
- **Severity:** High (previously accepted invalid syntax & rejected valid literals)
- **Fix:** Reworked float lexing to mirror `Nixfmt.Parser.Float`: require a decimal point, allow trailing dots only for non-zero prefixes, block multi-zero prefixes, and support optional exponents.
- **Verification:** Regression tests in `tests/regression_tests.rs` (`regression_float_trailing_dot`, `regression_float_with_exponent`, `regression_float_leading_dot_exponent`, `regression_float_double_zero_prefix`) match nixfmt output.
- **Residual risk:** Edge cases around giant exponents and lexer backtracking still benefit from fuzzing, but behaviour now aligns with upstream nixfmt.

## ❌ BUG #9: Path Trailing Slashes / Bare Separators
- **Status:** STILL UNFIXED (verified 2025-10-25)
- **File:** Parser (`src/parser.rs:900-990`)
- **Severity:** Low (accepts invalid syntax)
- **Issue:** We happily produce a `Path` AST for dangling separators like `./`, `~/`, `./foo/` that nixfmt rejects outright.
- **Minimal reproducers:** `./`, `~/`, `./foo/`, `~/foo/`
- **Notes:** Rooted in `parse_path` accumulating trailing `/` characters without verifying a following part.

## ✅ NOT A BUG #10: Unary Operator Stacking (Extended)
- **Status:** NOT A BUG - nixfmt is overly restrictive (verified 2025-10-25)
- **File:** Parser (`src/parser.rs:618-665`)
- **Severity:** N/A (valid Nix syntax)
- **Issue:** Mixed chains like `-!true` or `!-x` parse correctly in our implementation. While nixfmt rejects these, they are actually **valid Nix syntax** that the Nix evaluator accepts (though they result in type errors at runtime for incompatible types).
- **Minimal reproducers:** `!!!true` (→ false), `!-1` (type error), `-!true` (type error), `---1` (→ -1)
- **Notes:** This is a **nixfmt bug**, not ours. Our behavior matches the actual Nix evaluator's parser.

## ❌ BUG #11: Comparison Operator Chaining (Extended)
- **Status:** UNFIXED (extends BUG #3)
- **File:** Parser (operator precedence)
- **Severity:** Medium (accepts invalid syntax)
- **Issue:** We allow chaining for every comparison operator (`==`, `!=`, `<=`, `>=`, `>`, `<`) even though nixfmt stops at a single comparison.
- **Minimal reproducers:** `a == b == c`, `a != b != c`, `a <= b >= c`, `a < b > c`

## ❌ BUG #12: NOT with Member Check
- **Status:** STILL UNFIXED (verified 2025-10-25)
- **File:** Parser (`src/parser.rs:288-344`)
- **Severity:** Low (AST mismatch)
- **Issue:** `!a ? b` nests as `MemberCheck(Inversion(a)…` for us; nixfmt keeps the inversion outermost. Precedence between prefix `!` and postfix `?` remains incorrect.
- **Minimal reproducer:** `!a ? b`
- **Notes:** Confirmed via side-by-side `--ast` diff.

## ✅ BUG #13: Comment/Whitespace AST Formatting
- **Status:** FIXED
- **File:** Parser (`src/parser.rs` — `parse_binders` / `parse_list_items`)
- **Severity:** Low (AST formatting differences)
- **Fix:** Preserve trailing `pre_trivia` before closing delimiters by emitting explicit `Item::Comments` entries, matching nixfmt's handling of `Comments [EmptyLine]`.
- **Verification:** Regression `tests/regression_tests.rs:84` (`regression_attrset_trailing_empty_line`) now passes and nixpkgs attrset diffs disappear.
- **Observed in nixpkgs:** `lib/strings-with-deps.nix`, `lib/filesystem.nix`, `lib/debug.nix`, `lib/cli.nix`, `lib/meta.nix`, `lib/source-types.nix`, `lib/flake-version-info.nix`, `lib/versions.nix`, `lib/sources.nix`, `lib/gvariant.nix`

## ✅ BUG #14: Path/Import Function Application Requires Parentheses
- **Status:** FIXED
- **File:** Parser (`src/parser.rs` — `parse_postfix_selection`)
- **Severity:** High (previously rejected idiomatic nixpkgs usage)
- **Fix:** `.` now peeks ahead to confirm a selector before consuming; otherwise we leave it for the path parser, allowing literals like `./foo.nix` to act as function callees.
- **Verification:** Regression `tests/regression_tests.rs:70` (`regression_import_path_application`) passes; `nixfmt_rs --ast ../nixpkgs/flake.nix` succeeds again.
- **Observed in nixpkgs:** `flake.nix`, `default.nix`, `shell.nix`, `ci/parse.nix`, `ci/default.nix`, `lib/trivial.nix`, `lib/flake.nix` now parse without error.

## ✅ BUG #15: Multiline `''` String Indentation Normalisation
- **Status:** FIXED
- **File:** Lexer / Parser (`src/lexer.rs:585-631`, `src/parser.rs:1235-1322`)
- **Severity:** High (string content mismatch)
- **Fix:** After parsing `${…}` inside indented strings, rewind lexer trivia (new `rewind_trivia`) so newline + indentation remain part of the string, mirroring nixfmt's `stripIndentation`.
- **Verification:** Regression `tests/regression_tests.rs:95` (`regression_multiline_string_indentation`) passes; tailscale-related nixpkgs modules align with nixfmt.
- **Observed in nixpkgs:** Networking modules (`tailscale.nix`, `skydns.nix`, `soju.nix`, etc.) and top-level `flake.nix`, `default.nix`, `shell.nix`.

## Test Coverage Summary

**Total Test Cases:** 93
**Passing:** 46 (49%)
**Bugs Found:** 10 distinct bugs (3 "NOT A BUG" items where nixfmt is overly restrictive)

### Test Categories (focused suite):
- String/dynamic selectors & bindings: 14 tests (now passing after BUG #7 fix)
 - Numeric literal variants: 19 tests (now passing after BUG #8 fix)
- Unary operator stacking: 10 tests (NOT A BUG - valid Nix syntax that nixfmt incorrectly rejects)
- Comparison operator chains: 7 tests (guard against BUG #3/#11 regressions)
- Path parsing edge cases: 14 tests (new failures for dangling `./` and `~/`)
- `or` keyword as identifier: 7 tests (now passing after BUG #6 fix)
- Inherit variations (empty/from/multiple): 8 tests
- Whitespace + comments trivia: 11 tests (now passing after BUG #13 fix)

### Current Focus:
Keeping the archived exhaustive suite commented out, but actively hammering parser weak spots (string selectors, numbers, unary operators, comparison chains, trivia preservation) uncovered via nixfmt's regression corpus. Re-enable the historic set after addressing these regressions.

## 2026-04-29 — Differential format sweep (`scripts/diff_sweep.sh`)

Swept the first 2000 `*.nix` files under `~/git/nixpkgs/pkgs/` comparing
final formatted output of `./target/release/nixfmt_rs` against `nixfmt`
v1.2.0. 1197/2000 files diverged. Mismatches cluster into the following
root causes (each has a minimised reproducer in
`src/regression_tests/format.rs`):

| # | Reproducer | Haskell reference | Status |
|---|---|---|---|
| A | `{ a, b }: a` → we emitted `{ a, b, }: a` | `Nixfmt.Predoc.fits` drops `Text Trailing` in compact groups | **Fixed** in `src/predoc.rs` (`fits`) |
| B | `f (x: { …multiline… })` not absorbed onto `f` line | `Nixfmt.Pretty.absorbLast` / `isAbsorbableExpr` | **Fixed** by `pretty.rs` rewrite (chain) |
| C | `a: b: { … }` breaks before `{` | `Nixfmt.Pretty.absorbAbs` | **Fixed** by `pretty.rs` rewrite (chain) |
| D | `with X; { … }` breaks before `{` (lambda body & RHS) | `Nixfmt.Pretty` `With` instance / `absorbRHS` | **Fixed** by `pretty.rs` rewrite (chain) |
| E | `x = f "a" ''…'';` pushes application to next line | `Nixfmt.Pretty.absorbRHS` (Application) | **Fixed** by `pretty.rs` rewrite (chain) |
| F | `if … else if …` stays single-line; nixfmt forces expand | `Nixfmt.Pretty.prettyIf` | **Fixed** by `pretty.rs` rewrite (chain) |
| G | expanded `runCommand "n" {…} ''…''` loses 2-space continuation indent and drops first arg to next line | `Nixfmt.Pretty.prettyApp` | **Fixed** by `pretty.rs` rewrite (chain) |

Class A was a one-line layout bug where `fits()` rendered
`TextAnn::Trailing` despite the comment saying it shouldn't. Classes
B–G were IR-generation gaps in `src/pretty.rs`; after rebasing onto the
`comments-cleanup` chain (which carries the `pretty.rs`/`predoc.rs`
restructuring from earlier branches) all seven reproducers pass and are
enabled as active regression tests in `src/regression_tests/format.rs`.
