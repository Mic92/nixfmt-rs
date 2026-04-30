//! String parsing utilities
//!
//! This module handles parsing of Nix strings (both simple "..." and indented ''...'' strings),
//! including interpolations, escape sequences, and string normalization (splitting on newlines,
//! merging adjacent text parts, stripping common indentation).

use crate::error::{ErrorKind, ParseError, Result};
use crate::types::*;

use super::Parser;

impl Parser {
    /// Parse simple string literal and return annotated string structure
    pub(super) fn parse_simple_string_literal(&mut self) -> Result<Ann<Vec<Vec<StringPart>>>> {
        let open_quote_pos = self.current.span;
        let pre_trivia = std::mem::take(&mut self.current.pre_trivia);

        // DON'T advance - just verify we're at a quote
        if !matches!(self.current.value, Token::TDoubleQuote) {
            return Err(ParseError::unexpected(
                open_quote_pos,
                vec!["'\"'".to_string()],
                format!("'{}'", self.current.value.text()),
            ));
        }

        let _opening_quote = self.take_current();
        // DON'T call advance() - parse raw characters directly

        let mut parts = Vec::new();

        loop {
            match self.lexer.peek() {
                Some('"') => break,
                None => {
                    return Err(ParseError::unclosed(
                        self.lexer.current_pos(),
                        '"',
                        open_quote_pos,
                    ));
                }
                Some('$') if self.lexer.at("${") => {
                    let interp = self.parse_string_interpolation()?;
                    parts.push(interp);
                }
                _ => {
                    let text = self.parse_simple_string_part()?;
                    if !text.is_empty() {
                        parts.push(StringPart::TextPart(text));
                    }
                }
            }
        }

        self.lexer.advance();

        let trail_comment = self.parse_trailing_trivia_and_advance()?;
        let lines = process_simple(parts);

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
            // Bulk-copy the run up to the next byte that needs special
            // handling. This is the hot path for ordinary string content.
            let run = self.lexer.scan_until3(b'"', b'\\', b'$');
            if !run.is_empty() {
                text.push_str(run);
            }
            match self.lexer.peek() {
                Some('"') | None => break,
                Some('$') if self.lexer.at("${") => break,
                Some('\\') => {
                    self.lexer.advance();
                    match self.lexer.peek() {
                        Some('n') => {
                            text.push_str("\\n");
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
                            text.push('\\');
                            text.push(ch);
                            self.lexer.advance();
                        }
                        None => break,
                    }
                }
                Some('$') if self.lexer.at("$$") => {
                    text.push_str("$$");
                    self.lexer.advance_by(2);
                }
                Some('$') => {
                    text.push('$');
                    self.lexer.advance();
                }
                // Any other byte was consumed by `scan_until3`.
                Some(_) => unreachable!(),
            }
        }

        Ok(text)
    }

    /// Parse string interpolation: ${expr}
    pub(super) fn parse_string_interpolation(&mut self) -> Result<StringPart> {
        self.lexer.advance_by(2);

        // Re-sync parser
        self.current = self.lexer.lexeme()?;

        if matches!(self.current.value, Token::TBraceClose) {
            return Err(ParseError::invalid(
                self.current.span,
                "empty interpolation expression",
                Some("string interpolations require an expression inside ${...}".to_string()),
            ));
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
                    return Err(ParseError::unexpected(
                        *opening_span,
                        vec!["'}'".to_string()],
                        "'\"'",
                    ));
                }
                return Err(err);
            }
        };

        if !matches!(self.current.value, Token::TBraceClose) {
            return Err(ParseError::unexpected(
                self.current.span,
                vec!["'}'".to_string()],
                format!("'{}'", self.current.value.text()),
            ));
        }

        // The lexer is positioned past `}` and any following trivia; rewind to
        // immediately after `}` so the string scanner resumes on raw content.
        let trailing_trivia = std::mem::take(&mut self.current.pre_trivia);
        self.lexer.rewind_trivia();
        Ok(StringPart::Interpolation(Box::new(Whole {
            value: expr,
            trailing_trivia,
        })))
    }

    /// Parse indented string: ''...''
    /// Based on Haskell's indentedString parser
    pub(super) fn parse_indented_string(&mut self) -> Result<Term> {
        let open_quote_pos = self.current.span;
        let pre_trivia = std::mem::take(&mut self.current.pre_trivia);

        // Take the opening '' token (don't advance - just take it)
        let _opening = self.take_current();

        let mut lines = Vec::new();
        lines.push(self.parse_indented_string_line()?);

        while self.lexer.peek() == Some('\n') {
            self.lexer.advance();
            lines.push(self.parse_indented_string_line()?);
        }

        if !self.lexer.at("''") {
            // delimiter `'` represents the opening `''`
            return Err(ParseError::unclosed(
                self.lexer.current_pos(),
                '\'',
                open_quote_pos,
            ));
        }
        self.lexer.advance_by(2);

        let trail_comment = self.parse_trailing_trivia_and_advance()?;
        let lines = process_indented(lines);

        let ann = Ann {
            pre_trivia,
            span: open_quote_pos,
            value: lines,
            trail_comment,
        };

        Ok(classify_indented_string(ann))
    }

    /// Parse one line of an indented string
    /// Based on Haskell's indentedLine
    fn parse_indented_string_line(&mut self) -> Result<Vec<StringPart>> {
        let mut parts = Vec::new();

        loop {
            match self.lexer.peek() {
                Some('\'') if self.lexer.at("''") => {
                    // Could be end or escape
                    if matches!(
                        self.lexer.peek_ahead(2),
                        Some('$') | Some('\'') | Some('\\')
                    ) {
                        let text = self.parse_indented_string_part()?;
                        if !text.is_empty() {
                            parts.push(StringPart::TextPart(text));
                        }
                    } else {
                        break;
                    }
                }
                Some('$') if self.lexer.at("${") => {
                    let interp = self.parse_string_interpolation()?;
                    parts.push(interp);
                }
                Some('\n') | None => break,
                _ => {
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
            // `\n`, `'` and `$` are the only bytes that change control flow.
            let run = self.lexer.scan_until3(b'\n', b'\'', b'$');
            if !run.is_empty() {
                text.push_str(run);
            }
            match self.lexer.peek() {
                None | Some('\n') => break,
                Some('\'') if self.lexer.at("''") => {
                    match self.lexer.peek_ahead(2) {
                        Some('$') => {
                            // ''$ -> $
                            text.push_str("''$");
                            self.lexer.advance_by(3);
                        }
                        Some('\'') => {
                            // ''' -> '
                            text.push_str("'''");
                            self.lexer.advance_by(3);
                        }
                        Some('\\') => {
                            // ''\ escapes next char
                            text.push_str("''\\");
                            self.lexer.advance_by(3);
                            // Leave a following '\n' for the line loop so the
                            // next line is visible to common-indent stripping
                            // (matches Haskell `indentedStringPart`).
                            match self.lexer.peek() {
                                None | Some('\n') => {}
                                Some(ch) => {
                                    text.push(ch);
                                    self.lexer.advance();
                                }
                            }
                        }
                        _ => {
                            // Not an escape, end of string
                            break;
                        }
                    }
                }
                Some('$') if self.lexer.at("${") => break,
                Some('$') if self.lexer.at("$$") => {
                    text.push_str("$$");
                    self.lexer.advance_by(2);
                }
                Some('$') => {
                    text.push('$');
                    self.lexer.advance();
                }
                Some('\'') => {
                    // single `'` (the `''` case matched above)
                    text.push('\'');
                    self.lexer.advance();
                }
                // Any other byte was consumed by `scan_until3`.
                Some(_) => unreachable!(),
            }
        }

        Ok(text)
    }

    /// Parse selector interpolation: ${expr} within attribute paths
    pub(super) fn parse_selector_interpolation(&mut self) -> Result<Ann<StringPart>> {
        let mut open = self.take_current();
        debug_assert!(matches!(open.value, Token::TInterOpen));
        // Haskell parses `${` with `rawSymbol`, so a comment immediately after
        // it becomes leading trivia of the body's first token. Our `${` came
        // through `lexeme()`, which classified that comment as `trail_comment`;
        // re-queue it as leading trivia so it is not dropped.
        if let Some(tc) = open.trail_comment.take() {
            self.lexer
                .trivia_buffer
                .insert(0, Trivium::LineComment(format!(" {}", tc.0)));
        }
        self.advance()?;

        let expr = self.parse_expression()?;
        let close = self.expect_token(Token::TBraceClose, "'}'")?;

        Ok(Ann {
            pre_trivia: open.pre_trivia,
            span: open.span,
            value: StringPart::Interpolation(Box::new(Whole {
                value: expr,
                trailing_trivia: close.pre_trivia,
            })),
            trail_comment: close.trail_comment,
        })
    }
}

// String processing utilities
//
// These functions handle normalization of both simple ("...") and indented (''...'')
// strings in Nix, including splitting on newlines, merging adjacent text parts, and
// stripping common indentation for multi-line strings.

/// Process a simple string by splitting on newlines and merging adjacent text
///
/// Simple strings ("...") only need newline splitting and normalization,
/// without the indentation handling required for multi-line strings.
fn process_simple(parts: Vec<StringPart>) -> Vec<Vec<StringPart>> {
    split_on_newlines(parts)
        .into_iter()
        .map(merge_adjacent_text)
        .collect()
}

/// Reclassify a parsed indented string as a `SimpleString` when its content is
/// representable in `"..."` syntax. Mirrors `classifyString` in nixfmt's Parser.hs.
fn classify_indented_string(ann: Ann<Vec<Vec<StringPart>>>) -> Term {
    fn has_quote_or_backslash(part: &StringPart) -> bool {
        match part {
            StringPart::TextPart(t) => t.contains('"') || t.contains('\\'),
            StringPart::Interpolation(_) => false,
        }
    }

    // More than one line means the original literal contained a newline.
    let should_be_simple =
        ann.value.len() <= 1 && !ann.value.iter().flatten().any(has_quote_or_backslash);

    if !should_be_simple {
        return Term::IndentedString(ann);
    }

    fn convert_escapes(t: &str) -> String {
        let mut out = String::with_capacity(t.len());
        let bytes = t.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i..].starts_with(b"''$") {
                out.push_str("\\$");
                i += 3;
            } else if bytes[i..].starts_with(b"'''") {
                out.push_str("''");
                i += 3;
            } else {
                // Safe: input is valid UTF-8 and we only ever skip whole ASCII prefixes above.
                let ch = t[i..].chars().next().unwrap();
                out.push(ch);
                i += ch.len_utf8();
            }
        }
        out
    }

    let value = ann
        .value
        .into_iter()
        .map(|line| {
            line.into_iter()
                .map(|part| match part {
                    StringPart::TextPart(t) => StringPart::TextPart(convert_escapes(&t)),
                    interp @ StringPart::Interpolation(_) => interp,
                })
                .collect()
        })
        .collect();

    Term::SimpleString(Ann { value, ..ann })
}

/// Process an indented string by normalizing whitespace and stripping common indentation
///
/// This is the main entry point for processing Nix indented strings (''...''). It:
/// 1. Removes empty first/last lines
/// 2. Strips common indentation from all lines
/// 3. Splits text parts on newlines
/// 4. Normalizes adjacent text parts
fn process_indented(lines: Vec<Vec<StringPart>>) -> Vec<Vec<StringPart>> {
    let lines = remove_empty_first_line(lines);
    let lines = remove_empty_last_line(lines);
    let lines = strip_common_indentation(lines);
    let lines: Vec<_> = lines.into_iter().flat_map(split_on_newlines).collect();
    lines.into_iter().map(merge_adjacent_text).collect()
}

/// Split text parts on newlines, creating separate lines
fn split_on_newlines(parts: Vec<StringPart>) -> Vec<Vec<StringPart>> {
    let mut result: Vec<Vec<StringPart>> = Vec::new();
    let mut current: Vec<StringPart> = Vec::new();

    for part in parts {
        match part {
            StringPart::TextPart(text) => {
                let mut remaining = text.as_str();
                loop {
                    if let Some(pos) = remaining.find('\n') {
                        let segment = &remaining[..pos];
                        if !segment.is_empty() {
                            current.push(StringPart::TextPart(segment.to_string()));
                        }
                        result.push(current);
                        current = Vec::new();
                        remaining = &remaining[pos + 1..];
                    } else {
                        if !remaining.is_empty() {
                            current.push(StringPart::TextPart(remaining.to_string()));
                        }
                        break;
                    }
                }
            }
            other => current.push(other),
        }
    }

    result.push(current);
    result
}

/// Merge adjacent TextPart elements into a single TextPart
fn merge_adjacent_text(line: Vec<StringPart>) -> Vec<StringPart> {
    let mut result: Vec<StringPart> = Vec::new();
    for part in line {
        match part {
            StringPart::TextPart(text) => {
                if text.is_empty() {
                    continue;
                }
                if let Some(StringPart::TextPart(existing)) = result.last_mut() {
                    existing.push_str(&text);
                } else {
                    result.push(StringPart::TextPart(text));
                }
            }
            other => result.push(other),
        }
    }
    result
}

/// Check if a string contains only spaces
fn is_only_spaces(text: &str) -> bool {
    text.bytes().all(|b| b == b' ')
}

/// Check if a line is effectively empty (no parts or only spaces)
fn is_empty_line(line: &[StringPart]) -> bool {
    line.is_empty() || matches!(line, [StringPart::TextPart(text)] if is_only_spaces(text))
}

/// Remove the first line if it's empty or contains only spaces
fn remove_empty_first_line(mut lines: Vec<Vec<StringPart>>) -> Vec<Vec<StringPart>> {
    if let Some(first_line) = lines.first_mut() {
        let first = merge_adjacent_text(std::mem::take(first_line));
        if is_empty_line(&first) && lines.len() > 1 {
            lines.remove(0);
        } else {
            lines[0] = first;
        }
    }
    lines
}

/// Remove the last line if it's empty or contains only spaces
fn remove_empty_last_line(mut lines: Vec<Vec<StringPart>>) -> Vec<Vec<StringPart>> {
    match lines.len() {
        0 => lines,
        1 => {
            let last = merge_adjacent_text(lines.pop().unwrap());
            if is_empty_line(&last) {
                vec![Vec::new()]
            } else {
                vec![last]
            }
        }
        _ => {
            let last_index = lines.len() - 1;
            let last = merge_adjacent_text(std::mem::take(&mut lines[last_index]));
            lines[last_index] = if is_empty_line(&last) {
                Vec::new()
            } else {
                last
            };
            lines
        }
    }
}

/// Get the text content at the start of a line (for indentation calculation).
///
/// Whitespace-only lines are ignored, matching Nix: a short blank line must
/// not cap the strippable indent.
fn line_prefix(line: &[StringPart]) -> Option<String> {
    match line.first() {
        None => None,
        Some(StringPart::TextPart(text)) => {
            if line.len() == 1 && is_only_spaces(text) {
                None
            } else {
                Some(text.clone())
            }
        }
        Some(StringPart::Interpolation(_)) => Some(String::new()),
    }
}

/// Find the common leading space prefix across all lines
fn find_common_space_prefix(prefixes: Vec<String>) -> Option<String> {
    if prefixes.is_empty() {
        return None;
    }

    let mut common: String = prefixes[0].chars().take_while(|c| *c == ' ').collect();
    for prefix in prefixes.iter().skip(1) {
        let candidate: String = prefix.chars().take_while(|c| *c == ' ').collect();
        let mut new_common = String::new();
        for (a, b) in common.chars().zip(candidate.chars()) {
            if a == b {
                new_common.push(a);
            } else {
                break;
            }
        }
        common = new_common;
        if common.is_empty() {
            break;
        }
    }
    Some(common)
}

/// Strip a prefix from the first text part of a line.
///
/// A whitespace-only line shorter than the prefix is cleared (Nix treats it
/// as empty); longer ones keep their excess spaces.
fn strip_prefix_from_line(prefix: &str, mut line: Vec<StringPart>) -> Vec<StringPart> {
    if prefix.is_empty() {
        return line;
    }

    let single = line.len() == 1;
    if let Some(StringPart::TextPart(text)) = line.first_mut() {
        if let Some(stripped) = text.strip_prefix(prefix) {
            *text = stripped.to_string();
        } else if single && is_only_spaces(text) {
            return Vec::new();
        }
    }
    line
}

/// Strip common leading indentation from all lines
fn strip_common_indentation(lines: Vec<Vec<StringPart>>) -> Vec<Vec<StringPart>> {
    let prefixes: Vec<String> = lines.iter().filter_map(|line| line_prefix(line)).collect();

    match find_common_space_prefix(prefixes) {
        None => lines.into_iter().map(|_| Vec::new()).collect(),
        Some(prefix) => lines
            .into_iter()
            .map(|line| strip_prefix_from_line(&prefix, line))
            .collect(),
    }
}
