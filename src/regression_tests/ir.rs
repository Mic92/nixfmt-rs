//! IR regression tests
//!
//! Tests for IR formatting differences between nixfmt-rs and reference nixfmt

use crate::tests_common::test_ir_format;

/// Regression test: let expression should wrap letPart and inPart in groups
#[test]
fn test_let_expression_groups() {
    test_ir_format(
        "{ pinnedJson ? ./pinned.json, }: let pinned = (builtins.fromJSON (builtins.readFile pinnedJson)).pins; in pinned",
    );
}

/// Regression test: function arguments should use Priority groups
#[test]
fn test_function_arguments_priority() {
    test_ir_format("fetchTarball { url = \"x\"; }");
    test_ir_format("lib.filterAttrs (x: y)");
}

/// Regression test: transparent groups in parentheses need Break spacing
#[test]
fn test_transparent_in_parens_break() {
    test_ir_format("(lib.filterAttrs (x: y))");
}

/// Regression test: application in assignment RHS should group space with application
///
/// Issue: `nixfmt` groups the space before the application INSIDE the application group,
/// whereas `nixfmt-rs` was putting the space outside.
#[test]
fn test_assignment_rhs_application_grouping() {
    test_ir_format("{ x = f { }; }");
}

// `isAbsorbable` / `isAbsorbableExpr` / `absorbExpr` parity with Haskell nixfmt.
// Each test exercises one clause of the Haskell definitions and asserts our IR
// matches `nixfmt --ir -` exactly.

/// `absorbExpr True (Term t)` must use `prettyTermWide`, i.e. force-expand the
/// attrset on the RHS of an assignment.
#[test]
fn test_absorb_rhs_set_wide() {
    test_ir_format("{ x = { a = 1; }; }");
}

/// `absorbExpr` on a `with ...; { ... }` must go through `prettyWith True`.
#[test]
fn test_absorb_rhs_with_set() {
    test_ir_format("{ x = with p; { a = 1; }; }");
}

/// `isAbsorbableExpr` accepts `Abstraction (IDParameter _) _ (Term t)` when
/// the body is an absorbable term.
#[test]
fn test_absorb_rhs_lambda_set() {
    test_ir_format("{ x = a: { b = 1; }; }");
}

/// `isAbsorbableExpr` recurses through chained `IDParameter` abstractions.
#[test]
fn test_absorb_rhs_lambda_chain_set() {
    test_ir_format("{ x = a: b: { c = 1; }; }");
}

/// `isAbsorbable (Parenthesized (LoneAnn _) (Term t) _)` recurses into the
/// inner term.
#[test]
fn test_absorb_rhs_paren_set() {
    test_ir_format("{ x = ({ a = 1; }); }");
}

/// `isAbsorbable` on an empty set whose braces span multiple source lines.
#[test]
fn test_absorb_rhs_empty_set_multiline() {
    test_ir_format("{ x = {\n}; }");
}

/// `isAbsorbable` on a list that contains only comment items.
#[test]
fn test_absorb_rhs_comment_only_list() {
    test_ir_format("{ x = [\n# c\n]; }");
}

/// `isAbsorbable` on a multi-line indented string.
#[test]
fn test_absorb_rhs_indented_string() {
    test_ir_format("{ x = ''\n  a\n  b\n''; }");
}

/// Single-`inherit` attrset on the RHS goes through `absorbExpr False` and
/// must not be force-expanded.
#[test]
fn test_absorb_rhs_single_inherit() {
    test_ir_format("{ x = { inherit a; }; }");
}

/// Regression test: middle arguments in application should use absorbLast logic (RegularG if not absorbable)
#[test]
fn test_middle_arg_grouping() {
    test_ir_format("lib.pipe pkgs.lixPackageSets [ ]");
}

/// Regression test: With expression in assignment RHS should be grouped with the leading space
#[test]
fn test_with_grouping() {
    test_ir_format("{ x = with p; y; }");
}

/// Regression test: small simple list as inner application argument uses soft `line` separators
#[test]
fn test_inner_arg_simple_list() {
    test_ir_format("f [ 1 2 3 ] x");
}

/// Regression test: list rendering must match Haskell `renderList`/`prettyTerm (List ..)`
#[test]
fn test_list_rendering() {
    test_ir_format("[ ]");
    test_ir_format("[\n]");
    test_ir_format("[ # c\n]");
    test_ir_format("[ 1 2 ]");
    test_ir_format("[\n1 2\n]");
}
/// Regression test: language annotation inside an inner-argument list stays
/// attached to its string with a hardspace (nixfmt f4bbd8c).
#[test]
fn test_inner_arg_list_language_annotation() {
    test_ir_format(r#"f [ /* bash */ "x" ] y"#);
}

/// Regression test: empty lists/sets containing only a comment must be
/// absorbable and rendered with hardlines so formatting is idempotent.
/// https://github.com/NixOS/nixfmt/issues/362
#[test]
fn test_empty_container_with_comment_idempotent() {
    test_ir_format("{ x = [ /* foo */ ]; }");
    test_ir_format("{ x = { /* foo */ }; }");
    test_ir_format("{ x = [\n# foo\n]; }");
}

/// Regression test: comments after the last attr in a parameter set must be
/// nested like comments between attrs (nixfmt 0f6eb2b).
#[test]
fn test_param_set_trailing_comment_nesting() {
    test_ir_format("{ a,\n# c\n}: x");
}

/// Regression: `with` expression uses `nest (group expr0)` for the scope expression
#[test]
fn test_with_simple() {
    test_ir_format("with p; y");
}

/// Regression: `with` followed by an attrset body
#[test]
fn test_with_set_body() {
    test_ir_format("with p; { a = 1; }");
}

/// Regression: `assert` is rendered via the application path (`prettyApp`)
#[test]
fn test_assert_simple() {
    test_ir_format("assert c; x");
}

/// Regression: `let` uses `letPart <> hardline <> inPart` structure
#[test]
fn test_let_simple() {
    test_ir_format("let a = 1; in a");
}

/// Regression: `if` uses `surroundWith line (nest (group then))` and recursive `prettyIf`
#[test]
fn test_if_simple() {
    test_ir_format("if c then a else b");
}

/// Regression: `if` nested inside an attrset binding
#[test]
fn test_if_in_set() {
    test_ir_format("{ x = if c then a else b; }");
}

/// Regression: `else if` chains are flattened by `prettyIf` instead of nesting
#[test]
fn test_if_else_if_chain() {
    test_ir_format("if c then a else if d then e else f");
}
