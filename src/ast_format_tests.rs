//! AST formatting tests - compare our output with nixfmt --ast

use crate::tests_common::test_ast_format;

// ============================================================================
// Basic Literals - Tests Token formatting
// ============================================================================

#[test]
fn test_integer() {
    test_ast_format("42");
}

#[test]
fn test_float() {
    test_ast_format("3.14");
}

#[test]
fn test_identifier() {
    test_ast_format("foo");
}

// ============================================================================
// Comments - Tests Trivium formatting
// ============================================================================

#[test]
fn test_line_comment() {
    test_ast_format("# comment\n42");
}

#[test]
fn test_block_comment() {
    test_ast_format("/* block */ 42");
}

#[test]
fn test_multiline_block_comment() {
    test_ast_format("/* line 1\n   line 2\n   line 3 */\n42");
}

#[test]
fn test_doc_comment() {
    test_ast_format("/** doc comment */ 42");
}

// ============================================================================
// Attribute Sets - Tests Term::Set, Binder, Items formatting
// ============================================================================

#[test]
fn test_empty_set() {
    test_ast_format("{}");
}

#[test]
fn test_simple_set() {
    test_ast_format("{a=1;}");
}

#[test]
fn test_multiple_bindings() {
    test_ast_format("{a=1; b=2;}");
}

#[test]
fn test_nested_set() {
    test_ast_format("{a={b=1;};}");
}

#[test]
fn test_rec_set() {
    test_ast_format("rec {a=1;}");
}

#[test]
fn test_inherit() {
    test_ast_format("{inherit a;}");
}

#[test]
fn test_inherit_from() {
    test_ast_format("{inherit (x) a;}");
}

#[test]
fn test_dotted_path() {
    test_ast_format("{a.b.c=1;}");
}

// ============================================================================
// Lists - Tests Term::List, Items formatting
// ============================================================================

#[test]
fn test_empty_list() {
    test_ast_format("[]");
}

#[test]
fn test_simple_list() {
    test_ast_format("[1 2 3]");
}

#[test]
fn test_nested_list() {
    test_ast_format("[[1] [2]]");
}

// ============================================================================
// Strings - Tests SimpleString, IndentedString, StringPart formatting
// ============================================================================

#[test]
fn test_simple_string() {
    test_ast_format("\"hello\"");
}

#[test]
fn test_string_escape() {
    test_ast_format("\"hello\\nworld\"");
}

#[test]
fn test_string_interpolation() {
    test_ast_format("\"hello ${world}\"");
}

#[test]
fn test_nested_interpolation() {
    test_ast_format("\"outer ${\"inner ${x}\"} end\"");
}

#[test]
fn test_indented_string() {
    test_ast_format("''hello''");
}

#[test]
fn test_multiline_indented_string() {
    test_ast_format("''\n  line 1\n  line 2\n''");
}

// ============================================================================
// Paths - Tests Path formatting
// ============================================================================

#[test]
fn test_relative_path() {
    test_ast_format("./foo/bar");
}

#[test]
fn test_absolute_path() {
    test_ast_format("/usr/bin/foo");
}

#[test]
fn test_home_path() {
    test_ast_format("~/foo");
}

#[test]
fn test_angle_path() {
    test_ast_format("<nixpkgs>");
}

// ============================================================================
// Operators - Tests Expression::Operation formatting
// ============================================================================

#[test]
fn test_addition() {
    test_ast_format("1 + 2");
}

#[test]
fn test_subtraction() {
    test_ast_format("3 - 1");
}

#[test]
fn test_multiplication() {
    test_ast_format("2 * 3");
}

#[test]
fn test_division() {
    test_ast_format("6 / 2");
}

#[test]
fn test_concatenation() {
    test_ast_format("[1] ++ [2]");
}

#[test]
fn test_update() {
    test_ast_format("{a=1;} // {b=2;}");
}

#[test]
fn test_logical_and() {
    test_ast_format("true && false");
}

#[test]
fn test_logical_or() {
    test_ast_format("true || false");
}

#[test]
fn test_implication() {
    test_ast_format("a -> b");
}

#[test]
fn test_comparison() {
    test_ast_format("1 == 2");
}

#[test]
fn test_inequality() {
    test_ast_format("1 != 2");
}

#[test]
fn test_less_than() {
    test_ast_format("1 < 2");
}

#[test]
fn test_less_equal() {
    test_ast_format("1 <= 2");
}

#[test]
fn test_greater_than() {
    test_ast_format("2 > 1");
}

#[test]
fn test_greater_equal() {
    test_ast_format("2 >= 1");
}

// ============================================================================
// Unary Operators - Tests Expression::Negation, Expression::Inversion
// ============================================================================

#[test]
fn test_negation() {
    test_ast_format("-5");
}

#[test]
fn test_boolean_not() {
    test_ast_format("!true");
}

// ============================================================================
// Application - Tests Expression::Application formatting
// ============================================================================

#[test]
fn test_function_application() {
    test_ast_format("f x");
}

#[test]
fn test_multiple_application() {
    test_ast_format("f x y");
}

// ============================================================================
// Selection - Tests Term::Selection, Selector formatting
// ============================================================================

#[test]
fn test_attribute_selection() {
    test_ast_format("x.y");
}

#[test]
fn test_nested_selection() {
    test_ast_format("x.y.z");
}

#[test]
fn test_selection_with_default() {
    test_ast_format("x.y or 42");
}

// ============================================================================
// Member Check - Tests Expression::MemberCheck formatting
// ============================================================================

#[test]
fn test_member_check() {
    test_ast_format("x ? y");
}

#[test]
fn test_nested_member_check() {
    test_ast_format("x ? y.z");
}

// ============================================================================
// Lambda - Tests Expression::Abstraction, Parameter formatting
// ============================================================================

#[test]
fn test_simple_lambda() {
    test_ast_format("x: x");
}

#[test]
fn test_set_pattern() {
    test_ast_format("{x}: x");
}

#[test]
fn test_set_pattern_with_default() {
    test_ast_format("{x ? 1}: x");
}

#[test]
fn test_set_pattern_with_ellipsis() {
    test_ast_format("{x, ...}: x");
}

#[test]
fn test_context_pattern() {
    test_ast_format("args @ {x}: x");
}

#[test]
fn test_reverse_context_pattern() {
    test_ast_format("{x} @ args: x");
}

// ============================================================================
// Let Expressions - Tests Expression::Let formatting
// ============================================================================

#[test]
fn test_let_simple() {
    test_ast_format("let a=1; in a");
}

#[test]
fn test_let_multiple() {
    test_ast_format("let a=1; b=2; in a+b");
}

#[test]
fn test_let_inherit() {
    test_ast_format("let inherit a; in a");
}

// ============================================================================
// If Expressions - Tests Expression::If formatting
// ============================================================================

#[test]
fn test_if_then_else() {
    test_ast_format("if true then 1 else 2");
}

#[test]
fn test_nested_if() {
    test_ast_format("if a then if b then 1 else 2 else 3");
}

// ============================================================================
// With Expressions - Tests Expression::With formatting
// ============================================================================

#[test]
fn test_with() {
    test_ast_format("with x; y");
}

// ============================================================================
// Assert Expressions - Tests Expression::Assert formatting
// ============================================================================

#[test]
fn test_assert() {
    test_ast_format("assert true; 42");
}

// ============================================================================
// Parenthesized - Tests Term::Parenthesized formatting
// ============================================================================

#[test]
fn test_parenthesized() {
    test_ast_format("(1 + 2)");
}

// ============================================================================
// Complex Combinations
// ============================================================================

#[test]
fn test_complex_nested() {
    test_ast_format("let\n  f = x: x + 1;\n  g = {y}: y * 2;\nin\n  f (g {y=5;})");
}

#[test]
fn test_complex_with_comments() {
    test_ast_format("{\n  # First binding\n  a = 1;\n  /* Block comment */\n  b = 2;\n}");
}

#[test]
fn test_realistic_package() {
    test_ast_format(
        r#"{
  pname = "example";
  version = "1.0.0";

  src = ./src;

  buildInputs = [ pkgA pkgB ];

  meta = {
    description = "An example";
    license = licenses.mit;
  };
}"#,
    );
}

// String Literal Comment Handling
// Regression test: # should be treated as literal text in strings, not comments

#[test]
fn test_string_hash_not_comment() {
    // Inside multi-line strings, # starts literal text, not a comment
    // The hash and text should be a TextPart, not LineComment trivia
    test_ast_format(
        r#"''
foo ${bar}

# TODO: comment
badFiles=$(find ${filteredHead})
''"#,
    );
}

#[test]
fn test_parameter_patterns() {
    test_ast_format("{a ? 1, b, ...}: a + b");
    test_ast_format("args@{foo ? \"x\", bar, baz, ...}: foo");
}
