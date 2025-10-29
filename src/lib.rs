//! nixfmt-rs2: Rust implementation of nixfmt with exact Haskell compatibility

pub mod colored_writer;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod predoc;
pub mod pretty;
pub mod pretty_simple;
pub mod types;

use predoc::{render_with_config, Doc, Pretty, RenderConfig};
pub use error::{ParseError, Result};
pub use pretty_simple::PrettySimple;
pub use types::*;

/// Parse a Nix expression from source code
pub fn parse(source: &str) -> Result<File> {
    let mut parser = parser::Parser::new(source)?;
    parser.parse_file()
}

/// Format a Nix file
pub fn format(source: &str) -> Result<String> {
    let ast = parse(source)?;
    let mut doc = Doc::new();
    ast.pretty(&mut doc);
    let config = RenderConfig::default();
    let output = render_with_config(&doc, &config);
    Ok(output)
}
