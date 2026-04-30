//! Path and URI parsing utilities
//!
//! This module handles parsing of Nix paths (e.g., ./foo, ../bar, /abs, ~/home)
//! and URIs (e.g., `https://example.com`). Paths in Nix can contain interpolations
//! and have specific validation rules (e.g., no trailing slashes).

use crate::error::{ParseError, Result};
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
        let Token::Identifier(scheme) = &self.current.value else {
            return false;
        };
        // Cheap checks first: invoked for every identifier term, so avoid
        // scanning the scheme unless `:<uri-char>` already follows.
        if self.lexer.peek() != Some(':') {
            return false;
        }
        if !matches!(self.lexer.peek_ahead(1), Some(c) if is_uri_char(c)) {
            return false;
        }
        scheme.chars().all(is_scheme_char)
    }

    /// Parse a Nix path (e.g., ./foo, ../bar, /abs, ~/home, foo/bar.nix)
    ///
    /// Paths can contain interpolations and have specific validation rules.
    pub(super) fn parse_path(&mut self) -> Result<Term> {
        let start_pos = self.current.span;
        let pre_trivia = std::mem::take(&mut self.current.pre_trivia);
        let mut parts = Vec::new();

        // NOTE: Don't call self.advance() here - we need to read raw chars from lexer
        match &self.current.value {
            Token::Identifier(ident) => {
                // Path starting with identifier (e.g., common/file.nix, foo-bar/baz.nix)
                parts.push(StringPart::TextPart(ident.to_string()));
            }
            Token::TDot => {
                // ./ or ../
                if self.lexer.peek() == Some('.') {
                    parts.push(StringPart::TextPart("..".to_string()));
                    self.lexer.advance();
                } else {
                    parts.push(StringPart::TextPart(".".to_string()));
                }
                if self.lexer.peek() == Some('/') {
                    self.lexer.advance();
                    if let Some(StringPart::TextPart(text)) = parts.last_mut() {
                        text.push('/');
                    }
                }
            }
            Token::TDiv => {
                // Absolute path /
                parts.push(StringPart::TextPart("/".to_string()));
            }
            Token::TTilde => {
                // ~/
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

        loop {
            match self.lexer.peek() {
                Some('$') if self.lexer.at("${") => {
                    let interp = self.parse_string_interpolation()?;
                    parts.push(interp);
                }
                Some(ch) if ch.is_alphanumeric() || matches!(ch, '.' | '_' | '-' | '+') => {
                    let text = self.parse_path_part()?;
                    if !text.is_empty() {
                        if let Some(StringPart::TextPart(last_text)) = parts.last_mut() {
                            last_text.push_str(&text);
                        } else {
                            parts.push(StringPart::TextPart(text));
                        }
                    }
                }
                Some('/') => {
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

        // nixfmt's `pathTraversal` requires content after every `/`.
        if let Some(StringPart::TextPart(text)) = parts.last() {
            if text.ends_with('/') {
                // Point to the trailing slash, not the start of the path
                let current_pos = self.lexer.current_pos().start as usize;
                let slash_pos = Span::new(current_pos.saturating_sub(1), current_pos);
                return Err(ParseError::invalid(
                    slash_pos,
                    "path cannot end with a trailing slash",
                    Some("remove the trailing '/' or add more path components".to_string()),
                ));
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
            } else if ch == '$' && self.lexer.at("${") {
                break;
            } else if ch == '/' {
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
        let pre_trivia = std::mem::take(&mut self.current.pre_trivia);

        let Token::Identifier(scheme) = &self.current.value else {
            return Err(ParseError::unexpected(
                start_pos,
                vec!["identifier".to_string()],
                format!("'{}'", self.current.value.text()),
            ));
        };

        let mut uri_text = scheme.clone();

        if self.lexer.peek() != Some(':') {
            return Err(ParseError::missing(
                self.lexer.current_pos(),
                "':'",
                "URI scheme",
            ));
        }
        self.lexer.advance();
        uri_text.push(':');

        while let Some(ch) = self.lexer.peek() {
            if is_uri_char(ch) {
                uri_text.push(ch);
                self.lexer.advance();
            } else {
                break;
            }
        }

        let trail_comment = self.parse_trailing_trivia_and_advance()?;

        let parts = vec![vec![StringPart::TextPart(uri_text.to_string())]];
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
        let token_ann = self.take_and_advance()?;
        Ok(Term::Token(token_ann))
    }

    /// Check if there's path content at the given offset
    /// Used to validate that what follows is a valid path component
    fn is_path_content_at(&self, offset: usize) -> bool {
        match self.lexer.peek_ahead(offset) {
            Some(c) if c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '+' | '~') => true,
            Some('$') => self.lexer.peek_ahead(offset + 1) == Some('{'), // interpolation ${
            _ => false,
        }
    }

    /// Check if there's whitespace before current token
    /// Used to distinguish paths from operators: "a/b" (path) vs "a / b" (division)
    fn has_preceding_whitespace(&self) -> bool {
        self.lexer.recent_hspace > 0 || self.lexer.recent_newlines > 0
    }

    /// Check if current position starts a path
    /// Must check BEFORE consuming any tokens
    pub(super) fn looks_like_path(&self) -> bool {
        match &self.current.value {
            // identifier/ → path (no space), identifier /path → application (space before /)
            Token::Identifier(_) => {
                self.lexer.peek() == Some('/')
                    && !self.lexer.at("//")
                    && self.is_path_content_at(1)
                    && !self.has_preceding_whitespace()
            }

            // ./ or ../
            Token::TDot => match (self.lexer.peek(), self.lexer.peek_ahead(1)) {
                (Some('/'), _) => self.is_path_content_at(1), // ./
                (Some('.'), Some('/')) => self.is_path_content_at(2), // ../
                _ => false,
            },

            // /path → path (no space before), expr /path → division (space before)
            Token::TDiv => self.is_path_content_at(0) && !self.has_preceding_whitespace(),

            // ~/
            Token::TTilde => self.lexer.peek() == Some('/') && self.is_path_content_at(1),

            _ => false,
        }
    }
}
