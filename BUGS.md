# Known Bugs and Issues

## IR Representation Differences

### Issue: Nested Function Application IR Structure Differs from nixfmt

**Status:** Known limitation, does not affect output

**Description:**
The internal representation (IR) of nested function applications differs from the reference nixfmt implementation, particularly when dealing with parenthesized function applications.

**Example Input:**
```nix
{ pinnedJson ? ./pinned.json, }: let pinned = (builtins.fromJSON (builtins.readFile pinnedJson)).pins; in pinned
```

**Impact:**
- The final formatted output is correct and matches nixfmt
- Only the internal IR representation differs
- Indentation levels in the IR are different (e.g., levels 2-5 vs 3-7)
- Group structure nesting differs in parenthesized applications

**Details:**

The differences appear in:
1. **Indentation tracking**: nixfmt tracks deeper indentation levels through nested `nest` calls
2. **Group annotations**: nixfmt uses more fine-grained Priority/Transparent group annotations
3. **Spacing placement**: The placement of `Spacing Space/Break` elements differs

For example, in the expression `(builtins.fromJSON (builtins.readFile pinnedJson))`:

**nixfmt IR structure:**
```
Group RegularG [
  Spacing Space,
  Group RegularG [
    Text "(",
    Group RegularG [
      Text "builtins.fromJSON" (level 5),
      Spacing Hardspace,
      Group Priority [
        Text "(" (level 5),
        Spacing Break,
        Group RegularG [
          Text "builtins.readFile" (level 6),
          ...
```

**nixfmt-rs IR structure:**
```
Group RegularG [
  Group RegularG [
    Text "(",
    Group RegularG [
      Spacing Break,
      Text "builtins.fromJSON" (level 4)
    ],
    Spacing Hardspace,
    Group RegularG [
      Text "(",
      Group RegularG [
        Spacing Break,
        Text "builtins.readFile" (level 5)
      ],
      ...
```

**Root Cause:**
The implementation of `prettyApp` and related functions (`absorb_app`, `absorb_inner`, `push_absorb_paren`) creates a different group/nesting structure than the Haskell reference implementation, even though the logical structure is correct.

**Affected Tests:**
- `ir_format_tests::test_let_expression_groups`

**Workaround:**
This is an internal implementation detail. Users are not affected as the formatted output is correct.

**Future Work:**
To fully match the nixfmt IR:
1. Review the precise semantics of `nest` in the Haskell implementation vs our `push_nested`
2. Ensure proper propagation of indentation levels through nested groups
3. Match the exact placement of `Spacing` elements relative to `Group` boundaries
4. Consider whether the Priority/Transparent annotations need different handling

### Investigation Notes (2025-10-29)
- The regression test `ir_format_tests::test_let_expression_groups` remains the minimal reproducer; it fails immediately after `cargo test`.
- The mismatch sits in the parenthesized application branch: our `push_absorb_paren` → `pretty_app` chain drops the leading `Spacing Space`, nests one level less than nixfmt, and collapses the `Spacing Break` entries inside the priority group.
- Attempting incremental tweaks (flattening transparent groups, custom `unexpand_spacing`, ad‑hoc indentation) only shifted offsets and spacing without matching the reference. A faithful port of `prettyApp`, including its helpers `renderSimple`, `absorbApp`, `absorbInner`, `absorbLast`, plus the associated `isSimple` predicates, appears necessary.
- When picking this up again, start from the clean `src/pretty.rs` in `origin/main`, reapply the grouped wrappers for the `let`/`in` parts (those matched the reference), then port the Haskell logic above to ensure the spacing/priority semantics line up before re-running the regression test.
