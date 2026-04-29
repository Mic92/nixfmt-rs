//! IR formatting regression tests
//!
//! These tests compare our IR output with the reference nixfmt implementation
//! to ensure we match the expected pretty-printing structure.

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
fn test_selection_with_default_structure() {
    // Member selection with default exercises selector spacing and "or" clause layout
    test_ir_format("config.services.nginx.enable or false");
}

#[test]
fn test_update_absorbable_rhs_structure() {
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
    test_ir_format("''\n  line1\n  line2\n''");
}

#[test]
fn test_string_interpolation_structure() {
    // String interpolation exercises nested grouping with line_prime
    test_ir_format("\"prefix ${expr} suffix\"");
}

#[test]
fn test_nested_groups_structure() {
    test_ir_format("[ { a = 1; } { b = 2; } ]");
}

#[test]
fn test_negation_structure() {
    test_ir_format("-42");
}

#[test]
fn test_boolean_not_structure() {
    test_ir_format("!true");
}

#[test]
fn test_parenthesized_complex_structure() {
    // Parenthesized complex expressions with line_prime separators
    test_ir_format("(let x = 1; in x)");
}

#[test]
fn test_list_with_comments_structure() {
    test_ir_format("[\n  # comment\n  1\n  2\n]");
}

#[test]
fn test_rec_set_structure() {
    test_ir_format("rec { a = 1; b = a; }");
}

#[test]
fn test_path_term_structure() {
    // is_absorbable_term path branch
    test_ir_format("./path/to/file");
}

#[test]
fn test_float_token() {
    test_ir_format("3.14");
}

#[test]
fn test_env_path_token() {
    test_ir_format("<nixpkgs>");
}

#[test]
fn test_member_check_full() {
    test_ir_format("attrs ? foo.bar");
}

#[test]
fn test_concat_with_absorbable_rhs() {
    test_ir_format("[ 1 ] ++ [ 2 3 ]");
}

#[test]
fn test_lambda_chain_structure() {
    // absorbAbs chain of IDParameters
    test_ir_format("a: b: c: d: body");
}

#[test]
fn test_context_parameter() {
    test_ir_format("args@{ a, b }: a");
}

#[test]
fn test_parameter_with_defaults() {
    test_ir_format("{ a ? 1, b ? 2 }: a + b");
}

#[test]
fn test_parameter_with_ellipsis() {
    test_ir_format("{ ... }: x");
}

#[test]
fn test_empty_parameter_multiline() {
    test_ir_format("{\n\n}: x");
}

#[test]
fn test_doc_comment_structure() {
    test_ir_format("/** doc comment */\nx");
}

#[test]
fn test_language_annotation() {
    test_ir_format("/* nix */ \"code\"");
}

#[test]
fn test_string_interpolation_with_absorbable() {
    test_ir_format("\"prefix ${{ x = 1; }} suffix\"");
}

#[test]
fn test_string_with_leading_whitespace_interpolation() {
    test_ir_format("\"  ${expr}\"");
}

#[test]
fn test_operation_with_application_rhs() {
    test_ir_format("x + f a b");
}

#[test]
fn test_arithmetic_operators() {
    test_ir_format("a - b * c / d");
}

#[test]
fn test_language_annotation_with_string_item() {
    // language annotation as pre-trivia on a binder key
    test_ir_format("{\n  /* python */ a = \"code\";\n}");
}

#[test]
fn test_paren_simple_term() {
    // prettyTerm (Parenthesized ...) default branch: line' <> group expr <> line'
    test_ir_format("(a)");
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
