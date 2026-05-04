//! Error types for parsing and formatting

use crate::ast::Span;
use std::fmt;

pub mod context;
pub mod format;

/// A parse error with span and structured error kind
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub(crate) span: Span,
    pub(crate) kind: ErrorKind,
}

impl ParseError {
    pub(crate) fn unexpected(
        span: Span,
        expected: impl Into<Vec<String>>,
        found: impl Into<String>,
    ) -> Self {
        Self {
            span,
            kind: ErrorKind::UnexpectedToken {
                expected: expected.into(),
                found: found.into(),
            },
        }
    }

    pub(crate) fn invalid(
        span: Span,
        description: impl Into<String>,
        hint: Option<String>,
    ) -> Self {
        Self {
            span,
            kind: ErrorKind::InvalidSyntax {
                description: description.into(),
                hint,
            },
        }
    }

    pub(crate) const fn unclosed(span: Span, delimiter: char, opening_span: Span) -> Self {
        Self {
            span,
            kind: ErrorKind::UnclosedDelimiter {
                delimiter,
                opening_span,
            },
        }
    }

    pub(crate) fn missing(span: Span, token: &str, after: &str) -> Self {
        Self {
            span,
            kind: ErrorKind::MissingToken {
                token: token.into(),
                after: after.into(),
            },
        }
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

    /// Byte offsets `start..end` of the primary error location in the source.
    #[must_use]
    pub const fn byte_range(&self) -> std::ops::Range<usize> {
        self.span.range()
    }

    /// Short, actionable fix suggestion if one is known.
    ///
    /// Intended for editor integrations that want a one-line hint (e.g. an
    /// LSP code-action title) without parsing the rendered snippet.
    #[must_use]
    pub fn help(&self) -> Option<String> {
        match &self.kind {
            ErrorKind::InvalidSyntax { hint, .. } => hint.clone(),
            ErrorKind::UnclosedDelimiter { delimiter, .. } => {
                let (_, close) = format::delimiter_pair(*delimiter);
                Some(format!("add closing {close}"))
            }
            ErrorKind::ChainedComparison { .. } => {
                Some("use parentheses: (a < b) && (b < c)".to_string())
            }
            ErrorKind::MissingToken { token, .. } => Some(format!("add {token}")),
            ErrorKind::UnexpectedToken { expected, found } => match expected.as_slice() {
                [single] => {
                    format::unexpected_token_hint(single, found).map(|(_, help)| help.to_string())
                }
                _ => None,
            },
        }
    }

    /// Secondary locations worth pointing at alongside the primary span,
    /// as `(byte_range, label)` pairs (e.g. the unmatched opening delimiter).
    #[must_use]
    pub fn related(&self) -> Vec<(std::ops::Range<usize>, String)> {
        match &self.kind {
            ErrorKind::UnclosedDelimiter { opening_span, .. } => vec![(
                opening_span.range(),
                "unclosed delimiter opened here".to_string(),
            )],
            _ => Vec::new(),
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
            self.span.start(),
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
pub type Result<T> = std::result::Result<T, ParseError>;
