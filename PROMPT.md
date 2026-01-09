## Objective

Match `nixfmt-rs`’s IR output with the reference Haskell `nixfmt` for the failing regression test:

```
cargo test regression_tests::ir::test_let_expression_groups -- --exact --nocapture
```

This test feeds the formatter `{ pinnedJson ? ./pinned.json, }: let ...` and currently panics because the IR trees differ.

## Current State

- `src/pretty.rs` controls IR pretty-printing.
- The failure comes from the formatting of a parenthesized application inside a `let` binding. Our IR still emits nested `Group RegularG` nodes where Haskell emits `Group Priority`, and the spacing/break structure is off.
- There are no local modifications aside from regression test scaffolding (`src/regression_tests/ir.rs` already contains the test cases).

## Recommended Workflow

1. **Reproduce the failure**
   ```sh
   cargo test regression_tests::ir::test_let_expression_groups -- --exact --nocapture
   ```
   Keep the output handy; it prints the expected (Haskell) and got (Rust) IR trees.

2. **Inspect the reference IR directly**
   ```sh
   printf '{ pinnedJson ? ./pinned.json, }: let pinned = (builtins.fromJSON (builtins.readFile pinnedJson)).pins; in pinned' \
     | nixfmt --ir -
   ```
   Compare to:
   ```sh
   printf '{ pinnedJson ? ./pinned.json, }: let pinned = (builtins.fromJSON (builtins.readFile pinnedJson)).pins; in pinned' \
     | cargo run --quiet -- --ir
   ```

3. **Focus on `src/pretty.rs`**
   - `push_absorb_paren`, `push_last_arg`, and the code path for `Expression::Application` inside `push_parenthesized_inner` control how parenthesized applications are grouped.
   - The goal is to mirror the structure in Haskell’s `Nixfmt/Pretty.hs` `absorbParen`/`absorbLast`.

4. **Iterate with small edits**
   - After each change run the single regression test.
   - Once it passes, run:
     ```sh
     cargo test regression_tests::ir::test_function_arguments_priority -- --exact --nocapture
     ```
     to ensure related application formatting still matches the reference.

5. **Final verification**
   - Run the two targeted regression tests plus `cargo test` to confirm there are no regressions.
   - If everything is green, prepare the diff for review.

## Deliverables

- Updated `src/pretty.rs` that makes both targeted regression tests pass and keeps overall IR formatting aligned with Haskell `nixfmt`.
- No temporary debug prints.
- Test evidence (at least the specific regression tests).

