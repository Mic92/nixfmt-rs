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

#[test]
fn test_comment_structure() {
    // Comments generate Comment and TrailingComment annotations
    test_ir_format("/* block comment */ let a = 1; in a # line comment");
}

#[test]
fn test_empty_line_structure() {
    // Empty lines generate Emptyline spacing, Priority and Transparent groups
    test_ir_format("{\n\n  a = 1;\n}");
}

#[test]
fn test_multiline_string_structure() {
    // Multiline indented strings test line break handling
    test_ir_format("''\n  line1\n  line2\n''");
}

#[test]
fn test_string_interpolation_structure() {
    // String interpolation exercises nested grouping with line_prime
    test_ir_format("\"prefix ${expr} suffix\"");
}

#[test]
fn test_nested_groups_structure() {
    // Nested groups with various spacing types
    test_ir_format("[ { a = 1; } { b = 2; } ]");
}

#[test]
fn test_function_application_structure() {
    // Function application exercises hardspace between terms
    test_ir_format("map (x: x + 1) list");
}

#[test]
fn test_operation_structure() {
    // Binary operations test operator spacing
    test_ir_format("a + b * c");
}

#[test]
fn test_if_then_else_structure() {
    // Conditional expressions exercise hardspace placement
    test_ir_format("if cond then true else false");
}

#[test]
#[ignore]
fn test_lambda_structure() {
    // Lambda with absorbable body
    test_ir_format("x: { inherit x; }");
}

#[test]
#[ignore]
fn test_member_check_structure() {
    // Member check with ? operator
    test_ir_format("attrs ? foo");
}

#[test]
fn test_negation_structure() {
    // Negation operator spacing
    test_ir_format("-42");
}

#[test]
fn test_boolean_not_structure() {
    // Boolean not operator spacing
    test_ir_format("!true");
}

#[test]
fn test_parenthesized_complex_structure() {
    // Parenthesized complex expressions with line_prime separators
    test_ir_format("(let x = 1; in x)");
}

#[test]
#[ignore]
fn test_list_with_comments_structure() {
    // Lists with interspersed comments
    test_ir_format("[\n  # comment\n  1\n  2\n]");
}

#[test]
fn test_rec_set_structure() {
    // Recursive set with rec keyword
    test_ir_format("rec { a = 1; b = a; }");
}

#[test]
fn test_empty_set_with_spacing_structure() {
    // Empty set spanning multiple lines
    test_ir_format("{\n\n}");
}

#[test]
fn test_empty_list_with_spacing_structure() {
    // Empty list spanning multiple lines
    test_ir_format("[\n\n]");
}
