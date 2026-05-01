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
}

#[allow(clippy::unnecessary_box_returns)] // returned straight into Result<_, Box<ParseError>>
impl ParseError {
    pub(crate) fn unexpected(
        span: Span,
        expected: impl Into<Vec<String>>,
        found: impl Into<String>,
    ) -> Box<Self> {
        Box::new(Self {
            span,
            kind: ErrorKind::UnexpectedToken {
                expected: expected.into(),
                found: found.into(),
            },
        })
    }

    pub(crate) fn invalid(
        span: Span,
        description: impl Into<String>,
        hint: Option<String>,
    ) -> Box<Self> {
        Box::new(Self {
            span,
            kind: ErrorKind::InvalidSyntax {
                description: description.into(),
                hint,
            },
        })
    }

    #[must_use]
    pub(crate) fn unclosed(span: Span, delimiter: char, opening_span: Span) -> Box<Self> {
        Box::new(Self {
            span,
            kind: ErrorKind::UnclosedDelimiter {
                delimiter,
                opening_span,
            },
        })
    }

    #[must_use]
    pub(crate) fn missing(span: Span, token: &str, after: &str) -> Box<Self> {
        Box::new(Self {
            span,
            kind: ErrorKind::MissingToken {
                token: token.into(),
                after: after.into(),
            },
        })
    }

    /// Get the primary message for this error
    #[must_use]
    pub fn message(&self) -> String {
        match &self.kind {
            ErrorKind::UnexpectedToken { expected, found } => {
                if expected.is_empty() {
                    format!("unexpected token: {found}")
                } else if expected.len() == 1 {
                    format!("expected {}, found {}", expected[0], found)
                } else {
                    format!("expected one of {}, found {}", expected.join(", "), found)
                }
            }
            ErrorKind::UnclosedDelimiter { delimiter, .. } => match delimiter {
                // `'` encodes the `''` opener.
                '\'' => "unclosed indented string (missing closing '')".to_string(),
                '"' => "unclosed string literal (missing closing '\"')".to_string(),
                _ => format!("unclosed delimiter '{delimiter}'"),
            },
            ErrorKind::MissingToken { token, after } => {
                format!("missing {token} after {after}")
            }
            ErrorKind::InvalidSyntax { description, .. } => description.clone(),
            ErrorKind::ChainedComparison {
                first_op,
                second_op,
            } => {
                format!(
                    "chained comparison operators '{first_op}' and '{second_op}' are not allowed"
                )
            }
        }
    }

    /// Get error code if available
    #[must_use]
    pub const fn code(&self) -> Option<&str> {
        match &self.kind {
            ErrorKind::UnexpectedToken { .. } => Some("E001"),
            ErrorKind::UnclosedDelimiter { .. } => Some("E002"),
            ErrorKind::MissingToken { .. } => Some("E003"),
            ErrorKind::InvalidSyntax { .. } => Some("E005"),
            ErrorKind::ChainedComparison { .. } => Some("E006"),
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
}

/// Convenience alias for `Result<T, ParseError>`.
///
/// Boxed so the error variant stays pointer-sized; parse functions return
/// large `Ok` payloads and a wide error would otherwise bloat every `Result`
/// (and the `memmove`s threading them through the recursive descent).
pub type Result<T> = std::result::Result<T, Box<ParseError>>;
