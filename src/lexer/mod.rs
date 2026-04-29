//! Hand-written lexer for Nix
//!
//! Ports the comment normalization logic from nixfmt's Lexer.hs

use crate::types::{Token, Trivia};

mod comments;
mod numbers;
mod trivia;
pub(crate) use trivia::convert_trivia;

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
    pos: usize,
    byte_pos: usize,
    line: usize,
    column: usize,
}

/// Saved lexer state for backtracking
#[derive(Clone)]
pub(crate) struct LexerState {
    pub(crate) pos: usize,
    pub(crate) byte_pos: usize,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) trivia_buffer: Trivia,
    pub(crate) recent_newlines: usize,
    pub(crate) recent_hspace: usize,
}

pub(crate) struct Lexer {
    pub(crate) input: Vec<char>,
    pub(crate) pos: usize,
    /// Byte offset corresponding to `pos`, kept in lockstep so span
    /// construction is O(1) instead of re-scanning the prefix per token.
    pub(crate) byte_pos: usize,
    pub(crate) line: usize,
    pub(crate) column: usize,
    /// Accumulated leading trivia for next token
    pub(crate) trivia_buffer: Trivia,
    pub(crate) recent_newlines: usize,
    pub(crate) recent_hspace: usize,
    /// Position before last `parse_trivia()` call, for rewinding.
    /// Kept as a single value so the four cursor components can never
    /// drift out of sync (previously four independent `Option`s).
    trivia_start: Option<LexerPos>,
    /// Scratch buffer reused while collecting `#` comments
    line_comment_buffer: String,
    /// Scratch buffer reused while collecting block comments
    block_comment_buffer: String,
}

impl Lexer {
    pub(crate) fn new(source: &str) -> Self {
        Lexer {
            input: source.chars().collect(),
            pos: 0,
            byte_pos: 0,
            line: 1,
            column: 0,
            trivia_buffer: Trivia::new(),
            recent_newlines: 0,
            recent_hspace: 0,
            trivia_start: None,
            line_comment_buffer: String::new(),
            block_comment_buffer: String::new(),
        }
    }

    /// Save current state for backtracking
    pub(crate) fn save_state(&self) -> LexerState {
        LexerState {
            pos: self.pos,
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
        self.pos = state.pos;
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
        // Take accumulated leading trivia from buffer
        let mut leading_trivia = std::mem::take(&mut self.trivia_buffer);

        // Skip horizontal space first
        let _ = self.skip_hspace();

        // Check for newlines/trivia at current position (can happen after string parsing + rewind)
        // Parse them and split into trailing (for previous token) and leading (for next token)
        if matches!(self.peek(), Some('\n') | Some('\r') | Some('#') | Some('/')) {
            let extra_trivia = self.parse_trivia();

            // Split into trailing and leading
            let next_col = self.column;
            let (_trailing, next_leading) = trivia::convert_trivia(extra_trivia, next_col);

            leading_trivia.extend(next_leading);

            // Skip hspace after trivia
            let _ = self.skip_hspace();
        }

        // Record start position BEFORE parsing the token (in byte offsets and line numbers)
        let token_start = self.byte_pos;
        let start_line = self.line;

        // Parse the token (note: next_token() also skips hspace, but that's ok since we already did)
        let token = self.next_token()?;

        // Record position AFTER parsing token to create span (in byte offsets and line numbers)
        let token_end = self.byte_pos;
        let end_line = self.line;
        let token_span =
            crate::types::Span::with_lines(token_start, token_end, start_line, end_line);

        // For string/path delimiters, don't parse trivia immediately
        // The parser needs to access raw source content
        let skip_trivia = matches!(token, Token::TDoubleQuote | Token::TDoubleSingleQuote);

        let (trailing_comment, next_leading) = if skip_trivia {
            // Don't parse trivia yet - parser will handle string content
            (None, Trivia::new())
        } else {
            // Parse trivia after the token
            let parsed_trivia = self.parse_trivia();

            // Get the column of the next token
            let next_col = self.column;

            // Convert trivia to (trailing_comment, next_leading_trivia)
            trivia::convert_trivia(parsed_trivia, next_col)
        };

        // Store leading trivia for next token
        self.trivia_buffer = next_leading;

        // Return annotated token
        Ok(crate::types::Ann {
            pre_trivia: leading_trivia,
            span: token_span,
            value: token,
            trail_comment: trailing_comment,
        })
    }

    /// Parse a whole file (expression + final trivia)
    pub(crate) fn start_parse(&mut self) -> crate::error::Result<()> {
        // Parse initial trivia and convert to leading
        let initial_trivia = self.parse_trivia();
        self.trivia_buffer = trivia::convert_leading(&initial_trivia);
        Ok(())
    }

    /// Get remaining trivia at end of file
    pub(crate) fn finish_parse(&mut self) -> Trivia {
        std::mem::take(&mut self.trivia_buffer)
    }

    /// Get current position as a zero-length span (in byte offsets)
    pub(crate) fn current_pos(&self) -> crate::types::Span {
        crate::types::Span::point(self.byte_pos)
    }

    /// Parse next token (without trivia handling)
    /// Trivia should ONLY be managed by lexeme(), not by this function.
    /// This matches Haskell nixfmt's `rawSymbol` which parses tokens without trivia.
    pub(crate) fn next_token(&mut self) -> crate::error::Result<Token> {
        let _ = self.skip_hspace();

        if self.is_eof() {
            return Ok(Token::Sof); // Use SOF as EOF token
        }

        let ch = self.peek().unwrap();

        // Check for identifiers/keywords (ASCII alphabetic or underscore)
        // Nix only allows ASCII letters: [a-zA-Z_][a-zA-Z0-9_'-]*
        // This must come before the match to handle checking for valid identifier starts
        if ch.is_ascii_alphabetic() || ch == '_' {
            return self.parse_ident_or_keyword();
        }

        // Single character tokens and operators
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

                    Ok(Token::Float(num))
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
                // Check for angle bracket path <nixpkgs>
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
                    Err(crate::error::ParseError {
                        span: self.current_pos(),
                        kind: crate::error::ErrorKind::UnexpectedToken {
                            expected: vec!["'&&'".to_string()],
                            found: "'&'".to_string(),
                        },
                        labels: vec![],
                    })
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
                    _ => Err(crate::error::ParseError {
                        span: self.current_pos(),
                        kind: crate::error::ErrorKind::UnexpectedToken {
                            expected: vec!["'||'".to_string(), "'|>'".to_string()],
                            found: "'|'".to_string(),
                        },
                        labels: vec![],
                    }),
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
                    Err(crate::error::ParseError {
                        span: self.current_pos(),
                        kind: crate::error::ErrorKind::UnexpectedToken {
                            expected: vec!["''".to_string()],
                            found: "'".to_string(),
                        },
                        labels: vec![],
                    })
                }
            }
            '$' => {
                if self.at("${") {
                    self.advance_by(2);
                    Ok(Token::TInterOpen)
                } else {
                    Err(crate::error::ParseError {
                        span: self.current_pos(),
                        kind: crate::error::ErrorKind::UnexpectedToken {
                            expected: vec!["'${'".to_string()],
                            found: "'$'".to_string(),
                        },
                        labels: vec![],
                    })
                }
            }
            '0'..='9' => self.parse_number(),
            '~' => {
                // Tilde - used in paths ~/
                self.advance();
                Ok(Token::TTilde)
            }
            _ => Err(crate::error::ParseError {
                span: self.current_pos(),
                kind: crate::error::ErrorKind::UnexpectedToken {
                    expected: vec![],
                    found: format!("'{}'", ch),
                },
                labels: vec![],
            }),
        }
    }

    /// Parse identifier or keyword
    fn parse_ident_or_keyword(&mut self) -> crate::error::Result<Token> {
        let mut ident = String::new();

        while let Some(ch) = self.peek() {
            // Nix identifiers must be ASCII-only: [a-zA-Z_][a-zA-Z0-9_'-]*
            // Use is_ascii_alphanumeric() instead of is_alphanumeric() to reject Unicode
            if ch.is_ascii_alphanumeric() || ch == '_' || ch == '-' || ch == '\'' {
                ident.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        // Check for keywords
        let token = match ident.as_str() {
            "assert" => Token::KAssert,
            "else" => Token::KElse,
            "if" => Token::KIf,
            "in" => Token::KIn,
            "inherit" => Token::KInherit,
            "let" => Token::KLet,
            "rec" => Token::KRec,
            "then" => Token::KThen,
            "with" => Token::KWith,
            _ => Token::Identifier(ident),
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
                    return Ok(Token::EnvPath(path));
                }
                _ if ch.is_alphanumeric() || matches!(ch, '_' | '-' | '/' | '.') => {
                    path.push(self.advance().unwrap());
                }
                _ => {
                    return Err(crate::error::ParseError {
                        span: self.current_pos(),
                        kind: crate::error::ErrorKind::InvalidSyntax {
                            description: format!("invalid character '{}' in path", ch),
                            hint: Some("paths can only contain alphanumeric characters, '.', '_', '-', and '/'".to_string()),
                        },
                        labels: vec![],
                    })
                }
            }
        }

        Err(crate::error::ParseError {
            span: self.current_pos(),
            kind: crate::error::ErrorKind::UnclosedDelimiter {
                delimiter: '<',
                opening_span,
            },
            labels: vec![],
        })
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

    /// Peek at current character without consuming
    #[inline(always)]
    pub(crate) fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    /// Peek ahead n characters
    #[inline(always)]
    pub(crate) fn peek_ahead(&self, n: usize) -> Option<char> {
        self.input.get(self.pos + n).copied()
    }

    /// Check whether the upcoming input matches `s` character-for-character.
    /// Replaces open-coded `peek() == Some(a) && peek_ahead(1) == Some(b)` ladders.
    #[inline]
    pub(crate) fn at(&self, s: &str) -> bool {
        s.chars()
            .enumerate()
            .all(|(i, c)| self.peek_ahead(i) == Some(c))
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
            pos: self.pos,
            byte_pos: self.byte_pos,
            line: self.line,
            column: self.column,
        }
    }

    /// Restore the cursor from a snapshot taken by `mark()`.
    #[inline]
    fn reset(&mut self, mark: LexerPos) {
        self.pos = mark.pos;
        self.byte_pos = mark.byte_pos;
        self.line = mark.line;
        self.column = mark.column;
    }

    /// Consume and return current character
    #[inline(always)]
    pub(crate) fn advance(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += 1;
        self.byte_pos += ch.len_utf8();
        if ch == '\n' {
            self.line += 1;
            self.column = 0;
        } else {
            self.column += 1;
        }
        Some(ch)
    }

    /// Check if we're at end of input
    #[inline(always)]
    fn is_eof(&self) -> bool {
        self.pos >= self.input.len()
    }

    /// Skip horizontal whitespace (spaces and tabs, but not newlines)
    #[inline]
    fn skip_hspace(&mut self) -> usize {
        let start_pos = self.pos;
        // Direct array indexing is faster than peek/advance calls
        while self.pos < self.input.len() {
            match self.input[self.pos] {
                ' ' | '\t' => {
                    self.pos += 1;
                    self.byte_pos += 1;
                    self.column += 1;
                }
                _ => break,
            }
        }
        self.pos - start_pos
    }

    /// Parse trivia (comments and whitespace)
    pub(crate) fn parse_trivia(&mut self) -> Vec<ParseTrivium> {
        // Save position before parsing trivia, so we can rewind if needed
        self.trivia_start = Some(self.mark());

        let mut trivia = Vec::new();
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
                    trivia.push(ParseTrivium::Newlines(count));
                }
                Some('#') => {
                    trivia.push(self.parse_line_comment());
                }
                Some('/') if self.at("/*") => {
                    // Try language annotation first, fall back to block comment
                    let saved_state = self.save_state();

                    if let Some(lang_annot) = self.try_parse_language_annotation() {
                        trivia.push(lang_annot);
                    } else {
                        // Restore position and parse as block comment
                        self.restore_state(saved_state);
                        trivia.push(self.parse_block_comment());
                    }
                }
                _ => break,
            }
        }

        trivia
    }

    /// Parse consecutive newlines, return count
    fn parse_newlines(&mut self) -> usize {
        let mut count = 0;
        while let Some(ch) = self.peek() {
            if ch == '\r' {
                self.advance();
                if self.peek() == Some('\n') {
                    self.advance();
                }
                count += 1;
            } else if ch == '\n' {
                self.advance();
                count += 1;
            } else {
                break;
            }
        }
        count
    }

    /// Rewind the last trivia consumed (horizontal spaces, newlines, and comments)
    /// Also clears the trivia buffer since rewound trivia should not be attached to next token
    pub(crate) fn rewind_trivia(&mut self) {
        // Rewind to the position before parse_trivia() was called
        if let Some(mark) = self.trivia_start {
            self.reset(mark);
        }

        self.recent_hspace = 0;
        self.recent_newlines = 0;
        self.trivia_buffer.clear();
    }
}
