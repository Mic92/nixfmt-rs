//! Consolidated regression tests

use crate::tests_common::{assert_parse_error_contains, assert_parse_rejected, test_ast_format};

#[test]
fn regression_string_interpolation_selectors() {
    test_ast_format(r#"x."y""#);
    test_ast_format(r#"x.${"y"}"#);
    test_ast_format(r#"x.${foo}"#);
}

#[test]
fn regression_or_as_identifier() {
    test_ast_format("or");
}

#[test]
fn regression_or_operator_deprecated_syntax() {
    // From nix/tests/functional/lang/eval-okay-deprecate-cursed-or.nix line 3
    // In `[ (x: x) or ]`, the `or` is actually the binary `or` operator,
    // not a standalone identifier. Nix parses this as [(x: x) or <lookup-or>].
    // This is deprecated/ambiguous syntax that Nix accepts with warnings.
    // TODO: we currently parse this as 2 list items instead of 1.
    assert!(
        crate::parse("let or = 1; in [ (x: x) or ]").is_ok(),
        "we currently accept this but parse it incorrectly"
    );
}

#[test]
fn regression_language_annotation() {
    // nixfmt a061bd5: a `/* lang */` block comment is only a language
    // annotation when at most one newline separates it from the string.
    test_ast_format("/* python */\n\n\"x\"");
    test_ast_format("/* python */\n\"x\"");
    // LanguageAnnotation must be wrapped in brackets in --ast output.
    test_ast_format("/* python */ '' ''");
}

#[test]
fn regression_chained_prefix_operators() {
    // nixfmt 1f3fa2e / https://github.com/NixOS/nixfmt/issues/351
    test_ast_format("(--1)");
    test_ast_format("(---1)");
    test_ast_format("(!!a)");
    test_ast_format("(!!!!a)");
}

#[test]
fn regression_float_literals() {
    // Float lexer edge cases (also covered by fixtures/nixfmt/correct/numbers.nix
    // for idempotency, pinned here for --ast parity).
    test_ast_format(".5");
    test_ast_format("5.");
    test_ast_format("1.0e2");
    test_ast_format(".5e2");
    test_ast_format("00.5");
}

#[test]
fn regression_attrset_string_interpolated_key() {
    test_ast_format(r#"{"a" = 1;}"#);
    test_ast_format(r#"{${"a"} = 1;}"#);
}

#[test]
fn regression_import_relative_path() {
    // Paths starting with identifiers should be parsed as paths, not applications/division
    // Bug 1: "import common/file.nix" was parsed as application of import to common
    // Bug 2: "x = common/file.nix" was parsed as division "x = common / file.nix"
    // Bug 3: "x = foo-bar/baz.nix" was parsed as division with selection
    // Bug 4: "metaCommon // { ... }" was incorrectly detected as path "metaCommon//"
    // Bug 5: "(a / b)" was incorrectly detected as path starting with "a/"
    test_ast_format(
        r#"{
  a = import common/acme/server/snakeoil-certs.nix;
  b = common/file.nix;
  c = foo-bar/baz.nix;
  d = metaCommon // { mainProgram = "gopeed"; };
  e = (a / b);
  f = ((targetPodcastSize + lameMp3FileAdjust) / (lameMp3Bitrate / 8));
}
"#,
    );
}

#[test]
fn regression_let_string_interpolated_key() {
    test_ast_format(r#"let "foo" = 1; in foo"#);
    test_ast_format(r#"let ${"foo"} = 1; in foo"#);
}

#[test]
fn regression_comparison_chain_should_fail() {
    assert_parse_rejected("a == b == c");
}

#[test]
fn regression_import_path_application() {
    test_ast_format("import ./foo.nix self");
}

#[test]
fn regression_multiline_string_indentation() {
    test_ast_format("''\n  case\n    ;;\n''\n");
}

#[test]
fn regression_trailing_comment() {
    test_ast_format("{ test = foo; # trailing comment\n}");
}

#[test]
fn test_sourceline_multiline_list() {
    // Closing bracket should be on line 3, not line 2.
    test_ast_format("[\n  \"foo\"\n]");
}

#[test]
fn regression_comment_before_and_with_selectors() {
    // Comments before && operators are dropped when expressions contain
    // interpolation selectors like self.packages.${system}.isLinux
    // The third && operator is missing its preTrivia comment
    test_ast_format(
        r#"{
  x =
    lib.optionalAttrs
      (
        self.packages.${system}.isLinux
        # comment 1
        && self.packages.${system}.isPower64
        # comment 2
        && system != "armv6l-linux"
        # comment 3
        && system != "riscv64-linux"
      )
      {
        tests = {};
      };
}"#,
    );
}

#[test]
fn regression_emptyline_pretrivia_inline() {
    test_ast_format("\n\nlet x = 1; in x");
}

#[test]
fn regression_not_member_check() {
    // `?` binds tighter than `!`: Inversion(MemberCheck(a)...).
    test_ast_format("!a ? b");
}

#[test]
fn regression_implies_precedence() {
    // `->` binds looser than `||`: (a || b) -> c.
    test_ast_format("a || b -> c");
}

#[test]
fn regression_mixed_add_sub_associativity() {
    // Right-associative `+` restructuring must not apply across `-`: ((1 + 2) - 3).
    test_ast_format("1 + 2 - 3");
}

#[test]
fn regression_chained_string_concatenation() {
    // From nixpkgs/nixos/modules/config/resolvconf.nix lines 18-37
    test_ast_format(
        r#"''
  line1
''
+ lib.optionalString cond1 ''
  line2
''
+ lib.optionalString cond2 ''
  line3
''
+ lib.optionalString cond3 ''
  line4
''
+ cfg.extra"#,
    );
}

#[test]
fn regression_empty_container_with_comment() {
    // Comments inside otherwise-empty sets / lists / let-bindings should be
    // separate Comments items, not folded into preTrivia of the close token.
    test_ast_format("{\n  # comment\n}");
    test_ast_format("[\n  # comment\n]");
    test_ast_format("let\n  # comment\nin x");
}

#[test]
fn regression_path_trailing_slash_current() {
    assert_parse_rejected("./");
}

#[test]
fn regression_ansi_escape_codes_in_strings() {
    // Literal ESC (0x1b) must be preserved in the AST, not stripped.
    let test_input = "\"\x1b[1;31mtest\x1b[0m\"";
    test_ast_format(test_input);

    // nixfmt outputs \x9 for tab, not \x09 (nixpkgs/lib/generators.nix).
    let tab_test = "\"\t\"";
    test_ast_format(tab_test);
}

#[test]
fn regression_dot_selector_on_newline() {
    // From nixpkgs/nixos/release.nix line 262-267
    test_ast_format(
        r#"{
  armv6l-linux = ./foo.nix;
}
.${system}"#,
    );
}

#[test]
fn regression_context_parameter_variants() {
    // From nixpkgs/lib/generators.nix line 729
    test_ast_format("{ }@args: args");
    test_ast_format("{...}@args: args");
}

#[test]
fn regression_inline_comments_after_strings_and_paths() {
    // The lexer used to hit '#' raw after manually parsing these constructs
    // and fail with "unexpected character: '#'".
    // From nixpkgs/nixos/tests/public-inbox.nix line 97
    test_ast_format(
        r#"[
  "simple" # comment after simple string
  ''
    indented
  '' # comment after indented string
  ./path # comment after path
  "end"
]"#,
    );
}

#[test]
fn regression_old_style_let() {
    test_ast_format("let { body = 1; }");
}

#[test]
fn regression_unicode_escape_in_string() {
    // Zero-width space (U+200B) should be displayed as \x200b in AST output
    // From nixpkgs/pkgs/by-name/li/libcaca/package.nix line 68
    test_ast_format("\"famous \u{200B}AAlib library\"");
    // Soft hyphen (U+00AD) is a Format character (Cf category) and should be escaped as \xad
    // From nixpkgs/pkgs/tools/graphics/diagrams-builder/default.nix line 10
    test_ast_format("\"\u{00AD}~~~\"");
}

#[test]
fn regression_identifier_slash_path() {
    // Application(mkDefault, /tmp), not Path("mkDefault/tmp").
    // From nixpkgs/nixos/modules/services/monitoring/prometheus/exporters.nix line 353
    test_ast_format("mkDefault /tmp");
}

#[test]
fn regression_unquoted_url() {
    // "http://example.com" was tokenized as "http:" followed by TUpdate ("//").
    // From nix/tests/functional/lang/parse-okay-regression-20041027.nix line 6
    test_ast_format("{ url = http://example.com/path; }");
}

#[test]
fn regression_decorated_multiline_comment() {
    // From nix/tests/functional/lang/eval-okay-comments.nix lines 42-45
    test_ast_format(
        r#"/*
 * Multiline, decorated comments
 * # This ain't a nest'd comm'nt
 */
"x""#,
    );
}

#[test]
fn regression_trailing_empty_line_before_close() {
    // From nix/tests/functional/lang/parse-fail-dup-attrs-2.nix
    test_ast_format("let {\n  x = 1;\n  \n}\n");
    test_ast_format("{\n  foo = 1;\n\n}\n");
}

#[test]
fn regression_crlf_line_endings() {
    // From nix/tests/functional/lang/parse-okay-crlf.nix. Bare CR after a comment
    // is rejected by Haskell nixfmt but accepted here for cross-platform input.
    let input = "rec {\n  x =\n  # Comment\r  y;\n}\n";
    let result = crate::parse(input);
    assert!(
        result.is_ok(),
        "Failed to parse input with CRLF/bare CR: {:?}",
        result.err()
    );
}

#[test]
fn regression_or_operator_with_application() {
    // `or` after a non-selection term is the (deprecated) identifier `or`,
    // not the selection-default operator. Upstream Haskell nixfmt silently
    // drops the `or <term>` clause here; we deliberately do not.
    // From nix/tests/functional/lang/eval-okay-attrs5.nix line 20.
    let out = crate::format("(fold or [] [true false false])").unwrap();
    assert!(
        out.contains("or") && out.contains("[ ]"),
        "`or [ ]` must not be dropped, got: {out}"
    );
}

#[test]
fn regression_chained_comparison_operators() {
    // `2 > 1 == 1 < 2` parses as `(2 > 1) == (1 < 2)`; the chain ban is per-precedence.
    // From nix/tests/functional/lang/eval-okay-arithmetic.nix line 50
    test_ast_format("2 > 1 == 1 < 2");
}

#[test]
fn regression_utf8_identifier() {
    // Nix identifiers are ASCII-only: [a-zA-Z_][a-zA-Z0-9_'-]*
    // From nix/tests/functional/lang/parse-fail-utf8.nix
    assert_parse_rejected("123 é 4");
}

#[test]
fn regression_multiline_string_unicode_line_numbers() {
    // Line numbers after multi-byte chars in `''..''` once diverged from nixfmt.
    // From nix/tests/functional/nar-access.nix lines 6-20
    test_ast_format(
        r#"{
  x = ''
    line1
ä"§
  '';
}"#,
    );
}

#[test]
fn regression_duplicate_function_formals() {
    // From nix/tests/functional/lang/parse-fail-dup-formals.nix
    assert_parse_rejected("{x, y, x}: x");
}

#[test]
fn regression_pattern_shadows_formal() {
    // From nix/tests/functional/lang/parse-fail-patterns-1.nix
    assert_parse_rejected("args@{args, x, y, z}: x");
}

#[test]
fn regression_indented_string_to_simple() {
    // nixfmt 1.2.0: single-line indented strings without `"` or `\` become SimpleString,
    // with `''$` -> `\$` and `'''` -> `''` escape conversion.
    test_ast_format("''hello ${x} '''quoted''' ''$var''");
    // Kept as IndentedString when content contains `"` or `\`.
    test_ast_format(r#"''has"quote''"#);
    test_ast_format(r"''back\slash''");
}

// ============================================================================
// Migrated from legacy hand-written unit tests (parser/tests.rs,
// string_path_tests.rs, parameter_tests.rs, operator_associativity_test.rs,
// coverage_test.rs). Kept only inputs not already covered above or in
// ast_format_tests.rs.
// ============================================================================

#[test]
fn regression_empty_simple_string() {
    test_ast_format(r#""""#);
}

#[test]
fn regression_string_multiple_interpolations() {
    test_ast_format(r#""${a} and ${b}""#);
}

#[test]
fn regression_string_dollar_dollar() {
    test_ast_format(r#""$$test""#);
}

#[test]
fn regression_empty_indented_string() {
    test_ast_format("''''");
}

#[test]
fn regression_indented_string_escape_sequences() {
    test_ast_format(r"''test ''$ and ''' and ''\ ''");
}

#[test]
fn regression_path_relative_dotdot() {
    test_ast_format("../foo");
}

#[test]
fn regression_path_with_interpolation() {
    test_ast_format("./foo/${bar}/baz");
}

#[test]
fn regression_subtraction_not_application() {
    // `f -5` must parse as subtraction, not `f(-5)`
    test_ast_format("f -5");
}

#[test]
fn regression_application_with_parenthesized_negation() {
    test_ast_format("f (-5)");
}

#[test]
fn regression_inherit_multiple_names() {
    test_ast_format("{ inherit pkgs lib stdenv; }");
}

#[test]
fn regression_concat_right_associative() {
    test_ast_format("[1] ++ [2] ++ [3]");
}

#[test]
fn regression_update_right_associative() {
    test_ast_format("{a=1;} // {b=2;} // {c=3;}");
}

#[test]
fn regression_plus_right_associative() {
    test_ast_format("1 + 2 + 3");
}

#[test]
fn regression_minus_left_associative() {
    test_ast_format("1 - 2 - 3");
}

#[test]
fn regression_empty_set_parameter() {
    test_ast_format("{}: 42");
}

#[test]
fn regression_pipe_forward_operator() {
    test_ast_format("a |> b");
}

#[test]
fn regression_pipe_backward_operator() {
    test_ast_format("a <| b");
}

#[test]
fn regression_member_check_on_operation() {
    test_ast_format("(x + y) ? foo");
}

#[test]
fn regression_crlf_between_tokens() {
    test_ast_format("x\r\n+\r\ny");
}

#[test]
fn regression_at_without_colon_error() {
    assert_parse_error_contains("x @ y", "@ is only valid in lambda parameters");
}

#[test]
fn regression_single_ampersand_error() {
    assert_parse_error_contains("a & b", "expected '&&', found '&'");
}

#[test]
fn regression_single_pipe_error() {
    assert_parse_error_contains("a | b", "expected one of '||', '|>', found '|'");
}

#[test]
fn regression_ellipsis_without_colon_error() {
    assert_parse_error_contains("{ ... }", "{ ... } must be followed by ':' or '@'");
}

#[test]
fn regression_set_parameter_without_colon_error() {
    assert_parse_rejected("{ x, y }");
}

#[test]
fn regression_single_dollar_error() {
    let err = crate::parse("$x").unwrap_err().to_string();
    assert!(err.contains("unexpected '$'") || err.contains("expected '${'"));
}

#[test]
fn regression_unexpected_character_error() {
    let err = crate::parse("x ^ y").unwrap_err().to_string();
    assert!(err.contains("unexpected character") || err.contains("'^'"));
}

#[test]
fn regression_non_utf8_input() {
    // From nix/tests/functional/lang/eval-fail-toJSON-non-utf-8.nix.
    // Actual non-UTF-8 handling lives in main.rs; here exercise the parser on the
    // replacement char produced by lossy decoding.
    let result = crate::parse("builtins.toJSON \"_invalid UTF-8: �_\"");
    assert!(
        result.is_ok(),
        "Parser should handle Unicode replacement character"
    );
}

/// `inherit` names written as `${…}` are only valid when the body is a plain
/// string literal. Haskell: `Nixfmt.Parser.interpolationRestricted`.
/// Rejection cases are covered by `rejects_invalid_fixture_corpus`.
#[test]
fn regression_inherit_interpolation_restricted() {
    assert!(crate::parse(r#"{ inherit ${"ok"}; }"#).is_ok());
    assert_parse_rejected(r#"{ inherit ${bar}; }"#);
}
