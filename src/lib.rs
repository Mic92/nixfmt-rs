//! nixfmt-rs2: Rust implementation of nixfmt with exact Haskell compatibility

pub mod colored_writer;
pub mod error;
pub mod lexer;
pub mod parser;
pub mod pretty_simple;
pub mod types;
// pub mod pretty; // TODO
// pub mod predoc; // TODO

pub use error::{ParseError, Result};
pub use pretty_simple::PrettySimple;
pub use types::*;

/// Parse a Nix expression from source code
pub fn parse(source: &str) -> Result<File> {
    let mut parser = parser::Parser::new(source)?;
    parser.parse_file()
}

/// Format a Nix file
pub fn format(_source: &str) -> Result<String> {
    // TODO: implement full pipeline: parse -> pretty -> render
    todo!("formatter not yet implemented")
}
