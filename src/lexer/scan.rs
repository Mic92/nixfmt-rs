//! Token scanning: the big punctuation/keyword `match` and the small
//! sub-scanners it dispatches to (identifiers, env paths, dot-tokens).

use super::Lexer;
use crate::ast::Token;

impl Lexer {
    /// Parse next token (without trivia handling)
    /// Trivia should ONLY be managed by `lexeme()`, not by this function.
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
            return Ok(self.parse_ident_or_keyword());
        }

        match ch {
            '{' => Ok(self.single(Token::BraceOpen)),
            '}' => Ok(self.single(Token::BraceClose)),
            '[' => Ok(self.single(Token::BrackOpen)),
            ']' => Ok(self.single(Token::BrackClose)),
            '(' => Ok(self.single(Token::ParenOpen)),
            ')' => Ok(self.single(Token::ParenClose)),
            '=' => Ok(self.try_two_char('=', Token::Equal, Token::Assign)),
            '@' => Ok(self.single(Token::At)),
            ':' => Ok(self.single(Token::Colon)),
            ',' => Ok(self.single(Token::Comma)),
            ';' => Ok(self.single(Token::Semicolon)),
            '?' => Ok(self.single(Token::Question)),
            '.' => Ok(self.parse_dot_token()),
            '+' => Ok(self.try_two_char('+', Token::Concat, Token::Plus)),
            '-' => Ok(self.try_two_char('>', Token::Implies, Token::Minus)),
            '*' => Ok(self.single(Token::Mul)),
            '/' => Ok(self.try_two_char('/', Token::Update, Token::Div)),
            '!' => Ok(self.try_two_char('=', Token::Unequal, Token::Not)),
            '<' if self.peek_ahead(1).is_some_and(char::is_alphanumeric) => self.parse_env_path(),
            '<' => {
                self.advance();
                Ok(match self.peek() {
                    Some('=') => self.single(Token::LessEqual),
                    Some('|') => self.single(Token::PipeBackward),
                    _ => Token::Less,
                })
            }
            '>' => Ok(self.try_two_char('=', Token::GreaterEqual, Token::Greater)),
            '&' => {
                if self.at("&&") {
                    self.advance_by(2);
                    Ok(Token::And)
                } else {
                    // Don't advance: keep the error span on the '&' itself.
                    self.err_unexpected(&["'&&'"], "'&'")
                }
            }
            '|' => {
                if self.at("||") {
                    self.advance_by(2);
                    Ok(Token::Or)
                } else if self.at("|>") {
                    self.advance_by(2);
                    Ok(Token::PipeForward)
                } else {
                    self.err_unexpected(&["'||'", "'|>'"], "'|'")
                }
            }
            '"' => Ok(self.single(Token::DoubleQuote)),
            '\'' => {
                if self.at("''") {
                    self.advance_by(2);
                    Ok(Token::DoubleSingleQuote)
                } else {
                    self.err_unexpected(&["''"], "'")
                }
            }
            '$' => {
                if self.at("${") {
                    self.advance_by(2);
                    Ok(Token::InterOpen)
                } else {
                    self.err_unexpected(&["'${'"], "'$'")
                }
            }
            '0'..='9' => Ok(self.parse_number()),
            '~' => Ok(self.single(Token::Tilde)),
            _ => {
                // `ch` was derived from a single byte; for the error message
                // decode the actual codepoint so multi-byte input is reported
                // correctly.
                let ch = self.peek().unwrap();
                self.err_unexpected(&[], &format!("'{ch}'"))
            }
        }
    }

    /// Parse identifier or keyword
    fn parse_ident_or_keyword(&mut self) -> Token {
        // Nix identifiers are ASCII-only: [a-zA-Z_][a-zA-Z0-9_'-]*.
        let len = self
            .take_ascii_while(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'\''))
            .len();
        let start_byte = self.byte_pos - len;
        let bytes = self.source.as_bytes();
        let text = &self.source[start_byte..self.byte_pos];

        // First-byte + length dispatch keeps the common "not a keyword" path
        // to a single comparison instead of up to nine `memcmp`s.

        match (len, bytes[start_byte]) {
            (6, b'a') if text == "assert" => Token::Assert,
            (4, b'e') if text == "else" => Token::Else,
            (2, b'i') if text == "if" => Token::If,
            (2, b'i') if text == "in" => Token::In,
            (7, b'i') if text == "inherit" => Token::Inherit,
            (3, b'l') if text == "let" => Token::Let,
            (3, b'r') if text == "rec" => Token::Rec,
            (4, b't') if text == "then" => Token::Then,
            (4, b'w') if text == "with" => Token::With,
            _ => Token::Identifier(text.into()),
        }
    }

    /// `.` may start `...`, a leading-dot float, or be `Dot`.
    fn parse_dot_token(&mut self) -> Token {
        if self.at("...") {
            self.advance_by(3);
            Token::Ellipsis
        } else if self.peek_ahead(1).is_some_and(|c| c.is_ascii_digit()) {
            self.advance();
            let mut num = String::from(".");
            num.push_str(&self.consume_digits());
            if let Some(exp) = self.parse_exponent() {
                num.push_str(&exp);
            }
            Token::Float(num.into())
        } else {
            self.advance();
            Token::Dot
        }
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
                    return Err(crate::error::ParseError {
                        span: self.current_pos(),
                        kind: crate::error::ErrorKind::InvalidSyntax {
                            description: format!("invalid character '{ch}' in path"),
                            hint: Some("paths can only contain alphanumeric characters, '.', '_', '-', and '/'".to_string()),
                        },
                    });
                }
            }
        }

        Err(crate::error::ParseError {
            span: self.current_pos(),
            kind: crate::error::ErrorKind::UnclosedDelimiter {
                delimiter: '<',
                opening_span,
            },
        })
    }

    /// Build an `UnexpectedToken` error at the current cursor.
    #[cold]
    fn err_unexpected<T>(&self, expected: &[&str], found: &str) -> crate::error::Result<T> {
        Err(crate::error::ParseError {
            span: self.current_pos(),
            kind: crate::error::ErrorKind::UnexpectedToken {
                expected: expected
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect(),
                found: found.to_string(),
            },
        })
    }

    /// Helper for two-character tokens: advance and check if next char matches
    /// Returns `if_match` if second char matches, otherwise `if_single`
    fn try_two_char(&mut self, second: char, if_match: Token, if_single: Token) -> Token {
        self.advance();
        if self.peek() == Some(second) {
            self.advance();
            if_match
        } else {
            if_single
        }
    }

    /// Advance one char and return `tok`; for trivial single-char arms in
    /// `next_token`.
    #[inline]
    fn single(&mut self, tok: Token) -> Token {
        self.advance();
        tok
    }
}
