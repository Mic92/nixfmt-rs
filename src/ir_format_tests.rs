//! IR formatting regression tests
//!
//! These tests compare our IR output with the reference nixfmt implementation
//! to ensure we match the expected pretty-printing structure.
//!
//! NOTE: These tests are currently ignored because there are known IR representation
//! differences that don't affect the formatted output. See IR_FORMATTING_STATUS.md
//! for details on the remaining differences.

use crate::tests_common::test_ir_format;

/// Regression test: simple parameter pattern should have outer Group wrapper
///
/// Issue: nixfmt-rs was missing the outer `Group RegularG` wrapper that the
/// reference implementation adds when pretty-printing a Whole Expression (File).
/// This has been fixed by wrapping Whole<T>::pretty in push_group().
#[test]
fn test_simple_parameter_pattern() {
    test_ir_format("{ a, b }: x");
}

#[test]
fn test_let_binding_structure() {
    // Minimal reproducer: even the simplest let binding loses the inner group + spacing
    // structure that nixfmt emits for the binding body and the `in` branch.
    test_ir_format("let a = 1; in a");
}

#[test]
fn test_with_binding_structure() {
    // Simple with-expression: reference nixfmt groups the keyword, environment, and semicolon,
    // but nixfmt-rs currently flattens them into a single chunk.
    test_ir_format("with a; b");
}

#[test]
fn test_assert_structure() {
    // Simple assert expression: nixfmt keeps the keyword and condition separated in the IR,
    // whereas nixfmt-rs currently emits them as a single text token.
    test_ir_format("assert true; 42");
}

#[test]
fn test_selection_with_default_structure() {
    // Member selection with default exercises selector spacing and "or" clause layout
    test_ir_format("config.services.nginx.enable or false");
}

#[test]
fn test_update_absorbable_rhs_structure() {
    // Update expression with absorbable RHS currently diverges from reference IR
    test_ir_format("attrs // { inherit value; }");
}

#[test]
fn test_inherit_many_identifiers_structure() {
    // Many identifiers trigger the hardline-based spacing path for inherits
    test_ir_format("let inherit foo bar baz qux; in foo");
}

#[test]
fn test_inherit_with_source_structure() {
    // Inherit with explicit source exercises nested grouping and spacing branch
    test_ir_format("let inherit (inputs) foo bar; in foo");
}

#[test]
fn test_selection_from_parenthesized_term_structure() {
    // Selection starting from a parenthesized term forces softline_prime separator
    test_ir_format("({ inherit foo; }).foo or true");
}

#[test]
fn test_selection_from_record_term_structure() {
    // Selection from a record term forces line_prime separator before selectors
    test_ir_format("rec { nested = { }; }.nested or { }");
}
