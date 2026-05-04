//! Container parsing utilities
//!
//! This module handles parsing of Nix container structures:
//! - Attribute sets: `{ }`, `rec { }`, `let { }`
//! - Lists: `[ ]`
//! - Parenthesized expressions: `( )`

use crate::ast::{Items, Term, Token};
use crate::error::{ParseError, Result};

use super::Parser;

impl Parser {
    /// Parse attribute set: { ... } or rec { ... } or let { ... }
    pub(super) fn parse_set(&mut self) -> Result<Term> {
        let prefix_tok = if matches!(self.current.value, Token::Rec | Token::Let) {
            let tok = self.take_and_advance()?;
            Some(tok)
        } else {
            None
        };

        let open_brace = self.expect_token(Token::BraceOpen, "'{'")?;
        let opening_span = open_brace.span;
        let bindings = self.parse_binders()?;

        let close_brace = self.expect_closing_delimiter(opening_span, '{', Token::BraceClose)?;

        Ok(Term::Set {
            rec: prefix_tok,
            open: open_brace,
            items: bindings,
            close: close_brace,
        })
    }

    /// Parse list: [ ... ]
    pub(super) fn parse_list(&mut self) -> Result<Term> {
        let open_bracket = self.expect_token(Token::BrackOpen, "'['")?;
        let opening_span = open_bracket.span;
        let items = self.parse_list_items()?;

        let close_bracket = self.expect_closing_delimiter(opening_span, '[', Token::BrackClose)?;

        Ok(Term::List {
            open: open_bracket,
            items,
            close: close_bracket,
        })
    }

    /// Parse list items (terms)
    fn parse_list_items(&mut self) -> Result<Items<Term>> {
        self.parse_items(
            |t| matches!(t, Token::BrackClose | Token::Sof),
            |p| {
                // Check for commas (not valid in Nix lists)
                if matches!(p.current.value, Token::Comma) {
                    return Err(ParseError::invalid(
                        p.current.span,
                        "commas are not used to separate list elements in Nix",
                        Some("use spaces to separate list elements: [1 2 3]".to_string()),
                    ));
                }

                // Check for mismatched closing delimiters before trying to parse
                if matches!(
                    p.current.value,
                    Token::BraceClose | Token::ParenClose | Token::InterClose
                ) {
                    return Err(ParseError::invalid(
                        p.current.span,
                        format!(
                            "mismatched delimiter: expected ']', found '{}'",
                            p.current.value.text()
                        ),
                        Some(format!(
                            "change '{}' to ']' to match the opening bracket",
                            p.current.value.text()
                        )),
                    ));
                }

                p.parse_term()
            },
        )
    }

    /// Parse parenthesized expression: ( expr )
    pub(super) fn parse_parenthesized(&mut self) -> Result<Term> {
        let open_paren = self.expect_token(Token::ParenOpen, "'('")?;
        let opening_span = open_paren.span;

        let expr = self.parse_expression()?;

        let close_paren = self.expect_closing_delimiter(opening_span, '(', Token::ParenClose)?;

        Ok(Term::Parenthesized {
            open: open_paren,
            expr: Box::new(expr),
            close: close_paren,
        })
    }
}
