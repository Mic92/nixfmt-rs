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
mod colored_writer;
mod error;
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
    let ast = parse(source)?;
    let mut doc = predoc::Doc::new();
    ast.pretty(&mut doc);
    let config = RenderConfig {
        width: opts.width,
        indent_width: opts.indent,
    };
    let output = render_with_config(doc, &config);
    Ok(output)
}

pub(crate) fn ast_to_ir(ast: &File) -> predoc::IR {
    let mut doc = predoc::Doc::new();
    ast.pretty(&mut doc);
    predoc::IR(predoc::fixup(doc))
}

/// Format AST as colored debug output (for --ast mode).
///
/// # Errors
/// See [`parse`].
#[doc(hidden)]
pub fn format_ast(source: &str) -> Result<String> {
    use pretty_simple::PrettySimple;
    let ast = parse(source)?;
    let mut writer = colored_writer::ColoredWriter::new(source);
    ast.format(&mut writer);
    Ok(writer.finish())
}

/// Format IR as colored debug output (for --ir mode).
///
/// # Errors
/// See [`parse`].
#[doc(hidden)]
pub fn format_ir(source: &str) -> Result<String> {
    use pretty_simple::PrettySimple;
    let ast = parse(source)?;
    let ir = ast_to_ir(&ast);
    let mut writer = colored_writer::ColoredWriter::new(source);
    ir.format(&mut writer);
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
