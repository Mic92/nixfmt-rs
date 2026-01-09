# Non-Trivial Bug: Comment Loss in Selector Interpolations

## Summary

Comments before operators are lost when they appear after selector interpolations like `self.packages.${system}.isLinux`. Specifically, when multiple `&&` operators are used with interpolation selectors, the third and subsequent operators lose their leading comments.

## Affected Pattern

```nix
{
  x =
    lib.optionalAttrs
      (
        self.packages.${system}.isLinux
        # comment 1  ← preserved
        && self.packages.${system}.isPower64
        # comment 2  ← preserved
        && system != "armv6l-linux"
        # comment 3  ← LOST!
        && system != "riscv64-linux"
      )
      { tests = {}; };
}
```

## Root Cause

The issue lies in the interaction between `lexeme()` and `next_token()` in `src/lexer.rs`.

### Execution Flow

1. **lexeme()** (line 86-144):
   - Takes `trivia_buffer` as leading trivia for current token (line 97)
   - Calls `next_token()` to get the token (line 109)
   - Parses trailing trivia after the token (line 115)

2. **next_token()** (line 176-540):
   - Has special handling for trivia after string parsing (lines 204-238)
   - When it encounters newlines/comments, it parses them and adds to `trivia_buffer`
   - **BUG**: This trivia goes into the buffer for the NEXT token, not the current one

### The Problem

When parsing the third `&&` operator:

1. `lexeme()` takes empty buffer as leading trivia
2. `lexeme()` calls `next_token()`
3. `next_token()` sees `\n# comment 3\n` after position 198
4. `next_token()` parses trivia and adds "comment 3" to `trivia_buffer` (line 223)
5. `next_token()` returns `&&` token
6. `lexeme()` attaches empty leading trivia to `&&`
7. "comment 3" is now in buffer for the NEXT token, but that's the 4th `&&`, not the 3rd

The trivia buffer state is corrupted because `next_token()` adds trivia meant for the current token AFTER `lexeme()` has already captured the (empty) trivia.

## Why This Happens with Interpolations

The bug manifests specifically with selector interpolations because:

1. Parsing `${system}` involves parsing an expression
2. During expression parsing, the lexer may read ahead
3. The special trivia handling in `next_token()` (lines 204-238) was added for "after string parsing"
4. This causes trivia to be parsed at the wrong time and attached to the wrong token

## Haskell nixfmt Solution

The Haskell implementation avoids this issue through architectural differences:

1. **`rawSymbol`** (Parser.hs:83-84): Parses tokens WITHOUT trivia
   ```haskell
   rawSymbol :: Token -> Parser Token
   rawSymbol t = chunk (tokenText t) $> t
   ```

2. **`whole`** (Lexer.hs:237-241): Isolates trivia state for sub-expressions
   ```haskell
   whole :: Parser a -> Parsec Void Text (Whole a)
   whole pa = flip evalStateT [] do  -- Creates fresh trivia state!
     preLexeme $ pure ()
     pushTrivia . convertLeading =<< trivia
     Whole <$> pa <*> takeTrivia
   ```

3. **Interpolation parsing** (Parser.hs:166):
   ```haskell
   interpolation = Interpolation
     <$> (rawSymbol TInterOpen *> lift (whole expression) <* rawSymbol TInterClose)
   ```

The `whole` function **"does not interact with the trivia state of its surroundings"** by using `evalStateT []` to create an isolated trivia context.

## Attempted Fix

Attempted to isolate trivia state in `parse_selector_interpolation()` by saving and restoring the buffer:

```rust
let saved_trivia = self.lexer.save_trivia_state();
let expr = self.parse_expression()?;
let close = self.expect_token_match(|t| matches!(t, Token::TBraceClose))?;
self.lexer.restore_trivia_state(saved_trivia);
```

**This didn't work** because the problem occurs BEFORE the interpolation, not during it. The trivia is parsed by `next_token()` when returning the `&&` token that comes AFTER the interpolation's closing `}`.

## Proper Solution

The proper fix requires architectural changes:

### Option 1: Remove trivia parsing from next_token()

Remove lines 204-238 in `next_token()` that parse and add trivia to the buffer. This code was meant for handling trivia after string parsing, but it violates the design principle that trivia belongs to the current token, not the next one.

**Risk**: May break string literal parsing if it depends on this behavior.

### Option 2: Refactor trivia handling to match Haskell design

- Separate raw token parsing (no trivia) from lexeme parsing (with trivia)
- Implement `whole`-like isolation for all sub-expressions
- Ensure trivia buffer is only modified by `lexeme()`, never by `next_token()`

**Risk**: Large refactoring, potential for introducing new bugs.

### Option 3: Fix the specific case

Make `next_token()` return both token AND any trivia it collected, then have `lexeme()` use that trivia for the current token instead of the next one.

**Risk**: Moderate refactoring, but more targeted than Option 2.

## Test Case

Regression test added in `tests/regression_tests.rs` as BUG #13:

```rust
#[test]
#[ignore] // TODO: Fix comment dropping before && when interpolation selectors are involved
fn regression_comment_before_and_with_selectors() {
    test_ast_format(
        "comment_before_and_selectors",
        r#"{
  x =
    lib.optionalAttrs
      (
        self.packages.${system}.isLinux
        # comment 1
        && self.packages.${system}.isPower64
        # comment 2
        && system != "armv6l-linux"
        # comment 3
        && system != "riscv64-linux"
      )
      {
        tests = {};
      };
}"#,
    );
}
```

## Debug Evidence

```
!!!!! LEXER: Parsed comment 3 at line 10, pos 218
!!!!! parse_trivia: collected comment 3 at pos 198->227, line 9->11, 3 trivia items
!!!!! next_token: Adding comment 3 to trivia_buffer at pos 227, line 11
!!!!! next_token: Buffer HAS comment 3, about to return token starting with char '&'
DEBUG: Parsing && at line Pos(11), preTrivia len: 0, trivia: Trivia([])
```

The `&&` operator at line 11 (third occurrence) has empty `preTrivia` even though "comment 3" was just added to the buffer.

## Impact

This bug affects nixpkgs `flake.nix` where similar patterns occur around lines 120, 169, 171, 186, 188. Any expression with:
- Multiple `&&` operators (3 or more)
- Interpolation selectors (`${...}`)
- Comments between operators

will lose comments on the third and subsequent operators.
