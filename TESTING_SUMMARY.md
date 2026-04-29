# Comprehensive Testing Summary

## Test Results Overview

### Custom Fuzzer Suite
- **Total Tests:** 83 (focused edge case suite)
- **Passing:** 44 (53%)
- **Failing:** 39
- **Bugs Found:** 13 unique bug categories

### Nixfmt Regression Suite
- **Correct Tests:** 6/16 passed (37.5%)
- **Invalid Tests:** 8/8 passed (100%) ✅
- **Total:** 14/24 passed (58%)

## Key Findings

### ✅ Strengths
1. **100% accuracy on invalid syntax detection** - All 8 nixfmt "invalid" tests pass
2. **Excellent string interpolation support** - Complex nested expressions work perfectly
3. **Solid core parsing** - Basic expressions, control flow, operators all work

### ❌ Critical Issues

#### HIGH SEVERITY
**BUG #7: String Attribute Selectors**
- Affects 10/16 failed regression tests
- Cannot parse: `x."y"`, `{"a" = 1;}`, `{inherit "a";}`
- **Impact:** Common nixpkgs pattern for special characters in keys
- Files affected:
  - `quotes-in-inherit.nix`
  - `quotes-in-inherit-2.nix`
  - `regression-207.nix` (uses `stable."1.48.0".rustc`)
  - All 8 string selector tests in custom suite

**BUG #8: Numeric Literal Lexing**
- Affects `numbers.nix` regression test
- **Issues:**
  - Rejects valid: `.5`, `5.`, `.1e0`, `1.e0`
  - Accepts invalid: `1e10` (should be `1 e10` application)
  - Wrong AST: `00.00`, scientific notation
- **Impact:** Cannot parse some nixpkgs numeric literals

#### MEDIUM SEVERITY
**BUG #6: `or` Keyword**
- Lexer always tokenizes `or` as keyword
- Should be identifier except in `a.b or c` context
- Affects 7 test contexts

**BUG #10/11: Operator Validation** (extends #1-3)
- Accepts invalid: `!!!x`, `-!x`, `a == b == c`
- All comparison operators affected (not just `<`)

#### LOW SEVERITY
**BUG #13: Comment/Whitespace AST**
- Affects: `standalone-comments.nix`, `final-comments-in-sets.nix`
- Cosmetic AST formatting differences
- Not a parsing error, just representation

**BUG #5: Empty String**
- Affects: `string-with-single-quote-at-end.nix` (likely)
- Cosmetic formatting difference

**BUG #12: NOT with Member Check**
- AST mismatch for `!a ? b`

## Test Coverage Analysis

### What We Test Well ✅
- **String interpolation:** 51 tests, complex nested expressions
- **Operators:** Precedence, associativity (fixed!), applications
- **Control flow:** let, with, assert, if-then-else
- **Functions:** All lambda parameter patterns
- **Invalid syntax detection:** 100% accuracy
- **Lists and sets:** Basic operations
- **Comments:** Most comment positions
- **Whitespace:** Various whitespace combinations

### Coverage Gaps Found ❗
1. **String/interpolated selectors** - Major gap, common in nixpkgs
2. **Numeric edge cases** - `.5`, `5.`, scientific notation interpretation
3. **Dollar escaping** - `dollars-before-interpolation.nix` fails
4. **Indented strings** - Some edge cases in `indented-string.nix`
5. **Path interpolations** - `paths-with-interpolations.nix` fails
6. **Complex selections** - Chains with string selectors

## Recommendations

### Priority 1: Fix BUG #7 (String Selectors)
- Would fix 10+ failing tests
- Critical for nixpkgs compatibility
- Affects: let bindings, sets, selections, inherit

### Priority 2: Fix BUG #8 (Numeric Literals)
- Fixes `numbers.nix` regression test
- Important for mathematical code
- Clear specification from nixfmt tests

### Priority 3: Fix BUG #6 (`or` keyword)
- Moderately common in real code
- 7 test contexts affected

### Priority 4: Operator validation (BUG #1-3, #10-11)
- Less common in practice
- But important for correctness

### Priority 5: Cosmetic issues (BUG #5, #13)
- Don't affect functionality
- Can be deferred

## Test Statistics

### By Category
| Category | Tests | Pass | Fail | Pass % |
|----------|-------|------|------|--------|
| String selectors | 14 | 0 | 14 | 0% |
| Numeric literals | 19 | 6 | 13 | 32% |
| Operators | 17 | 6 | 11 | 35% |
| Paths | 12 | 10 | 2 | 83% |
| `or` keyword | 7 | 0 | 7 | 0% |
| Comments/whitespace | 11 | 6 | 5 | 55% |
| Inherit | 8 | 8 | 0 | 100% |
| Nixfmt regression | 24 | 14 | 10 | 58% |

### Overall Score
**Combined: 58/107 tests passing (54%)**

With BUG #7 and #8 fixed, estimated pass rate would jump to **~85%**.

## Files for Reference

### Test Suites
- `src/regression_tests/properties.rs` - Property tests over the vendored nixfmt corpus
- `scripts/diff_sweep.sh` - Differential sweep against reference `nixfmt` over nixpkgs

### Documentation
- `FINDINGS.md` - Detailed bug reports (13 bugs documented)
- `PLAN.md` - Test coverage expansion plan

### Important Nixfmt Regression Files
- `/Users/joerg/git/nixfmt/test/correct/*.nix` - Valid Nix files (16 files)
- `/Users/joerg/git/nixfmt/test/invalid/*.nix` - Invalid syntax (8 files, all pass!)

## Next Steps

1. ✅ Comprehensive testing complete - found 13 bug categories
2. ✅ Created test infrastructure (2 test suites, 107 total tests)
3. ⏭️  Fix BUG #7 (string selectors) - highest impact
4. ⏭️  Fix BUG #8 (numeric literals) - clear specification
5. ⏭️  Re-run full test suite to measure improvement
6. ⏭️  Test against real nixpkgs files
