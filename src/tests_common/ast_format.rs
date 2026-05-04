//! AST and IR formatting test helpers
//!
//! Provides utilities for testing AST and IR formatting by comparing our output
//! with the reference `nixfmt` implementation.

use super::diff;
use crate::colored_writer::ColoredWriter;
use crate::dump::Dump;
use std::process::Command;

/// Which reference-nixfmt mode to invoke and where its output lands.
#[derive(Clone, Copy)]
enum RefMode {
    Ast,
    Ir,
    Format,
}

/// Spawn the reference `nixfmt` binary in the requested mode, feed it `input`
/// on stdin, and return the textual output we want to compare against.
///
/// `--ast`/`--ir` write to stderr; the plain formatter writes to stdout and is
/// additionally required to exit successfully.
fn run_reference_nixfmt(input: &str, mode: RefMode) -> String {
    let mut cmd = Command::new("nixfmt");
    match mode {
        RefMode::Ast => {
            cmd.arg("--ast");
        }
        RefMode::Ir => {
            cmd.arg("--ir");
        }
        RefMode::Format => {}
    }
    cmd.arg("-");

    let nixfmt_output = cmd
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            child.stdin.as_mut().unwrap().write_all(input.as_bytes())?;
            child.wait_with_output()
        })
        .expect("Failed to run nixfmt");

    match mode {
        // nixfmt --ast / --ir write to stderr, not stdout.
        RefMode::Ast | RefMode::Ir => {
            String::from_utf8(nixfmt_output.stderr).expect("nixfmt output is not valid UTF-8")
        }
        RefMode::Format => {
            assert!(
                nixfmt_output.status.success(),
                "reference nixfmt rejected input:\n{}\n--- stderr ---\n{}",
                input,
                String::from_utf8_lossy(&nixfmt_output.stderr)
            );
            String::from_utf8(nixfmt_output.stdout).expect("nixfmt output is not valid UTF-8")
        }
    }
}

/// Print the standard "TEST FAILED" block (input, expected, got, colored diff)
/// and panic.
fn fail_with_diff(kind: &str, input: &str, expected: &str, got: &str) -> ! {
    eprintln!("TEST FAILED: {kind}");
    eprintln!("INPUT:\n{input}");
    eprintln!("\n=== EXPECTED (nixfmt) ===");
    eprintln!("{expected}");
    eprintln!("\n=== GOT (ours) ===");
    eprintln!("{got}");
    eprintln!("\n=== DIFF ===");

    diff::print_colored_diff(expected, got);

    panic!("{kind} output mismatch");
}

/// Test helper: run nixfmt --ast and compare with our output
///
/// This function:
/// 1. Parses the input with our parser
/// 2. Formats it using our trait-based formatter
/// 3. Runs the reference `nixfmt --ast` to get expected output
/// 4. Compares the two outputs with a colored diff
///
/// # Panics
///
/// Panics if:
/// - Our parser fails to parse the input
/// - `nixfmt` is not available or fails to run
/// - The outputs don't match (shows a colored diff)
///
/// # Example
///
/// ```rust
/// test_ast_format("let x = 1; in x");
/// ```
pub fn test_ast_format(input: &str) {
    let ast = crate::parse(input).expect("Failed to parse input");

    let mut writer = ColoredWriter::new(input);
    ast.dump(&mut writer);
    let our_output = writer.finish();

    let expected = run_reference_nixfmt(input, RefMode::Ast);

    if our_output != expected {
        fail_with_diff("AST", input, &expected, &our_output);
    }
}

/// Test helper: run nixfmt --ir and compare with our output
///
/// This function:
/// 1. Parses the input with our parser
/// 2. Converts AST to IR using our implementation
/// 3. Formats the IR using our trait-based formatter
/// 4. Runs the reference `nixfmt --ir` to get expected output
/// 5. Compares the two outputs with a colored diff
///
/// # Panics
///
/// Panics if:
/// - Our parser fails to parse the input
/// - `nixfmt` is not available or fails to run
/// - The outputs don't match (shows a colored diff)
///
/// # Example
///
/// ```rust
/// test_ir_format("{ a, b }: x");
/// ```
pub fn test_ir_format(input: &str) {
    let ast = crate::parse(input).expect("Failed to parse input");
    let ir = crate::ast_to_ir(&ast);

    let mut writer = ColoredWriter::new(input);
    ir.dump(&mut writer);
    let our_output = writer.finish();

    let expected = run_reference_nixfmt(input, RefMode::Ir);

    if our_output != expected {
        fail_with_diff("IR", input, &expected, &our_output);
    }
}

/// Test helper: run `nixfmt -` and compare with our formatted output.
///
/// Unlike [`test_ir_format`], this exercises the full pipeline including the
/// layout pass, so it can detect divergences that only show up in the final
/// rendered text even when the IR matches.
pub fn test_format(input: &str) {
    let our_output = crate::format(input).expect("nixfmt_rs failed to format input");

    let expected = run_reference_nixfmt(input, RefMode::Format);

    if our_output != expected {
        fail_with_diff("format", input, &expected, &our_output);
    }
}
