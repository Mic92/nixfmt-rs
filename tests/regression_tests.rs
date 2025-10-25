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
#[ignore = "BUG #8: Trailing-dot float (`5.`) rejected"]
fn regression_float_trailing_dot() {
    test_ast_format("float_trailing_dot", "5.");
}

#[test]
#[ignore = "BUG #13: Attrset trailing trivia diverges from nixfmt"]
fn regression_attrset_trailing_empty_line() {
    test_ast_format(
        "attrset_trailing_empty_line",
        "{\n  foo = 1;\n\n}\n",
    );
}
