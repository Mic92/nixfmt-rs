//! Hand-written lexer for Nix
//!
//! Ports the comment normalization logic from nixfmt's Lexer.hs

use crate::types::{Token, TrailingComment, Trivia, Trivium};

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

/// Saved lexer state for backtracking
#[derive(Clone)]
pub(crate) struct LexerState {
    pub(crate) pos: usize,
    pub(crate) line: usize,
    pub(crate) column: usize,
    pub(crate) trivia_buffer: Trivia,
    pub(crate) recent_newlines: usize,
    pub(crate) recent_hspace: usize,
}

pub(crate) struct Lexer {
    pub(crate) input: Vec<char>,
    pub(crate) pos: usize,
    pub(crate) line: usize,
    pub(crate) column: usize,
    /// Accumulated leading trivia for next token
    pub(crate) trivia_buffer: Trivia,
    pub(crate) recent_newlines: usize,
    pub(crate) recent_hspace: usize,
    /// Position before last parse_trivia() call, for rewinding
    trivia_start_pos: Option<usize>,
    trivia_start_line: Option<usize>,
    trivia_start_column: Option<usize>,
}

impl Lexer {
    pub(crate) fn new(source: &str) -> Self {
        Lexer {
            input: source.chars().collect(),
            pos: 0,
            line: 1,
            column: 0,
            trivia_buffer: Trivia::new(),
            recent_newlines: 0,
            recent_hspace: 0,
            trivia_start_pos: None,
            trivia_start_line: None,
            trivia_start_column: None,
        }
    }

    /// Save current state for backtracking
    pub(crate) fn save_state(&self) -> LexerState {
        LexerState {
            pos: self.pos,
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
            let (_trailing, next_leading) = convert_trivia(extra_trivia, next_col);

            leading_trivia.extend(next_leading);

            // Skip hspace after trivia
            let _ = self.skip_hspace();
        }

        // Parse the token (note: next_token() also skips hspace, but that's ok since we already did)
        let token = self.next_token()?;

        // Record position AFTER next_token() has consumed any leading trivia
        // This ensures sourceLine reflects where the token actually appears
        let token_pos = self.current_pos();

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
            convert_trivia(parsed_trivia, next_col)
        };

        // Store leading trivia for next token
        self.trivia_buffer = next_leading;

        // Return annotated token
        Ok(crate::types::Ann {
            pre_trivia: leading_trivia,
            source_line: token_pos,
            value: token,
            trail_comment: trailing_comment,
        })
    }

    /// Parse a whole file (expression + final trivia)
    pub(crate) fn start_parse(&mut self) -> crate::error::Result<()> {
        // Parse initial trivia and convert to leading
        let initial_trivia = self.parse_trivia();
        self.trivia_buffer = convert_leading(&initial_trivia);
        Ok(())
    }

    /// Get remaining trivia at end of file
    pub(crate) fn finish_parse(&mut self) -> Trivia {
        std::mem::take(&mut self.trivia_buffer)
    }

    /// Get current position
    pub(crate) fn current_pos(&self) -> crate::types::Pos {
        crate::types::Pos(self.line)
    }

    /// Parse next token (without trivia handling)
    /// Trivia should ONLY be managed by lexeme(), not by this function.
    /// This matches Haskell nixfmt's `rawSymbol` which parses tokens without trivia.
    pub(crate) fn next_token(&mut self) -> crate::error::Result<Token> {
        let _ = self.skip_hspace();

        if self.is_eof() {
            return Ok(Token::SOF); // Use SOF as EOF token
        }

        let ch = self.peek().unwrap();

        // Check for identifiers/keywords (Unicode alphabetic or underscore)
        // This must come before the match to handle non-ASCII alphabetic characters
        // which can't be matched in patterns
        if ch.is_alphabetic() || ch == '_' {
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
                if self.peek_ahead(1) == Some('.') && self.peek_ahead(2) == Some('.') {
                    self.advance();
                    self.advance();
                    self.advance();
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
                    Err(crate::error::ParseError::new(
                        self.current_pos(),
                        "unexpected '&', expected '&&'",
                    ))
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
                    _ => Err(crate::error::ParseError::new(
                        self.current_pos(),
                        "unexpected '|', expected '||' or '|>'",
                    )),
                }
            }
            '"' => {
                self.advance();
                Ok(Token::TDoubleQuote)
            }
            '\'' => {
                if self.peek_ahead(1) == Some('\'') {
                    self.advance();
                    self.advance();
                    Ok(Token::TDoubleSingleQuote)
                } else {
                    Err(crate::error::ParseError::new(
                        self.current_pos(),
                        "unexpected single quote, expected ''",
                    ))
                }
            }
            '$' => {
                if self.peek_ahead(1) == Some('{') {
                    self.advance();
                    self.advance();
                    Ok(Token::TInterOpen)
                } else {
                    Err(crate::error::ParseError::new(
                        self.current_pos(),
                        "unexpected '$', expected '${'",
                    ))
                }
            }
            '0'..='9' => self.parse_number(),
            '~' => {
                // Tilde - used in paths ~/
                self.advance();
                Ok(Token::TTilde)
            }
            _ => Err(crate::error::ParseError::new(
                self.current_pos(),
                format!("unexpected character: '{}'", ch),
            )),
        }
    }

    /// Parse identifier or keyword
    fn parse_ident_or_keyword(&mut self) -> crate::error::Result<Token> {
        let mut ident = String::new();

        while let Some(ch) = self.peek() {
            if ch.is_alphanumeric() || ch == '_' || ch == '-' || ch == '\'' {
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
                    return Err(crate::error::ParseError::new(
                        self.current_pos(),
                        "invalid character in path",
                    ))
                }
            }
        }

        Err(crate::error::ParseError::new(
            self.current_pos(),
            "unclosed path",
        ))
    }

    /// Parse number (integer or float)
    fn parse_number(&mut self) -> crate::error::Result<Token> {
        let mut num = self.consume_digits();
        let mut is_float = false;

        // Check for decimal point
        if self.peek() == Some('.') {
            let next = self.peek_ahead(1);
            let has_leading_zero = num.starts_with('0');
            let has_multiple_leading_zeroes = has_leading_zero && num.len() > 1;

            if next.is_some_and(|c| c.is_ascii_digit()) && !has_multiple_leading_zeroes {
                is_float = true;
                num.push('.');
                self.advance();
                num.push_str(&self.consume_digits());
            } else if next.is_none_or(|c| !c.is_ascii_digit()) && !num.is_empty() && num != "0" {
                // Allow trailing '.' for numbers starting with non-zero digit (e.g., 1.)
                is_float = true;
                num.push('.');
                self.advance();
            }
        }

        if is_float {
            if let Some(exp) = self.parse_exponent() {
                num.push_str(&exp);
            }
            return Ok(Token::Float(num));
        }

        Ok(Token::Integer(num))
    }

    fn parse_exponent(&mut self) -> Option<String> {
        if !matches!(self.peek(), Some('e' | 'E')) {
            return None;
        }

        let saved_state = self.save_state();
        let mut exponent = String::new();
        exponent.push(self.advance().unwrap());

        if matches!(self.peek(), Some('+' | '-')) {
            exponent.push(self.advance().unwrap());
        }

        if self.peek().is_some_and(|c| c.is_ascii_digit()) {
            exponent.push_str(&self.consume_digits());
            Some(exponent)
        } else {
            self.restore_state(saved_state);
            None
        }
    }

    /// Consume consecutive ASCII digits and return as String
    fn consume_digits(&mut self) -> String {
        let mut digits = String::new();
        while self.peek().is_some_and(|ch| ch.is_ascii_digit()) {
            digits.push(self.advance().unwrap());
        }
        digits
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
    pub(crate) fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    /// Peek ahead n characters
    pub(crate) fn peek_ahead(&self, n: usize) -> Option<char> {
        self.input.get(self.pos + n).copied()
    }

    /// Consume and return current character
    pub(crate) fn advance(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += 1;
        if ch == '\n' {
            self.line += 1;
            self.column = 0;
        } else {
            self.column += 1;
        }
        Some(ch)
    }

    /// Check if we're at end of input
    fn is_eof(&self) -> bool {
        self.pos >= self.input.len()
    }

    /// Skip horizontal whitespace (spaces and tabs, but not newlines)
    fn skip_hspace(&mut self) -> usize {
        let mut count = 0;
        while matches!(self.peek(), Some(' ' | '\t')) {
            self.advance();
            count += 1;
        }
        count
    }

    /// Parse trivia (comments and whitespace)
    pub(crate) fn parse_trivia(&mut self) -> Vec<ParseTrivium> {
        // Save position before parsing trivia, so we can rewind if needed
        self.trivia_start_pos = Some(self.pos);
        self.trivia_start_line = Some(self.line);
        self.trivia_start_column = Some(self.column);

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
                Some('/') if self.peek_ahead(1) == Some('*') => {
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
        if let Some(pos) = self.trivia_start_pos {
            self.pos = pos;
            self.line = self.trivia_start_line.unwrap();
            self.column = self.trivia_start_column.unwrap();
        }

        self.recent_hspace = 0;
        self.recent_newlines = 0;
        self.trivia_buffer.clear();
    }

    /// Parse line comment starting with #
    fn parse_line_comment(&mut self) -> ParseTrivium {
        let col = self.column;
        self.advance(); // consume '#'

        let mut text = String::new();
        while let Some(ch) = self.peek() {
            if matches!(ch, '\n' | '\r') {
                break;
            }
            text.push(self.advance().unwrap());
        }

        ParseTrivium::LineComment {
            text: text.trim_end().to_string(),
            col,
        }
    }

    /// Parse block comment /* ... */
    fn parse_block_comment(&mut self) -> ParseTrivium {
        let start_col = self.column;
        self.advance(); // consume '/'
        self.advance(); // consume '*'

        // Check for doc comment /**
        let is_doc = self.peek() == Some('*') && self.peek_ahead(1) != Some('/');
        if is_doc {
            self.advance();
        }

        let mut chars = String::new();
        while !self.is_eof() {
            if matches!((self.peek(), self.peek_ahead(1)), (Some('*'), Some('/'))) {
                self.advance(); // consume '*'
                self.advance(); // consume '/'
                break;
            }
            chars.push(self.advance().unwrap());
        }

        // Normalize the comment according to Haskell logic
        let lines = Self::split_lines(&chars);
        let lines = Self::remove_stars(start_col, lines);
        let lines = Self::fix_indent(start_col, lines);

        // Drop leading and trailing empty lines
        let lines = Self::drop_while_empty_start(lines);
        let lines = Self::drop_while_empty_end(lines);

        ParseTrivium::BlockComment(is_doc, lines)
    }

    /// Try to parse a language annotation like /* lua */
    fn try_parse_language_annotation(&mut self) -> Option<ParseTrivium> {
        let saved_state = self.save_state();

        // Parse as block comment
        let pt = self.parse_block_comment();

        // Check if it's a single-line, non-doc block comment
        if let ParseTrivium::BlockComment(false, lines) = &pt {
            if lines.len() == 1 {
                let content = lines[0].trim();

                // Check if it's a valid language identifier
                if Self::is_valid_language_identifier(content) {
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

    /// Check if identifier is valid for language annotation
    fn is_valid_language_identifier(s: &str) -> bool {
        !s.is_empty()
            && s.len() <= 30
            && s.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'+' | b'.' | b'_'))
    }

    /// Check if next non-whitespace token is " or ''
    fn is_next_string_delimiter(&mut self) -> bool {
        let saved_state = self.save_state();

        let _ = self.skip_hspace();

        // Optionally consume one newline
        if matches!(self.peek(), Some('\n') | Some('\r')) {
            self.parse_newlines();
            let _ = self.skip_hspace();
        }

        let result = matches!(
            (self.peek(), self.peek_ahead(1)),
            (Some('"'), _) | (Some('\''), Some('\''))
        );

        self.restore_state(saved_state);
        result
    }

    // Comment normalization functions (from Haskell Lexer.hs)

    /// Split text into lines, normalize line endings, and drop trailing empty lines
    /// This matches nixfmt's splitLines function which does `dropWhileEnd Text.null`
    fn split_lines(text: &str) -> Vec<String> {
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
    fn remove_stars(pos: usize, lines: Vec<String>) -> Vec<String> {
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
            result.push(Self::strip_indentation(common_indent, line));
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
}

// Trivia conversion (from Haskell Lexer.hs:162-202)

/// Check if ParseTrivium can be a trailing comment
fn is_trailing(pt: &ParseTrivium) -> bool {
    match pt {
        ParseTrivium::LineComment { .. } => true,
        ParseTrivium::BlockComment(false, lines) => lines.len() <= 1,
        _ => false,
    }
}

/// Convert trailing trivia to TrailingComment
fn convert_trailing(pts: &[ParseTrivium]) -> Option<TrailingComment> {
    let texts: Vec<String> = pts
        .iter()
        .filter_map(|pt| match pt {
            ParseTrivium::LineComment { text, .. } => Some(text.trim().to_string()),
            ParseTrivium::BlockComment(false, lines) if lines.len() == 1 => {
                Some(lines[0].trim().to_string())
            }
            _ => None,
        })
        .filter(|s| !s.is_empty())
        .collect();

    let joined = texts.join(" ");
    if joined.is_empty() {
        None
    } else {
        Some(TrailingComment(joined))
    }
}

/// Convert leading trivia to Trivia
/// Merges consecutive Newlines (matching Haskell's `some (preLexeme eol)` behavior)
/// and converts to final Trivium entries in a single pass to avoid intermediate allocations.
fn convert_leading(pts: &[ParseTrivium]) -> Trivia {
    // State: (result_vec, accumulated_newline_count)
    let (mut result, pending_newlines) =
        pts.iter()
            .fold((Vec::new(), 0), |(mut acc, newline_count), pt| match pt {
                ParseTrivium::Newlines(count) => {
                    // Accumulate consecutive newlines
                    (acc, newline_count + count)
                }
                other => {
                    // Flush pending newlines first (single newlines are discarded)
                    if newline_count > 1 {
                        acc.push(Trivium::EmptyLine());
                    }

                    // Then convert and add the current non-newline trivium
                    match other {
                        ParseTrivium::LineComment { text, .. } => {
                            acc.push(Trivium::LineComment(text.clone()));
                        }
                        ParseTrivium::BlockComment(_, lines) if lines.is_empty() => {}
                        ParseTrivium::BlockComment(false, lines) if lines.len() == 1 => {
                            acc.push(Trivium::LineComment(format!(" {}", lines[0].trim())));
                        }
                        ParseTrivium::BlockComment(is_doc, lines) => {
                            acc.push(Trivium::BlockComment(*is_doc, lines.clone()));
                        }
                        ParseTrivium::LanguageAnnotation(text) => {
                            acc.push(Trivium::LanguageAnnotation(text.clone()));
                        }
                        ParseTrivium::Newlines(_) => unreachable!(),
                    }

                    (acc, 0)
                }
            });

    // Flush any trailing newlines
    if pending_newlines > 1 {
        result.push(Trivium::EmptyLine());
    }

    result.into()
}

/// Convert ParseTrivium list to (trailing_comment, leading_trivia)
/// This is the main conversion function (Lexer.hs:192-202)
pub(crate) fn convert_trivia(
    pts: Vec<ParseTrivium>,
    next_col: usize,
) -> (Option<TrailingComment>, Trivia) {
    // Split into trailing and leading parts
    let split_pos = pts
        .iter()
        .position(|pt| !is_trailing(pt))
        .unwrap_or(pts.len());
    let (trailing_pts, leading_pts) = pts.split_at(split_pos);

    // Special case: if trailing comment visually forms a block with following line,
    // treat it as leading instead
    match (trailing_pts, leading_pts) {
        // Case 1: [ # comment ] followed by single newline and another # at same column
        (
            [ParseTrivium::LineComment { col: col1, .. }],
            [ParseTrivium::Newlines(1), ParseTrivium::LineComment { col: col2, .. }, ..],
        ) if col1 == col2 => (None, convert_leading(&pts)),

        // Case 2: [ # comment ] followed by single newline, and next token is at same column
        ([ParseTrivium::LineComment { col, .. }], [ParseTrivium::Newlines(1)])
            if *col == next_col =>
        {
            (None, convert_leading(&pts))
        }

        // Default: split normally
        _ => (convert_trailing(trailing_pts), convert_leading(leading_pts)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_lines() {
        let input = "line1\nline2\r\nline3";
        let lines = Lexer::split_lines(input);
        assert_eq!(lines, vec!["line1", "line2", "line3"]);
    }

    #[test]
    fn test_remove_stars() {
        // Stars at column 0: " *" gets replaced with ""
        let lines = vec![
            "first line".to_string(),
            " * second".to_string(),
            " * third".to_string(),
        ];
        let result = Lexer::remove_stars(0, lines);
        // After stripping " *", we get " second" (space from after the star)
        assert_eq!(
            result,
            vec![
                "first line".to_string(),
                " second".to_string(),
                " third".to_string()
            ]
        );

        // Stars at column 1: "  *" gets replaced with " "
        let lines2 = vec![
            "first".to_string(),
            "  * line2".to_string(),
            "  * line3".to_string(),
        ];
        let result2 = Lexer::remove_stars(1, lines2);
        assert_eq!(
            result2,
            vec![
                "first".to_string(),
                "  line2".to_string(),
                "  line3".to_string()
            ]
        );

        // No stars - should return unchanged
        let lines3 = vec!["a".to_string(), "b".to_string()];
        let result3 = Lexer::remove_stars(0, lines3.clone());
        assert_eq!(result3, lines3);
    }

    #[test]
    fn test_parse_line_comment() {
        let mut lexer = Lexer::new("# hello world\n");
        let trivia = lexer.parse_trivia();
        assert_eq!(trivia.len(), 2); // comment + newline
    }

    #[test]
    fn test_parse_block_comment() {
        let mut lexer = Lexer::new("/* hello */");
        let trivia = lexer.parse_trivia();
        assert_eq!(trivia.len(), 1);
    }

    #[test]
    fn test_tokenize_keywords() {
        let mut lexer = Lexer::new("let in if then else");
        assert!(matches!(lexer.next_token(), Ok(Token::KLet)));
        assert!(matches!(lexer.next_token(), Ok(Token::KIn)));
        assert!(matches!(lexer.next_token(), Ok(Token::KIf)));
        assert!(matches!(lexer.next_token(), Ok(Token::KThen)));
        assert!(matches!(lexer.next_token(), Ok(Token::KElse)));
    }

    #[test]
    fn test_tokenize_operators() {
        let mut lexer = Lexer::new("+ - * / ++ // == != < > <= >= && || ->");
        assert!(matches!(lexer.next_token(), Ok(Token::TPlus)));
        assert!(matches!(lexer.next_token(), Ok(Token::TMinus)));
        assert!(matches!(lexer.next_token(), Ok(Token::TMul)));
        assert!(matches!(lexer.next_token(), Ok(Token::TDiv)));
        assert!(matches!(lexer.next_token(), Ok(Token::TConcat)));
        assert!(matches!(lexer.next_token(), Ok(Token::TUpdate)));
        assert!(matches!(lexer.next_token(), Ok(Token::TEqual)));
        assert!(matches!(lexer.next_token(), Ok(Token::TUnequal)));
        assert!(matches!(lexer.next_token(), Ok(Token::TLess)));
        assert!(matches!(lexer.next_token(), Ok(Token::TGreater)));
        assert!(matches!(lexer.next_token(), Ok(Token::TLessEqual)));
        assert!(matches!(lexer.next_token(), Ok(Token::TGreaterEqual)));
        assert!(matches!(lexer.next_token(), Ok(Token::TAnd)));
        assert!(matches!(lexer.next_token(), Ok(Token::TOr)));
        assert!(matches!(lexer.next_token(), Ok(Token::TImplies)));
    }

    #[test]
    fn test_tokenize_numbers() {
        let mut lexer = Lexer::new("42 3.14 1.5e10 2e-5");

        assert!(matches!(lexer.next_token(), Ok(Token::Integer(s)) if s == "42"));
        assert!(matches!(lexer.next_token(), Ok(Token::Float(s)) if s == "3.14"));
    }

    #[test]
    fn test_tokenize_delimiters() {
        let mut lexer = Lexer::new("{ } [ ] ( ) , ; : @ . ...");
        assert!(matches!(lexer.next_token(), Ok(Token::TBraceOpen)));
        assert!(matches!(lexer.next_token(), Ok(Token::TBraceClose)));
        assert!(matches!(lexer.next_token(), Ok(Token::TBrackOpen)));
        assert!(matches!(lexer.next_token(), Ok(Token::TBrackClose)));
        assert!(matches!(lexer.next_token(), Ok(Token::TParenOpen)));
        assert!(matches!(lexer.next_token(), Ok(Token::TParenClose)));
        assert!(matches!(lexer.next_token(), Ok(Token::TComma)));
        assert!(matches!(lexer.next_token(), Ok(Token::TSemicolon)));
        assert!(matches!(lexer.next_token(), Ok(Token::TColon)));
        assert!(matches!(lexer.next_token(), Ok(Token::TAt)));
        assert!(matches!(lexer.next_token(), Ok(Token::TDot)));
        assert!(matches!(lexer.next_token(), Ok(Token::TEllipsis)));
    }

    #[test]
    fn test_tokenize_identifiers() {
        let mut lexer = Lexer::new("foo bar_baz hello-world");
        assert!(matches!(lexer.next_token(), Ok(Token::Identifier(s)) if s == "foo"));
        assert!(matches!(lexer.next_token(), Ok(Token::Identifier(s)) if s == "bar_baz"));
        assert!(matches!(lexer.next_token(), Ok(Token::Identifier(s)) if s == "hello-world"));
    }

    #[test]
    fn test_tokenize_simple_expression() {
        let mut lexer = Lexer::new("{ a = 1; }");
        assert!(matches!(lexer.next_token(), Ok(Token::TBraceOpen)));
        assert!(matches!(lexer.next_token(), Ok(Token::Identifier(s)) if s == "a"));
        assert!(matches!(lexer.next_token(), Ok(Token::TAssign)));
        assert!(matches!(lexer.next_token(), Ok(Token::Integer(s)) if s == "1"));
        assert!(matches!(lexer.next_token(), Ok(Token::TSemicolon)));
        assert!(matches!(lexer.next_token(), Ok(Token::TBraceClose)));
    }

    #[test]
    fn test_lexeme_with_comments() {
        let mut lexer = Lexer::new("# leading comment\n{ a = 1; # trailing\n}");
        lexer.start_parse().unwrap();

        // First token: { with leading comment
        let brace = lexer.lexeme().unwrap();
        assert!(matches!(brace.value, Token::TBraceOpen));
        assert_eq!(brace.pre_trivia.len(), 1); // Should have the leading comment
        assert!(matches!(&brace.pre_trivia[0], Trivium::LineComment(s) if s == " leading comment"));

        // Second token: a
        let ident = lexer.lexeme().unwrap();
        assert!(matches!(&ident.value, Token::Identifier(s) if s == "a"));

        // Third token: =
        let eq = lexer.lexeme().unwrap();
        assert!(matches!(eq.value, Token::TAssign));

        // Fourth token: 1
        let num = lexer.lexeme().unwrap();
        assert!(matches!(&num.value, Token::Integer(s) if s == "1"));

        // Fifth token: ; with trailing comment
        let semi = lexer.lexeme().unwrap();
        assert!(matches!(semi.value, Token::TSemicolon));
        assert!(semi.trail_comment.is_some());
        if let Some(ref tc) = semi.trail_comment {
            assert_eq!(tc.0, "trailing");
        }

        // Sixth token: }
        let close = lexer.lexeme().unwrap();
        assert!(matches!(close.value, Token::TBraceClose));
    }

    #[test]
    fn test_lexeme_preserves_trivia() {
        let mut lexer = Lexer::new("let\n\n  # comment\n  a = 1; in a");
        lexer.start_parse().unwrap();

        let let_tok = lexer.lexeme().unwrap();
        assert!(matches!(let_tok.value, Token::KLet));

        // 'a' should have EmptyLine and comment
        let a_tok = lexer.lexeme().unwrap();
        assert!(matches!(&a_tok.value, Token::Identifier(s) if s == "a"));
        assert!(a_tok.pre_trivia.len() >= 1);
        // Should have EmptyLine and LineComment
        assert!(a_tok
            .pre_trivia
            .iter()
            .any(|t| matches!(t, Trivium::EmptyLine())));
    }
}
