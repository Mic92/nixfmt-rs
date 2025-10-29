//! AST formatting test helper
//!
//! Provides utilities for testing AST formatting by comparing our output
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
    let expected = String::from_utf8(nixfmt_output.stderr).expect("nixfmt output is not valid UTF-8");

    // Compare
    if our_output != expected {
        eprintln!("TEST FAILED");
        eprintln!("INPUT:\n{}", input);
        eprintln!("\n=== EXPECTED (nixfmt) ===");
        eprintln!("{}", expected);
        eprintln!("\n=== GOT (ours) ===");
        eprintln!("{}", our_output);
        eprintln!("\n=== DIFF ===");

        // Show colored diff
        diff::print_colored_diff(&expected, &our_output);

        panic!("Output mismatch");
    }
}
