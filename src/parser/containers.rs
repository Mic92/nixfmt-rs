//! Container parsing utilities
//!
//! This module handles parsing of Nix container structures:
//! - Attribute sets: `{ }`, `rec { }`, `let { }`
//! - Lists: `[ ]`
//! - Parenthesized expressions: `( )`

use crate::error::{ErrorKind, ParseError, Result};
use crate::types::*;

use super::Parser;

impl Parser {
    /// Parse attribute set: { ... } or rec { ... } or let { ... }
    pub(super) fn parse_set(&mut self) -> Result<Term> {
        let prefix_tok = if matches!(self.current.value, Token::KRec | Token::KLet) {
            let tok = self.take_and_advance()?;
            Some(tok)
        } else {
            None
        };

        let open_brace = self.expect_token_match(|t| matches!(t, Token::TBraceOpen))?;
        let opening_span = open_brace.span;
        let bindings = self.parse_binders()?;

        let close_brace = self.expect_closing_delimiter(opening_span, '{', Token::TBraceClose)?;

        Ok(Term::Set(prefix_tok, open_brace, bindings, close_brace))
    }

    /// Parse list: [ ... ]
    pub(super) fn parse_list(&mut self) -> Result<Term> {
        let open_bracket = self.expect_token_match(|t| matches!(t, Token::TBrackOpen))?;
        let opening_span = open_bracket.span;
        let items = self.parse_list_items()?;

        let close_bracket = self.expect_closing_delimiter(opening_span, '[', Token::TBrackClose)?;

        Ok(Term::List(open_bracket, items, close_bracket))
    }

    /// Parse list items (terms)
    fn parse_list_items(&mut self) -> Result<Items<Term>> {
        let mut items = Vec::new();

        while !matches!(self.current.value, Token::TBrackClose | Token::Sof) {
            // Check for commas (not valid in Nix lists)
            if matches!(self.current.value, Token::TComma) {
                return Err(Box::new(ParseError {
                    span: self.current.span,
                    kind: ErrorKind::InvalidSyntax {
                        description: "commas are not used to separate list elements in Nix"
                            .to_string(),
                        hint: Some("use spaces to separate list elements: [1 2 3]".to_string()),
                    },
                    labels: vec![],
                }));
            }

            // Check for mismatched closing delimiters before trying to parse
            if matches!(
                self.current.value,
                Token::TBraceClose | Token::TParenClose | Token::TInterClose
            ) {
                return Err(Box::new(ParseError {
                    span: self.current.span,
                    kind: ErrorKind::InvalidSyntax {
                        description: format!(
                            "mismatched delimiter: expected ']', found '{}'",
                            self.current.value.text()
                        ),
                        hint: Some(format!(
                            "change '{}' to ']' to match the opening bracket",
                            self.current.value.text()
                        )),
                    },
                    labels: vec![],
                }));
            }

            self.collect_trivia_as_comments(&mut items);

            let term = self.parse_term()?;
            items.push(Item::Item(term));
        }

        if matches!(self.current.value, Token::TBrackClose) {
            self.collect_trivia_as_comments(&mut items);
        }

        Ok(Items(items))
    }

    /// Parse parenthesized expression: ( expr )
    pub(super) fn parse_parenthesized(&mut self) -> Result<Term> {
        let open_paren = self.expect_token_match(|t| matches!(t, Token::TParenOpen))?;
        let opening_span = open_paren.span;

        let expr = self.parse_expression()?;

        let close_paren = self.expect_closing_delimiter(opening_span, '(', Token::TParenClose)?;

        Ok(Term::Parenthesized(open_paren, Box::new(expr), close_paren))
    }
}
