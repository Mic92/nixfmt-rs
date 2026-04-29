//! Comment parsing and normalization
//!
//! This module handles parsing and normalizing Nix comments:
//! - Line comments (`# comment`)
//! - Block comments (`/* comment */`)
//! - Doc comments (`/** comment */`)
//! - Language annotations (`/* lua */`)
//!
//! Block comments are normalized according to nixfmt's Haskell implementation,
//! including star alignment removal and indentation fixing.

use super::{Lexer, ParseTrivium};

impl Lexer {
    /// Parse a line comment starting with '#'
    pub(super) fn parse_line_comment(&mut self) -> ParseTrivium {
        let col = self.column;
        self.advance(); // consume '#'

        let rest = &self.source.as_bytes()[self.byte_pos..];
        let len = memchr::memchr2(b'\n', b'\r', rest).unwrap_or(rest.len());
        let text = self.advance_bytes_no_newline(len).trim_end().to_owned();

        ParseTrivium::LineComment { text, col }
    }

    /// Parse block comment /* ... */
    pub(super) fn parse_block_comment(&mut self) -> ParseTrivium {
        let start_col = self.column;
        self.advance(); // consume '/'
        self.advance(); // consume '*'

        // Check for doc comment /**
        let is_doc = self.peek() == Some('*') && !self.at("*/");
        if is_doc {
            self.advance();
        }

        // Find the closing `*/` by scanning for `*` and checking the next byte.
        let body_start = self.byte_pos;
        let mut scan = body_start;
        let bytes = self.source.as_bytes();
        let body_end = loop {
            match memchr::memchr(b'*', &bytes[scan..]) {
                // Unterminated; consume to EOF (matches previous behaviour).
                None => break bytes.len(),
                Some(off) if bytes.get(scan + off + 1) == Some(&b'/') => break scan + off,
                Some(off) => scan += off + 1,
            }
        };
        self.seek_to(body_end);
        self.advance_by(2); // `*/` (no-op at EOF)
        let body = &self.source[body_start..body_end];

        // Normalize the comment according to Haskell logic
        let lines = split_lines(body);
        let lines = remove_stars(start_col, lines);
        let lines = fix_indent(start_col, lines);

        // Drop leading and trailing empty lines
        let lines = drop_while_empty_start(lines);
        let lines = drop_while_empty_end(lines);

        ParseTrivium::BlockComment(is_doc, lines)
    }

    /// Try to parse a language annotation like /* lua */
    pub(super) fn try_parse_language_annotation(&mut self) -> Option<ParseTrivium> {
        let saved_state = self.save_state();

        // Parse as block comment
        let pt = self.parse_block_comment();

        // Check if it's a single-line, non-doc block comment
        if let ParseTrivium::BlockComment(false, lines) = &pt {
            if lines.len() == 1 {
                let content = lines[0].trim();

                // Check if it's a valid language identifier
                if is_valid_language_identifier(content) {
                    // Check if next token is a string delimiter
                    if self.is_next_string_delimiter() {
                        return Some(ParseTrivium::LanguageAnnotation(content.to_string()));
                    }
                }
            }
        }

        // Not a language annotation, restore state
        self.restore_state(saved_state);
        None
    }

    /// Check if next non-whitespace token is " or ''
    fn is_next_string_delimiter(&mut self) -> bool {
        let saved_state = self.save_state();

        let _ = self.skip_hspace();

        // Optionally consume exactly one newline; a blank line between the
        // comment and the string disqualifies it as a language annotation.
        match self.peek() {
            Some('\n') => {
                self.advance();
            }
            Some('\r') => {
                self.advance();
                if self.peek() == Some('\n') {
                    self.advance();
                }
            }
            _ => {}
        }
        let _ = self.skip_hspace();

        let result = self.peek() == Some('"') || self.at("''");

        self.restore_state(saved_state);
        result
    }
}

// Comment normalization functions (from Haskell Lexer.hs)

/// Check if identifier is valid for language annotation
fn is_valid_language_identifier(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 30
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'+' | b'.' | b'_'))
}

/// Split text into lines, normalize line endings, and drop trailing empty lines
/// This matches nixfmt's splitLines function which does `dropWhileEnd Text.null`
pub(super) fn split_lines(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text
        .replace("\r\n", "\n")
        .lines()
        .map(|line| line.trim_end().to_string())
        .collect();

    // Drop trailing empty lines (matches Haskell's dropWhileEnd Text.null)
    while lines.last().is_some_and(|line| line.is_empty()) {
        lines.pop();
    }

    lines
}

/// Remove aligned stars from block comments (Lexer.hs:110-118)
/// If all continuation lines have " *" at position `pos`, remove them
pub(super) fn remove_stars(pos: usize, lines: Vec<String>) -> Vec<String> {
    if lines.is_empty() {
        return Vec::new();
    }

    let star_prefix = format!("{} *", " ".repeat(pos));
    let new_prefix = " ".repeat(pos);

    // Check if ALL continuation lines (not first) start with aligned star
    let all_have_star = lines[1..].iter().all(|line| line.starts_with(&star_prefix));

    if all_have_star && !lines[1..].is_empty() {
        // Keep first line, replace star prefix in continuation lines
        let mut result = vec![lines[0].clone()];
        for line in &lines[1..] {
            result.push(line.replacen(&star_prefix, &new_prefix, 1));
        }
        result
    } else {
        lines
    }
}

/// Fix indentation of block comment lines (Lexer.hs:123-128)
fn fix_indent(pos: usize, lines: Vec<String>) -> Vec<String> {
    if lines.is_empty() {
        return Vec::new();
    }

    let first = &lines[0];

    // If first line starts with space, offset is pos+3, otherwise pos+2
    let offset = if first.starts_with(' ') {
        pos + 3
    } else {
        pos + 2
    };

    // Find common indentation among non-empty continuation lines
    let common_indent = lines[1..]
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.bytes().take_while(|&b| b == b' ').count())
        .min()
        .unwrap_or(0)
        .min(offset);

    // Strip first line and apply common indentation to rest
    let mut result = vec![first.trim().to_string()];
    for line in &lines[1..] {
        result.push(strip_indentation(common_indent, line));
    }
    result
}

fn strip_indentation(n: usize, text: &str) -> String {
    text.strip_prefix(&" ".repeat(n))
        .unwrap_or_else(|| text.trim_start())
        .to_string()
}

fn drop_while_empty_start(lines: Vec<String>) -> Vec<String> {
    lines
        .into_iter()
        .skip_while(|line| line.trim().is_empty())
        .collect()
}

fn drop_while_empty_end(mut lines: Vec<String>) -> Vec<String> {
    while lines.last().is_some_and(|line| line.trim().is_empty()) {
        lines.pop();
    }
    lines
}
