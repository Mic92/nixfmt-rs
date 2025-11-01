//! IR regression tests
//!
//! Tests for IR formatting differences between nixfmt-rs and reference nixfmt

use crate::tests_common::test_ir_format;

/// Regression test: let expression should wrap letPart and inPart in groups
///
/// Issue: nixfmt-rs was not wrapping the let and in parts in groups, resulting in
/// different IR structure compared to the reference implementation.
/// Fixed by using push_group for both letPart and inPart in Expression::Let formatting.
#[test]
#[ignore]
fn test_let_expression_groups() {
    test_ir_format("{ pinnedJson ? ./pinned.json, }: let pinned = (builtins.fromJSON (builtins.readFile pinnedJson)).pins; in pinned");
}

/// Regression test: function arguments should use Priority groups
///
/// Issue: nixfmt-rs was using RegularG for function arguments (parenthesized, sets, lambdas),
/// but reference nixfmt uses Priority groups. This affects how arguments break across lines.
#[test]
fn test_function_arguments_priority() {
    // Set argument
    test_ir_format("fetchTarball { url = \"x\"; }");
    // Lambda argument
    test_ir_format("lib.filterAttrs (x: y)");
}

/// Regression test: transparent groups in parentheses need Break spacing
///
/// Issue: When a Transparent group (like a function name) appears in a parenthesized
/// context, it should be wrapped in a RegularG with Break spacing before the text.
///
/// Note: IR representation differs from reference implementation, but formatted output is identical.
/// Test is ignored until IR representation matches exactly.
#[test]
fn test_transparent_in_parens_break() {
    test_ir_format("(lib.filterAttrs (x: y))");
}
