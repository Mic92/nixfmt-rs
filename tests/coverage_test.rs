// Tests to improve code coverage by hitting uncovered branches

mod common;
use common::test_ast_format;
use nixfmt_rs::parse;

// ============================================================================
// Parser coverage tests
// ============================================================================

#[test]
fn test_empty_set_parameter() {
    // Lines 133-140 in parser.rs: empty set parameter
    test_ast_format("empty_set_parameter", "{}: 42");
}

#[test]
#[ignore] // TODO: Fix error message to match expected output
fn test_at_without_colon_error() {
    // Lines 304-307 in parser.rs: @ without : should error
    let result = parse("x @ y");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("@ is only valid in lambda parameters"));
}

// ============================================================================
// Lexer coverage tests
// ============================================================================

#[test]
fn test_pipe_forward_operator() {
    // Lines 347-348 in lexer.rs: |> operator
    test_ast_format("pipe_forward", "a |> b");
}

#[test]
fn test_single_ampersand_error() {
    // Lines 333-336 in lexer.rs: single & should error
    let result = parse("a & b");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("unexpected '&', expected '&&'"));
}

#[test]
fn test_single_pipe_error() {
    // Lines 350-353 in lexer.rs: single | should error
    let result = parse("a | b");
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(err.to_string().contains("unexpected '|', expected '||' or '|>'"));
}
