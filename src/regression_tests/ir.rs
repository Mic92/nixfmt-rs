//! IR regression tests
//!
//! Tests for IR formatting differences between nixfmt-rs and reference nixfmt

use crate::tests_common::{test_ast_format, test_ir_format};

#[test]
fn regression_param_with_default_trailing_comma() {
    // Parameters with defaults should use hardline separator
    // even when on one line in input, if there's a trailing comma
    test_ast_format("{ base ? ../., }:\nx");
}

#[test]
fn test_simple_parameter_pattern() {
    // Issue: nixfmt-rs was missing the outer `Group RegularG` wrapper that the
    // reference implementation adds when pretty-printing a Whole Expression (File).
    // This has been fixed by wrapping Whole<T>::pretty in push_group().
    test_ir_format("{ a, b }: x");
}
