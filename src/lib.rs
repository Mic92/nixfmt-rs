//! nixfmt-rs2: Rust implementation of nixfmt with exact Haskell compatibility

#![warn(clippy::pedantic, clippy::nursery, missing_docs)]
// --- pedantic/nursery allows -----------------------------------------------
// This crate intentionally mirrors the structure of nixfmt's Haskell source
// (Types.hs / Predoc.hs / Pretty.hs) line-for-line where possible. Many of
// clippy's stylistic suggestions would diverge from that reference and make
// cross-checking harder, so they are allowed crate-wide below.
#![allow(
    // Style churn that would obscure the 1:1 Haskell mapping.
    clippy::use_self,
    clippy::wildcard_imports,
    clippy::enum_glob_use,
    clippy::match_same_arms,
    clippy::single_match_else,
    clippy::if_not_else,
    clippy::redundant_else,
    clippy::manual_let_else,
    clippy::option_if_let_else,
    clippy::map_unwrap_or,
    clippy::unnested_or_patterns,
    clippy::match_wildcard_for_single_variants,
    clippy::comparison_chain,
    clippy::items_after_statements,
    clippy::branches_sharing_code,
    clippy::redundant_closure_for_method_calls,
    // Naming follows the Haskell identifiers verbatim.
    clippy::similar_names,
    clippy::many_single_char_names,
    clippy::doc_markdown,
    clippy::too_long_first_doc_paragraph,
    // Function shape follows Haskell; size/signature lints are noise here.
    clippy::too_many_lines,
    clippy::needless_pass_by_value,
    clippy::unnecessary_wraps,
    clippy::unused_self,
    clippy::ref_option,
    clippy::must_use_candidate,
    clippy::missing_errors_doc,
    // Visibility is intentionally explicit on private-module items.
    clippy::redundant_pub_crate,
    // Lexer/layout hot paths: leave casts, inlining and allocation patterns
    // to the perf work; nursery suggestions here are not always correct.
    clippy::missing_const_for_fn,
    clippy::inline_always,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::format_push_string,
    clippy::uninlined_format_args,
    clippy::needless_collect,
    clippy::redundant_clone,
    // Test fixtures use r#""# uniformly for Nix snippets and explicit
    // `if .. panic!` for clearer failure messages.
    clippy::needless_raw_string_hashes,
    clippy::manual_assert,
)]

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
pub(crate) use types::*;

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
