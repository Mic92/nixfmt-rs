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
    test_ast_format(
        "sourceline_multiline_list",
        "[\n  \"foo\"\n]",
    );
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
