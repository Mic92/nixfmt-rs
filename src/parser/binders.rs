//! Binder parsing utilities
//!
//! This module handles parsing of bindings in Nix attribute sets and let expressions,
//! including both `inherit` statements and attribute assignments (name = value).

use crate::error::{ParseError, Result};
use crate::types::{Binder, Expression, Items, Selector, SimpleSelector, StringPart, Term, Token};

use super::{Parser, spans};

impl Parser {
    /// Parse a list of binders (for let expressions and attribute sets)
    pub(super) fn parse_binders(&mut self) -> Result<Items<Binder>> {
        self.parse_items(
            |t| matches!(t, Token::KIn | Token::TBraceClose | Token::Sof),
            Self::parse_binder,
        )
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
        let inherit_tok = self.expect_token(Token::KInherit, "'inherit'")?;

        let from = if matches!(self.current.value, Token::TParenOpen) {
            let open = self.take_and_advance()?;
            let expr = self.parse_expression()?;
            let close = self.expect_token(Token::TParenClose, "')'")?;
            Some(Term::Parenthesized {
                open,
                expr: Box::new(expr),
                close,
            })
        } else {
            None
        };

        let mut selectors = Vec::new();
        while self.is_simple_selector_start() {
            let span = self.current.span;
            let sel = self.parse_simple_selector()?;
            // Haskell `simpleSelector interpolationRestricted`: an `inherit`
            // name written as `${…}` is only accepted when the body is a
            // plain string literal (`Term (SimpleString _)`).
            if let SimpleSelector::Interpol(ann) = &sel {
                let ok = matches!(
                    &ann.value,
                    StringPart::Interpolation(w)
                        if matches!(&w.value, Expression::Term(Term::SimpleString(_)))
                );
                if !ok {
                    return Err(ParseError::unexpected(
                        span,
                        vec!["identifier".into(), "string".into()],
                        "interpolation",
                    ));
                }
            }
            selectors.push(sel);
        }

        let semi = self.expect_token(Token::TSemicolon, "';'")?;

        Ok(Binder::Inherit {
            kw: inherit_tok,
            from,
            attrs: selectors,
            semi,
        })
    }

    /// Parse assignment: selector = expr ;
    fn parse_assignment(&mut self) -> Result<Binder> {
        let mut selectors = Vec::new();

        let first_sel = self.parse_selector()?;
        selectors.push(first_sel);
        self.parse_dotted_tail(&mut selectors)?;

        // Check for common mistake: attribute path followed by semicolon (forgot = and value)
        if matches!(self.current.value, Token::TSemicolon) {
            return Err(ParseError::unexpected(
                self.current.span,
                vec!["'='".to_string()],
                "';'",
            ));
        }

        let eq = self.expect_token(Token::TAssign, "'='")?;
        let expr = self.parse_expression()?;

        // Special case: if the expression is an Application, the user likely forgot
        // a semicolon and the parser treated the next line as a function argument.
        // Point to the end of the LEFT side (the function) instead of the RIGHT side.
        let expr_end_span = match &expr {
            Expression::Application { func, .. } => spans::expr_end(func),
            _ => spans::expr_end(&expr),
        };

        if matches!(self.current.value, Token::TSemicolon) {
            let semi = self.take_and_advance()?;
            Ok(Binder::Assignment {
                path: selectors,
                eq,
                value: expr,
                semi,
            })
        } else if matches!(self.current.value, Token::Sof) {
            // EOF found - check if this is an unclosed nested set
            // If the expression is a set, the closing brace might have belonged to an outer scope
            if let Expression::Term(Term::Set {
                open: open_brace,
                close: close_brace,
                ..
            }) = &expr
            {
                Err(ParseError::unclosed(close_brace.span, '{', open_brace.span))
            } else {
                Err(ParseError::unexpected(
                    expr_end_span,
                    vec!["';'".to_string()],
                    "'end of file'",
                ))
            }
        } else {
            // Missing semicolon - point to the END of the expression
            Err(ParseError::unexpected(
                expr_end_span,
                vec!["';'".to_string()],
                format!("'{}'", self.current.value.text()),
            ))
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
                let ident = self.take_and_advance()?;
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
            _ => Err(ParseError::unexpected(
                self.current.span,
                vec![
                    "identifier".to_string(),
                    "string".to_string(),
                    "interpolation".to_string(),
                ],
                format!("'{}'", self.current.value.text()),
            )),
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
        self.parse_dotted_tail(&mut selectors)?;

        Ok(selectors)
    }

    /// Consume zero or more `.selector` segments and append them to `selectors`.
    ///
    /// Shared by attribute-path parsing in assignments and `?` member checks.
    /// `parse_postfix_selection` has its own backtracking variant and does not
    /// use this.
    fn parse_dotted_tail(&mut self, selectors: &mut Vec<Selector>) -> Result<()> {
        while matches!(self.current.value, Token::TDot) {
            let dot = self.take_and_advance()?;
            let simple_sel = self.parse_simple_selector()?;
            selectors.push(Selector {
                dot: Some(dot),
                selector: simple_sel,
            });
        }
        Ok(())
    }

    /// Check if current token can start a simple selector
    pub(super) const fn is_simple_selector_start(&self) -> bool {
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
