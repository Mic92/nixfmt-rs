//! Error types for parsing and formatting

use crate::types::Span;
use std::fmt;

pub mod context;
pub mod format;

/// A parse error with span and structured error kind
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    /// Primary error location (byte offsets into source)
    pub span: Span,

    /// Error kind with structured data
    pub kind: ErrorKind,

    /// Additional related spans with labels
    pub labels: Vec<Label>,
}

impl ParseError {
    /// Create a simple error with just a message
    pub fn new(span: Span, msg: impl Into<String>) -> Self {
        Self {
            span,
            kind: ErrorKind::Message(msg.into()),
            labels: Vec::new(),
        }
    }

    /// Get the primary message for this error
    pub fn message(&self) -> String {
        match &self.kind {
            ErrorKind::UnexpectedToken { expected, found } => {
                if expected.is_empty() {
                    format!("unexpected token: {}", found)
                } else if expected.len() == 1 {
                    format!("expected {}, found {}", expected[0], found)
                } else {
                    format!("expected one of {}, found {}", expected.join(", "), found)
                }
            }
            ErrorKind::UnclosedDelimiter { delimiter, .. } => {
                format!("unclosed delimiter '{}'", delimiter)
            }
            ErrorKind::MissingToken { token, after } => {
                format!("missing {} after {}", token, after)
            }
            ErrorKind::InvalidSyntax { description, .. } => description.clone(),
            ErrorKind::ChainedComparison {
                first_op,
                second_op,
            } => {
                format!(
                    "chained comparison operators '{}' and '{}' are not allowed",
                    first_op, second_op
                )
            }
            ErrorKind::Message(msg) => msg.clone(),
        }
    }

    /// Get error code if available
    pub fn code(&self) -> Option<&str> {
        match &self.kind {
            ErrorKind::UnexpectedToken { .. } => Some("E001"),
            ErrorKind::UnclosedDelimiter { .. } => Some("E002"),
            ErrorKind::MissingToken { .. } => Some("E003"),
            ErrorKind::InvalidSyntax { .. } => Some("E005"),
            ErrorKind::ChainedComparison { .. } => Some("E006"),
            ErrorKind::Message(_) => None,
        }
    }
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "Parse error at byte {}: {}",
            self.span.start,
            self.message()
        )
    }
}

impl std::error::Error for ParseError {}

/// Structured error kinds
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorKind {
    /// Unexpected token: expected X, found Y
    UnexpectedToken {
        /// Human-readable tokens that would have been accepted, e.g. `["';'", "'}'"]`.
        expected: Vec<String>,
        /// Human-readable token actually found, e.g. `"'in'"`.
        found: String,
    },

    /// Unclosed delimiter (brace, bracket, paren, string)
    UnclosedDelimiter {
        /// The opening delimiter character: `{`, `[`, `(`, `"` or `'`.
        delimiter: char,
        /// Location of the unmatched opening delimiter.
        opening_span: Span,
    },

    /// Missing required token
    MissingToken {
        /// The token that was expected, e.g. `"';'"`.
        token: String,
        /// Description of the preceding construct, e.g. `"attribute definition"`.
        after: String,
    },

    /// Invalid syntax pattern
    InvalidSyntax {
        /// Human-readable description of what is wrong.
        description: String,
        /// Optional suggestion for how to fix it.
        hint: Option<String>,
    },

    /// Chained comparison operators (`1 < 2 < 3`)
    ChainedComparison {
        /// Source text of the first comparison operator.
        first_op: String,
        /// Source text of the second comparison operator.
        second_op: String,
    },

    /// Generic message (for gradual migration)
    Message(String),
}

/// Labeled related location
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Label {
    /// Source range this label points at.
    pub span: Span,
    /// Text shown next to the span in diagnostics.
    pub message: String,
    /// How the label is rendered (primary / secondary / note).
    pub style: LabelStyle,
}

/// Label style for secondary locations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LabelStyle {
    /// Main error location.
    Primary,
    /// Related / context location.
    Secondary,
    /// Informational annotation.
    Note,
}

/// Convenience alias for `Result<T, ParseError>`.
/// Boxed so the error variant stays pointer-sized; parse functions return
/// large `Ok` payloads and a wide error would otherwise bloat every `Result`
/// (and the `memmove`s threading them through the recursive descent).
pub type Result<T> = std::result::Result<T, Box<ParseError>>;
