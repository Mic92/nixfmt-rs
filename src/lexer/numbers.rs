//! Number literal parsing
//!
//! This module handles parsing of Nix numeric literals:
//! - Integers: `42`, `0`, `123`
//! - Floats: `3.14`, `1.`, `.5`, `1.5e10`, `2e-5`
//!
//! Special cases:
//! - Numbers with leading zeros (e.g., `01`) cannot have decimal points
//! - Trailing dots are allowed for non-zero integers (e.g., `1.`)
//! - Leading dots are allowed (e.g., `.5`)
//! - Scientific notation with optional sign (e.g., `1e10`, `2e-5`)

use super::Lexer;
use crate::types::Token;

impl Lexer {
    /// Parse a number literal (integer or float)
    pub(super) fn parse_number(&mut self) -> crate::error::Result<Token> {
        let mut num = self.consume_digits();
        let mut is_float = false;

        if self.peek() == Some('.') {
            let next = self.peek_ahead(1);
            let has_leading_zero = num.starts_with('0');
            let has_multiple_leading_zeroes = has_leading_zero && num.len() > 1;

            if next.is_some_and(|c| c.is_ascii_digit()) && !has_multiple_leading_zeroes {
                is_float = true;
                num.push('.');
                self.advance();
                num.push_str(&self.consume_digits());
            } else if next.is_none_or(|c| !c.is_ascii_digit()) && !num.is_empty() && num != "0" {
                // Allow trailing '.' for numbers starting with non-zero digit (e.g., 1.)
                is_float = true;
                num.push('.');
                self.advance();
            }
        }

        if is_float {
            if let Some(exp) = self.parse_exponent() {
                num.push_str(&exp);
            }
            return Ok(Token::Float(num.into()));
        }

        Ok(Token::Integer(num.into()))
    }

    /// Parse scientific notation exponent (e.g., `e10`, `E-5`, `e+3`)
    pub(super) fn parse_exponent(&mut self) -> Option<String> {
        if !matches!(self.peek(), Some('e' | 'E')) {
            return None;
        }

        self.try_with_cursor(|this| {
            let mut exponent = String::new();
            exponent.push(this.advance().unwrap());

            if matches!(this.peek(), Some('+' | '-')) {
                exponent.push(this.advance().unwrap());
            }

            if this.peek().is_some_and(|c| c.is_ascii_digit()) {
                exponent.push_str(&this.consume_digits());
                Some(exponent)
            } else {
                None
            }
        })
    }

    /// Consume consecutive ASCII digits and return as String
    pub(super) fn consume_digits(&mut self) -> String {
        self.take_ascii_while(|b| b.is_ascii_digit()).to_owned()
    }
}
