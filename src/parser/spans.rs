//! Span utilities for locating the end positions of AST nodes
//!
//! These functions traverse AST nodes to find their rightmost/ending spans,
//! which is useful for error reporting and source location tracking.

use crate::types::{Expression, SimpleSelector, Span, Term};

/// Get the ending span of an expression (the span of its last/rightmost token)
pub(super) fn expr_end(expr: &Expression) -> Span {
    match expr {
        Expression::Term(term) => term_end(term),
        Expression::With(_, _, _, expr)
        | Expression::Let(_, _, _, expr)
        | Expression::Assert(_, _, _, expr)
        | Expression::If(_, _, _, _, _, expr)
        | Expression::Abstraction(_, _, expr)
        | Expression::Application(_, expr)
        | Expression::Operation(_, _, expr)
        | Expression::Negation(_, expr)
        | Expression::Inversion(_, expr) => expr_end(expr),
        Expression::MemberCheck(_, _, selectors) => selectors
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
        Term::List(_, _, close) | Term::Set(_, _, _, close) | Term::Parenthesized(_, _, close) => {
            close.span
        }
        Term::Selection(base, selectors, default) => {
            // Return the rightmost element: default > last selector > base
            if let Some((_, default_expr)) = default {
                term_end(default_expr)
            } else if let Some(last) = selectors.last() {
                simple_selector_end(&last.selector)
            } else {
                term_end(base)
            }
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
