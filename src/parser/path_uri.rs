//! Path and URI parsing utilities
//!
//! This module handles parsing of Nix paths (e.g., ./foo, ../bar, /abs, ~/home)
//! and URIs (e.g., https://example.com). Paths in Nix can contain interpolations
//! and have specific validation rules (e.g., no trailing slashes).

use crate::error::{ErrorKind, ParseError, Result};
use crate::types::*;

use super::Parser;

/// Characters allowed in URI schemes (in addition to alphanumeric)
/// Based on nixfmt's schemeChar: "-.+"
const URI_SCHEME_SPECIAL_CHARS: &[char] = &['-', '.', '+'];

/// Characters allowed in URIs (in addition to alphanumeric)
/// Based on nixfmt's uriChar: "~!@$%&*-=_+:',./?"
const URI_SPECIAL_CHARS: &[char] = &[
    '~', '!', '@', '$', '%', '&', '*', '-', '=', '_', '+', ':', '\'', ',', '.', '/', '?',
];

/// Check if character is valid in a URI scheme
/// Based on nixfmt's schemeChar: "-.+" + alphanumeric
fn is_scheme_char(c: char) -> bool {
    c.is_alphanumeric() || URI_SCHEME_SPECIAL_CHARS.contains(&c)
}

/// Check if character is valid in URI
/// Based on nixfmt's uriChar: "~!@$%&*-=_+:',./?" + alphanumeric
fn is_uri_char(c: char) -> bool {
    c.is_alphanumeric() || URI_SPECIAL_CHARS.contains(&c)
}

impl Parser {
    /// Check if current position starts a URI
    /// Pattern: scheme_chars ":" uri_chars (e.g., http://example.com)
    pub(super) fn looks_like_uri(&self) -> bool {
        // Must be an identifier
        let Token::Identifier(scheme) = &self.current.value else {
            return false;
        };

        // All characters in scheme must be valid scheme chars
        if !scheme.chars().all(is_scheme_char) {
            return false;
        }

        // Must be followed by ":"
        if self.lexer.peek() != Some(':') {
            return false;
        }

        // Must be followed by at least one URI char after ":"
        matches!(self.lexer.peek_ahead(1), Some(c) if is_uri_char(c))
    }

    /// Parse a Nix path (e.g., ./foo, ../bar, /abs, ~/home, foo/bar.nix)
    ///
    /// Paths can contain interpolations and have specific validation rules.
    pub(super) fn parse_path(&mut self) -> Result<Term> {
        let start_pos = self.current.span;
        let pre_trivia = self.current.pre_trivia.clone();
        let mut parts = Vec::new();

        // Handle the prefix that was already tokenized
        // NOTE: Don't call self.advance() here - we need to read raw chars from lexer
        match &self.current.value {
            Token::Identifier(ident) => {
                // Path starting with identifier (e.g., common/file.nix, foo-bar/baz.nix)
                // The identifier has already been consumed by the lexer
                parts.push(StringPart::TextPart(ident.clone()));
            }
            Token::TDot => {
                // ./ or ../
                // The lexer is positioned just after the '.' character
                if self.lexer.peek() == Some('.') {
                    // ../
                    parts.push(StringPart::TextPart("..".to_string()));
                    self.lexer.advance();
                } else {
                    // ./
                    parts.push(StringPart::TextPart(".".to_string()));
                }
                // Now expect /
                if self.lexer.peek() == Some('/') {
                    self.lexer.advance();
                    if let Some(StringPart::TextPart(text)) = parts.last_mut() {
                        text.push('/');
                    }
                }
            }
            Token::TDiv => {
                // Absolute path /
                // The lexer is positioned just after the '/' character
                // Don't call self.advance() - just start with "/"
                parts.push(StringPart::TextPart("/".to_string()));
            }
            Token::TTilde => {
                // ~/
                // The lexer is positioned just after the '~' character
                parts.push(StringPart::TextPart("~".to_string()));
                if self.lexer.peek() == Some('/') {
                    self.lexer.advance();
                    if let Some(StringPart::TextPart(text)) = parts.last_mut() {
                        text.push('/');
                    }
                }
            }
            _ => {}
        }

        // Parse rest of path
        loop {
            match self.lexer.peek() {
                Some('$') if self.lexer.peek_ahead(1) == Some('{') => {
                    // Interpolation in path
                    let interp = self.parse_string_interpolation()?;
                    parts.push(interp);
                }
                Some(ch) if ch.is_alphanumeric() || matches!(ch, '.' | '_' | '-' | '+') => {
                    // Path text (not / here, that's handled specially)
                    let text = self.parse_path_part()?;
                    if !text.is_empty() {
                        // Append to last TextPart if it exists, otherwise create new one
                        if let Some(StringPart::TextPart(last_text)) = parts.last_mut() {
                            last_text.push_str(&text);
                        } else {
                            parts.push(StringPart::TextPart(text));
                        }
                    }
                }
                Some('/') => {
                    // Path separator
                    self.lexer.advance();
                    if let Some(StringPart::TextPart(text)) = parts.last_mut() {
                        text.push('/');
                    } else {
                        parts.push(StringPart::TextPart("/".to_string()));
                    }
                }
                _ => break,
            }
        }

        // Validate: paths cannot end with a trailing slash
        // This matches nixfmt's requirement that pathTraversal must have content after the slash
        if let Some(StringPart::TextPart(text)) = parts.last() {
            if text.ends_with('/') {
                // Point to the trailing slash, not the start of the path
                let current_pos = self.lexer.current_pos().start;
                let slash_pos = Span {
                    start: current_pos.saturating_sub(1),
                    end: current_pos,
                };
                return Err(ParseError {
                    span: slash_pos,
                    kind: ErrorKind::InvalidSyntax {
                        description: "path cannot end with a trailing slash".to_string(),
                        hint: Some(
                            "remove the trailing '/' or add more path components".to_string(),
                        ),
                    },
                    labels: vec![],
                });
            }
        }

        let trail_comment = self.parse_trailing_trivia_and_advance()?;

        let ann = Ann {
            pre_trivia,
            span: start_pos,
            value: parts,
            trail_comment,
        };

        Ok(Term::Path(ann))
    }

    /// Parse path text component (without /)
    /// Based on Haskell's pathText
    fn parse_path_part(&mut self) -> Result<String> {
        let mut text = String::new();

        while let Some(ch) = self.lexer.peek() {
            if ch.is_alphanumeric() || matches!(ch, '.' | '_' | '-' | '+' | '~') {
                text.push(ch);
                self.lexer.advance();
            } else if ch == '$' && self.lexer.peek_ahead(1) == Some('{') {
                // Interpolation coming
                break;
            } else if ch == '/' {
                // Don't consume / here - it's handled in the main loop
                break;
            } else {
                break;
            }
        }

        Ok(text)
    }

    /// Parse URI as a SimpleString
    /// Based on nixfmt's uri parser
    pub(super) fn parse_uri(&mut self) -> Result<Term> {
        let start_pos = self.current.span;
        let pre_trivia = self.current.pre_trivia.clone();

        // Get the scheme (already tokenized as identifier)
        let Token::Identifier(scheme) = &self.current.value else {
            return Err(ParseError {
                span: start_pos,
                kind: ErrorKind::UnexpectedToken {
                    expected: vec!["identifier".to_string()],
                    found: format!("'{}'", self.current.value.text()),
                },
                labels: vec![],
            });
        };

        let mut uri_text = scheme.clone();

        // Expect ":"
        if self.lexer.peek() != Some(':') {
            return Err(ParseError {
                span: self.lexer.current_pos(),
                kind: ErrorKind::MissingToken {
                    token: "':'".to_string(),
                    after: "URI scheme".to_string(),
                },
                labels: vec![],
            });
        }
        self.lexer.advance();
        uri_text.push(':');

        // Parse URI characters
        while let Some(ch) = self.lexer.peek() {
            if is_uri_char(ch) {
                uri_text.push(ch);
                self.lexer.advance();
            } else {
                break;
            }
        }

        // Parse trailing trivia and advance
        let trail_comment = self.parse_trailing_trivia_and_advance()?;

        // Wrap as SimpleString
        let parts = vec![vec![StringPart::TextPart(uri_text)]];
        let ann = Ann {
            pre_trivia,
            span: start_pos,
            value: parts,
            trail_comment,
        };

        Ok(Term::SimpleString(ann))
    }

    /// Parse environment path term (e.g., <nixpkgs>)
    pub(super) fn parse_env_path_term(&mut self) -> Result<Term> {
        let token_ann = self.take_current();
        self.advance()?;
        Ok(Term::Token(token_ann))
    }
}
