#!/usr/bin/env bash
# Systematic bug finder - tests various Nix expressions against nixfmt

set -euo pipefail

NIXFMT_RS="./target/debug/nixfmt_rs"
NIXFMT="nixfmt"

# Build our parser first
cargo build --quiet 2>&1 | grep -v "warning:" || true

test_expression() {
    local name="$1"
    local expr="$2"

    # Get nixfmt's AST (outputs to stderr)
    local nixfmt_ast
    nixfmt_ast=$(echo "$expr" | $NIXFMT --ast - 2>&1 1>/dev/null | sed '${/^$/d;}' || echo "NIXFMT_FAILED")

    if [[ "$nixfmt_ast" == "NIXFMT_FAILED" ]]; then
        return 0  # Skip invalid expressions
    fi

    # Get our AST
    local our_ast
    our_ast=$(echo "$expr" | $NIXFMT_RS --ast 2>&1 || echo "PARSE_FAILED")

    if [[ "$our_ast" == "PARSE_FAILED" ]]; then
        echo "❌ BUG: $name - We failed to parse but nixfmt succeeded"
        echo "   Expression: $expr"
        return 1
    fi

    # Compare ASTs
    if [[ "$nixfmt_ast" != "$our_ast" ]]; then
        echo "❌ BUG: $name - AST mismatch"
        echo "   Expression: $expr"
        echo "   Diff:"
        diff -u <(echo "$nixfmt_ast") <(echo "$our_ast") | head -20
        return 1
    fi

    return 0
}

echo "🔍 Searching for bugs..."
echo ""

failed=0
total=0

# Test various expression types
test_cases=(
    # Basic expressions
    "integer:42"
    "float:3.14"
    "string:\"hello\""
    "path:./foo"

    # Functions
    "simple_lambda:x: x"
    "lambda_with_pattern:{x}: x"
    "lambda_with_default:{x ? 1}: x"
    "lambda_with_ellipsis:{x, ...}: x"
    "lambda_at_pattern:x@{y}: x"
    "lambda_pattern_at:x: y: x"

    # Let expressions
    "let_simple:let x = 1; in x"
    "let_multiple:let x = 1; y = 2; in x"
    "let_inherit:let inherit x; in x"

    # With expressions
    "with_simple:with x; y"

    # Assert
    "assert_simple:assert true; 1"

    # If-then-else
    "if_simple:if true then 1 else 2"
    "if_nested:if a then if b then 1 else 2 else 3"

    # Operations
    "negation:-1"
    "double_negation:- -1"
    "not:!true"

    # Binary operations - various combinations
    "mul_div:1 * 2 / 3"
    "add_mul:1 + 2 * 3"
    "mul_add:1 * 2 + 3"
    "comparison_chain:1 < 2"
    "equality:1 == 2"
    "and_or:true && false || true"
    "implies:true -> false"

    # Application
    "app_simple:f x"
    "app_multiple:f x y"
    "app_with_op:f x + y"

    # Selection
    "select_simple:a.b"
    "select_chain:a.b.c"
    "select_default:a.b or 1"

    # Member check
    "member_check:a ? b"
    "member_check_chain:a ? b.c"

    # Lists
    "list_empty:[]"
    "list_single:[1]"
    "list_multiple:[1 2 3]"

    # Sets
    "set_empty:{}"
    "set_simple:{a=1;}"
    "set_multiple:{a=1; b=2;}"
    "set_nested:{a={b=1;};}"
    "set_rec:rec {a=1;}"

    # Strings
    "string_interp:\"\${x}\""
    "indented_string:''hello''"
    "indented_interp:''\${x}''"

    # Complex combinations
    "lambda_let:x: let y = 1; in x"
    "let_if:let x = 1; in if x then 2 else 3"
    "with_let:with x; let y = 1; in y"
)

for test_case in "${test_cases[@]}"; do
    name="${test_case%%:*}"
    expr="${test_case#*:}"
    total=$((total + 1))

    if ! test_expression "$name" "$expr"; then
        failed=$((failed + 1))
    fi
done

echo ""
echo "================================"
echo "Results: $((total - failed))/$total passed"
if [ $failed -gt 0 ]; then
    echo "❌ Found $failed bugs!"
    exit 1
else
    echo "✅ No bugs found in basic tests"
fi
