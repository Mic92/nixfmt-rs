//! Formatted-output regression tests
//!
//! Minimal reproducers for divergences between our final formatted output and
//! the reference `nixfmt` (v1.2.0), discovered by `scripts/diff_sweep.sh`
//! over `nixpkgs/pkgs/`. Each test names the Haskell function in
//! `Nixfmt.Pretty` / `Nixfmt.Predoc` whose behaviour we diverge from.

use crate::tests_common::{test_format, test_ir_format};

/// Layout: `Trailing` text must be dropped when a group is rendered compact.
/// Haskell: `Nixfmt.Predoc.fits` skips `Text Trailing`.
#[test]
fn format_trailing_comma_compact_param_set() {
    test_format("{ a, b }: a");
    test_format("{\n  a,\n  b,\n}: a");
}

/// `f (x: { ... })` should absorb the parenthesised abstraction onto the
/// function line. Haskell: `Nixfmt.Pretty.absorbLast` / `isAbsorbableExpr`.
#[test]
#[ignore = "Nixfmt.Pretty.absorbLast: parenthesised abstraction not absorbed"]
fn format_paren_abstraction_absorbed_as_last_arg() {
    test_ir_format("f (finalAttrs: {\n  x = 1;\n  y = 2;\n})");
}

/// Nested simple lambda parameters should stay on one line before an
/// expanded body. Haskell: `Nixfmt.Pretty.absorbAbs`.
#[test]
#[ignore = "Nixfmt.Pretty.absorbAbs: nested lambda body not absorbed"]
fn format_nested_lambda_body_absorbed() {
    test_ir_format("final: prev: {\n  a = 1;\n  b = 2;\n}");
}

/// `with X;` followed by an attrset should keep the `{` on the same line,
/// both as a lambda body and as an assignment RHS.
/// Haskell: `Nixfmt.Pretty` `instance Pretty Expression` (With) / `absorbRHS`.
#[test]
#[ignore = "Nixfmt.Pretty With/absorbRHS: with-body attrset not absorbed"]
fn format_with_body_absorbed() {
    test_ir_format("self: with self; {\n  a = 1;\n  b = 2;\n}");
    test_ir_format("{\n  meta = with lib; {\n    license = mit;\n  };\n}");
}

/// `x = f \"a\" ''..'';` should keep the application on the `=` line when the
/// last argument is an absorbable multiline string.
/// Haskell: `Nixfmt.Pretty.absorbRHS` (Application case).
#[test]
#[ignore = "Nixfmt.Pretty.absorbRHS: application with string last arg not absorbed"]
fn format_assignment_rhs_app_with_string_last_arg() {
    test_ir_format("{\n  w = writeShellScript \"n\" ''\n    echo a\n    echo b\n  '';\n}");
}

/// Chained `if ... else if ...` must always be expanded onto multiple lines.
/// Haskell: `Nixfmt.Pretty.prettyIf` emits hardlines between branches.
#[test]
#[ignore = "Nixfmt.Pretty.prettyIf: else-if chain not forced multiline"]
fn format_if_elseif_chain_forced_multiline() {
    test_ir_format("{ x = if a then \"x\" else if b then \"y\" else \"z\"; }");
}

/// Multi-argument application that expands should keep the first argument on
/// the function line and indent continuation arguments by two spaces.
/// Haskell: `Nixfmt.Pretty.prettyApp`.
#[test]
#[ignore = "Nixfmt.Pretty.prettyApp: expanded application continuation indent"]
fn format_multi_arg_application_continuation_indent() {
    test_ir_format("runCommand \"n\"\n  {\n    a = 1;\n  }\n  ''\n    echo a\n  ''");
}
