//! String parsing utilities
//!
//! This module handles parsing of Nix strings (both simple "..." and indented ''...'' strings),
//! including interpolations and escape sequences.

use crate::error::{ErrorKind, ParseError, Result};
use crate::types::*;

use super::{string_processing, Parser};

impl Parser {
    /// Parse simple string literal and return annotated string structure
    pub(super) fn parse_simple_string_literal(&mut self) -> Result<Ann<Vec<Vec<StringPart>>>> {
        let open_quote_pos = self.current.span;
        let pre_trivia = self.current.pre_trivia.clone();

        // DON'T advance - just verify we're at a quote
        if !matches!(self.current.value, Token::TDoubleQuote) {
            return Err(ParseError {
                span: open_quote_pos,
                kind: ErrorKind::UnexpectedToken {
                    expected: vec!["'\"'".to_string()],
                    found: format!("'{}'", self.current.value.text()),
                },
                labels: vec![],
            });
        }

        let _opening_quote = self.take_current();
        // DON'T call advance() - parse raw characters directly

        // Now parse string content directly from lexer.input
        let mut parts = Vec::new();

        loop {
            match self.lexer.peek() {
                Some('"') => {
                    // End of string
                    break;
                }
                None => {
                    return Err(ParseError {
                        span: self.lexer.current_pos(),
                        kind: ErrorKind::UnclosedDelimiter {
                            delimiter: '"',
                            opening_span: open_quote_pos,
                        },
                        labels: vec![],
                    });
                }
                Some('$') if self.lexer.peek_ahead(1) == Some('{') => {
                    // Interpolation
                    let interp = self.parse_string_interpolation()?;
                    parts.push(interp);
                }
                _ => {
                    // Text part
                    let text = self.parse_simple_string_part()?;
                    if !text.is_empty() {
                        parts.push(StringPart::TextPart(text));
                    }
                }
            }
        }

        // Consume closing "
        self.lexer.advance();

        let trail_comment = self.parse_trailing_trivia_and_advance()?;
        let lines = string_processing::process_simple(parts);

        Ok(Ann {
            pre_trivia,
            span: open_quote_pos,
            value: lines,
            trail_comment,
        })
    }

    /// Parse simple string: "..."
    /// Parses string content directly from source (not tokens!)
    pub(super) fn parse_simple_string(&mut self) -> Result<Term> {
        let ann = self.parse_simple_string_literal()?;
        Ok(Term::SimpleString(ann))
    }

    /// Parse a text part in a simple string (handles escapes)
    /// Based on Haskell's simpleStringPart
    fn parse_simple_string_part(&mut self) -> Result<String> {
        let mut text = String::new();

        loop {
            match self.lexer.peek() {
                Some('"') | None => break,
                Some('$') if self.lexer.peek_ahead(1) == Some('{') => break,
                Some('\\') => {
                    // Escape sequence
                    self.lexer.advance(); // consume \
                    match self.lexer.peek() {
                        Some('n') => {
                            text.push_str("\\n"); // Keep escaped form
                            self.lexer.advance();
                        }
                        Some('r') => {
                            text.push_str("\\r");
                            self.lexer.advance();
                        }
                        Some('t') => {
                            text.push_str("\\t");
                            self.lexer.advance();
                        }
                        Some(ch) => {
                            // Keep as \x
                            text.push('\\');
                            text.push(ch);
                            self.lexer.advance();
                        }
                        None => break,
                    }
                }
                Some('$') if self.lexer.peek_ahead(1) == Some('$') => {
                    // $$ -> single $
                    text.push_str("$$"); // Keep as $$
                    self.lexer.advance();
                    self.lexer.advance();
                }
                Some('$') => {
                    // Lone $
                    text.push('$');
                    self.lexer.advance();
                }
                Some(ch) => {
                    text.push(ch);
                    self.lexer.advance();
                }
            }
        }

        Ok(text)
    }

    /// Parse string interpolation: ${expr}
    pub(super) fn parse_string_interpolation(&mut self) -> Result<StringPart> {
        // Consume ${
        self.lexer.advance(); // $
        self.lexer.advance(); // {

        // Re-sync parser
        self.current = self.lexer.lexeme()?;

        // Check for empty interpolation ${}
        if matches!(self.current.value, Token::TBraceClose) {
            return Err(ParseError {
                span: self.current.span,
                kind: ErrorKind::InvalidSyntax {
                    description: "empty interpolation expression".to_string(),
                    hint: Some(
                        "string interpolations require an expression inside ${...}".to_string(),
                    ),
                },
                labels: vec![],
            });
        }

        // Parse expression, catching errors to provide better messages for common mistakes
        let expr = match self.parse_expression() {
            Ok(e) => e,
            Err(err) => {
                // If we got an UnclosedDelimiter error for a quote, it's likely because
                // the user forgot to close the interpolation with }, and the expression
                // parser treated the closing quote of the outer string as a new string start
                if let ErrorKind::UnclosedDelimiter {
                    delimiter: '"',
                    opening_span,
                } = &err.kind
                {
                    // Check if the "opening" position is likely the closing quote of outer string
                    // by seeing if there's a quote at that position
                    return Err(ParseError {
                        span: *opening_span,
                        kind: ErrorKind::UnexpectedToken {
                            expected: vec!["'}'".to_string()],
                            found: "'\"'".to_string(),
                        },
                        labels: vec![],
                    });
                }
                return Err(err);
            }
        };

        // Verify we're at }
        if !matches!(self.current.value, Token::TBraceClose) {
            return Err(ParseError {
                span: self.current.span,
                kind: ErrorKind::UnexpectedToken {
                    expected: vec!["'}'".to_string()],
                    found: format!("'{}'", self.current.value.text()),
                },
                labels: vec![],
            });
        }

        // The } token was already consumed by the lexer when creating TBraceClose
        // So lexer.pos is already AFTER the }
        // DON'T call advance() or lexer.advance() - just continue from current lexer.pos

        // Now lexer.pos is right after }, and we can continue parsing string content
        // DON'T resync current - we'll continue with raw parsing

        self.lexer.rewind_trivia();
        Ok(StringPart::Interpolation(Box::new(Whole {
            value: expr,
            trailing_trivia: Trivia::new(),
        })))
    }

    /// Parse indented string: ''...''
    /// Based on Haskell's indentedString parser
    pub(super) fn parse_indented_string(&mut self) -> Result<Term> {
        let open_quote_pos = self.current.span;
        let pre_trivia = self.current.pre_trivia.clone();

        // Take the opening '' token (don't advance - just take it)
        let _opening = self.take_current();
        // Now lexer.pos is right after the '' token

        // Parse lines (separated by \n)
        let mut lines = Vec::new();
        lines.push(self.parse_indented_string_line()?);

        // Parse additional lines
        while self.lexer.peek() == Some('\n') {
            self.lexer.advance(); // consume \n
            lines.push(self.parse_indented_string_line()?);
        }

        // Expect closing ''
        if self.lexer.peek() != Some('\'') || self.lexer.peek_ahead(1) != Some('\'') {
            return Err(ParseError {
                span: self.lexer.current_pos(),
                kind: ErrorKind::UnclosedDelimiter {
                    delimiter: '\'', // represents ''
                    opening_span: open_quote_pos,
                },
                labels: vec![],
            });
        }
        self.lexer.advance(); // '
        self.lexer.advance(); // '

        let trail_comment = self.parse_trailing_trivia_and_advance()?;
        let lines = string_processing::process_indented(lines);

        let ann = Ann {
            pre_trivia,
            span: open_quote_pos,
            value: lines,
            trail_comment,
        };

        Ok(Term::IndentedString(ann))
    }

    /// Parse one line of an indented string
    /// Based on Haskell's indentedLine
    fn parse_indented_string_line(&mut self) -> Result<Vec<StringPart>> {
        let mut parts = Vec::new();

        loop {
            match self.lexer.peek() {
                Some('\'') if self.lexer.peek_ahead(1) == Some('\'') => {
                    // Could be end or escape
                    if matches!(
                        self.lexer.peek_ahead(2),
                        Some('$') | Some('\'') | Some('\\')
                    ) {
                        // Escape sequence: parse it
                        let text = self.parse_indented_string_part()?;
                        if !text.is_empty() {
                            parts.push(StringPart::TextPart(text));
                        }
                    } else {
                        // End of string
                        break;
                    }
                }
                Some('$') if self.lexer.peek_ahead(1) == Some('{') => {
                    // Interpolation
                    let interp = self.parse_string_interpolation()?;
                    parts.push(interp);
                }
                Some('\n') | None => {
                    // End of line
                    break;
                }
                _ => {
                    // Regular text
                    let text = self.parse_indented_string_part()?;
                    if !text.is_empty() {
                        parts.push(StringPart::TextPart(text));
                    }
                }
            }
        }

        Ok(parts)
    }

    /// Parse text part in indented string
    /// Based on Haskell's indentedStringPart
    fn parse_indented_string_part(&mut self) -> Result<String> {
        let mut text = String::new();

        loop {
            match self.lexer.peek() {
                None | Some('\n') => break,
                Some('\'') if self.lexer.peek_ahead(1) == Some('\'') => {
                    // Check for escape sequences
                    match self.lexer.peek_ahead(2) {
                        Some('$') => {
                            // ''$ -> $
                            text.push_str("''$");
                            self.lexer.advance();
                            self.lexer.advance();
                            self.lexer.advance();
                        }
                        Some('\'') => {
                            // ''' -> '
                            text.push_str("'''");
                            self.lexer.advance();
                            self.lexer.advance();
                            self.lexer.advance();
                        }
                        Some('\\') => {
                            // ''\ escapes next char
                            text.push_str("''\\");
                            self.lexer.advance();
                            self.lexer.advance();
                            self.lexer.advance();
                            if let Some(ch) = self.lexer.peek() {
                                text.push(ch);
                                self.lexer.advance();
                            }
                        }
                        _ => {
                            // Not an escape, end of string
                            break;
                        }
                    }
                }
                Some('$') if self.lexer.peek_ahead(1) == Some('{') => break,
                Some('$') if self.lexer.peek_ahead(1) == Some('$') => {
                    // $$ in indented string
                    text.push_str("$$");
                    self.lexer.advance();
                    self.lexer.advance();
                }
                Some('$') => {
                    // Lone $
                    text.push('$');
                    self.lexer.advance();
                }
                Some('\'') if self.lexer.peek_ahead(1) != Some('\'') => {
                    // Single '
                    text.push('\'');
                    self.lexer.advance();
                }
                Some(ch) => {
                    text.push(ch);
                    self.lexer.advance();
                }
            }
        }

        Ok(text)
    }

    /// Parse selector interpolation: ${expr} within attribute paths
    pub(super) fn parse_selector_interpolation(&mut self) -> Result<Ann<StringPart>> {
        let open = self.take_current();
        debug_assert!(matches!(open.value, Token::TInterOpen));
        self.advance()?;

        let expr = self.parse_expression()?;
        let close = self.expect_token_match(|t| matches!(t, Token::TBraceClose))?;

        Ok(Ann {
            pre_trivia: open.pre_trivia,
            span: open.span,
            value: StringPart::Interpolation(Box::new(Whole {
                value: expr,
                trailing_trivia: close.pre_trivia.clone(),
            })),
            trail_comment: close.trail_comment,
        })
    }
}
