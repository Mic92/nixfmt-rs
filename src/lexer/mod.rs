//! Hand-written lexer for Nix
//!
//! Ports the comment normalization logic from nixfmt's Lexer.hs

use crate::types::{Token, Trivia};

mod comments;
mod numbers;
mod trivia;

#[cfg(test)]
mod tests;

/// Intermediate trivia representation during parsing
#[derive(Debug, Clone)]
pub(crate) enum ParseTrivium {
    /// Multiple newlines
    Newlines(usize),
    /// Line comment with text and column position
    LineComment { text: String, col: usize },
    /// Block comment (is_doc, lines)
    BlockComment(bool, Vec<String>),
    /// Language annotation like /* lua */
    LanguageAnnotation(String),
}

/// Cursor-only snapshot of the lexer (no heap state).
#[derive(Clone, Copy)]
struct LexerPos {
    byte_pos: usize,
    line: usize,
    column: usize,
}

/// Saved lexer state for backtracking
#[derive(Clone)]
pub(crate) struct LexerState {
    byte_pos: usize,
    line: usize,
    column: usize,
    trivia_buffer: Trivia,
    recent_newlines: usize,
    recent_hspace: usize,
}

pub(crate) struct Lexer {
    /// Original source. The lexer scans it byte-wise for ASCII tokens and
    /// only decodes UTF-8 at the cursor when a multi-byte char is observed,
    /// avoiding the up-front `Vec<char>` materialisation.
    source: Box<str>,
    /// Byte offset of the cursor; always on a UTF-8 char boundary.
    byte_pos: usize,
    line: usize,
    pub(crate) column: usize,
    /// Accumulated leading trivia for next token
    pub(crate) trivia_buffer: Trivia,
    pub(crate) recent_newlines: usize,
    pub(crate) recent_hspace: usize,
    /// Position before last `parse_trivia()` call, for rewinding.
    /// Kept as a single value so the four cursor components can never
    /// drift out of sync (previously four independent `Option`s).
    trivia_start: Option<LexerPos>,
    /// Scratch buffer reused by `parse_trivia` so the per-token trivia list
    /// does not allocate on every call.
    trivia_scratch: Vec<ParseTrivium>,
}

impl Lexer {
    pub(crate) fn new(source: &str) -> Self {
        Lexer {
            source: source.into(),
            byte_pos: 0,
            line: 1,
            column: 0,
            trivia_buffer: Trivia::new(),
            recent_newlines: 0,
            recent_hspace: 0,
            trivia_start: None,
            trivia_scratch: Vec::new(),
        }
    }

    /// Save current state for backtracking
    pub(crate) fn save_state(&self) -> LexerState {
        LexerState {
            byte_pos: self.byte_pos,
            line: self.line,
            column: self.column,
            trivia_buffer: self.trivia_buffer.clone(),
            recent_newlines: self.recent_newlines,
            recent_hspace: self.recent_hspace,
        }
    }

    /// Restore saved state
    pub(crate) fn restore_state(&mut self, state: LexerState) {
        self.byte_pos = state.byte_pos;
        self.line = state.line;
        self.column = state.column;
        self.trivia_buffer = state.trivia_buffer;
        self.recent_newlines = state.recent_newlines;
        self.recent_hspace = state.recent_hspace;
    }

    /// Parse a lexeme (token with trivia annotations)
    /// This is the main entry point for the parser
    pub(crate) fn lexeme(&mut self) -> crate::error::Result<crate::types::Ann<Token>> {
        let mut leading_trivia = std::mem::take(&mut self.trivia_buffer);

        let _ = self.skip_hspace();

        // Re-sync: when entering expression mode mid-source (after `${` in a
        // string), the lexer has not yet consumed the trivia before the first
        // body token. There is no preceding Nix token here, so treat all of it
        // as leading trivia rather than splitting off a discarded "trailing".
        if matches!(self.peek_byte(), Some(b'\n' | b'\r' | b'#' | b'/')) {
            self.parse_trivia();
            leading_trivia.extend(trivia::convert_leading(&self.trivia_scratch));
            let _ = self.skip_hspace();
        }

        let token_start = self.byte_pos;
        let start_line = self.line;

        // next_token() also skips hspace; redundant here but harmless.
        let token = self.next_token()?;

        let token_end = self.byte_pos;
        let end_line = self.line;
        let token_span =
            crate::types::Span::with_lines(token_start, token_end, start_line, end_line);

        // String/path delimiters: defer trivia so the parser sees raw source content.
        let skip_trivia = matches!(token, Token::TDoubleQuote | Token::TDoubleSingleQuote);

        let trailing_comment;
        if skip_trivia {
            trailing_comment = None;
            self.trivia_buffer = Trivia::new();
        } else if let Some(newlines) = self.fast_ws_trivia() {
            // Fast path hit: only whitespace between this token and the next.
            trailing_comment = None;
            self.trivia_buffer = if newlines > 1 {
                Trivia::one(crate::types::Trivium::EmptyLine())
            } else {
                Trivia::new()
            };
        } else {
            self.parse_trivia();
            let (tc, next) = trivia::convert_trivia(&self.trivia_scratch, self.column);
            trailing_comment = tc;
            self.trivia_buffer = next;
        }

        Ok(crate::types::Ann {
            pre_trivia: leading_trivia,
            span: token_span,
            value: token,
            trail_comment: trailing_comment,
        })
    }

    /// Parse a whole file (expression + final trivia)
    pub(crate) fn start_parse(&mut self) -> crate::error::Result<()> {
        self.parse_trivia();
        self.trivia_buffer = trivia::convert_leading(&self.trivia_scratch);
        Ok(())
    }

    /// Parse trivia and classify it into `(trailing, next_leading)` so the
    /// parser does not need direct access to the scratch buffer.
    pub(crate) fn parse_and_convert_trivia(
        &mut self,
    ) -> (Option<crate::types::TrailingComment>, Trivia) {
        self.parse_trivia();
        trivia::convert_trivia(&self.trivia_scratch, self.column)
    }

    /// Get current position as a zero-length span (in byte offsets)
    pub(crate) fn current_pos(&self) -> crate::types::Span {
        crate::types::Span::point(self.byte_pos)
    }

    /// Parse next token (without trivia handling)
    /// Trivia should ONLY be managed by lexeme(), not by this function.
    /// This matches Haskell nixfmt's `rawSymbol` which parses tokens without trivia.
    pub(super) fn next_token(&mut self) -> crate::error::Result<Token> {
        let _ = self.skip_hspace();

        let Some(b) = self.peek_byte() else {
            return Ok(Token::Sof); // Use SOF as EOF token
        };
        // All token-start characters are ASCII; non-ASCII falls through to the
        // error arm which decodes the full codepoint for the message.
        let ch = b as char;

        // Nix identifiers are ASCII-only: [a-zA-Z_][a-zA-Z0-9_'-]*. Must be
        // checked before the punctuation match below.
        if ch.is_ascii_alphabetic() || ch == '_' {
            return self.parse_ident_or_keyword();
        }

        match ch {
            '{' => {
                self.advance();
                Ok(Token::TBraceOpen)
            }
            '}' => {
                self.advance();
                Ok(Token::TBraceClose)
            }
            '[' => {
                self.advance();
                Ok(Token::TBrackOpen)
            }
            ']' => {
                self.advance();
                Ok(Token::TBrackClose)
            }
            '(' => {
                self.advance();
                Ok(Token::TParenOpen)
            }
            ')' => {
                self.advance();
                Ok(Token::TParenClose)
            }
            '=' => Ok(self.try_two_char('=', Token::TEqual, Token::TAssign)),
            '@' => {
                self.advance();
                Ok(Token::TAt)
            }
            ':' => {
                self.advance();
                Ok(Token::TColon)
            }
            ',' => {
                self.advance();
                Ok(Token::TComma)
            }
            ';' => {
                self.advance();
                Ok(Token::TSemicolon)
            }
            '?' => {
                self.advance();
                Ok(Token::TQuestion)
            }
            '.' => {
                if self.at("...") {
                    self.advance_by(3);
                    Ok(Token::TEllipsis)
                } else if self.peek_ahead(1).is_some_and(|c| c.is_ascii_digit()) {
                    self.advance();
                    let mut num = String::from(".");
                    num.push_str(&self.consume_digits());

                    if let Some(exp) = self.parse_exponent() {
                        num.push_str(&exp);
                    }

                    Ok(Token::Float(num.into()))
                } else {
                    self.advance();
                    Ok(Token::TDot)
                }
            }
            '+' => Ok(self.try_two_char('+', Token::TConcat, Token::TPlus)),
            '-' => Ok(self.try_two_char('>', Token::TImplies, Token::TMinus)),
            '*' => {
                self.advance();
                Ok(Token::TMul)
            }
            '/' => Ok(self.try_two_char('/', Token::TUpdate, Token::TDiv)),
            '!' => Ok(self.try_two_char('=', Token::TUnequal, Token::TNot)),
            '<' => {
                if self.peek_ahead(1).is_some_and(|c| c.is_alphanumeric()) {
                    self.parse_env_path()
                } else {
                    self.advance();
                    match self.peek() {
                        Some('=') => {
                            self.advance();
                            Ok(Token::TLessEqual)
                        }
                        Some('|') => {
                            self.advance();
                            Ok(Token::TPipeBackward)
                        }
                        _ => Ok(Token::TLess),
                    }
                }
            }
            '>' => Ok(self.try_two_char('=', Token::TGreaterEqual, Token::TGreater)),
            '&' => {
                self.advance();
                if self.peek() == Some('&') {
                    self.advance();
                    Ok(Token::TAnd)
                } else {
                    self.err_unexpected(&["'&&'"], "'&'")
                }
            }
            '|' => {
                self.advance();
                match self.peek() {
                    Some('|') => {
                        self.advance();
                        Ok(Token::TOr)
                    }
                    Some('>') => {
                        self.advance();
                        Ok(Token::TPipeForward)
                    }
                    _ => self.err_unexpected(&["'||'", "'|>'"], "'|'"),
                }
            }
            '"' => {
                self.advance();
                Ok(Token::TDoubleQuote)
            }
            '\'' => {
                if self.at("''") {
                    self.advance_by(2);
                    Ok(Token::TDoubleSingleQuote)
                } else {
                    self.err_unexpected(&["''"], "'")
                }
            }
            '$' => {
                if self.at("${") {
                    self.advance_by(2);
                    Ok(Token::TInterOpen)
                } else {
                    self.err_unexpected(&["'${'"], "'$'")
                }
            }
            '0'..='9' => self.parse_number(),
            '~' => {
                self.advance();
                Ok(Token::TTilde)
            }
            _ => {
                // `ch` was derived from a single byte; for the error message
                // decode the actual codepoint so multi-byte input is reported
                // correctly.
                let ch = self.peek().unwrap();
                self.err_unexpected(&[], &format!("'{}'", ch))
            }
        }
    }

    /// Parse identifier or keyword
    fn parse_ident_or_keyword(&mut self) -> crate::error::Result<Token> {
        let start_byte = self.byte_pos;
        let bytes = self.source.as_bytes();

        // Nix identifiers are ASCII-only: [a-zA-Z_][a-zA-Z0-9_'-]*, so every
        // accepted byte is a full char and cannot be a newline.
        let mut i = self.byte_pos;
        while i < bytes.len() {
            let b = bytes[i];
            if b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'\'') {
                i += 1;
            } else {
                break;
            }
        }
        let len = i - start_byte;
        self.byte_pos = i;
        self.column += len;

        let text = &self.source[start_byte..i];

        // First-byte + length dispatch keeps the common "not a keyword" path
        // to a single comparison instead of up to nine `memcmp`s.
        let token = match (len, bytes[start_byte]) {
            (6, b'a') if text == "assert" => Token::KAssert,
            (4, b'e') if text == "else" => Token::KElse,
            (2, b'i') if text == "if" => Token::KIf,
            (2, b'i') if text == "in" => Token::KIn,
            (7, b'i') if text == "inherit" => Token::KInherit,
            (3, b'l') if text == "let" => Token::KLet,
            (3, b'r') if text == "rec" => Token::KRec,
            (4, b't') if text == "then" => Token::KThen,
            (4, b'w') if text == "with" => Token::KWith,
            _ => Token::Identifier(text.into()),
        };

        Ok(token)
    }

    /// Parse angle bracket path: <nixpkgs>
    fn parse_env_path(&mut self) -> crate::error::Result<Token> {
        let opening_span = self.current_pos();
        self.advance(); // consume '<'

        let mut path = String::new();
        while let Some(ch) = self.peek() {
            match ch {
                '>' => {
                    self.advance();
                    return Ok(Token::EnvPath(path.into()));
                }
                _ if ch.is_alphanumeric() || matches!(ch, '_' | '-' | '/' | '.') => {
                    path.push(self.advance().unwrap());
                }
                _ => {
                    return Err(Box::new(crate::error::ParseError {
                        span: self.current_pos(),
                        kind: crate::error::ErrorKind::InvalidSyntax {
                            description: format!("invalid character '{}' in path", ch),
                            hint: Some("paths can only contain alphanumeric characters, '.', '_', '-', and '/'".to_string()),
                        },
                        labels: vec![],
                    }))
                }
            }
        }

        Err(Box::new(crate::error::ParseError {
            span: self.current_pos(),
            kind: crate::error::ErrorKind::UnclosedDelimiter {
                delimiter: '<',
                opening_span,
            },
            labels: vec![],
        }))
    }

    /// Build an `UnexpectedToken` error at the current cursor.
    #[cold]
    fn err_unexpected<T>(&self, expected: &[&str], found: &str) -> crate::error::Result<T> {
        Err(Box::new(crate::error::ParseError {
            span: self.current_pos(),
            kind: crate::error::ErrorKind::UnexpectedToken {
                expected: expected.iter().map(|s| s.to_string()).collect(),
                found: found.to_string(),
            },
            labels: vec![],
        }))
    }

    /// Helper for two-character tokens: advance and check if next char matches
    /// Returns if_match if second char matches, otherwise if_single
    fn try_two_char(&mut self, second: char, if_match: Token, if_single: Token) -> Token {
        self.advance();
        if self.peek() == Some(second) {
            self.advance();
            if_match
        } else {
            if_single
        }
    }

    /// Remaining input from the cursor.
    #[inline(always)]
    fn rest(&self) -> &str {
        // `byte_pos` is always on a char boundary.
        unsafe { self.source.get_unchecked(self.byte_pos..) }
    }

    /// Peek at current byte without consuming (None at EOF).
    #[inline(always)]
    pub(crate) fn peek_byte(&self) -> Option<u8> {
        self.source.as_bytes().get(self.byte_pos).copied()
    }

    /// Peek at current character without consuming
    #[inline(always)]
    pub(crate) fn peek(&self) -> Option<char> {
        let b = self.peek_byte()?;
        if b < 0x80 {
            Some(b as char)
        } else {
            self.rest().chars().next()
        }
    }

    /// Peek ahead n characters
    #[inline]
    pub(crate) fn peek_ahead(&self, n: usize) -> Option<char> {
        // `n` is at most 3 in practice, so a short char walk is fine.
        self.rest().chars().nth(n)
    }

    /// Check whether the upcoming input matches `s` byte-for-byte.
    /// Replaces open-coded `peek() == Some(a) && peek_ahead(1) == Some(b)` ladders.
    #[inline(always)]
    pub(crate) fn at(&self, s: &str) -> bool {
        self.source.as_bytes()[self.byte_pos..].starts_with(s.as_bytes())
    }

    /// Advance `n` characters.
    #[inline]
    pub(crate) fn advance_by(&mut self, n: usize) {
        for _ in 0..n {
            self.advance();
        }
    }

    /// Snapshot the cursor (position only, no trivia).
    #[inline]
    fn mark(&self) -> LexerPos {
        LexerPos {
            byte_pos: self.byte_pos,
            line: self.line,
            column: self.column,
        }
    }

    /// Restore the cursor from a snapshot taken by `mark()`.
    #[inline]
    fn reset(&mut self, mark: LexerPos) {
        self.byte_pos = mark.byte_pos;
        self.line = mark.line;
        self.column = mark.column;
    }

    /// Consume and return current character
    #[inline(always)]
    pub(crate) fn advance(&mut self) -> Option<char> {
        let b = self.peek_byte()?;
        if b < 0x80 {
            self.byte_pos += 1;
            if b == b'\n' {
                self.line += 1;
                self.column = 0;
            } else {
                self.column += 1;
            }
            Some(b as char)
        } else {
            let ch = self.rest().chars().next()?;
            self.byte_pos += ch.len_utf8();
            self.column += 1;
            Some(ch)
        }
    }

    /// Advance past the longest prefix containing none of the three given
    /// bytes and return it. Newlines inside the run update `line`/`column`.
    /// SIMD-accelerated via `memchr3`, used for string-body scanning.
    #[inline]
    pub(crate) fn scan_until3(&mut self, a: u8, b: u8, c: u8) -> &str {
        let rest = &self.source.as_bytes()[self.byte_pos..];
        let len = memchr::memchr3(a, b, c, rest).unwrap_or(rest.len());
        if len == 0 {
            return "";
        }
        let start = self.byte_pos;
        let end = start + len;
        let slice = &self.source[start..end];
        self.byte_pos = end;
        // Maintain line/column. Typical string content has no newlines on the
        // indented path (caller passes `b'\n'` as a stop byte) and short runs
        // on the simple path, so prefer the no-newline ASCII fast path.
        match memchr::memrchr(b'\n', slice.as_bytes()) {
            None => {
                self.column += if slice.is_ascii() {
                    len
                } else {
                    slice.chars().count()
                };
            }
            Some(last_nl) => {
                self.line += memchr::memchr_iter(b'\n', slice.as_bytes()).count();
                let tail = &slice[last_nl + 1..];
                self.column = if tail.is_ascii() {
                    tail.len()
                } else {
                    tail.chars().count()
                };
            }
        }
        slice
    }

    /// Move the cursor to absolute byte offset `target` (which must be on a
    /// char boundary and `>= self.byte_pos`), updating `line`/`column` from
    /// the skipped slice. Used after a `memchr` jump.
    pub(super) fn seek_to(&mut self, target: usize) {
        debug_assert!(target >= self.byte_pos);
        let slice = &self.source[self.byte_pos..target];
        match memchr::memrchr(b'\n', slice.as_bytes()) {
            None => {
                self.column += if slice.is_ascii() {
                    slice.len()
                } else {
                    slice.chars().count()
                };
            }
            Some(last_nl) => {
                self.line += memchr::memchr_iter(b'\n', slice.as_bytes()).count();
                let tail = &slice[last_nl + 1..];
                self.column = if tail.is_ascii() {
                    tail.len()
                } else {
                    tail.chars().count()
                };
            }
        }
        self.byte_pos = target;
    }

    /// Bulk-advance over the next `len` bytes of source, which must contain no
    /// `\n`. Updates `column` by the number of *chars* in that slice.
    /// Returns the consumed text. Used by string/comment scanners after a
    /// `memchr` hit so the per-char `advance()` loop is skipped for the run.
    #[inline]
    pub(super) fn advance_bytes_no_newline(&mut self, len: usize) -> &str {
        let start = self.byte_pos;
        let end = start + len;
        let slice = &self.source[start..end];
        debug_assert!(!slice.as_bytes().contains(&b'\n'));
        self.byte_pos = end;
        // Nix source is overwhelmingly ASCII; only count chars when it isn't.
        self.column += if slice.is_ascii() {
            len
        } else {
            slice.chars().count()
        };
        slice
    }

    /// Check if we're at end of input
    #[inline(always)]
    fn is_eof(&self) -> bool {
        self.byte_pos >= self.source.len()
    }

    /// Skip horizontal whitespace (spaces and tabs, but not newlines)
    #[inline]
    fn skip_hspace(&mut self) -> usize {
        let bytes = self.source.as_bytes();
        let start = self.byte_pos;
        let mut i = start;
        while i < bytes.len() && matches!(bytes[i], b' ' | b'\t') {
            i += 1;
        }
        let n = i - start;
        self.byte_pos = i;
        self.column += n;
        n
    }

    /// Consume trivia when it is purely horizontal/vertical whitespace.
    /// Returns `Some(newlines)` and leaves the cursor on the next token if no
    /// `#` / `/*` was encountered; returns `None` *without consuming anything*
    /// otherwise so the slow `parse_trivia` can handle comments.
    ///
    /// This is the overwhelmingly common inter-token case and lets `lexeme`
    /// skip both the scratch-vector bookkeeping and `convert_trivia`.
    #[inline]
    fn fast_ws_trivia(&mut self) -> Option<usize> {
        let bytes = self.source.as_bytes();
        let mut i = self.byte_pos;
        let mut newlines = 0usize;
        let mut last_hspace = 0usize;
        let mut line = self.line;
        while i < bytes.len() {
            match bytes[i] {
                b' ' | b'\t' => {
                    i += 1;
                    last_hspace += 1;
                }
                b'\n' => {
                    i += 1;
                    newlines += 1;
                    line += 1;
                    last_hspace = 0;
                }
                // Comment start (or rare `\r`): bail out to the full path.
                b'#' | b'\r' => return None,
                b'/' if bytes.get(i + 1) == Some(&b'*') => return None,
                _ => break,
            }
        }
        self.trivia_start = Some(self.mark());
        if newlines > 0 {
            self.line = line;
            self.column = last_hspace;
        } else {
            self.column += last_hspace;
        }
        self.byte_pos = i;
        self.recent_newlines = newlines;
        self.recent_hspace = last_hspace;
        Some(newlines)
    }

    /// Parse trivia (comments and whitespace) into `self.trivia_scratch`.
    fn parse_trivia(&mut self) {
        // Save position before parsing trivia, so we can rewind if needed
        self.trivia_start = Some(self.mark());

        self.trivia_scratch.clear();
        self.recent_newlines = 0;
        self.recent_hspace = 0;

        loop {
            let hspace = self.skip_hspace();
            self.recent_hspace = hspace;

            if self.is_eof() {
                break;
            }

            match self.peek() {
                Some('\n') | Some('\r') => {
                    let count = self.parse_newlines();
                    self.recent_newlines = count;
                    self.trivia_scratch.push(ParseTrivium::Newlines(count));
                }
                Some('#') => {
                    let c = self.parse_line_comment();
                    self.trivia_scratch.push(c);
                }
                Some('/') if self.at("/*") => {
                    let saved_state = self.save_state();

                    if let Some(lang_annot) = self.try_parse_language_annotation() {
                        self.trivia_scratch.push(lang_annot);
                    } else {
                        self.restore_state(saved_state);
                        let c = self.parse_block_comment();
                        self.trivia_scratch.push(c);
                    }
                }
                _ => break,
            }
        }
    }

    /// Parse consecutive newlines, return count
    fn parse_newlines(&mut self) -> usize {
        let bytes = self.source.as_bytes();
        let mut count = 0;
        while self.byte_pos < bytes.len() {
            match bytes[self.byte_pos] {
                b'\n' => {
                    self.byte_pos += 1;
                    self.line += 1;
                    self.column = 0;
                    count += 1;
                }
                b'\r' => {
                    self.byte_pos += 1;
                    self.column += 1;
                    if bytes.get(self.byte_pos) == Some(&b'\n') {
                        self.byte_pos += 1;
                        self.line += 1;
                        self.column = 0;
                    }
                    count += 1;
                }
                _ => break,
            }
        }
        count
    }

    /// Rewind the last trivia consumed (horizontal spaces, newlines, and comments)
    /// Also clears the trivia buffer since rewound trivia should not be attached to next token
    pub(crate) fn rewind_trivia(&mut self) {
        if let Some(mark) = self.trivia_start {
            self.reset(mark);
        }

        self.recent_hspace = 0;
        self.recent_newlines = 0;
        self.trivia_buffer.clear();
    }
}
