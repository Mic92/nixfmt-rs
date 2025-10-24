//! Error types for parsing and formatting

use crate::types::Pos;
use std::fmt;

/// Parse error with position and context
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub pos: Pos,
    pub message: String,
    pub context: Vec<String>,
}

impl ParseError {
    pub fn new(pos: Pos, msg: impl Into<String>) -> Self {
        Self {
            pos,
            message: msg.into(),
            context: Vec::new(),
        }
    }

    pub fn with_context(mut self, ctx: impl Into<String>) -> Self {
        self.context.push(ctx.into());
        self
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Parse error at line {}: {}", self.pos.0, self.message)?;
        for ctx in self.context.iter().rev() {
            write!(f, "\n  while parsing {}", ctx)?;
        }
        Ok(())
    }
}

impl std::error::Error for ParseError {}

pub type Result<T> = std::result::Result<T, ParseError>;
