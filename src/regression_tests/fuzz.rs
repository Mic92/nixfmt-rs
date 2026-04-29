//! Regressions discovered by the `cargo-fuzz` harness in `fuzz/`.
//!
//! Each case is a minimised crash/divergence; the assertions encode the
//! invariant that was violated.

use crate::tests_common::test_format;
use crate::{format, parse_normalized};

fn roundtrip(input: &str) {
    let ast1 = parse_normalized(input).expect("input must parse");
    let f1 = format(input).expect("format must succeed");
    let ast2 = parse_normalized(&f1)
        .unwrap_or_else(|e| panic!("formatted output must re-parse: {e:?}\n{f1}"));
    assert_eq!(
        ast1, ast2,
        "AST changed after format\ninput: {input:?}\nformatted: {f1:?}"
    );
    let f2 = format(&f1).expect("second format must succeed");
    assert_eq!(f1, f2, "format not idempotent\nf1: {f1:?}\nf2: {f2:?}");
}

/// `or` after a non-selection term used to be parsed and silently discarded,
/// turning `t or a` into `t`. Also a bug in upstream Haskell nixfmt.
#[test]
fn fuzz_or_without_selectors_dropped() {
    roundtrip("t or a");
}

/// `1\n.a` is a selection on an integer; emitting it as `1.a` re-lexes as the
/// float `1.` applied to `a`. The printer must keep a space.
#[test]
fn fuzz_integer_selection_relex_as_float() {
    roundtrip("1\n.a");
    assert_eq!(format("1 .a").unwrap(), "1 .a\n");
    // Floats need no protective space; guard the fix against over-application.
    test_format("1.5.x");
    test_format("1..x");
}

/// `''\` followed by a newline must not swallow the newline into the text
/// part: doing so hid the next line from common-indent stripping and made
/// the output grow by two spaces on every format pass.
#[test]
fn fuzz_indented_string_escape_newline_grows() {
    roundtrip("''''\\\nX''");
}

/// A blank (whitespace-only) line inside an indented string must not cap the
/// common indentation; Nix ignores such lines for that calculation.
#[test]
fn fuzz_indented_string_blank_line_indent() {
    roundtrip("[''\n \n  h'']");
}

/// Blank lines at least as long as the common indent keep their excess spaces
/// (Nix evaluation semantics); only shorter blank lines are cleared.
#[test]
fn fuzz_indented_string_blank_line_preserves_excess() {
    test_format("''\n   \n  b''");
    test_format("''\n  \nb''");
}
