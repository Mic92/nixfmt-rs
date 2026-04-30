//! Common test helpers shared across test files

mod ast_format;
pub mod diff;

pub use ast_format::{test_ast_format, test_format, test_ir_format};

/// Declare a batch of `#[test]` functions that each call `$helper` on one or
/// more input strings. Used to collapse the repetitive four-line
/// `#[test] fn name() { test_X_format("..."); }` wrappers in the oracle/
/// regression test suites while keeping one test name per case.
#[macro_export]
macro_rules! oracle_tests {
    ($helper:path; $( $(#[$m:meta])* $name:ident => [ $($input:expr),+ $(,)? ] ),* $(,)?) => {
        $( $(#[$m])* #[test] fn $name() { $( $helper($input); )+ } )*
    };
}

/// Assert that `input` is rejected by the parser.
#[track_caller]
pub fn assert_parse_rejected(input: &str) {
    if crate::parse(input).is_ok() {
        panic!("expected parser to reject input, but it was accepted:\n{input}");
    }
}

/// Assert that `input` is rejected *and* the error message contains `needle`.
#[track_caller]
pub fn assert_parse_error_contains(input: &str, needle: &str) {
    match crate::parse(input) {
        Ok(_) => panic!("expected parser to reject input, but it was accepted:\n{input}"),
        Err(err) => {
            let msg = err.to_string();
            assert!(
                msg.contains(needle),
                "parse error for {input:?} did not contain {needle:?}\nactual error: {msg}"
            );
        }
    }
}
