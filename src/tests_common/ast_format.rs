//! Snapshot helpers for the AST/IR/format regression suites.
//!
//! Expected output lives under `src/**/snapshots/`. Refresh with
//! `cargo insta review` after an intentional change; re-diff against the
//! upstream Haskell formatter via `examples/diff_sweep`.

use crate::colored_writer::ColoredWriter;
use crate::dump::Dump;

/// Render the `--ast` dump for `input`.
pub fn ast_dump(input: &str) -> String {
    let ast = crate::parse(input).expect("Failed to parse input");
    let mut w = ColoredWriter::new(input);
    ast.dump(&mut w);
    w.finish()
}

/// Render the `--ir` dump for `input`.
pub fn ir_dump(input: &str) -> String {
    let ast = crate::parse(input).expect("Failed to parse input");
    let ir = crate::ast_to_ir(&ast);
    let mut w = ColoredWriter::new(input);
    ir.dump(&mut w);
    w.finish()
}

/// Render the final formatted output for `input`.
pub fn fmt_output(input: &str) -> String {
    crate::format(input).expect("nixfmt_rs failed to format input")
}

/// Snapshot `$produce(input)` with the input recorded as the snapshot
/// description so reviews show what was fed in.
///
/// Expands at the call site so `insta` derives the snapshot name from the
/// enclosing test function (and auto-numbers multiple calls).
#[macro_export]
macro_rules! snapshot_with_input {
    ($produce:path, $input:expr) => {{
        let __input: &str = $input;
        ::insta::with_settings!(
            { description => __input, omit_expression => true },
            { ::insta::assert_snapshot!($produce(__input)); }
        );
    }};
}

/// Snapshot the `--ast` dump of `input`.
#[macro_export]
macro_rules! test_ast_format {
    ($input:expr $(,)?) => {
        $crate::snapshot_with_input!($crate::tests_common::ast_dump, $input)
    };
}

/// Snapshot the `--ir` dump of `input`.
#[macro_export]
macro_rules! test_ir_format {
    ($input:expr $(,)?) => {
        $crate::snapshot_with_input!($crate::tests_common::ir_dump, $input)
    };
}

/// Snapshot the formatted output of `input`.
#[macro_export]
macro_rules! test_format {
    ($input:expr $(,)?) => {
        $crate::snapshot_with_input!($crate::tests_common::fmt_output, $input)
    };
}
