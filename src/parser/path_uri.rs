//! Path and URI parsing utilities
//!
//! This module handles parsing of Nix paths (e.g., ./foo, ../bar, /abs, ~/home)
//! and URIs (e.g., `https://example.com`). Paths in Nix can contain interpolations
//! and have specific validation rules (e.g., no trailing slashes).

use crate::error::{ParseError, Result};
use crate::types::{Span, StringPart, Term, Token};

use super::Parser;

/// Characters allowed in URI schemes (in addition to alphanumeric)
/// Based on nixfmt's schemeChar: "-.+"
const URI_SCHEME_SPECIAL_CHARS: &[char] = &['-', '.', '+'];

/// Characters allowed in URIs (in addition to alphanumeric)
/// Based on nixfmt's uriChar: "~!@$%&*-=_+:',./?"
const URI_SPECIAL_CHARS: &[char] = &[
    '~', '!', '@', '$', '%', '&', '*', '-', '=', '_', '+', ':', '\'', ',', '.', '/', '?',
];

/// nixfmt `schemeChar`: "-.+" + alphanumeric
fn is_scheme_char(c: char) -> bool {
    c.is_alphanumeric() || URI_SCHEME_SPECIAL_CHARS.contains(&c)
}

/// nixfmt `uriChar`: "~!@$%&*-=_+:',./?" + alphanumeric
fn is_uri_char(c: char) -> bool {
    c.is_alphanumeric() || URI_SPECIAL_CHARS.contains(&c)
}

/// nixfmt `pathChar`: "._-+~" + alphanumeric
fn is_path_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '+' | '~')
}

/// Append `s` to a trailing `TextPart`, or push a new one. Paths are built
/// from a token-level head and a char-level tail; this keeps adjacent text in
/// one part so the formatter sees the literal source slice.
fn push_path_text(parts: &mut Vec<StringPart>, s: &str) {
    if let Some(StringPart::TextPart(last)) = parts.last_mut() {
        let mut combined = String::from(std::mem::take(last));
        combined.push_str(s);
        *last = combined.into_boxed_str();
    } else {
        parts.push(StringPart::TextPart(s.into()));
    }
}

impl Parser {
    /// Check if current position starts a URI
    /// Pattern: `scheme_chars` ":" `uri_chars` (e.g., <http://example.com>)
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
        let ann = self.with_raw_ann(|p| {
            let mut parts = Vec::new();

            // The head was already lexed as a token; do not advance() — the
            // tail must be read as raw chars so `a/b` is one path, not `a / b`.
            match &p.current.value {
                // e.g. common/file.nix, foo-bar/baz.nix
                Token::Identifier(ident) => {
                    parts.push(StringPart::TextPart(ident.as_str().into()));
                }
                // ./ or ../ — the following `/` is consumed by the tail loop.
                Token::TDot if p.lexer.peek() == Some('.') => {
                    p.lexer.advance();
                    push_path_text(&mut parts, "..");
                }
                Token::TDot => push_path_text(&mut parts, "."),
                Token::TDiv => push_path_text(&mut parts, "/"),
                Token::TTilde => push_path_text(&mut parts, "~"),
                _ => {}
            }

            loop {
                match p.lexer.peek() {
                    Some('$') if p.lexer.at("${") => {
                        parts.push(p.parse_string_interpolation()?);
                    }
                    Some(ch) if ch.is_alphanumeric() || matches!(ch, '.' | '_' | '-' | '+') => {
                        push_path_text(&mut parts, &p.parse_path_part());
                    }
                    Some('/') => {
                        p.lexer.advance();
                        push_path_text(&mut parts, "/");
                    }
                    _ => break,
                }
            }

            // nixfmt's `pathTraversal` requires content after every `/`.
            if let Some(StringPart::TextPart(text)) = parts.last()
                && text.ends_with('/')
            {
                // Point to the trailing slash, not the start of the path
                let current_pos = p.lexer.current_pos().start();
                let slash_pos = Span::new(current_pos.saturating_sub(1), current_pos);
                return Err(ParseError::invalid(
                    slash_pos,
                    "path cannot end with a trailing slash",
                    Some("remove the trailing '/' or add more path components".to_string()),
                ));
            }

            Ok(parts)
        })?;

        Ok(Term::Path(ann))
    }

    /// One path segment (no `/`). Haskell `pathText`.
    fn parse_path_part(&mut self) -> String {
        let mut text = String::new();
        while let Some(ch) = self.lexer.peek().filter(|&c| is_path_char(c)) {
            text.push(ch);
            self.lexer.advance();
        }
        text
    }

    /// Parse URI as a `SimpleString`. Only called when [`looks_like_uri`]
    /// has already verified `current == Identifier(_)` and the lexer is
    /// positioned at `':' <uri-char>`, so neither check can fail here.
    pub(super) fn parse_uri(&mut self) -> Result<Term> {
        let ann = self.with_raw_ann(|p| {
            let Token::Identifier(scheme) = &p.current.value else {
                unreachable!("looks_like_uri guards this")
            };
            let mut uri_text = scheme.clone();

            debug_assert_eq!(p.lexer.peek(), Some(':'));
            p.lexer.advance();
            uri_text.push(':');

            while let Some(ch) = p.lexer.peek() {
                if is_uri_char(ch) {
                    uri_text.push(ch);
                    p.lexer.advance();
                } else {
                    break;
                }
            }

            Ok(vec![vec![StringPart::TextPart(uri_text.as_str().into())]])
        })?;

        Ok(Term::SimpleString(ann))
    }

    /// Whether a path segment (text or `${…}`) starts at `offset`.
    fn is_path_content_at(&self, offset: usize) -> bool {
        match self.lexer.peek_ahead(offset) {
            Some(c) if is_path_char(c) => true,
            Some('$') => self.lexer.peek_ahead(offset + 1) == Some('{'),
            _ => false,
        }
    }

    /// Check if there's whitespace before current token
    /// Used to distinguish paths from operators: "a/b" (path) vs "a / b" (division)
    const fn has_preceding_whitespace(&self) -> bool {
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
