//! Common test helpers shared across test files

mod ast_format;
pub mod diff;

pub use ast_format::{ast_dump, fmt_output, ir_dump};

/// Declare a batch of `#[test]` functions that each call `$helper!`.
///
/// `$helper` is one of the snapshot macros from `ast_format` (`test_format`,
/// `test_ast_format`, `test_ir_format`); the bang is added here so callers
/// keep the `name => ["input", ...]` table shape.
#[macro_export]
macro_rules! oracle_tests {
    ($helper:ident; $( $(#[$m:meta])* $name:ident => [ $($input:expr),+ $(,)? ] ),* $(,)?) => {
        $( $(#[$m])* #[test] fn $name() { $( $crate::$helper!($input); )+ } )*
    };
}

/// Assert that `input` is rejected by the parser.
#[track_caller]
pub fn assert_parse_rejected(input: &str) {
    assert!(
        crate::parse(input).is_err(),
        "expected parser to reject input, but it was accepted:\n{input}"
    );
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
