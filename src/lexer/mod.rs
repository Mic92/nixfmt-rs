//! Hand-written lexer for Nix
//!
//! Ports the comment normalization logic from nixfmt's Lexer.hs

use crate::ast::{Token, Trivia};

mod comments;
mod cursor;
mod numbers;
mod scan;
mod trivia;

#[cfg(test)]
mod tests;

/// Intermediate trivia representation during parsing
#[derive(Debug, Clone)]
pub enum RawTrivia {
    /// Multiple newlines
    Newlines(usize),
    /// Line comment with text and column position
    LineComment { text: String, col: usize },
    /// Block comment (`is_doc`, lines)
    BlockComment(bool, Vec<String>),
    /// Language annotation like /* lua */
    LanguageAnnotation(String),
}

/// Cursor-only snapshot of the lexer (no heap state).
#[derive(Clone, Copy)]
pub struct LexerPos {
    byte_pos: usize,
    line: usize,
    column: usize,
}

/// Saved lexer state for backtracking
#[derive(Clone)]
pub struct LexerState {
    byte_pos: usize,
    line: usize,
    column: usize,
    trivia_buffer: Trivia,
    recent_newlines: usize,
    recent_hspace: usize,
}

pub struct Lexer {
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
    trivia_scratch: Vec<RawTrivia>,
}

impl Lexer {
    pub(crate) fn new(source: &str) -> Self {
        Self {
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
    pub(crate) fn lexeme(&mut self) -> crate::error::Result<crate::ast::Annotated<Token>> {
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
        let token_span = crate::ast::Span::with_lines(token_start, token_end, start_line, end_line);

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
                Trivia::one(crate::ast::TriviaPiece::EmptyLine())
            } else {
                Trivia::new()
            };
        } else {
            self.parse_trivia();
            let (tc, next) = trivia::convert_trivia(&self.trivia_scratch, self.column);
            trailing_comment = tc;
            self.trivia_buffer = next;
        }

        Ok(crate::ast::Annotated {
            pre_trivia: leading_trivia,
            span: token_span,
            value: token,
            trail_comment: trailing_comment,
        })
    }

    /// Parse a whole file (expression + final trivia)
    pub(crate) fn start_parse(&mut self) {
        self.parse_trivia();
        self.trivia_buffer = trivia::convert_leading(&self.trivia_scratch);
    }

    /// Parse trivia and classify it into `(trailing, next_leading)` so the
    /// parser does not need direct access to the scratch buffer.
    pub(crate) fn parse_and_convert_trivia(
        &mut self,
    ) -> (Option<crate::ast::TrailingComment>, Trivia) {
        self.parse_trivia();
        trivia::convert_trivia(&self.trivia_scratch, self.column)
    }

    /// Get current position as a zero-length span (in byte offsets)
    pub(crate) const fn current_pos(&self) -> crate::ast::Span {
        crate::ast::Span::point(self.byte_pos)
    }

    /// Skip horizontal whitespace (spaces and tabs, but not newlines)
    #[inline]
    fn skip_hspace(&mut self) -> usize {
        self.take_ascii_while(|b| matches!(b, b' ' | b'\t')).len()
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
                Some('\n' | '\r') => {
                    let count = self.parse_newlines();
                    self.recent_newlines = count;
                    self.trivia_scratch.push(RawTrivia::Newlines(count));
                }
                Some('#') => {
                    let c = self.parse_line_comment();
                    self.trivia_scratch.push(c);
                }
                Some('/') if self.at("/*") => {
                    // try_parse_language_annotation already restores state on
                    // failure, so no outer save/restore is needed here.
                    if let Some(lang_annot) = self.try_parse_language_annotation() {
                        self.trivia_scratch.push(lang_annot);
                    } else {
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
        let mut count = 0;
        while self.eat_one_eol() {
            count += 1;
        }
        count
    }

    /// Consume a single end-of-line sequence (`\n`, `\r\n`, or bare `\r`).
    /// A bare `\r` advances `column` but not `line`, matching the historical
    /// behaviour of `parse_newlines`.
    #[inline]
    pub(super) fn eat_one_eol(&mut self) -> bool {
        let bytes = self.source.as_bytes();
        match bytes.get(self.byte_pos) {
            Some(&b'\n') => {
                self.byte_pos += 1;
                self.line += 1;
                self.column = 0;
                true
            }
            Some(&b'\r') => {
                self.byte_pos += 1;
                self.column += 1;
                if bytes.get(self.byte_pos) == Some(&b'\n') {
                    self.byte_pos += 1;
                    self.line += 1;
                    self.column = 0;
                }
                true
            }
            _ => false,
        }
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
