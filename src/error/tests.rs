//! Unit tests for the structured `ParseError` accessors.
//!
//! `format.rs` already snapshots the rendered output; these target the
//! programmatic API (`message`/`help`/`related`) that editor integrations
//! consume directly and that cargo-mutants flagged as untested.

use super::ParseError;
use crate::ast::Span;

fn span(r: std::ops::Range<usize>) -> Span {
    Span::new(r.start, r.end)
}

#[test]
fn unexpected_single_expected_message_and_help() {
    let err = ParseError::unexpected(span(5..6), vec!["';'".into()], "'in'");
    assert_eq!(err.message(), "expected ';', found 'in'");
    assert_eq!(
        err.help().as_deref(),
        Some("add a semicolon at the end of the previous line")
    );
    assert!(err.related().is_empty());
    assert_eq!(err.code(), Some("E001"));
}

#[test]
fn unexpected_multiple_expected_has_no_help() {
    let err = ParseError::unexpected(
        span(0..1),
        vec!["';'".into(), "'='".into()],
        "'in'".to_string(),
    );
    assert_eq!(err.message(), "expected one of ';', '=', found 'in'");
    assert_eq!(err.help(), None);
}

#[test]
fn unclosed_string_message_help_and_related() {
    let err = ParseError::unclosed(span(10..11), '"', span(2..3));
    assert_eq!(
        err.message(),
        "unclosed string literal (missing closing '\"')"
    );
    assert_eq!(err.help().as_deref(), Some("add closing '\"'"));
    assert_eq!(
        err.related(),
        vec![(2..3, "unclosed delimiter opened here".to_string())]
    );
    assert_eq!(err.code(), Some("E002"));
}

#[test]
fn unclosed_indented_string_message() {
    let err = ParseError::unclosed(span(10..11), '\'', span(0..2));
    assert_eq!(
        err.message(),
        "unclosed indented string (missing closing '')"
    );
    assert_eq!(err.help().as_deref(), Some("add closing ''"));
}

#[test]
fn unclosed_brace_generic_message() {
    let err = ParseError::unclosed(span(10..11), '{', span(0..1));
    assert_eq!(err.message(), "unclosed delimiter '{'");
    assert_eq!(err.help().as_deref(), Some("add closing '}'"));
}

#[test]
fn missing_token_help() {
    let err = ParseError::missing(span(4..4), "';'", "binding");
    assert_eq!(err.message(), "missing ';' after binding");
    assert_eq!(err.help().as_deref(), Some("add ';'"));
}

#[test]
fn invalid_syntax_passes_hint_through() {
    let err = ParseError::invalid(span(0..1), "bad float", Some("remove trailing dot".into()));
    assert_eq!(err.message(), "bad float");
    assert_eq!(err.help().as_deref(), Some("remove trailing dot"));
    assert!(err.related().is_empty());
}
