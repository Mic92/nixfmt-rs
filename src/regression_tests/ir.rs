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

/// Regression test: import application should be absorbable
#[test]
fn test_import_absorbability() {
    test_ir_format("{ x = import ./foo; }");
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
