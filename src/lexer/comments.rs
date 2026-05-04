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

use super::{Lexer, RawTrivia};

impl Lexer {
    /// Parse a line comment starting with '#'
    pub(super) fn parse_line_comment(&mut self) -> RawTrivia {
        let col = self.column;
        self.advance(); // consume '#'

        let rest = &self.source.as_bytes()[self.byte_pos..];
        let len = memchr::memchr2(b'\n', b'\r', rest).unwrap_or(rest.len());
        let text = self.advance_bytes_no_newline(len).trim_end().to_owned();

        RawTrivia::LineComment { text, col }
    }

    /// Parse block comment /* ... */
    pub(super) fn parse_block_comment(&mut self) -> RawTrivia {
        let start_col = self.column;
        self.advance(); // consume '/'
        self.advance(); // consume '*'

        let is_doc = self.peek() == Some('*') && !self.at("*/");
        if is_doc {
            self.advance();
        }

        let body_start = self.byte_pos;
        let mut scan = body_start;
        let bytes = self.source.as_bytes();
        let body_end = loop {
            match memchr::memchr(b'*', &bytes[scan..]) {
                // Unterminated: consume to EOF.
                None => break bytes.len(),
                Some(off) if bytes.get(scan + off + 1) == Some(&b'/') => break scan + off,
                Some(off) => scan += off + 1,
            }
        };
        self.seek_to(body_end);
        self.advance_by(2); // `*/` (no-op at EOF)
        let body = &self.source[body_start..body_end];

        let lines = split_lines(body);
        let lines = remove_stars(start_col, lines);
        let lines = fix_indent(start_col, &lines);
        let lines = drop_while_empty_start(lines);
        let lines = drop_while_empty_end(lines);

        RawTrivia::BlockComment(is_doc, lines)
    }

    /// Try to parse a language annotation like /* lua */
    pub(super) fn try_parse_language_annotation(&mut self) -> Option<RawTrivia> {
        self.try_with_cursor(|this| {
            let pt = this.parse_block_comment();

            if let RawTrivia::BlockComment(false, lines) = &pt
                && lines.len() == 1
            {
                let content = lines[0].trim();
                if is_valid_language_identifier(content) && this.is_next_string_delimiter() {
                    return Some(RawTrivia::LanguageAnnotation(content.to_string()));
                }
            }

            None
        })
    }

    /// Check if next non-whitespace token is " or ''
    fn is_next_string_delimiter(&mut self) -> bool {
        let mark = self.mark();

        let _ = self.skip_hspace();

        // Optionally consume exactly one newline; a blank line between the
        // comment and the string disqualifies it as a language annotation.
        self.eat_one_eol();
        let _ = self.skip_hspace();

        let result = self.peek() == Some('"') || self.at("''");

        self.reset(mark);
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
/// This matches nixfmt's splitLines function which does `dropWhileEnd Text.null`.
///
/// Unlike upstream nixfmt we also split on a bare `\r`: Nix (and our lexer)
/// treats `\r` as a line terminator, so a block-comment body that contains one
/// is *not* single-line and must not be rewritten to a `#` comment, or the
/// bytes after the `\r` re-lex as code.
pub(super) fn split_lines(text: &str) -> Vec<String> {
    let lines: Vec<String> = text
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .lines()
        .map(|line| line.trim_end().to_string())
        .collect();

    // Drop trailing empty lines (matches Haskell's dropWhileEnd Text.null)
    drop_while_empty_end(lines)
}

/// Remove aligned stars from block comments (Lexer.hs:110-118)
/// If all continuation lines have " *" at position `pos`, remove them
pub(super) fn remove_stars(pos: usize, lines: Vec<String>) -> Vec<String> {
    if lines.is_empty() {
        return Vec::new();
    }

    let star_prefix = format!("{} *", " ".repeat(pos));
    let new_prefix = " ".repeat(pos);

    let all_have_star = lines[1..].iter().all(|line| line.starts_with(&star_prefix));

    if all_have_star && !lines[1..].is_empty() {
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
fn fix_indent(pos: usize, lines: &[String]) -> Vec<String> {
    if lines.is_empty() {
        return Vec::new();
    }

    let first = &lines[0];

    let offset = if first.starts_with(' ') {
        pos + 3
    } else {
        pos + 2
    };

    let common_indent = lines[1..]
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.bytes().take_while(|&b| b == b' ').count())
        .min()
        .unwrap_or(0)
        .min(offset);

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
