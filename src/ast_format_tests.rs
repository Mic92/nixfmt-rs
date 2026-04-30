//! AST formatting smoke tests - compare our output with `nixfmt --ast`.
//!
//! One minimal test per top-level `Expression`/`Term` variant (plus the
//! `Trivium` shapes and `Binder`/`Parameter` smoke) so a regression in any
//! variant fails fast with a small diff. Anything more complex lives in
//! `regression_tests/`.

use crate::oracle_tests;
use crate::tests_common::test_ast_format;

oracle_tests! {
    test_ast_format;

    // -----------------------------------------------------------------------
    // Term variants
    // -----------------------------------------------------------------------

    // Term::Token
    test_integer => ["42"],

    // Term::SimpleString
    test_simple_string => ["\"hello\""],

    // Term::IndentedString
    test_indented_string => ["''hello''"],

    // Term::Path
    test_relative_path => ["./foo/bar"],

    // Term::List
    test_simple_list => ["[1 2 3]"],

    // Term::Set / Binder::Assignment
    test_simple_set => ["{a=1;}"],

    // Selector list inside a Binder::Assignment key
    test_dotted_path => ["{a.b.c=1;}"],

    // Binder::Inherit
    test_inherit => ["{inherit a;}"],

    // Term::Selection (with `or` default)
    test_selection_with_default => ["x.y or 42"],

    // Term::Parenthesized
    test_parenthesized => ["(1 + 2)"],

    // -----------------------------------------------------------------------
    // Expression variants
    // -----------------------------------------------------------------------

    test_with => ["with x; y"],

    test_let_simple => ["let a=1; in a"],

    test_assert => ["assert true; 42"],

    test_if_then_else => ["if true then 1 else 2"],

    // Expression::Abstraction / Parameter::ID
    test_simple_lambda => ["x: x"],

    // Parameter::Set
    test_set_pattern => ["{x}: x"],

    test_function_application => ["f x"],

    // Expression::Operation
    test_addition => ["1 + 2"],

    // `<=` / `>=` are not exercised by any other oracle test; keep one
    // smoke so the corresponding Token Display / pretty arms stay covered.
    test_comparison_operators => ["a <= b && c >= d"],

    test_member_check => ["x ? y"],

    test_negation => ["-5"],

    // Expression::Inversion
    test_boolean_not => ["!true"],

    // -----------------------------------------------------------------------
    // Trivium shapes
    // -----------------------------------------------------------------------

    test_line_comment => ["# comment\n42"],

    test_block_comment => ["/* block */ 42"],

    // -----------------------------------------------------------------------
    // Non-redundant regression kept here (no equivalent in regression_tests/)
    // -----------------------------------------------------------------------

    // Inside multi-line strings, # starts literal text, not a comment.
    // The hash and text should be a TextPart, not LineComment trivia.
    test_string_hash_not_comment => [
        r"''
foo ${bar}

# TODO: comment
badFiles=$(find ${filteredHead})
''",
    ],
}
