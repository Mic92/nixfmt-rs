//! Regression tests for operator associativity
//!
//! These tests verify that our parser produces the same AST structures as nixfmt
//! for operators with different associativity rules.

mod common;
use common::test_ast_format;

// =============================================================================
// Right-associative operators: ++, //, +
// =============================================================================

#[test]
fn test_concat_associativity() {
    // ++ is right-associative: [1] ++ [2] ++ [3] parses as [1] ++ ([2] ++ [3])
    test_ast_format("concat_chain", "[1] ++ [2] ++ [3]");
}

#[test]
fn test_update_associativity() {
    // // is right-associative: {a=1;} // {b=2;} // {c=3;} parses as {a=1;} // ({b=2;} // {c=3;})
    test_ast_format("update_chain", "{a=1;} // {b=2;} // {c=3;}");
}

#[test]
fn test_plus_associativity() {
    // + is treated as right-associative via nixfmt's AST conversion hack
    // 1 + 2 + 3 produces AST as 1 + (2 + 3)
    test_ast_format("plus_chain", "1 + 2 + 3");
}

// =============================================================================
// Left-associative operators: -
// =============================================================================

#[test]
fn test_minus_associativity() {
    // - is left-associative: 1 - 2 - 3 produces AST as (1 - 2) - 3
    test_ast_format("minus_chain", "1 - 2 - 3");
}

// =============================================================================
// Additional test cases
// =============================================================================

#[test]
fn test_concat_simple() {
    // Simplest case: just two concat operations
    test_ast_format("concat_simple", "[1] ++ [2] ++ [3]");
}

#[test]
fn test_update_simple() {
    // Simplest case: just two update operations
    test_ast_format("update_simple", "{a=1;} // {b=2;} // {c=3;}");
}
