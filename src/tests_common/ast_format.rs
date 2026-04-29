//! AST and IR formatting test helpers
//!
//! Provides utilities for testing AST and IR formatting by comparing our output
//! with the reference `nixfmt` implementation.

use super::diff;
use crate::colored_writer::ColoredWriter;
use crate::pretty_simple::PrettySimple;
use std::process::Command;

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
    // Parse with our parser
    let ast = crate::parse(input).expect("Failed to parse input");

    // Format with our trait-based formatter
    let mut writer = ColoredWriter::new(input);
    ast.format(&mut writer);
    let our_output = writer.finish();

    // Get nixfmt's output (--ast outputs to stderr!)
    let nixfmt_output = Command::new("nixfmt")
        .arg("--ast")
        .arg("-")
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

    // nixfmt --ast writes to stderr!
    let expected =
        String::from_utf8(nixfmt_output.stderr).expect("nixfmt output is not valid UTF-8");

    // Compare
    if our_output != expected {
        eprintln!("TEST FAILED: AST");
        eprintln!("INPUT:\n{}", input);
        eprintln!("\n=== EXPECTED (nixfmt) ===");
        eprintln!("{}", expected);
        eprintln!("\n=== GOT (ours) ===");
        eprintln!("{}", our_output);
        eprintln!("\n=== DIFF ===");

        // Show colored diff
        diff::print_colored_diff(&expected, &our_output);

        panic!("AST output mismatch");
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
    // Parse with our parser
    let ast = crate::parse(input).expect("Failed to parse input");

    // Convert to IR
    let ir = crate::ast_to_ir(&ast);

    // Format with our trait-based formatter
    let mut writer = ColoredWriter::new(input);
    ir.format(&mut writer);
    let our_output = writer.finish();

    // Get nixfmt's output (--ir outputs to stderr!)
    let nixfmt_output = Command::new("nixfmt")
        .arg("--ir")
        .arg("-")
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

    // nixfmt --ir writes to stderr!
    let expected =
        String::from_utf8(nixfmt_output.stderr).expect("nixfmt output is not valid UTF-8");

    // Compare
    if our_output != expected {
        eprintln!("TEST FAILED: IR");
        eprintln!("INPUT:\n{}", input);
        eprintln!("\n=== EXPECTED (nixfmt) ===");
        eprintln!("{}", expected);
        eprintln!("\n=== GOT (ours) ===");
        eprintln!("{}", our_output);
        eprintln!("\n=== DIFF ===");

        // Show colored diff
        diff::print_colored_diff(&expected, &our_output);

        panic!("IR output mismatch");
    }
}

/// Test helper: run `nixfmt -` and compare with our formatted output.
///
/// Unlike [`test_ir_format`], this exercises the full pipeline including the
/// layout pass, so it can detect divergences that only show up in the final
/// rendered text even when the IR matches.
pub fn test_format(input: &str) {
    let our_output = crate::format(input).expect("nixfmt_rs failed to format input");

    let nixfmt_output = Command::new("nixfmt")
        .arg("-")
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

    assert!(
        nixfmt_output.status.success(),
        "reference nixfmt rejected input:\n{}\n--- stderr ---\n{}",
        input,
        String::from_utf8_lossy(&nixfmt_output.stderr)
    );

    let expected =
        String::from_utf8(nixfmt_output.stdout).expect("nixfmt output is not valid UTF-8");

    if our_output != expected {
        eprintln!("TEST FAILED: format");
        eprintln!("INPUT:\n{}", input);
        eprintln!("\n=== EXPECTED (nixfmt) ===");
        eprintln!("{}", expected);
        eprintln!("\n=== GOT (ours) ===");
        eprintln!("{}", our_output);
        eprintln!("\n=== DIFF ===");
        diff::print_colored_diff(&expected, &our_output);
        panic!("format output mismatch");
    }
}
