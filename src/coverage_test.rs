// Tests to improve code coverage by hitting uncovered branches

use crate::parse;
use crate::tests_common::test_ast_format;

// ============================================================================
// Parser coverage tests
// ============================================================================

#[test]
fn test_empty_set_parameter() {
    test_ast_format("{}: 42");
}

#[test]
fn test_at_without_colon_error() {
    let result = parse("x @ y");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err
        .to_string()
        .contains("@ is only valid in lambda parameters"));
}

// ============================================================================
// Lexer coverage tests
// ============================================================================

#[test]
fn test_pipe_forward_operator() {
    test_ast_format("a |> b");
}

#[test]
fn test_single_ampersand_error() {
    let result = parse("a & b");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("expected '&&', found '&'"));
}

#[test]
fn test_single_pipe_error() {
    let result = parse("a | b");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err
        .to_string()
        .contains("expected one of '||', '|>', found '|'"));
}

#[test]
fn test_pipe_backward_operator() {
    // Lines 284-285 in lexer.rs: <| operator
    test_ast_format("a <| b");
}

#[test]
fn test_ellipsis_without_colon_error() {
    // Lines 237-240 in parser.rs: { ... } must be followed by :
    let result = parse("{ ... }");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err
        .to_string()
        .contains("{ ... } must be followed by ':' or '@'"));
}

#[test]
fn test_set_parameter_without_colon_error() {
    // Lines 198-201 in parser.rs: set with parameter-like syntax but no :
    let result = parse("{ x, y }");
    assert!(result.is_err());
    // Just check it errors - the specific error message may vary
}

#[test]
fn test_single_dollar_error() {
    // Lines 351-354 in lexer.rs: $ not followed by {
    let result = parse("$x");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("unexpected '$'") || err.to_string().contains("expected '${'")
    );
}

#[test]
fn test_unexpected_character_error() {
    // Lines 364-367 in lexer.rs: unexpected character
    let result = parse("x ^ y");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("unexpected character") || err.to_string().contains("'^'"));
}

#[test]
fn test_member_check_on_operation() {
    // Lines 322-328 in parser.rs: member check via parse_operation_or_lambda
    test_ast_format("(x + y) ? foo");
}

#[test]
fn test_windows_line_endings() {
    // Lines 552-556 in lexer.rs: \r\n handling
    let result = parse("x\r\n+\r\ny");
    assert!(result.is_ok());
}
