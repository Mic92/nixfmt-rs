//! Binder parsing utilities
//!
//! This module handles parsing of bindings in Nix attribute sets and let expressions,
//! including both `inherit` statements and attribute assignments (name = value).

use crate::error::{ErrorKind, ParseError, Result};
use crate::types::*;

use super::{Parser, spans};

impl Parser {
    /// Parse a list of binders (for let expressions and attribute sets)
    pub(super) fn parse_binders(&mut self) -> Result<Items<Binder>> {
        let mut items = Vec::new();

        while !matches!(
            self.current.value,
            Token::KIn | Token::TBraceClose | Token::Sof
        ) {
            self.collect_trivia_as_comments(&mut items);

            let binder = self.parse_binder()?;
            items.push(Item::Item(binder));
        }

        if matches!(self.current.value, Token::KIn | Token::TBraceClose) {
            self.collect_trivia_as_comments(&mut items);
        }

        Ok(Items(items))
    }

    /// Parse a single binder (inherit or assignment)
    fn parse_binder(&mut self) -> Result<Binder> {
        if matches!(self.current.value, Token::KInherit) {
            self.parse_inherit()
        } else {
            self.parse_assignment()
        }
    }

    /// Parse inherit statement: inherit [ (expr) ] names... ;
    fn parse_inherit(&mut self) -> Result<Binder> {
        let inherit_tok = self.expect_token_match(|t| matches!(t, Token::KInherit))?;

        let from = if matches!(self.current.value, Token::TParenOpen) {
            let open = self.take_current();
            self.advance()?;
            let expr = self.parse_expression()?;
            let close = self.expect_token_match(|t| matches!(t, Token::TParenClose))?;
            Some(Term::Parenthesized(open, Box::new(expr), close))
        } else {
            None
        };

        let mut selectors = Vec::new();
        while self.is_simple_selector_start() {
            let sel = self.parse_simple_selector()?;
            selectors.push(sel);
        }

        let semi = self.expect_token_match(|t| matches!(t, Token::TSemicolon))?;

        Ok(Binder::Inherit(inherit_tok, from, selectors, semi))
    }

    /// Parse assignment: selector = expr ;
    fn parse_assignment(&mut self) -> Result<Binder> {
        let mut selectors = Vec::new();

        let first_sel = self.parse_selector()?;
        selectors.push(first_sel);

        while matches!(self.current.value, Token::TDot) {
            let dot = self.take_current();
            self.advance()?;

            let simple_sel = self.parse_simple_selector()?;
            selectors.push(Selector {
                dot: Some(dot),
                selector: simple_sel,
            });
        }

        // Check for common mistake: attribute path followed by semicolon (forgot = and value)
        if matches!(self.current.value, Token::TSemicolon) {
            return Err(ParseError {
                span: self.current.span,
                kind: ErrorKind::UnexpectedToken {
                    expected: vec!["'='".to_string()],
                    found: "';'".to_string(),
                },
                labels: vec![],
            });
        }

        let eq = self.expect_token_match(|t| matches!(t, Token::TAssign))?;
        let expr = self.parse_expression()?;

        // Get the end of the expression for error reporting
        // Special case: if the expression is an Application, the user likely forgot
        // a semicolon and the parser treated the next line as a function argument.
        // Point to the end of the LEFT side (the function) instead of the RIGHT side.
        let expr_end_span = match &expr {
            Expression::Application(func, _arg) => spans::expr_end(func),
            _ => spans::expr_end(&expr),
        };

        if matches!(self.current.value, Token::TSemicolon) {
            let semi = self.take_current();
            self.advance()?;
            Ok(Binder::Assignment(selectors, eq, expr, semi))
        } else if matches!(self.current.value, Token::Sof) {
            // EOF found - check if this is an unclosed nested set
            // If the expression is a set, the closing brace might have belonged to an outer scope
            if let Expression::Term(Term::Set(_, open_brace, _, close_brace)) = &expr {
                Err(ParseError {
                    span: close_brace.span,
                    kind: ErrorKind::UnclosedDelimiter {
                        delimiter: '{',
                        opening_span: open_brace.span,
                    },
                    labels: vec![],
                })
            } else {
                Err(ParseError {
                    span: expr_end_span,
                    kind: ErrorKind::UnexpectedToken {
                        expected: vec!["';'".to_string()],
                        found: "'end of file'".to_string(),
                    },
                    labels: vec![],
                })
            }
        } else {
            // Missing semicolon - point to the END of the expression
            Err(ParseError {
                span: expr_end_span,
                kind: ErrorKind::UnexpectedToken {
                    expected: vec!["';'".to_string()],
                    found: format!("'{}'", self.current.value.text()),
                },
                labels: vec![],
            })
        }
    }

    /// Parse a selector (with optional dot)
    pub(super) fn parse_selector(&mut self) -> Result<Selector> {
        let simple_sel = self.parse_simple_selector()?;
        Ok(Selector {
            dot: None,
            selector: simple_sel,
        })
    }

    /// Parse simple selector (identifier, string, or interpolation)
    pub(super) fn parse_simple_selector(&mut self) -> Result<SimpleSelector> {
        match &self.current.value {
            Token::Identifier(_) => {
                let ident = self.take_current();
                self.advance()?;
                Ok(SimpleSelector::ID(ident))
            }
            Token::TDoubleQuote => {
                let string = self.parse_simple_string_literal()?;
                Ok(SimpleSelector::String(string))
            }
            Token::TInterOpen => {
                let interpol = self.parse_selector_interpolation()?;
                Ok(SimpleSelector::Interpol(interpol))
            }
            _ => Err(ParseError {
                span: self.current.span,
                kind: ErrorKind::UnexpectedToken {
                    expected: vec![
                        "identifier".to_string(),
                        "string".to_string(),
                        "interpolation".to_string(),
                    ],
                    found: format!("'{}'", self.current.value.text()),
                },
                labels: vec![],
            }),
        }
    }

    /// Parse selector path (used in ? member checks)
    pub(super) fn parse_selector_path(&mut self) -> Result<Vec<Selector>> {
        let mut selectors = Vec::new();

        let first_sel = self.parse_simple_selector()?;
        selectors.push(Selector {
            dot: None,
            selector: first_sel,
        });

        while matches!(self.current.value, Token::TDot) {
            let dot = self.take_current();
            self.advance()?;

            let simple_sel = self.parse_simple_selector()?;
            selectors.push(Selector {
                dot: Some(dot),
                selector: simple_sel,
            });
        }

        Ok(selectors)
    }

    /// Check if current token can start a simple selector
    pub(super) fn is_simple_selector_start(&self) -> bool {
        matches!(
            self.current.value,
            Token::Identifier(_) | Token::TDoubleQuote | Token::TInterOpen
        )
    }

    /// Check if the current token represents the `or` keyword (identifier or actual keyword)
    pub(super) fn is_or_token(&self) -> bool {
        matches!(self.current.value, Token::KOr)
            || matches!(&self.current.value, Token::Identifier(name) if name == "or")
    }
}
