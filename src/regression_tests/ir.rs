//! IR regression tests
//!
//! Tests for IR formatting differences between nixfmt-rs and reference nixfmt

use crate::tests_common::test_ir_format;

/// Regression test: let expression should wrap letPart and inPart in groups
///
/// Issue: nixfmt-rs was not wrapping the let and in parts in groups, resulting in
/// different IR structure compared to the reference implementation.
/// Fixed by using push_group for both letPart and inPart in Expression::Let formatting.
#[test]
#[ignore]
fn test_let_expression_groups() {
    test_ir_format("{ pinnedJson ? ./pinned.json, }: let pinned = (builtins.fromJSON (builtins.readFile pinnedJson)).pins; in pinned");
}
