//! nixfmt-rs: Rust implementation of nixfmt with exact Haskell compatibility

// Clippy pedantic/nursery are enabled workspace-wide via Cargo.toml [lints];
// see the allow-list there for rationale.
#![warn(missing_docs)]

// Internal modules - hidden from public API
mod colored_writer;
pub mod error; // Keep public for ParseError export
mod predoc;
mod pretty_simple;

// Internal modules - not exposed as public API
mod lexer;
mod normalize;
mod parser;
mod pretty;
mod types;

pub use error::ParseError;
use predoc::{Pretty, RenderConfig, render_with_config};

// Internal-only Result type and AST types
pub(crate) use error::Result;
pub(crate) use types::File;

/// Parse a Nix expression from source code
pub fn parse(source: &str) -> Result<File> {
    let mut parser = parser::Parser::new(source)?;
    parser.parse_file()
}

/// Parse and return an AST with all trivia/spans stripped, suitable for
/// structural equality comparison. Intended for the fuzzing harness.
pub fn parse_normalized(source: &str) -> Result<File> {
    let mut ast = parse(source)?;
    normalize::normalize_file(&mut ast);
    Ok(ast)
}

/// Format a Nix file
pub fn format(source: &str) -> Result<String> {
    format_with(source, 100, 2)
}

/// Format a Nix file with explicit layout parameters.
///
/// Exposed so the CLI can honour `--width` / `--indent` without re-exporting
/// the internal `RenderConfig` type.
pub fn format_with(source: &str, width: usize, indent: usize) -> Result<String> {
    let ast = parse(source)?;
    let mut doc = predoc::Doc::new();
    ast.pretty(&mut doc);
    let config = RenderConfig {
        width,
        indent_width: indent,
    };
    let output = render_with_config(doc, &config);
    Ok(output)
}

/// Convert AST to IR (intermediate representation) for debugging
/// Returns an opaque IR that can be formatted for display
#[must_use]
pub fn ast_to_ir(ast: &File) -> predoc::IR {
    let mut doc = predoc::Doc::new();
    ast.pretty(&mut doc);
    let doc = predoc::fixup(doc);
    predoc::IR(doc)
}

/// Format AST as colored debug output (for --ast mode)
pub fn format_ast(source: &str) -> Result<String> {
    let ast = parse(source)?;
    let mut writer = colored_writer::ColoredWriter::new(source);
    use pretty_simple::PrettySimple;
    ast.format(&mut writer);
    Ok(writer.finish())
}

/// Format IR as colored debug output (for --ir mode)
pub fn format_ir(source: &str) -> Result<String> {
    let ast = parse(source)?;
    let ir = ast_to_ir(&ast);
    let mut writer = colored_writer::ColoredWriter::new(source);
    use pretty_simple::PrettySimple;
    ir.format(&mut writer);
    Ok(writer.finish())
}

/// Format a parse error as a user-friendly colored error message
#[must_use]
pub fn format_error(source: &str, filename: Option<&str>, error: &ParseError) -> String {
    let context = error::context::ErrorContext::new(source, filename);
    let formatter = error::format::ErrorFormatter::new(&context);
    formatter.format(error)
}

// Include test modules
#[cfg(test)]
mod ast_format_tests;

#[cfg(test)]
mod ir_format_tests;

#[cfg(test)]
mod regression_tests;

#[cfg(test)]
mod tests_common;
