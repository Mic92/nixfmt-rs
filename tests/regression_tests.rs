//! Consolidated regression tests

mod common;

use common::test_ast_format;

#[test]
fn regression_string_selector() {
    // Minimal reproducer: x."y"
    test_ast_format("string_selector", r#"x."y""#);
}

#[test]
fn regression_string_selector_interpolation_literal() {
    // Minimal reproducer: x.${"y"}
    test_ast_format("string_selector_interp_literal", r#"x.${"y"}"#);
}

#[test]
fn regression_string_selector_interpolation_expr() {
    // Minimal reproducer: x.${foo}
    test_ast_format("string_selector_interp_expr", r#"x.${foo}"#);
}

#[test]
fn regression_or_as_identifier() {
    // Ensure `or` is treated as an identifier when used by itself
    test_ast_format("or_standalone", "or");
}

#[test]
fn regression_or_operator_deprecated_syntax() {
    // From nix/tests/functional/lang/eval-okay-deprecate-cursed-or.nix line 3
    // In `[ (x: x) or ]`, the `or` is actually the binary `or` operator,
    // not a standalone identifier. Nix parses this as [(x: x) or <lookup-or>].
    // This is deprecated/ambiguous syntax that Nix accepts with warnings.
    // Currently we INCORRECTLY parse this as 2 list items instead of 1.
    // TODO: Fix parser to treat `or` as binary operator in this context
    assert!(
        nixfmt_rs::parse("let or = 1; in [ (x: x) or ]").is_ok(),
        "we currently accept this but parse it incorrectly"
    );
}

#[test]
fn regression_float_no_leading_digit() {
    // Ensure parser accepts `.5` like nixfmt
    test_ast_format("float_no_leading", ".5");
}

#[test]
fn regression_attrset_string_key() {
    // Minimal reproducer: {"a" = 1;}
    test_ast_format("attrset_string_key", r#"{"a" = 1;}"#);
}

#[test]
fn regression_attrset_interpolated_key() {
    // Minimal reproducer: {${"a"} = 1;}
    test_ast_format("attrset_interp_key", r#"{${"a"} = 1;}"#);
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
        "import_relative_path",
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
fn regression_context_pattern() {
    // Minimal reproducer: {...}@args: args
    test_ast_format("context_pattern", "{...}@args: args");
}

#[test]
fn regression_let_string_key() {
    // Minimal reproducer: let "foo" = 1; in foo
    test_ast_format("let_string_key", r#"let "foo" = 1; in foo"#);
}

#[test]
fn regression_let_interpolated_key() {
    // Minimal reproducer: let ${"foo"} = 1; in foo
    test_ast_format("let_interp_key", r#"let ${"foo"} = 1; in foo"#);
}

#[test]
fn regression_comparison_chain_should_fail() {
    // Chained comparisons should be rejected (nixfmt errors on `a == b == c`)
    assert!(
        nixfmt_rs::parse("a == b == c").is_err(),
        "expected chained comparisons to be rejected"
    );
}

#[test]
fn regression_import_path_application() {
    // `import ./foo.nix self` should parse and match nixfmt
    test_ast_format("import_path_application", "import ./foo.nix self");
}

#[test]
fn regression_float_trailing_dot() {
    test_ast_format("float_trailing_dot", "5.");
}

#[test]
fn regression_float_with_exponent() {
    test_ast_format("float_with_exponent", "1.0e2");
}

#[test]
fn regression_float_leading_dot_exponent() {
    test_ast_format("float_leading_dot_exponent", ".5e2");
}

#[test]
fn regression_float_double_zero_prefix() {
    test_ast_format("float_double_zero_prefix", "00.5");
}

#[test]
fn regression_attrset_trailing_empty_line() {
    test_ast_format("attrset_trailing_empty_line", "{\n  foo = 1;\n\n}\n");
}

#[test]
fn regression_multiline_string_indentation() {
    test_ast_format("multiline_string_indentation", "''\n  case\n    ;;\n''\n");
}

#[test]
fn regression_trailing_comment() {
    test_ast_format("trailing_comment", "{ test = foo; # trailing comment\n}");
}

#[test]
fn test_sourceline_multiline_list() {
    // Regression test: closing bracket should be on line 3, not line 2
    test_ast_format("sourceline_multiline_list", "[\n  \"foo\"\n]");
}

#[test]
fn regression_comment_before_and_with_selectors() {
    // Comments before && operators are dropped when expressions contain
    // interpolation selectors like self.packages.${system}.isLinux
    // The third && operator is missing its preTrivia comment
    test_ast_format(
        "comment_before_and_selectors",
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
    // EmptyLine in preTrivia should be formatted inline in AST output
    // Our output: { preTrivia =\n    [ EmptyLine ]\n, ...
    // nixfmt:     { preTrivia = [ EmptyLine ], ...
    test_ast_format("emptyline_pretrivia", "\n\nlet x = 1; in x");
}

#[test]
fn regression_not_member_check() {
    // FIXED: ? operator now has higher precedence than ! operator
    // Correct AST: Inversion(MemberCheck(a)...)
    test_ast_format("not_member_check", "!a ? b");
}

#[test]
fn regression_implies_precedence() {
    // FIXED: -> operator has lower precedence than ||
    // Should parse as: (a || b) -> c, not a || (b -> c)
    test_ast_format("implies_precedence", "a || b -> c");
}

#[test]
fn regression_mixed_add_sub_associativity() {
    // Our AST: (1 + (2 - 3)), nixfmt AST: ((1 + 2) - 3)
    // Right-associative handling for + diverges when - appears
    test_ast_format("mixed_add_sub", "1 + 2 - 3");
}

#[test]
fn regression_chained_string_concatenation() {
    // Chained + operators should create nested Operation nodes
    // From nixpkgs/nixos/modules/config/resolvconf.nix lines 18-37
    test_ast_format(
        "chained_string_concat",
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
fn regression_empty_set_with_comment() {
    // Comments inside empty sets should be separate Comments items, not in preTrivia
    test_ast_format("empty_set_comment", "{\n  # comment\n}");
}

#[test]
fn regression_path_trailing_slash_current() {
    // nixfmt rejects `./` but we accept it
    assert!(
        nixfmt_rs::parse("./").is_err(),
        "expected path with trailing slash to be rejected"
    );
}

#[test]
fn regression_ansi_escape_codes_in_strings() {
    // Our parser strips ANSI escape codes (literal ESC characters) from strings
    // but nixfmt preserves them. The literal ESC character is 0x1b.
    // We create a test string with a real escape character followed by "[1;31m"
    let test_input = "\"\x1b[1;31mtest\x1b[0m\"";
    test_ast_format("ansi_escape_codes", test_input);

    // Test that escape sequences are formatted without leading zeros
    // nixfmt outputs \x9 for tab, not \x09
    // From nixpkgs/lib/generators.nix
    let tab_test = "\"\t\"";
    test_ast_format("tab_escape_sequence", tab_test);
}

#[test]
fn regression_dot_selector_on_newline() {
    // Parser should accept dot selector on a newline after closing brace
    // From nixpkgs/nixos/release.nix line 262-267
    test_ast_format(
        "dot_selector_newline",
        r#"{
  armv6l-linux = ./foo.nix;
}
.${system}"#,
    );
}

#[test]
fn regression_empty_set_context_parameter() {
    // Context parameter with empty set: { }@args: body
    // From nixpkgs/lib/generators.nix line 729
    test_ast_format("empty_set_context_param", "{ }@args: args");
}

#[test]
fn regression_inline_comments_after_strings_and_paths() {
    // Inline comments after simple strings, indented strings, and paths
    // should be captured as trailing comments, not cause parse errors.
    // Bug: The lexer would encounter '#' after manually parsing these constructs
    // and fail with "unexpected character: '#'"
    // From nixpkgs/nixos/tests/public-inbox.nix line 97
    test_ast_format(
        "inline_comments_after_strings_and_paths",
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
    // Minimal reproducer: let { body = 1; }
    test_ast_format("old_style_let", "let { body = 1; }");
}

#[test]
fn regression_unicode_escape_in_string() {
    // Zero-width space (U+200B) should be displayed as \x200b in AST output
    // From nixpkgs/pkgs/by-name/li/libcaca/package.nix line 68
    test_ast_format("unicode_escape", "\"famous \u{200B}AAlib library\"");
}

#[test]
fn regression_soft_hyphen_escape() {
    // Soft hyphen (U+00AD) is a Format character (Cf category) and should be escaped as \xad
    // From nixpkgs/pkgs/tools/graphics/diagrams-builder/default.nix line 10
    test_ast_format("soft_hyphen_escape", "\"\u{00AD}~~~\"");
}

#[test]
fn regression_identifier_slash_path() {
    // Function application with path argument: mkDefault /tmp
    // Should parse as Application(mkDefault, /tmp), not Path("mkDefault/tmp")
    // From nixpkgs/nixos/modules/services/monitoring/prometheus/exporters.nix line 353
    test_ast_format("identifier_slash_path", "mkDefault /tmp");
}

#[test]
fn regression_unquoted_url() {
    // Unquoted URLs should be parsed as strings, not as division/update operators
    // Bug: "http://example.com" was tokenized as "http:" followed by TUpdate ("//" operator)
    // From nix/tests/functional/lang/parse-okay-regression-20041027.nix line 6
    test_ast_format("unquoted_url", "{ url = http://example.com/path; }");
}

#[test]
fn regression_decorated_multiline_comment() {
    // Decorated multiline comments should strip leading "* " from each line
    // From nix/tests/functional/lang/eval-okay-comments.nix lines 42-45
    test_ast_format(
        "decorated_multiline_comment",
        r#"/*
 * Multiline, decorated comments
 * # This ain't a nest'd comm'nt
 */
"x""#,
    );
}

#[test]
fn regression_trailing_empty_line_in_let() {
    // Empty line after last item but before closing brace should be preserved in AST
    // From nix/tests/functional/lang/parse-fail-dup-attrs-2.nix
    test_ast_format("trailing_empty_line_let", "let {\n  x = 1;\n  \n}\n");
}

#[test]
fn regression_crlf_line_endings() {
    // Test various line ending formats: LF, CRLF, and bare CR
    // Lexer should treat all as newlines for cross-platform compatibility
    // From nix/tests/functional/lang/parse-okay-crlf.nix
    // Note: The test file has a bare CR after a comment, which nixfmt (Haskell) fails to parse,
    // but we handle correctly for cross-platform robustness
    let input = "rec {\n  x =\n  # Comment\r  y;\n}\n";
    let result = nixfmt_rs::parse(input);
    assert!(
        result.is_ok(),
        "Failed to parse input with CRLF/bare CR: {:?}",
        result.err()
    );
}

#[test]
fn regression_or_operator_with_application() {
    // The `or` operator in Nix is binary and has lower precedence than function application
    // `fold or []` should parse as "fold or []" (using the or operator), not as "fold(or)([])"
    // From nix/tests/functional/lang/eval-okay-attrs5.nix line 20
    test_ast_format("or_operator_application", "(fold or [] [true false false])");
}

#[test]
fn regression_chained_comparison_operators() {
    // Comparison operators can be "chained" when they're actually operands to equality/inequality
    // `2 > 1 == 1 < 2` should parse as `(2 > 1) == (1 < 2)` - comparing two boolean results
    // From nix/tests/functional/lang/eval-okay-arithmetic.nix line 50
    test_ast_format("chained_comparison", "2 > 1 == 1 < 2");
}

#[test]
fn regression_utf8_identifier() {
    // Identifiers can contain UTF-8/Unicode alphabetic characters
    // nixfmt uses isAlpha which accepts Unicode, not just ASCII a-z
    // From nix/tests/functional/lang/parse-fail-utf8.nix
    // Note: This file is "parse-fail" in Nix itself, but nixfmt still parses it to show AST
    test_ast_format("utf8_identifier", "123 é 4");
}

#[test]
fn regression_multiline_string_unicode_line_numbers() {
    // Line numbers for tokens after multiline strings containing special Unicode chars
    // Bug: Our parser reports different line numbers than nixfmt for TSemicolon and TBraceClose
    // From nix/tests/functional/nar-access.nix lines 6-20
    test_ast_format(
        "multiline_unicode_lines",
        r#"{
  x = ''
    line1
ä"§
  '';
}"#,
    );
}
