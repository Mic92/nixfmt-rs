//! Span utilities for locating the end positions of AST nodes
//!
//! These methods traverse AST nodes to find their rightmost/ending spans,
//! which is useful for error reporting and source location tracking.

use crate::ast::{Expression, SimpleSelector, Span, Term};

impl Expression {
    /// Get the ending span of an expression (the span of its last/rightmost token)
    pub(super) fn end_span(&self) -> Span {
        match self {
            Self::Term(term) => term.end_span(),
            Self::With { body: expr, .. }
            | Self::Let { body: expr, .. }
            | Self::Assert { body: expr, .. }
            | Self::If {
                else_branch: expr, ..
            }
            | Self::Abstraction { body: expr, .. }
            | Self::Application { arg: expr, .. }
            | Self::Operation { rhs: expr, .. }
            | Self::Negation { expr, .. }
            | Self::Inversion { expr, .. } => expr.end_span(),
            Self::MemberCheck {
                path: selectors, ..
            } => selectors
                .last()
                // No selectors - shouldn't happen for a parsed MemberCheck.
                .map_or(Span::point(0), |last| last.selector.end_span()),
        }
    }
}

impl Term {
    /// Get the ending span of a term
    fn end_span(&self) -> Span {
        match self {
            Self::Token(leaf) => leaf.span,
            Self::SimpleString(s) | Self::IndentedString(s) => s.span,
            Self::Path(p) => p.span,
            Self::List { close, .. }
            | Self::Set { close, .. }
            | Self::Parenthesized { close, .. } => close.span,
            Self::Selection {
                selectors, default, ..
            } => {
                // Rightmost element: default > last selector. The parser only
                // builds `Selection` when `selectors` is non-empty.
                default.as_ref().map_or_else(
                    || selectors.last().expect("≥1 selector").selector.end_span(),
                    |d| d.value.end_span(),
                )
            }
        }
    }
}

impl SimpleSelector {
    /// Get the ending span of a simple selector
    const fn end_span(&self) -> Span {
        match self {
            Self::ID(leaf) => leaf.span,
            Self::Interpol(ann) => ann.span,
            Self::String(s) => s.span,
        }
    }
}
