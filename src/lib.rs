//! nixfmt-rs: Rust implementation of nixfmt with exact Haskell compatibility.
//!
//! # Example
//!
//! ```
//! let src = "{foo=1;}";
//! assert_eq!(nixfmt_rs::format(src).unwrap(), "{ foo = 1; }\n");
//!
//! let mut opts = nixfmt_rs::Options::default();
//! opts.width = 40;
//! let _ = nixfmt_rs::format_with(src, &opts).unwrap();
//! ```
//!
//! On parse failure the returned [`ParseError`] can be rendered for users via
//! [`format_error`].

// Clippy pedantic/nursery are enabled workspace-wide via Cargo.toml [lints];
// see the allow-list there for rationale.
#![warn(missing_docs)]
#![forbid(unsafe_code)]

// Internal modules - hidden from public API
mod doc;
mod error;

// Debug-dump machinery (Haskell `Show` / pretty-simple parity). Only needed
// by the `nixfmt` binary's --ast/--ir flags and the regression test suite.
#[cfg(any(test, feature = "debug-dump"))]
mod colored_writer;
#[cfg(any(test, feature = "debug-dump"))]
mod dump;

// Internal modules - not exposed as public API
mod ast;
mod format;
mod lexer;
mod normalize;
mod parser;

pub use error::ParseError;

/// Version of the `nixfmt_rs` crate (and thus the formatting rules).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

use doc::{Emit, RenderConfig};

// Internal-only Result type and AST types
pub(crate) use ast::File;
pub(crate) use error::Result;

/// Layout options for [`format_with`].
///
/// Construct via [`Options::default`] and override fields; the struct is
/// `#[non_exhaustive]` so new options can be added without a breaking change.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Options {
    /// Maximum line width the formatter targets (soft limit).
    pub width: usize,
    /// Spaces per indentation level.
    pub indent: usize,
}

impl Default for Options {
    /// Matches the upstream Haskell `nixfmt` defaults (`--width 100 --indent 2`).
    fn default() -> Self {
        Self {
            width: 100,
            indent: 2,
        }
    }
}

/// Parse a Nix expression from source code.
///
/// # Errors
/// Returns a [`ParseError`] if `source` is not valid Nix.
#[doc(hidden)] // AST type is internal; exposed for in-tree bin/benches/fuzz only
pub fn parse(source: &str) -> Result<File> {
    let mut parser = parser::Parser::new(source)?;
    parser.parse_file()
}

/// Parse and return an AST with all trivia/spans stripped, suitable for
/// structural equality comparison. Intended for the fuzzing harness.
///
/// # Errors
/// See [`parse`].
#[doc(hidden)]
pub fn parse_normalized(source: &str) -> Result<File> {
    let mut ast = parse(source)?;
    normalize::normalize_file(&mut ast);
    Ok(ast)
}

/// Format a Nix file.
///
/// # Errors
/// See [`parse`]; formatting itself never fails.
pub fn format(source: &str) -> Result<String> {
    format_with(source, &Options::default())
}

/// Format a Nix file with explicit layout [`Options`].
///
/// # Errors
/// See [`parse`].
pub fn format_with(source: &str, opts: &Options) -> Result<String> {
    let mut ast = parse(source)?;
    strip_leading_empty_lines(&mut ast);
    let mut doc = doc::Doc::new();
    ast.emit(&mut doc);
    let config = RenderConfig {
        width: opts.width,
        indent_width: opts.indent,
    };
    let output = doc.render(&config);
    Ok(output)
}

/// Drop leading `EmptyLine` trivia on the file's very first token.
///
/// The renderer discards leading vertical whitespace anyway, so this never
/// changes the output bytes. It does, however, change layout *decisions*:
/// `Term::is_simple` treats any pre-trivia as "not simple", so a file starting
/// with blank lines took the non-simple `prettyApp` branch on pass 1, then the
/// simple branch on pass 2 once the blanks were gone, breaking idempotency.
/// Upstream Haskell nixfmt 1.2.0 has the same bug; this is an intentional
/// divergence so `format` is a fixed point.
fn strip_leading_empty_lines(ast: &mut File) {
    use ast::{FirstToken, TriviaPiece};
    let slot = ast.value.first_token_mut();
    let n = slot
        .pre_trivia
        .iter()
        .take_while(|p| matches!(p, TriviaPiece::EmptyLine))
        .count();
    if n > 0 {
        let mut v: Vec<TriviaPiece> = std::mem::take(slot.pre_trivia).into();
        v.drain(..n);
        *slot.pre_trivia = v.into();
    }
}

#[cfg(any(test, feature = "debug-dump"))]
pub(crate) fn ast_to_ir(ast: &File) -> doc::IR {
    let mut ast = ast.clone();
    strip_leading_empty_lines(&mut ast);
    let mut doc = doc::Doc::new();
    ast.emit(&mut doc);
    doc::IR(doc.fixup())
}

/// Format AST as colored debug output (for --ast mode).
///
/// # Errors
/// See [`parse`].
#[cfg(any(test, feature = "debug-dump"))]
#[doc(hidden)]
pub fn format_ast(source: &str) -> Result<String> {
    use dump::Dump;
    let ast = parse(source)?;
    let mut writer = colored_writer::ColoredWriter::new(source);
    ast.dump(&mut writer);
    Ok(writer.finish())
}

/// Format IR as colored debug output (for --ir mode).
///
/// # Errors
/// See [`parse`].
#[cfg(any(test, feature = "debug-dump"))]
#[doc(hidden)]
pub fn format_ir(source: &str) -> Result<String> {
    use dump::Dump;
    let ast = parse(source)?;
    let ir = ast_to_ir(&ast);
    let mut writer = colored_writer::ColoredWriter::new(source);
    ir.dump(&mut writer);
    Ok(writer.finish())
}

/// Render a [`ParseError`] as a multi-line diagnostic with source snippet and
/// caret, in the style of rustc.
///
/// # Example
///
/// ```
/// let src = "{ x = ";
/// let err = nixfmt_rs::format(src).unwrap_err();
/// let msg = nixfmt_rs::format_error(src, Some("default.nix"), &err);
/// assert!(msg.contains("default.nix:1:"));
/// assert!(msg.contains("{ x = "));
/// ```
#[must_use]
pub fn format_error(source: &str, filename: Option<&str>, error: &ParseError) -> String {
    let context = error::ErrorContext::new(source, filename);
    format!("{}", error::render(&context, error))
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
