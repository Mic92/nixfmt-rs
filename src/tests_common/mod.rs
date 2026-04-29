//! Common test helpers shared across test files

mod ast_format;
pub mod diff;

pub use ast_format::{test_ast_format, test_format, test_ir_format};
