//! Formatted-output regression tests
//!
//! Minimal reproducers for divergences between our final formatted output and
//! the reference `nixfmt` (v1.2.0), discovered by `scripts/diff_sweep.sh`
//! over `nixpkgs/pkgs/`. Each test names the corresponding Haskell
//! function in `Nixfmt.Pretty` / `Nixfmt.Predoc`.

use crate::tests_common::{test_format, test_ir_format};

/// Layout: `Trailing` text must be dropped when a group is rendered compact.
/// Haskell: `Nixfmt.Predoc.fits` skips `Text Trailing`.
/// Fixture: fast reproducer for `tests/fixtures/nixfmt/diff/lambda/`.
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
/// Fixture: fast reproducer for `tests/fixtures/nixfmt/diff/with/`.
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
/// Fixture: fast reproducer for `tests/fixtures/nixfmt/diff/if_else/`.
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
/// Fixture: fast reproducer for `tests/fixtures/nixfmt/diff/operation/`.
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
/// Fixture: fast reproducer for `tests/fixtures/nixfmt/diff/attr_set/`.
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

/// A lone interpolation on an indented-string line whose body is a function
/// application must keep `${` on the string line and absorb the call body
/// (no `line'` between `${` and the application).
/// Haskell: `Nixfmt.Pretty.instance Pretty [StringPart]` Application arm.
#[test]
fn format_interp_lone_application_absorbed() {
    // 7zip-zstd: non-simple application (parenthesised arg / >2 args).
    test_format("''\n  ${lib.optionalString (!isWindows) ''\n    one\n    two\n  ''}\n''");
    test_format("''\n  ${lib.optionalString a b ''\n    one\n    two\n  ''}\n''");
    // emacs wrapper: simple application; IR must place `${` inside the group
    // and nest the call body one level deeper.
    test_ir_format("''\n  ${lib.optionalString cond ''\n    one\n    two\n  ''}\n''");
    // With leading whitespace and a wide body that overflows the budget.
    test_format(concat!(
        "''\n  x\n    ${lib.optionalString cond ''\n",
        "      linkPath one two three four five six seven eight nine ten eleven twelve thirteen fourteen\n",
        "    ''}\n''",
    ));
    // vscodeWithConfiguration: application whose middle arg is a parenthesised
    // abstraction that itself wraps; `${` must still hug the call head.
    test_format(concat!(
        "''\n  ${lib.concatMapStringsSep \"n\" (\n",
        "    e: \"ln -sfn ${e}/share/vscode/extensions/aaaa/bbb/ccc/dddd\"\n",
        "  ) nixExtsDrvs}\n''",
    ));
}

/// A short single-line interpolation that is not the only thing on the
/// string line must stay inline (forced compact up to 30 columns) even when
/// the surrounding line already overflows.
/// Haskell: `Nixfmt.Pretty.instance Pretty StringPart` (`unexpandSpacing' (Just 30)`).
#[test]
fn format_interp_inline_short_forced_compact() {
    // ms-vscode.cpptools: `${lib.makeBinPath [ gdb ]}` after a long line.
    test_format(concat!(
        "''\n  wrap a/very/long/share/vscode/extensions/ms-vscode.cpptools/debug/bin/OpenDebugAD7 ",
        "--prefix PATH : ${lib.makeBinPath [ gdb ]}\n''",
    ));
    test_ir_format("''\n  prefix ${lib.makeBinPath [ gdb ]} suffix\n''");
}

/// Non-chainable comparison operators (`<`, `>`, `<=`, `>=`, `==`, `!=`) use
/// `softline` before the operator and `hardspace` after, with no extra `nest`
/// on the RHS, so a short RHS stays on the LHS's last line even when the LHS
/// is multi-line.
/// Haskell: `Nixfmt.Pretty.instance Pretty Expression` `Operation` comparison arm.
#[test]
fn format_comparison_op_softline() {
    // fetchgithub: multi-line application LHS, short identifier RHS.
    test_format(concat!(
        "{\n  x =\n    f (\n      a:\n      if cond ",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa then\n",
        "        a\n      else\n        b\n    ) y != y;\n}",
    ));
    // nixops: comment as preTrivia on the RHS must not be nested under the op.
    test_format("{\n  x =\n    a ==\n    # note\n    b.c (d: e);\n}");
    test_ir_format("a == b");
    // dev-shell-tools: LHS is `//` chain, RHS is an attrset.
    test_format("a // { b = 1; } == { c = 1; }");
}

/// A binding whose LHS is long or uses a non-ID selector (string /
/// interpolation key) gets a `line'` before the RHS so the value can move to
/// its own indented line when the whole binding overflows.
/// Haskell: `Nixfmt.Pretty.instance Pretty Binder` `Assignment` `rhs` guard.
#[test]
fn format_assignment_non_simple_selector_breaks_rhs() {
    test_format(concat!(
        "{\n  \"PKG_CONFIG_GIMP_${pkgConfigMajorVersion}_0_GIMPLIBDIR\" = ",
        "\"${placeholder \"out\"}/${gimp.targetLibDir}\";\n}",
    ));
    // >4 selectors also triggers the break even when each is a plain id.
    test_format(concat!(
        "{\n  a.b.c.d.e = \"",
        "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\";\n}",
    ));
}

/// Comments after the top-level expression must be preserved.
/// Haskell: `Nixfmt.Parser.file` attaches trailing trivia to `Whole`.
#[test]
fn format_trailing_file_trivia_preserved() {
    test_format("{ a = 1; }\n# trailing\n");
    test_format("1\n/* block */\n");
    test_ir_format("{ a = 1; }\n# trailing\n");
}

/// A comment between the interpolation body and `}` must be preserved as the
/// `Whole`'s trailing trivia and force the `${ … }` onto multiple lines.
/// Haskell: `Nixfmt.Parser.interpolation` (`whole expression`).
#[test]
fn format_interp_trailing_trivia_preserved() {
    test_format("''\n  ${ f a b\n  # c\n  }\n''");
    test_format("\"${ x\n# c\n}\"");
}

/// A comment between `${` and the interpolation body must be preserved as
/// leading trivia of the body's first token.
/// Haskell: `Nixfmt.Lexer.lexeme` (no special-cased trailing slot for `${`).
#[test]
fn format_interp_leading_trivia_preserved() {
    test_format("''\n  ${ # leading\n  x }\n''");
    test_format("\"${ /* c */ x}\"");
    // Selector interpolations go through `lexeme()` for `${`, so the comment
    // is first classified as its `trail_comment` and must be re-queued.
    test_format("{ ${ # c\nx } = 1; }");
    test_format("a.${ # c\nx }");
}

/// Layout width must count Unicode scalars, not UTF-8 bytes, so multi-byte
/// glyphs like `«»` don't push a line over the 100-column budget.
/// Haskell: `Nixfmt.Predoc.textWidth` = `Text.length`.
/// Reproduces: nixpkgs `pkgs/stdenv/generic/check-meta.nix`.
#[test]
fn format_text_width_counts_chars_not_bytes() {
    test_format(
        "{\n  getName =\n    attrs: attrs.name or \"${attrs.pname or \"\u{ab}name-missing\u{bb}\"}-${attrs.version or \"\u{ab}version-missing\u{bb}\"}\";\n}\n",
    );
}

/// An empty set whose `{` carries pre-trivia (so the LoneAnn fast path is
/// skipped) must still honour the source line break between `{` and `}`.
/// Haskell: `Nixfmt.Pretty.prettySet` second clause, `sep` condition.
/// Reproduces: nixpkgs `nixos/modules/system/service/systemd/user.nix`.
#[test]
fn format_empty_set_with_pretrivia_keeps_linebreak() {
    test_format("# c\n{\n}\n");
}
