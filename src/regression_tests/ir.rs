//! IR regression tests
//!
//! Tests for IR formatting differences between nixfmt-rs and reference nixfmt

use crate::oracle_tests;
use crate::tests_common::test_ir_format;

oracle_tests! {
    test_ir_format;

    /// Regression test: let expression should wrap letPart and inPart in groups
    test_let_expression_groups => [
        "{ pinnedJson ? ./pinned.json, }: let pinned = (builtins.fromJSON (builtins.readFile pinnedJson)).pins; in pinned",
    ],

    /// Regression test: function arguments should use Priority groups
    test_function_arguments_priority => [
        "fetchTarball { url = \"x\"; }",
        "lib.filterAttrs (x: y)",
    ],

    /// Regression test: transparent groups in parentheses need Break spacing
    test_transparent_in_parens_break => ["(lib.filterAttrs (x: y))"],

    /// Regression test: application in assignment RHS should group space with application
    ///
    /// Issue: `nixfmt` groups the space before the application INSIDE the application group,
    /// whereas `nixfmt-rs` was putting the space outside.
    test_assignment_rhs_application_grouping => ["{ x = f { }; }"],

    // `isAbsorbable` / `isAbsorbableExpr` / `absorbExpr` parity with Haskell nixfmt.
    // Each test exercises one clause of the Haskell definitions and asserts our IR
    // matches `nixfmt --ir -` exactly.

    /// `absorbExpr True (Term t)` must use `prettyTermWide`, i.e. force-expand the
    /// attrset on the RHS of an assignment.
    test_absorb_rhs_set_wide => ["{ x = { a = 1; }; }"],

    /// `absorbExpr` on a `with ...; { ... }` must go through `prettyWith True`.
    test_absorb_rhs_with_set => ["{ x = with p; { a = 1; }; }"],

    /// `isAbsorbableExpr` accepts `Abstraction (IDParameter _) _ (Term t)` when
    /// the body is an absorbable term.
    test_absorb_rhs_lambda_set => ["{ x = a: { b = 1; }; }"],

    /// `isAbsorbableExpr` recurses through chained `IDParameter` abstractions.
    test_absorb_rhs_lambda_chain_set => ["{ x = a: b: { c = 1; }; }"],

    /// `isAbsorbable (Parenthesized (LoneAnn _) (Term t) _)` recurses into the
    /// inner term.
    test_absorb_rhs_paren_set => ["{ x = ({ a = 1; }); }"],

    /// `isAbsorbable` on an empty set whose braces span multiple source lines.
    test_absorb_rhs_empty_set_multiline => ["{ x = {\n}; }"],

    /// `isAbsorbable` on a list that contains only comment items.
    test_absorb_rhs_comment_only_list => ["{ x = [\n# c\n]; }"],

    /// `isAbsorbable` on a multi-line indented string.
    test_absorb_rhs_indented_string => ["{ x = ''\n  a\n  b\n''; }"],

    /// Single-`inherit` attrset on the RHS goes through `absorbExpr False` and
    /// must not be force-expanded.
    test_absorb_rhs_single_inherit => ["{ x = { inherit a; }; }"],

    /// Regression test: middle arguments in application should use `absorbLast` logic (`RegularG` if not absorbable)
    test_middle_arg_grouping => ["lib.pipe pkgs.lixPackageSets [ ]"],

    /// Regression test: With expression in assignment RHS should be grouped with the leading space
    test_with_grouping => ["{ x = with p; y; }"],

    /// Regression test: small simple list as inner application argument uses soft `line` separators
    test_inner_arg_simple_list => ["f [ 1 2 3 ] x"],

    /// Regression test: list rendering must match Haskell `renderList`/`prettyTerm (List ..)`
    test_list_rendering => [
        "[ ]",
        "[\n]",
        "[ # c\n]",
        "[ 1 2 ]",
        "[\n1 2\n]",
    ],

    /// Regression test: language annotation inside an inner-argument list stays
    /// attached to its string with a hardspace (nixfmt f4bbd8c).
    test_inner_arg_list_language_annotation => [r#"f [ /* bash */ "x" ] y"#],

    /// Regression test: empty lists/sets containing only a comment must be
    /// absorbable and rendered with hardlines so formatting is idempotent.
    /// <https://github.com/NixOS/nixfmt/issues/362>
    test_empty_container_with_comment_idempotent => [
        "{ x = [ /* foo */ ]; }",
        "{ x = { /* foo */ }; }",
        "{ x = [\n# foo\n]; }",
    ],

    /// Regression test: comments after the last attr in a parameter set must be
    /// nested like comments between attrs (nixfmt 0f6eb2b).
    test_param_set_trailing_comment_nesting => ["{ a,\n# c\n}: x"],

    /// Regression: `with` followed by an attrset body
    test_with_set_body => ["with p; { a = 1; }"],

    /// Regression: `if` nested inside an attrset binding
    test_if_in_set => ["{ x = if c then a else b; }"],

    /// Regression: `else if` chains are flattened by `prettyIf` instead of nesting
    test_if_else_if_chain => ["if c then a else if d then e else f"],

    /// Regression: string selectors (`x."y"`) must render their actual content,
    /// not a `"..."` placeholder.
    test_string_selector_pretty => [r#"x."hello""#],

    /// `absorbLast`/`absorbExpr False` must call `prettyTerm`, which (unlike
    /// `instance Pretty Term`) does *not* wrap a `List` in an extra group.
    /// Haskell: `Nixfmt.Pretty.absorbLast` / `absorbExpr`.
    test_absorb_uses_pretty_term_for_list => [
        "f [ a ]",
        "(x: [ a ])",
    ],

    /// Set-pattern abstraction with an absorbable body wraps the body in
    /// `group (prettyTermWide t)`. Haskell: `Nixfmt.Pretty` `Abstraction` clause.
    test_set_param_abstraction_absorbs_body => ["{ lib }: { a = 1; }"],
}
