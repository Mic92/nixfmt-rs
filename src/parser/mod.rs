//! Hand-written recursive descent parser for Nix
//!
//! Ports parsing logic from nixfmt's Parser.hs

mod binders;
mod containers;
mod expressions;
mod operators;
mod parameters;
mod path_uri;
mod spans;
mod strings;
mod term;

use crate::ast::{
    Annotated, Binder, Expression, File, Item, Items, Leaf, Parameter, Span, Term, Token, Trailed,
    Trivia,
};
use crate::error::{ParseError, Result};
use crate::lexer::Lexer;

pub struct Parser {
    lexer: Lexer,
    /// Current token
    current: Annotated<Token>,
}

/// Outcome of `finish_abstraction`: either a full lambda was parsed, or no
/// `:`/`@` followed and the tentative parameter is handed back so the caller
/// can reinterpret the surrounding `{ ... }` as a set literal (or report an
/// error). A named enum reads better at call sites than `ControlFlow`, which
/// is conventionally tied to `?`-style short-circuit semantics.
enum SetOrLambda {
    /// A complete `param: body` (optionally with `@`) abstraction.
    Lambda(Expression),
    /// No colon followed; the tentative parameter is returned untouched.
    Set(Parameter),
}

/// Saved parser state for checkpointing
struct ParserState {
    lexer_state: crate::lexer::LexerState,
    current: Annotated<Token>,
}

impl Parser {
    pub(crate) fn new(source: &str) -> Result<Self> {
        let mut lexer = Lexer::new(source);
        lexer.start_parse();
        let current = lexer.lexeme()?;
        Ok(Self { lexer, current })
    }

    /// Parse a complete Nix file
    pub(crate) fn parse_file(&mut self) -> Result<File> {
        let expr = self.parse_expression()?;
        self.expect_eof()?;
        // `lexeme()` already moved trivia after the last real token into the
        // EOF token's `pre_trivia`; that is the file's trailing trivia.
        let trailing_trivia = std::mem::take(&mut self.current.pre_trivia);

        Ok(Trailed {
            value: expr,
            trailing_trivia,
        })
    }

    /// Parse an expression (top-level)
    fn parse_expression(&mut self) -> Result<Expression> {
        // Match Haskell's order: try operation, then abstraction, then keywords
        match &self.current.value {
            Token::Let => {
                // Old-style `let { }` vs modern `let ... in ...`
                if self.lexer.peek() == Some('{') {
                    self.parse_abstraction_or_operation()
                } else {
                    self.parse_let()
                }
            }
            Token::If => self.parse_if(),
            Token::With => self.parse_with(),
            Token::Assert => self.parse_assert(),
            _ => self.parse_abstraction_or_operation(),
        }
    }

    /// Parse abstraction or operation (handles ambiguity)
    fn parse_abstraction_or_operation(&mut self) -> Result<Expression> {
        match &self.current.value {
            Token::BraceOpen => self.parse_set_parameter_or_literal(),
            Token::Identifier(_) => {
                // URI check must precede the lambda-parameter check: both look for `:`.
                if self.looks_like_uri() {
                    return self.parse_operation_or_lambda();
                }

                if self.lexer.peek() == Some('/') && !self.lexer.at("//") {
                    return self.parse_operation_or_lambda();
                }

                let ident = self.take_and_advance()?;

                match self.finish_abstraction(Parameter::Id(ident))? {
                    SetOrLambda::Lambda(abs) => Ok(abs),
                    SetOrLambda::Set(Parameter::Id(ident)) => {
                        let term = self.parse_postfix_selection(Term::Token(ident))?;
                        self.continue_operation_from(Expression::Term(term))
                    }
                    SetOrLambda::Set(_) => unreachable!(),
                }
            }
            _ => self.parse_operation_or_lambda(),
        }
    }

    /// Parse { as either set parameter or set literal
    fn parse_set_parameter_or_literal(&mut self) -> Result<Expression> {
        let saved_state = self.save_state();
        let open_brace = self.take_and_advance()?;
        let open_span = open_brace.span;

        match &self.current.value {
            Token::BraceClose => {
                // Empty set: {} - could be parameter or literal
                let close_brace = self.take_and_advance()?;

                match self.finish_abstraction(Parameter::Set {
                    open: open_brace,
                    attrs: Vec::new(),
                    close: close_brace,
                })? {
                    SetOrLambda::Lambda(abs) => Ok(abs),
                    SetOrLambda::Set(Parameter::Set {
                        open: open_brace,
                        close: mut close_brace,
                        ..
                    }) => {
                        // Empty set literal: trivia on `}` becomes the set's comment items.
                        let items = if close_brace.pre_trivia.is_empty() {
                            Vec::new()
                        } else {
                            vec![Item::Comments(std::mem::take(&mut close_brace.pre_trivia))]
                        };
                        self.finish_set_literal_expr(open_brace, Items(items), close_brace)
                    }
                    SetOrLambda::Set(_) => unreachable!(),
                }
            }
            Token::Identifier(_) => {
                // Try to parse as parameter attributes first
                // If it fails (sees = or .), parse as bindings
                if let Some(attrs) = self.try_parse_param_attrs()? {
                    Self::check_duplicate_formals(&attrs)?;

                    let close_brace =
                        self.expect_closing_delimiter(open_span, '{', Token::BraceClose)?;
                    let close_span = close_brace.span;

                    match self.finish_abstraction(Parameter::Set { open: open_brace, attrs, close: close_brace })? {
                        SetOrLambda::Lambda(abs) => Ok(abs),
                        SetOrLambda::Set(_) => Err(ParseError::invalid(
                            close_span,
                            "set with parameter-like syntax but no colon",
                            Some("use '{ x = ...; }' for set literals or '{ x }: body' for parameters".to_string()),
                        )),
                    }
                } else {
                    // Failed to parse as parameters (saw `=` or `.`); retry as set literal
                    self.restore_state(saved_state);

                    let open_brace = self.take_and_advance()?;

                    let bindings = self.parse_binders()?;
                    let close_brace =
                        self.expect_closing_delimiter(open_span, '{', Token::BraceClose)?;
                    self.finish_set_literal_expr(open_brace, bindings, close_brace)
                }
            }
            Token::Ellipsis => {
                // Definitely a parameter: { ... }
                let attrs = self.parse_param_attrs()?;
                Self::check_duplicate_formals(&attrs)?;

                let close_brace =
                    self.expect_closing_delimiter(open_span, '{', Token::BraceClose)?;
                let close_span = close_brace.span;

                match self.finish_abstraction(Parameter::Set {
                    open: open_brace,
                    attrs,
                    close: close_brace,
                })? {
                    SetOrLambda::Lambda(abs) => Ok(abs),
                    SetOrLambda::Set(_) => Err(ParseError::invalid(
                        close_span,
                        "{ ... } must be followed by ':' or '@'",
                        Some("use '{ x }: body' for function parameters".to_string()),
                    )),
                }
            }
            _ => {
                // Must be set literal with bindings
                let bindings = self.parse_binders()?;
                let close_brace =
                    self.expect_closing_delimiter(open_span, '{', Token::BraceClose)?;
                self.finish_set_literal_expr(open_brace, bindings, close_brace)
            }
        }
    }

    /// After a candidate lambda parameter `first` has been parsed, try to
    /// consume the optional `@ second` part and the mandatory `: body` and
    /// build an `Expression::Lambda`.
    ///
    /// * `Lambda(expr)` – a full abstraction was parsed.
    /// * `Set(first)` – neither `:` nor `@` follows; `first` is handed
    ///   back untouched so the caller can reinterpret it (set literal,
    ///   operation, or a hard error).
    fn finish_abstraction(&mut self, first: Parameter) -> Result<SetOrLambda> {
        match self.current.value {
            Token::Colon => {
                let colon = self.take_and_advance()?;
                let body = self.parse_expression()?;
                Ok(SetOrLambda::Lambda(Expression::Lambda {
                    param: first,
                    colon,
                    body: Box::new(body),
                }))
            }
            Token::At => {
                let at_tok = self.take_and_advance()?;
                let second = self.parse_context_second(&first)?;
                if !matches!(self.current.value, Token::Colon) {
                    if matches!(self.current.value, Token::At) {
                        return Err(ParseError::unexpected(
                            self.current.span,
                            vec!["':'".to_string()],
                            "'@'",
                        ));
                    }
                    return Err(ParseError::invalid(
                        at_tok.span,
                        "@ is only valid in lambda parameters",
                        Some("use 'name1 @ name2: body' for function parameters".to_string()),
                    ));
                }
                let colon = self.expect_token(Token::Colon, "':'")?;
                let body = self.parse_expression()?;
                Ok(SetOrLambda::Lambda(Expression::Lambda {
                    param: Parameter::Context {
                        lhs: Box::new(first),
                        at: at_tok,
                        rhs: Box::new(second),
                    },
                    colon,
                    body: Box::new(body),
                }))
            }
            _ => Ok(SetOrLambda::Set(first)),
        }
    }

    /// Shared tail of the set-literal branches in `parse_set_parameter_or_literal`.
    fn finish_set_literal_expr(
        &mut self,
        open: Leaf,
        bindings: Items<Binder>,
        close: Leaf,
    ) -> Result<Expression> {
        let set_term = Term::Set {
            rec: None,
            open,
            items: bindings,
            close,
        };
        let term = self.parse_postfix_selection(set_term)?;
        self.continue_operation_from(Expression::Term(term))
    }

    /// Parse operation or lambda (needs lookahead for :)
    fn parse_operation_or_lambda(&mut self) -> Result<Expression> {
        let expr = self.parse_application()?;

        // Member check (?) is handled in parse_application so that `?` binds tighter than prefix `!`/`-`.

        if matches!(self.current.value, Token::Colon | Token::At) {
            return Err(Self::reject_non_parameter_expr(&expr));
        }

        self.maybe_parse_binary_operation(expr)
    }

    /// Parse trivia after manually consuming content (strings, paths, etc.)
    /// and return the trailing comment for the previous construct.
    /// This also stores leading trivia for the next token and advances to it.
    fn parse_trailing_trivia_and_advance(
        &mut self,
        prev_multiline: bool,
    ) -> Result<Option<crate::ast::TrailingComment>> {
        let (trail_comment, next_leading) = self.lexer.parse_and_convert_trivia(prev_multiline);

        self.lexer.trivia_buffer = next_leading;
        self.current = self.lexer.lexeme()?;

        Ok(trail_comment)
    }

    /// Wrap a raw-content scanner (strings, paths, URIs) in the common
    /// `Annotated` prologue/epilogue: capture span and leading trivia, run the
    /// scanner, then collect the trailing comment and advance.
    fn with_raw_ann<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<Annotated<T>> {
        let span = self.current.span;
        let pre_trivia = std::mem::take(&mut self.current.pre_trivia);
        let value = f(self)?;
        let prev_multiline = self.lexer.line > span.start_line();
        let trail_comment = self.parse_trailing_trivia_and_advance(prev_multiline)?;
        Ok(Annotated {
            pre_trivia,
            span,
            value,
            trail_comment,
        })
    }

    /// Advance to next token
    fn advance(&mut self) -> Result<()> {
        self.current = self.lexer.lexeme()?;
        Ok(())
    }

    /// Take current token (consumes it)
    const fn take_current(&mut self) -> Annotated<Token> {
        let dummy = Annotated {
            pre_trivia: Trivia::new(),
            span: Span::point(0),
            value: Token::Sof,
            trail_comment: None,
        };
        std::mem::replace(&mut self.current, dummy)
    }

    /// Take current token and advance to next (common pattern).
    /// Fused so `self.current` is overwritten once with the next lexeme
    /// instead of once with a dummy and again with the lexeme.
    #[inline]
    fn take_and_advance(&mut self) -> Result<Annotated<Token>> {
        let next = self.lexer.lexeme()?;
        Ok(std::mem::replace(&mut self.current, next))
    }

    /// Collect `pre_trivia` as Comments item if not empty (common pattern)
    /// Parse a sequence of items separated by trivia until `is_end` matches
    /// the current token. Leading trivia on each item (and on the closing
    /// token) is hoisted into `Item::Comments`. Callers must include
    /// `Token::Sof` in `is_end` so EOF terminates the loop; trailing trivia
    /// is not collected at EOF since there is no closing delimiter to own it.
    fn parse_items<T>(
        &mut self,
        is_end: impl Fn(&Token) -> bool,
        mut one: impl FnMut(&mut Self) -> Result<T>,
    ) -> Result<Items<T>> {
        let mut items = Vec::new();
        while !is_end(&self.current.value) {
            self.collect_trivia_as_comments(&mut items);
            let item = one(self)?;
            items.push(Item::Item(item));
        }
        if !matches!(self.current.value, Token::Sof) {
            self.collect_trivia_as_comments(&mut items);
        }
        Ok(Items(items))
    }

    fn collect_trivia_as_comments<T>(&mut self, items: &mut Vec<Item<T>>) {
        if !self.current.pre_trivia.is_empty() {
            items.push(Item::Comments(std::mem::take(&mut self.current.pre_trivia)));
        }
    }

    /// Save current parser state for potential backtracking
    fn save_state(&self) -> ParserState {
        ParserState {
            lexer_state: self.lexer.save_state(),
            current: self.current.clone(),
        }
    }

    /// Restore parser state from a checkpoint
    fn restore_state(&mut self, state: ParserState) {
        self.lexer.restore_state(state.lexer_state);
        self.current = state.current;
    }
    /// Expect a closing delimiter, providing helpful error if not found
    #[allow(clippy::needless_pass_by_value)] // call sites pass `Token::TFoo` literals
    fn expect_closing_delimiter(
        &mut self,
        opening_span: Span,
        opening_char: char,
        closing_token: Token,
    ) -> Result<Annotated<Token>> {
        if self.current.value == closing_token {
            self.take_and_advance()
        } else if matches!(self.current.value, Token::Sof) {
            Err(ParseError::unclosed(
                self.current.span,
                opening_char,
                opening_span,
            ))
        } else {
            // Special case: comma inside parentheses (common mistake from other languages)
            if opening_char == '(' && matches!(self.current.value, Token::Comma) {
                return Err(ParseError::invalid(
                    self.current.span,
                    "comma not allowed inside parentheses",
                    Some("Nix doesn't use commas in parenthesized expressions. For function calls, use spaces: f x y. For multiple values, use a list [x y] or set { a = x; b = y; }".to_string()),
                ));
            }

            if opening_char == '{' && matches!(self.current.value, Token::Colon) {
                return Err(ParseError::invalid(
                    self.current.span,
                    "unexpected ':' inside '{ ... }'",
                    Some(
                        "for a function use '{ args }: body'; for an attribute use 'name = value;'"
                            .to_string(),
                    ),
                ));
            }

            if matches!(
                self.current.value,
                Token::BraceClose | Token::BrackClose | Token::ParenClose | Token::InterClose
            ) {
                return Err(ParseError::invalid(
                    self.current.span,
                    format!(
                        "mismatched delimiter: expected '{}', found '{}'",
                        closing_token.text(),
                        self.current.value.text()
                    ),
                    Some(format!(
                        "change '{}' to '{}' to match the opening '{opening_char}'",
                        self.current.value.text(),
                        closing_token.text(),
                    )),
                ));
            }

            Err(ParseError::unexpected(
                self.current.span,
                vec![format!("'{}'", closing_token.text())],
                format!("'{}'", self.current.value.text()),
            ))
        }
    }

    /// Expect a specific token, advance if it matches, otherwise emit an
    /// `UnexpectedToken` error using `label` as the expected description.
    #[allow(clippy::needless_pass_by_value)] // call sites pass `Token::TFoo` literals
    fn expect_token(&mut self, tok: Token, label: &'static str) -> Result<Leaf> {
        if self.current.value == tok {
            self.take_and_advance()
        } else {
            Err(ParseError::unexpected(
                self.current.span,
                vec![label.to_string()],
                format!("'{}'", self.current.value.text()),
            ))
        }
    }

    /// Expect a `;` and emit a `MissingToken` error mentioning the preceding
    /// construct otherwise.
    fn expect_semicolon_after(&mut self, after: &'static str) -> Result<Leaf> {
        if matches!(self.current.value, Token::Semicolon) {
            self.take_and_advance()
        } else {
            Err(ParseError::missing(self.current.span, "';'", after))
        }
    }

    /// Expect EOF
    fn expect_eof(&self) -> Result<()> {
        if matches!(self.current.value, Token::Sof) {
            Ok(())
        } else {
            Err(ParseError::unexpected(
                self.current.span,
                vec!["end of file".to_string()],
                format!("'{}'", self.current.value.text()),
            ))
        }
    }
}
