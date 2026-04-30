//! IR formatting regression tests
//!
//! These tests compare our IR output with the reference nixfmt implementation
//! to ensure we match the expected pretty-printing structure.

use crate::oracle_tests;
use crate::tests_common::test_ir_format;

oracle_tests! {
    test_ir_format;

    /// Regression test: simple parameter pattern should have outer Group wrapper
    ///
    /// Issue: nixfmt-rs was missing the outer `Group RegularG` wrapper that the
    /// reference implementation adds when pretty-printing a Whole Expression (File).
    /// This has been fixed by wrapping `Whole<T>::pretty` in `push_group()`.
    test_simple_parameter_pattern => ["{ a, b }: x"],

    // Member selection with default exercises selector spacing and "or" clause layout
    test_selection_with_default_structure => ["config.services.nginx.enable or false"],

    test_update_absorbable_rhs_structure => ["attrs // { inherit value; }"],

    // Selection as a non-final argument must use a RegularG (not Priority) group.
    test_app_selection_inner_arg => ["f a.b c"],

    // Two consecutive list arguments share a RegularG so they wrap together.
    test_app_two_consecutive_list_args => ["f [ 1 ] [ 2 ] x"],

    // Two trailing list arguments take the dedicated prettyApp branch.
    test_app_two_trailing_list_args => ["f a b [ 1 ] [ 2 ]"],

    // absorbLast: parenthesised `ident { ... }` renders the body wide inline.
    test_app_absorb_last_paren_application => ["f (g { a = 1; })"],

    // Many identifiers trigger the hardline-based spacing path for inherits
    test_inherit_many_identifiers_structure => ["let inherit foo bar baz qux; in foo"],

    // Inherit with explicit source exercises nested grouping and spacing branch
    test_inherit_with_source_structure => ["let inherit (inputs) foo bar; in foo"],

    // Selection starting from a parenthesized term forces softline_prime separator
    test_selection_from_parenthesized_term_structure => ["({ inherit foo; }).foo or true"],

    // Selection from a record term forces line_prime separator before selectors
    test_selection_from_record_term_structure => ["rec { nested = { }; }.nested or { }"],

    // Comments generate Comment and TrailingComment annotations
    test_comment_structure => ["/* block comment */ let a = 1; in a # line comment"],

    // Empty lines generate Emptyline spacing, Priority and Transparent groups
    test_empty_line_structure => ["{\n\n  a = 1;\n}"],

    test_multiline_string_structure => ["''\n  line1\n  line2\n''"],

    // String interpolation exercises nested grouping with line_prime
    test_string_interpolation_structure => ["\"prefix ${expr} suffix\""],

    test_nested_groups_structure => ["[ { a = 1; } { b = 2; } ]"],

    test_negation_structure => ["-42"],

    test_boolean_not_structure => ["!true"],

    // Parenthesized complex expressions with line_prime separators
    test_parenthesized_complex_structure => ["(let x = 1; in x)"],

    test_list_with_comments_structure => ["[\n  # comment\n  1\n  2\n]"],

    test_rec_set_structure => ["rec { a = 1; b = a; }"],

    // is_absorbable_term path branch
    test_path_term_structure => ["./path/to/file"],

    test_float_token => ["3.14"],

    test_env_path_token => ["<nixpkgs>"],

    test_member_check_full => ["attrs ? foo.bar"],

    test_concat_with_absorbable_rhs => ["[ 1 ] ++ [ 2 3 ]"],

    // absorbAbs chain of IDParameters
    test_lambda_chain_structure => ["a: b: c: d: body"],

    test_context_parameter => ["args@{ a, b }: a"],

    test_parameter_with_defaults => ["{ a ? 1, b ? 2 }: a + b"],

    test_parameter_with_ellipsis => ["{ ... }: x"],

    test_empty_parameter_multiline => ["{\n\n}: x"],

    test_doc_comment_structure => ["/** doc comment */\nx"],

    test_language_annotation => ["/* nix */ \"code\""],

    test_string_interpolation_with_absorbable => ["\"prefix ${{ x = 1; }} suffix\""],

    test_string_with_leading_whitespace_interpolation => ["\"  ${expr}\""],

    test_operation_with_application_rhs => ["x + f a b"],

    test_arithmetic_operators => ["a - b * c / d"],

    // language annotation as pre-trivia on a binder key
    test_language_annotation_with_string_item => ["{\n  /* python */ a = \"code\";\n}"],

    // prettyTerm (Parenthesized ...) default branch: line' <> group expr <> line'
    test_paren_simple_term => ["(a)"],

    // moveTrailingCommentUp: trailing comment on `(` becomes pre-trivia before it
    test_paren_trailing_comment_on_open => ["( # c\n a )"],

    // absorbRHS for parenthesized: nest $ hardspace <> absorbParen
    test_paren_absorb_rhs => ["{ x = (a b); }"],

    // prettyApp: comment on the last argument must stay inside the same
    // nesting as the function head so the arg is indented under it
    test_app_last_arg_with_pre_comment => ["(map toString\n  # comment\n  (builtins.filter f version))"],

    // renderSimple unexpands the function chain so an inner parenthesized arg
    // is flattened into the surrounding hardspace-separated token stream
    test_paren_inner_arg_unexpanded => ["f (a b) c"],

    test_param_trailing_comment_after_comma => ["{ a ? false, # c\n b }: a"],

    test_param_trailing_comment_last_attr => ["{ a, # c\n b ? 1, # d\n}: a"],

    test_param_trailing_comment_before_comma => ["{ a # c\n, b }: a"],

    test_param_trailing_comment_on_default_before_comma => ["{ a ? 1 # c\n, b }: a"],

    test_param_comma_pretrivia_moves_to_next => [
        "{ a\n# comment\n, b }: a",
        "{ a\n# comment\n, ... }: a",
    ],
}
