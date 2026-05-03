//! Span utilities for locating the end positions of AST nodes
//!
//! These functions traverse AST nodes to find their rightmost/ending spans,
//! which is useful for error reporting and source location tracking.

use crate::types::{Expression, SimpleSelector, Span, Term};

/// Get the ending span of an expression (the span of its last/rightmost token)
pub(super) fn expr_end(expr: &Expression) -> Span {
    match expr {
        Expression::Term(term) => term_end(term),
        Expression::With { body: expr, .. }
        | Expression::Let { body: expr, .. }
        | Expression::Assert { body: expr, .. }
        | Expression::If {
            else_branch: expr, ..
        }
        | Expression::Abstraction { body: expr, .. }
        | Expression::Application { arg: expr, .. }
        | Expression::Operation { rhs: expr, .. }
        | Expression::Negation { expr, .. }
        | Expression::Inversion { expr, .. } => expr_end(expr),
        Expression::MemberCheck {
            path: selectors, ..
        } => selectors
            .last()
            // No selectors - shouldn't happen for a parsed MemberCheck.
            .map_or(Span::point(0), |last| simple_selector_end(&last.selector)),
    }
}

/// Get the ending span of a term
fn term_end(term: &Term) -> Span {
    match term {
        Term::Token(leaf) => leaf.span,
        Term::SimpleString(s) | Term::IndentedString(s) => s.span,
        Term::Path(p) => p.span,
        Term::List { close, .. } | Term::Set { close, .. } | Term::Parenthesized { close, .. } => {
            close.span
        }
        Term::Selection {
            selectors, default, ..
        } => {
            // Rightmost element: default > last selector. The parser only
            // builds `Selection` when `selectors` is non-empty.
            default.as_ref().map_or_else(
                || simple_selector_end(&selectors.last().expect("≥1 selector").selector),
                |d| term_end(&d.value),
            )
        }
    }
}

/// Get the ending span of a simple selector
const fn simple_selector_end(sel: &SimpleSelector) -> Span {
    match sel {
        SimpleSelector::ID(leaf) => leaf.span,
        SimpleSelector::Interpol(ann) => ann.span,
        SimpleSelector::String(s) => s.span,
    }
}
