//! AST formatting smoke tests - compare our output with `nixfmt --ast`.
//!
//! One minimal test per top-level `Expression`/`Term` variant (plus the
//! `Trivium` shapes and `Binder`/`Parameter` smoke) so a regression in any
//! variant fails fast with a small diff. Anything more complex lives in
//! `regression_tests/`.

use crate::tests_common::test_ast_format;

// ---------------------------------------------------------------------------
// Term variants
// ---------------------------------------------------------------------------

#[test]
fn test_integer() {
    // Term::Token
    test_ast_format("42");
}

#[test]
fn test_simple_string() {
    // Term::SimpleString
    test_ast_format("\"hello\"");
}

#[test]
fn test_indented_string() {
    // Term::IndentedString
    test_ast_format("''hello''");
}

#[test]
fn test_relative_path() {
    // Term::Path
    test_ast_format("./foo/bar");
}

#[test]
fn test_simple_list() {
    // Term::List
    test_ast_format("[1 2 3]");
}

#[test]
fn test_simple_set() {
    // Term::Set / Binder::Assignment
    test_ast_format("{a=1;}");
}

#[test]
fn test_dotted_path() {
    // Selector list inside a Binder::Assignment key
    test_ast_format("{a.b.c=1;}");
}

#[test]
fn test_inherit() {
    // Binder::Inherit
    test_ast_format("{inherit a;}");
}

#[test]
fn test_selection_with_default() {
    // Term::Selection (with `or` default)
    test_ast_format("x.y or 42");
}

#[test]
fn test_parenthesized() {
    // Term::Parenthesized
    test_ast_format("(1 + 2)");
}

// ---------------------------------------------------------------------------
// Expression variants
// ---------------------------------------------------------------------------

#[test]
fn test_with() {
    test_ast_format("with x; y");
}

#[test]
fn test_let_simple() {
    test_ast_format("let a=1; in a");
}

#[test]
fn test_assert() {
    test_ast_format("assert true; 42");
}

#[test]
fn test_if_then_else() {
    test_ast_format("if true then 1 else 2");
}

#[test]
fn test_simple_lambda() {
    // Expression::Abstraction / Parameter::ID
    test_ast_format("x: x");
}

#[test]
fn test_set_pattern() {
    // Parameter::Set
    test_ast_format("{x}: x");
}

#[test]
fn test_function_application() {
    test_ast_format("f x");
}

#[test]
fn test_addition() {
    // Expression::Operation
    test_ast_format("1 + 2");
}

#[test]
fn test_comparison_operators() {
    // `<=` / `>=` are not exercised by any other oracle test; keep one
    // smoke so the corresponding Token Display / pretty arms stay covered.
    test_ast_format("a <= b && c >= d");
}

#[test]
fn test_member_check() {
    test_ast_format("x ? y");
}

#[test]
fn test_negation() {
    test_ast_format("-5");
}

#[test]
fn test_boolean_not() {
    // Expression::Inversion
    test_ast_format("!true");
}

// ---------------------------------------------------------------------------
// Trivium shapes
// ---------------------------------------------------------------------------

#[test]
fn test_line_comment() {
    test_ast_format("# comment\n42");
}

#[test]
fn test_block_comment() {
    test_ast_format("/* block */ 42");
}

// ---------------------------------------------------------------------------
// Non-redundant regression kept here (no equivalent in regression_tests/)
// ---------------------------------------------------------------------------

#[test]
fn test_string_hash_not_comment() {
    // Inside multi-line strings, # starts literal text, not a comment.
    // The hash and text should be a TextPart, not LineComment trivia.
    test_ast_format(
        r#"''
foo ${bar}

# TODO: comment
badFiles=$(find ${filteredHead})
''"#,
    );
}
