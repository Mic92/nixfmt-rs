//! Formatted-output regression tests
//!
//! Minimal reproducers for divergences between our final formatted output and
//! the reference `nixfmt` (v1.2.0), discovered by `scripts/diff_sweep.sh`
//! over `nixpkgs/pkgs/`. Each test names the corresponding Haskell
//! function in `Nixfmt.Pretty` / `Nixfmt.Predoc`.

use crate::tests_common::{test_format, test_ir_format};

/// Layout: `Trailing` text must be dropped when a group is rendered compact.
/// Haskell: `Nixfmt.Predoc.fits` skips `Text Trailing`.
#[test]
fn format_trailing_comma_compact_param_set() {
    test_format("{ a, b }: a");
    test_format("{\n  a,\n  b,\n}: a");
}

/// Trailing comments must not count toward line width in `fits`; the
/// binding body stays on the `=` line. The second case checks that a
/// genuinely over-wide *non-comment* RHS still wraps.
/// Haskell: `Nixfmt.Predoc.fits` (`Text TrailingComment` arm).
#[test]
fn format_trailing_comment_ignored_for_width() {
    // From nixpkgs melpa.nix: well under 100 columns, must not wrap.
    test_format("{\n  unstableVersionInNixFormat = parsed != null; # heuristics\n}\n");
    // Short binding + long trailing comment: still must not wrap.
    test_format(concat!(
        "{\n  x = a != null; ",
        "# a trailing comment that is quite a bit longer than the binding itself but still irrelevant\n",
        "}\n",
    ));
    // List element on its own line: `ni` must reflect the indent step so
    // `fits` does not inject a second space before the comment.
    test_format("[\n  a # c\n  b\n]\n");
    // Non-comment part alone exceeds 100 cols → must wrap regardless of comment.
    test_format(concat!(
        "{\n  someVeryVeryVeryVeryVeryVeryVeryVeryVeryVeryVeryVeryVeryLongName = ",
        "aaaaaaaa != bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb; # c\n}\n",
    ));
}

/// A trailing comment after a one-char token must be shifted one column so
/// the lexer still classifies it as trailing on reparse (idempotency).
/// Haskell: `Nixfmt.Predoc.goOne` `TrailingComment` guard.
#[test]
fn format_trailing_comment_shift_for_idempotency() {
    test_format("{ a # b\n= 1; }");
    test_format("[ # c\n1\n]");
    test_format("{ b # a\n? # a\nnull\n,}: b");
}

/// `f (x: { ... })` should absorb the parenthesised abstraction onto the
/// function line. Haskell: `Nixfmt.Pretty.absorbLast` / `isAbsorbableExpr`.
#[test]
fn format_paren_abstraction_absorbed_as_last_arg() {
    test_ir_format("f (finalAttrs: {\n  x = 1;\n  y = 2;\n})");
}

/// A lambda chain whose body is a non-absorbable application must stay on
/// one line when the whole group fits the target width *ignoring leading
/// indentation*, matching Haskell `Nixfmt.Predoc.goGroup` (cc == 0 branch)
/// which calls `fits` with `tw - firstLineWidth rest` and does **not**
/// subtract the pending indent.
#[test]
fn format_lambda_chain_stays_one_line_when_fits() {
    // Reduced from nixpkgs `lib-override-helper.nix`.
    test_format(
        "{\n  addPackageRequires =\n    pkg: packageRequires: addPackageRequiresWhen pkg packageRequires (finalAttrs: previousAttrs: true);\n}",
    );
    // Reduced from `cask/package.nix`: single-param lambda with string body.
    test_format(
        "{\n  formatLoadPath =\n    loadPathItem: \"-L ${\n      if builtins.isString loadPathItem then loadPathItem else \"${loadPathItem}/share/emacs/site-lisp\"\n    }\";\n}",
    );
}

/// Nested simple lambda parameters should stay on one line before an
/// expanded body. Haskell: `Nixfmt.Pretty.absorbAbs`.
#[test]
fn format_nested_lambda_body_absorbed() {
    test_ir_format("final: prev: {\n  a = 1;\n  b = 2;\n}");
}

/// `with X;` followed by an attrset should keep the `{` on the same line,
/// both as a lambda body and as an assignment RHS.
/// Haskell: `Nixfmt.Pretty` `instance Pretty Expression` (With) / `absorbRHS`.
#[test]
fn format_with_body_absorbed() {
    test_ir_format("self: with self; {\n  a = 1;\n  b = 2;\n}");
    test_ir_format("{\n  meta = with lib; {\n    license = mit;\n  };\n}");
}

/// `x = f \"a\" ''..'';` should keep the application on the `=` line when the
/// last argument is an absorbable multiline string.
/// Haskell: `Nixfmt.Pretty.absorbRHS` (Application case).
#[test]
fn format_assignment_rhs_app_with_string_last_arg() {
    test_ir_format("{\n  w = writeShellScript \"n\" ''\n    echo a\n    echo b\n  '';\n}");
}

/// Chained `if ... else if ...` must always be expanded onto multiple lines.
/// Haskell: `Nixfmt.Pretty.prettyIf` emits hardlines between branches.
#[test]
fn format_if_elseif_chain_forced_multiline() {
    test_ir_format("{ x = if a then \"x\" else if b then \"y\" else \"z\"; }");
}

/// Multi-argument application that expands should keep the first argument on
/// the function line and indent continuation arguments by two spaces.
/// Haskell: `Nixfmt.Pretty.prettyApp`.
#[test]
fn format_multi_arg_application_continuation_indent() {
    test_ir_format("runCommand \"n\"\n  {\n    a = 1;\n  }\n  ''\n    echo a\n  ''");
}

/// A line comment before the last argument forces expansion; the comment and
/// the argument must still be indented under the function head.
/// Haskell: `Nixfmt.Pretty.prettyApp` / `absorbLast` (`absorbParen` keeps the
/// argument's pre-trivia inside the same `nest` as the function chain).
#[test]
fn format_app_comment_before_last_arg_indent() {
    test_format("(map toString\n  # comment\n  (builtins.filter f version))");
    test_format(
        "{\n  v = lib.concat \".\" (\n    map toString\n      # comment\n      (builtins.filter f version)\n  );\n}",
    );
}

/// `//` / `++` / `+` on the RHS of an assignment must absorb onto the `=` line
/// when the LHS is an absorbable term and there is no leading trivia.
/// Haskell: `Nixfmt.Pretty.absorbRHS` Operation Case 1 / `prettyOp True`.
#[test]
fn format_assignment_rhs_update_concat_plus_case1() {
    test_format("{ meta = oldAttrs.meta // { description = \"x\"; }; }");
    test_ir_format("{ meta = oldAttrs.meta // { description = \"x\"; }; }");
    test_format("{ x = a.b or [ ] ++ [ y ]; }");
    test_ir_format("{ x = a.b or [ ] ++ [ y ]; }");
    test_format("{ x = lib.optionals a [ p ] ++ lib.optionals b [ q ]; }");
    // Leading trivia on the LHS term disables Case 1 and falls through to Case 2.
    test_ir_format("{ x = /* c */ [ a ] ++ [ b ]; }");
    test_ir_format("{ x = [ a ] ++ [ b ] ++ [ c ]; }");
}

/// `//` with an absorbable RHS term and a non-absorbable LHS keeps `// {` on
/// the LHS line and only expands the attrset.
/// Haskell: `Nixfmt.Pretty.absorbRHS` Operation Case 2.
#[test]
fn format_assignment_rhs_update_concat_plus_case2() {
    test_format("{ x = a // { y = 1;\n z = 2;\n}; }");
    test_ir_format("{ x = a // { y = 1;\n z = 2;\n}; }");
}

/// Layout: when the priority-expansion of one argument fails because the
/// remaining arguments cannot be rendered compactly, the layout state must be
/// restored before falling through to full expansion. Otherwise the indent
/// stack is poisoned and continuation arguments lose their 2-space indent.
/// Haskell: `Nixfmt.Predoc.layoutGreedy` (`goPriorityGroup` via `StateT _ Maybe`).
#[test]
fn format_app_multi_absorbable_args_indent() {
    // sddm/default.nix: `runCommand "name" { ... } ''script''`
    test_format("runCommand \"n\"\n  {\n    a = 1;\n  }\n  ''\n    echo a\n  ''");
    // android-studio/common.nix: app + attrset arg + parenthesised last arg
    test_format("f\n  {\n    a = 1;\n  }\n  (\n    b: c\n  )");
}

/// Layout: with the application nested inside a binding, the `{` must stay on
/// the function line and the body indented relative to the binding.
/// Haskell: `Nixfmt.Predoc.layoutGreedy` priority-group fallback.
#[test]
fn format_app_set_absorb_in_binding() {
    // wrapper.nix style: `x = mk { ... } ''..'';`
    test_format("{\n  x = mk {\n    a = 1;\n  } ''\n    echo a\n  '';\n}");
}
