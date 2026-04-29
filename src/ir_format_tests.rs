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
fn test_app_selection_inner_arg() {
    // Selection as a non-final argument must use a RegularG (not Priority) group.
    test_ir_format("f a.b c");
}

#[test]
fn test_app_two_consecutive_list_args() {
    // Two consecutive list arguments share a RegularG so they wrap together.
    test_ir_format("f [ 1 ] [ 2 ] x");
}

#[test]
fn test_app_two_trailing_list_args() {
    // Two trailing list arguments take the dedicated prettyApp branch.
    test_ir_format("f a b [ 1 ] [ 2 ]");
}

#[test]
fn test_app_absorb_last_paren_abstraction() {
    // absorbLast: parenthesised `x: { ... }` renders the body wide inline.
    test_ir_format("f (x: { a = 1; })");
}

#[test]
fn test_app_absorb_last_paren_application() {
    // absorbLast: parenthesised `ident { ... }` renders the body wide inline.
    test_ir_format("f (g { a = 1; })");
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
fn test_lambda_structure() {
    // Lambda with absorbable body
    test_ir_format("x: { inherit x; }");
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

#[test]
fn test_path_term_structure() {
    // Path terms test is_absorbable_term path branch (line 17 in pretty.rs)
    test_ir_format("./path/to/file");
}

#[test]
fn test_float_token() {
    // Test float literal token (line 613 in pretty.rs)
    test_ir_format("3.14");
}

#[test]
fn test_env_path_token() {
    // Test environment path token (line 615 in pretty.rs)
    // Currently has IR mismatch but we run it for coverage
    test_ir_format("<nixpkgs>");
}

#[test]
fn test_member_check_full() {
    // Test member check (? operator) structure (lines 984-991 in pretty.rs)
    test_ir_format("attrs ? foo.bar");
}

#[test]
fn test_concat_with_absorbable_rhs() {
    // Test ++ operator with absorbable RHS (lines 967-968 in pretty.rs)
    test_ir_format("[ 1 ] ++ [ 2 3 ]");
}

#[test]
fn test_lambda_chain_structure() {
    // Test nested lambdas (abstraction chain) (lines 270-286 in pretty.rs)
    test_ir_format("a: b: c: d: body");
}

#[test]
fn test_context_parameter() {
    // Test context parameter (left @ right pattern) (lines 1231-1237 in pretty.rs)
    test_ir_format("args@{ a, b }: a");
}

#[test]
fn test_parameter_with_defaults() {
    // Test parameter attributes with default values (lines 1114-1138 in pretty.rs)
    test_ir_format("{ a ? 1, b ? 2 }: a + b");
}

#[test]
fn test_parameter_with_ellipsis() {
    // Test parameter with ellipsis (lines 1140, 1149-1150, 1159 in pretty.rs)
    test_ir_format("{ ... }: x");
}

#[test]
fn test_empty_parameter_multiline() {
    // Test empty parameter set spanning multiple lines (lines 1201-1214 in pretty.rs)
    test_ir_format("{\n\n}: x");
}

#[test]
fn test_doc_comment_structure() {
    // Test doc comment rendering (line 388 in pretty.rs)
    test_ir_format("/** doc comment */\nx");
}

#[test]
fn test_language_annotation() {
    // Test language annotation comment (lines 402-405, 418-421 in pretty.rs)
    test_ir_format("/* nix */ \"code\"");
}

#[test]
fn test_string_interpolation_with_absorbable() {
    // Test string interpolation with absorbable term (lines 702-724 in pretty.rs)
    test_ir_format("\"prefix ${{ x = 1; }} suffix\"");
}

#[test]
fn test_string_with_leading_whitespace_interpolation() {
    // Test string with leading whitespace and interpolation (lines 750-770 in pretty.rs)
    test_ir_format("\"  ${expr}\"");
}

#[test]
fn test_operation_with_application_rhs() {
    // Test operation with application on RHS (lines 223-228 in pretty.rs)
    test_ir_format("x + f a b");
}

#[test]
fn test_arithmetic_operators() {
    // Test various arithmetic operators (lines 645-651 in pretty.rs: TPlus, TMinus, TMul, TDiv)
    test_ir_format("a - b * c / d");
}

#[test]
fn test_language_annotation_with_string_item() {
    // Test language annotation followed by string in set items (lines 525-543 in pretty.rs)
    test_ir_format("{\n  /* python */ a = \"code\";\n}");
}

#[test]
fn test_assignment_rhs_operation_lhs_set() {
    // Test assignment RHS with operation where LHS is absorbable set
    // The reference uses absorbRHS -> absorbExpr True -> prettyOp True
    // which forces wide rendering (hardlines) for the LHS set
    test_ir_format("{ x = { a = 1; } // y; }");
}

#[test]
fn test_paren_simple_term() {
    // prettyTerm (Parenthesized ...) default branch: line' <> group expr <> line'
    test_ir_format("(a)");
}

#[test]
fn test_paren_simple_application() {
    // prettyTerm (Parenthesized ...) Application branch: prettyApp True mempty True
    test_ir_format("(f a)");
}

#[test]
fn test_paren_trailing_comment_on_open() {
    // moveTrailingCommentUp: trailing comment on `(` becomes pre-trivia before it
    test_ir_format("( # c\n a )");
}

#[test]
fn test_paren_absorb_rhs() {
    // absorbRHS for parenthesized: nest $ hardspace <> absorbParen
    test_ir_format("{ x = (a b); }");
}

#[test]
fn test_app_last_arg_with_pre_comment() {
    // prettyApp: comment on the last argument must stay inside the same
    // nesting as the function head so the arg is indented under it
    test_ir_format("(map toString\n  # comment\n  (builtins.filter f version))");
}

#[test]
fn test_paren_inner_arg_unexpanded() {
    // renderSimple unexpands the function chain so an inner parenthesized arg
    // is flattened into the surrounding hardspace-separated token stream
    test_ir_format("f (a b) c");
}

#[test]
fn test_param_trailing_comment_after_comma() {
    test_ir_format("{ a ? false, # c\n b }: a");
}

#[test]
fn test_param_trailing_comment_last_attr() {
    test_ir_format("{ a, # c\n b ? 1, # d\n}: a");
}

#[test]
fn test_param_trailing_comment_before_comma() {
    test_ir_format("{ a # c\n, b }: a");
}

#[test]
fn test_param_trailing_comment_on_default_before_comma() {
    test_ir_format("{ a ? 1 # c\n, b }: a");
}

#[test]
fn test_param_comma_pretrivia_moves_to_next() {
    test_ir_format("{ a\n# comment\n, b }: a");
    test_ir_format("{ a\n# comment\n, ... }: a");
}
