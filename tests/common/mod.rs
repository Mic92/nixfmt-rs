//! Common test helpers shared across test files

use nixfmt_rs::colored_writer::ColoredWriter;
use nixfmt_rs::pretty_simple::PrettySimple;
use std::process::Command;

/// Test helper: run nixfmt --ast and compare with our output
pub fn test_ast_format(name: &str, input: &str) {
    // Parse with our parser
    let ast = nixfmt_rs::parse(input).expect(&format!("Failed to parse test '{}'", name));

    // Format with our trait-based formatter
    let mut writer = ColoredWriter::new();
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
        .expect(&format!("Failed to run nixfmt for test '{}'", name));

    // nixfmt --ast writes to stderr!
    let expected = String::from_utf8(nixfmt_output.stderr).expect(&format!(
        "nixfmt output is not valid UTF-8 for test '{}'",
        name
    ));

    // Compare
    if our_output != expected {
        eprintln!("TEST FAILED: {}", name);
        eprintln!("INPUT:\n{}", input);
        eprintln!("\n=== EXPECTED (nixfmt) ===");
        eprintln!("{}", expected);
        eprintln!("\n=== GOT (ours) ===");
        eprintln!("{}", our_output);
        eprintln!("\n=== DIFF ===");

        // Show diff
        for diff in diff::lines(&expected, &our_output) {
            match diff {
                diff::Result::Left(l) => eprintln!("- {}", l),
                diff::Result::Both(l, _) => eprintln!("  {}", l),
                diff::Result::Right(r) => eprintln!("+ {}", r),
            }
        }

        panic!("Output mismatch for test '{}'", name);
    }
}
