//! Term-level parsing: atoms, function application, and `.`/`or` selection.

use super::Parser;
use crate::ast::{Expression, Selector, Term, Token};
use crate::error::{ParseError, Result};

impl Parser {
    /// Parse function application (left-associative)
    /// Apply only consumes TERMS, not unary expressions
    pub(super) fn parse_application(&mut self) -> Result<Expression> {
        // Prefix unary operators recurse so that postfix `?` (handled below) binds tighter.
        match &self.current.value {
            Token::TMinus => {
                let op = self.take_and_advance()?;
                let inner = self.parse_application()?;
                return Ok(Expression::Negation {
                    minus: op,
                    expr: Box::new(inner),
                });
            }
            Token::TNot => {
                let op = self.take_and_advance()?;
                let inner = self.parse_application()?;
                return Ok(Expression::Inversion {
                    bang: op,
                    expr: Box::new(inner),
                });
            }
            _ => {}
        }

        let mut expr = Expression::Term(self.parse_term()?);

        // Keep applying while we see more TERMS (not unary ops)
        // IMPORTANT: Don't treat binary operators as term starts even if they could start paths
        while self.is_term_start() && !self.is_binary_op() && !self.is_expression_end() {
            let arg = Expression::Term(self.parse_term()?);
            expr = Expression::Apply {
                func: Box::new(expr),
                arg: Box::new(arg),
            };
        }

        // Postfix `?` has higher precedence than prefix `!`/`-`; checking it here ensures
        // `!a ? b` parses as `!(a ? b)`, not `(!a) ? b`.
        if matches!(self.current.value, Token::TQuestion) {
            let question = self.take_and_advance()?;
            let selectors = self.parse_selector_path()?;
            expr = Expression::MemberCheck {
                lhs: Box::new(expr),
                question,
                path: selectors,
            };
        }

        Ok(expr)
    }

    /// Check if current token starts a term
    pub(super) fn is_term_start(&self) -> bool {
        match &self.current.value {
            Token::Identifier(_)
            | Token::Integer(_)
            | Token::Float(_)
            | Token::EnvPath(_)
            | Token::TBraceOpen
            | Token::KRec
            | Token::TBrackOpen
            | Token::TParenOpen
            | Token::TDoubleQuote
            | Token::TDoubleSingleQuote => true,

            // These can start paths, but only in specific contexts
            Token::TDot | Token::TDiv | Token::TTilde => self.looks_like_path(),

            _ => false,
        }
    }

    /// Check if we're at the end of an expression
    pub(super) const fn is_expression_end(&self) -> bool {
        matches!(
            self.current.value,
            Token::TSemicolon
                | Token::KThen
                | Token::KElse
                | Token::KIn
                | Token::TParenClose
                | Token::TBraceClose
                | Token::TBrackClose
                | Token::Sof
        )
    }

    /// Parse a term (atom), including postfix selection
    pub(super) fn parse_term(&mut self) -> Result<Term> {
        if self.looks_like_uri() {
            return self.parse_uri();
        }

        if self.looks_like_path() {
            return self.parse_path();
        }

        let base_term = match &self.current.value {
            Token::Identifier(_) | Token::Integer(_) | Token::Float(_) | Token::EnvPath(_) => {
                self.parse_token_term()
            }
            Token::TBraceOpen | Token::KRec | Token::KLet => self.parse_set(),
            Token::TBrackOpen => self.parse_list(),
            Token::TParenOpen => self.parse_parenthesized(),
            Token::TDoubleQuote => self.parse_simple_string(),
            Token::TDoubleSingleQuote => self.parse_indented_string(),
            _ => Err(ParseError::unexpected(
                self.current.span,
                vec![
                    "identifier".to_string(),
                    "number".to_string(),
                    "string".to_string(),
                    "set".to_string(),
                    "list".to_string(),
                    "path".to_string(),
                ],
                format!("'{}'", self.current.value.text()),
            )),
        }?;

        self.parse_postfix_selection(base_term)
    }

    /// Parse postfix selection: term.attr.attr or term.attr or term
    pub(super) fn parse_postfix_selection(&mut self, base_term: Term) -> Result<Term> {
        let mut selectors = Vec::new();

        while matches!(self.current.value, Token::TDot) {
            let saved_state = self.save_state();

            self.advance()?;
            if !self.is_simple_selector_start() {
                self.restore_state(saved_state);
                break;
            }

            let dot_token = saved_state.current;

            let simple_sel = self.parse_simple_selector()?;
            selectors.push(Selector {
                dot: Some(dot_token),
                selector: simple_sel,
            });
        }

        // `or` is only the selection-default operator after at least one
        // selector; otherwise it is the (deprecated) identifier `or` and
        // must be left for the application parser.
        let or_default = if !selectors.is_empty() && self.is_or_token() {
            let mut or_tok = self.take_current();
            // is_or_token() guarantees this is either KOr or Identifier("or").
            or_tok.value = Token::KOr;
            self.advance()?;

            // Nix requires a default expression here; `a.b or ]`/`a.b or;`
            // are syntax errors, so do not backtrack to or-as-identifier.
            if !self.is_term_start() {
                return Err(ParseError::unexpected(
                    self.current.span,
                    vec!["expression".to_string()],
                    format!("'{}'", self.current.value.text()),
                ));
            }
            let default_term = self.parse_term()?;
            Some(crate::ast::SetDefault {
                or_kw: or_tok,
                value: Box::new(default_term),
            })
        } else {
            None
        };

        if selectors.is_empty() && or_default.is_none() {
            Ok(base_term)
        } else {
            Ok(Term::Selection {
                base: Box::new(base_term),
                selectors,
                default: or_default,
            })
        }
    }

    /// Parse a single-token term (identifier, integer, float, env path).
    fn parse_token_term(&mut self) -> Result<Term> {
        let token_ann = self.take_and_advance()?;
        Ok(Term::Token(token_ann))
    }
}
