# Test Coverage Guide

This document explains how to measure and increase test coverage for nixfmt-rs.

## Current Coverage

As of the last measurement:

- **lexer.rs**: 92.43% regions, 94.69% lines
- **parser.rs**: 80.83% regions, 83.73% lines
- **error.rs**: 50.00% regions, 70.59% lines
- **Overall**: 83.30% regions, 84.35% lines

Total: 165 passing tests, 3 ignored

## Prerequisites

The project uses `cargo-llvm-cov` for coverage measurement, which requires:

1. **Rust toolchain with profiler support** (installed via rustup, not nix)
2. **cargo-llvm-cov** tool

These are already set up in the nix development shell.

## Measuring Coverage

### Generate Coverage Report

```bash
# Enter the nix development shell
nix develop

# Generate HTML coverage report
cargo llvm-cov --html

# Open the report (macOS)
open target/llvm-cov/html/index.html

# Generate lcov format for parsing
cargo llvm-cov --lcov --output-path coverage.lcov

# Get summary only
cargo llvm-cov --summary-only
```

### Analyze Uncovered Lines

Use the included Python script to find uncovered lines in key files:

```bash
# From within nix develop
python3 <<'EOF'
import sys
from collections import defaultdict

def parse_lcov(filename):
    uncovered = defaultdict(list)
    current_file = None

    with open(filename) as f:
        for line in f:
            line = line.strip()
            if line.startswith('SF:'):
                current_file = line[3:]
            elif line.startswith('DA:'):
                parts = line[3:].split(',')
                if len(parts) == 2:
                    line_num = int(parts[0])
                    hit_count = int(parts[1])
                    if hit_count == 0:
                        uncovered[current_file].append(line_num)
    return uncovered

uncovered = parse_lcov('coverage.lcov')

for file in ['src/parser.rs', 'src/lexer.rs', 'src/error.rs']:
    full_path = f'{os.getcwd()}/{file}'
    if full_path in uncovered:
        lines = uncovered[full_path]
        print(f"\n{file}: {len(lines)} uncovered lines")
        print(f"Lines: {sorted(lines)[:50]}")
EOF
```

## Finding Coverage Opportunities

### 1. Identify Uncovered Branches

Look at the coverage HTML report to see:
- Red lines (not executed)
- Yellow lines (partially covered branches)

Focus on:
- **Error handling paths** - Often uncovered because tests focus on success cases
- **Edge cases** - Boundary conditions, empty inputs, special characters
- **Alternative parsing paths** - Different ways to parse the same construct

### 2. Check Against nixfmt

Before writing a test, verify the syntax is valid in nixfmt:

```bash
echo "your test code" | nixfmt --ast
```

If nixfmt rejects it, it's likely an error case worth testing.

### 3. Common Uncovered Patterns

**Lexer error cases:**
- Invalid characters: `x ^ y`, `x # y`
- Incomplete tokens: `$x` (should be `${x}`), `'x'` (should be `''x''`)
- Invalid paths: `<unclosed`, `<invalid@char>`
- Platform-specific: Windows line endings `\r\n`

**Parser error cases:**
- Incomplete syntax: `{ ... }` without `:`, `x @` without continuation
- Invalid combinations: `{ x, y }` without `:` or `=`
- Malformed parameters

**Alternative code paths:**
- Same feature through different parsing routes
- Example: Member check on simple term vs. on operation

## Writing Coverage Tests

### Test File Structure

Coverage-focused tests go in `tests/coverage_test.rs`:

```rust
mod common;
use common::test_ast_format;
use nixfmt_rs::parse;

#[test]
fn test_feature_name() {
    // Lines X-Y in file.rs: description
    test_ast_format("test_name", "nix code");
}

#[test]
fn test_error_case() {
    // Lines X-Y in file.rs: error description
    let result = parse("invalid code");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("expected error message"));
}
```

### Best Practices

1. **Comment the line numbers**: Include the source file and line numbers being tested
2. **Test both success and error paths**: Don't just test valid code
3. **Verify error messages**: Check that error messages are helpful
4. **Compare with nixfmt**: Ensure AST output matches nixfmt's behavior
5. **Keep tests focused**: One feature per test
6. **Run coverage after each test**: Verify the test actually hits the target lines

### Example Test Development Workflow

```bash
# 1. Generate coverage and identify uncovered lines
cargo llvm-cov --html
# Find lines 339-342 in lexer.rs are uncovered

# 2. Examine the code
# Lines 339-342: Error for single quote without another quote

# 3. Test against nixfmt to understand expected behavior
echo "x 'y" | nixfmt --ast
# Error: unexpected character

# 4. Write the test
cat >> tests/coverage_test.rs << 'EOF'
#[test]
fn test_single_quote_error() {
    // Lines 339-342 in lexer.rs: single quote without ''
    let result = parse("x 'y");
    assert!(result.is_err());
}
EOF

# 5. Run the test
cargo test test_single_quote_error

# 6. Verify coverage improved
cargo llvm-cov --html
# Check if lines 339-342 are now green
```

## Common Pitfalls

### 1. Dead Code

Some uncovered lines may be defensive code or invalid syntax paths:

```rust
// This might never execute if nixfmt doesn't support the syntax
if matches!(self.current.value, Token::TAt) {
    // Parse complex @ pattern
}
```

Check if the syntax is valid Nix before trying to test it.

### 2. Alternative Paths to Same Feature

Sometimes a feature is tested through one path but not another:

```rust
// Path 1: parse_abstraction_or_operation (tested)
x @ y: body

// Path 2: parse_operation_or_lambda (not tested, might be dead code)
(expr) @ y: body
```

If you can't find valid syntax for a path, it may be defensive/dead code.

### 3. Platform-Specific Code

```rust
if ch == '\r' {  // Windows line endings
    // Only tested on Windows or with explicit \r\n in test
}
```

Add explicit tests with `\r\n` sequences to cover these paths on Unix systems.

## Test Categories

### Existing Test Files

- `tests/ast_format_tests.rs` - Core AST formatting tests (69 tests)
- `tests/coverage_test.rs` - Coverage-focused tests (12 tests)
- `tests/operator_associativity_test.rs` - Operator precedence (6 tests)
- `tests/parameter_tests.rs` - Lambda parameters (13 tests)
- `tests/regression_tests.rs` - Known bugs (10 + 3 ignored)
- `tests/string_path_tests.rs` - Strings and paths (18 tests)
- `src/lexer.rs` - Lexer unit tests (37 tests)
- `src/parser.rs` - Parser unit tests (counted in lib tests)

### When to Add Tests Where

- **ast_format_tests.rs**: Core Nix language features
- **coverage_test.rs**: Error cases, edge cases, alternative paths
- **regression_tests.rs**: Bugs found in real usage
- **Unit tests**: Internal helper functions

## Measuring Progress

Track coverage over time:

```bash
# Before changes
cargo llvm-cov --summary-only > coverage_before.txt

# Make changes and add tests

# After changes
cargo llvm-cov --summary-only > coverage_after.txt

# Compare
diff coverage_before.txt coverage_after.txt
```

## Goals

- **Lexer**: Target 95%+ (currently 92.43%)
- **Parser**: Target 85%+ (currently 80.83%)
- **Error handling**: Target 60%+ (currently 50%)
- **Overall**: Target 85%+ (currently 83.30%)

## Resources

- [cargo-llvm-cov documentation](https://github.com/taiki-e/cargo-llvm-cov)
- [nixfmt Haskell implementation](https://github.com/serokell/nixfmt) - Reference for expected behavior
- Coverage HTML report: `target/llvm-cov/html/index.html`

## Recent Coverage Improvements

Session summary of recent improvements:

**Initial → Final:**
- lexer.rs: 88.56% → 92.43% (+3.87%)
- parser.rs: 79.15% → 80.83% (+1.68%)
- error.rs: 19.23% → 50.00% (+30.77%)

**Tests added:**
- Fixed 2 ignored tests (pipe-forward operator, @ error message)
- Added 8 new coverage tests
- Total: 10 tests added, 25+ new lines covered

**Key wins:**
1. Implemented `|>` (pipe-forward) and `<|` (pipe-backward) operators
2. Improved error messages for `@` without `:`
3. Added error case tests for invalid syntax
4. Covered Windows line endings
5. Tested member check on operations
