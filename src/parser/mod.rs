//! Hand-written recursive descent parser for Nix
//!
//! Ports parsing logic from nixfmt's Parser.hs

mod binders;
mod containers;
mod expressions;
mod parameters;
mod path_uri;
mod spans;
mod strings;

use std::ops::ControlFlow::{self, Break, Continue};

use crate::error::{ParseError, Result};
use crate::lexer::Lexer;
use crate::types::{
    Ann, Binder, Expression, File, Item, Items, Leaf, Parameter, Selector, Span, Term, Token,
    Trivia, Whole,
};

pub struct Parser {
    lexer: Lexer,
    /// Current token
    current: Ann<Token>,
}

/// Saved parser state for checkpointing
struct ParserState {
    lexer_state: crate::lexer::LexerState,
    current: Ann<Token>,
}

/// Check if a token is a comparison operator
const fn is_comparison_operator(token: &Token) -> bool {
    matches!(
        token,
        Token::TEqual
            | Token::TUnequal
            | Token::TLess
            | Token::TGreater
            | Token::TLessEqual
            | Token::TGreaterEqual
    )
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

        Ok(Whole {
            value: expr,
            trailing_trivia,
        })
    }

    /// Parse an expression (top-level)
    fn parse_expression(&mut self) -> Result<Expression> {
        // Match Haskell's order: try operation, then abstraction, then keywords
        match &self.current.value {
            Token::KLet => {
                // Old-style `let { }` vs modern `let ... in ...`
                if self.lexer.peek() == Some('{') {
                    self.parse_abstraction_or_operation()
                } else {
                    self.parse_let()
                }
            }
            Token::KIf => self.parse_if(),
            Token::KWith => self.parse_with(),
            Token::KAssert => self.parse_assert(),
            _ => self.parse_abstraction_or_operation(),
        }
    }

    /// Parse abstraction or operation (handles ambiguity)
    fn parse_abstraction_or_operation(&mut self) -> Result<Expression> {
        match &self.current.value {
            Token::TBraceOpen => self.parse_set_parameter_or_literal(),
            Token::Identifier(_) => {
                // URI check must precede the lambda-parameter check: both look for `:`.
                if self.looks_like_uri() {
                    return self.parse_operation_or_lambda();
                }

                if self.lexer.peek() == Some('/') && !self.lexer.at("//") {
                    return self.parse_operation_or_lambda();
                }

                let ident = self.take_and_advance()?;

                match self.finish_abstraction(Parameter::ID(ident))? {
                    Break(abs) => Ok(abs),
                    Continue(Parameter::ID(ident)) => {
                        let term = self.parse_postfix_selection(Term::Token(ident))?;
                        self.continue_operation_from(Expression::Term(term))
                    }
                    Continue(_) => unreachable!(),
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
            Token::TBraceClose => {
                // Empty set: {} - could be parameter or literal
                let close_brace = self.take_and_advance()?;

                match self.finish_abstraction(Parameter::Set(
                    open_brace,
                    Vec::new(),
                    close_brace,
                ))? {
                    Break(abs) => Ok(abs),
                    Continue(Parameter::Set(open_brace, _, mut close_brace)) => {
                        // Empty set literal: trivia on `}` becomes the set's comment items.
                        let items = if close_brace.pre_trivia.is_empty() {
                            Vec::new()
                        } else {
                            vec![Item::Comments(std::mem::take(&mut close_brace.pre_trivia))]
                        };
                        self.finish_set_literal_expr(open_brace, Items(items), close_brace)
                    }
                    Continue(_) => unreachable!(),
                }
            }
            Token::Identifier(_) => {
                // Try to parse as parameter attributes first
                // If it fails (sees = or .), parse as bindings
                if let Some(attrs) = self.try_parse_param_attrs()? {
                    Self::check_duplicate_formals(&attrs)?;

                    let close_brace =
                        self.expect_closing_delimiter(open_span, '{', Token::TBraceClose)?;
                    let close_span = close_brace.span;

                    match self.finish_abstraction(Parameter::Set(open_brace, attrs, close_brace))? {
                        Break(abs) => Ok(abs),
                        Continue(_) => Err(ParseError::invalid(
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
                        self.expect_closing_delimiter(open_span, '{', Token::TBraceClose)?;
                    self.finish_set_literal_expr(open_brace, bindings, close_brace)
                }
            }
            Token::TEllipsis => {
                // Definitely a parameter: { ... }
                let attrs = self.parse_param_attrs()?;
                Self::check_duplicate_formals(&attrs)?;

                let close_brace =
                    self.expect_closing_delimiter(open_span, '{', Token::TBraceClose)?;
                let close_span = close_brace.span;

                match self.finish_abstraction(Parameter::Set(open_brace, attrs, close_brace))? {
                    Break(abs) => Ok(abs),
                    Continue(_) => Err(ParseError::invalid(
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
                    self.expect_closing_delimiter(open_span, '{', Token::TBraceClose)?;
                self.finish_set_literal_expr(open_brace, bindings, close_brace)
            }
        }
    }

    /// After a candidate lambda parameter `first` has been parsed, try to
    /// consume the optional `@ second` part and the mandatory `: body` and
    /// build an `Expression::Abstraction`.
    ///
    /// * `Break(expr)`  – a full abstraction was parsed.
    /// * `Continue(first)` – neither `:` nor `@` follows; `first` is handed
    ///   back untouched so the caller can reinterpret it (set literal,
    ///   operation, or a hard error).
    fn finish_abstraction(
        &mut self,
        first: Parameter,
    ) -> Result<ControlFlow<Expression, Parameter>> {
        match self.current.value {
            Token::TColon => {
                let colon = self.take_and_advance()?;
                let body = self.parse_expression()?;
                Ok(Break(Expression::Abstraction(first, colon, Box::new(body))))
            }
            Token::TAt => {
                let at_tok = self.take_and_advance()?;
                let second = self.parse_context_second(&first)?;
                if !matches!(self.current.value, Token::TColon) {
                    if matches!(self.current.value, Token::TAt) {
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
                let colon = self.expect_token(Token::TColon, "':'")?;
                let body = self.parse_expression()?;
                Ok(Break(Expression::Abstraction(
                    Parameter::Context(Box::new(first), at_tok, Box::new(second)),
                    colon,
                    Box::new(body),
                )))
            }
            _ => Ok(Continue(first)),
        }
    }

    /// Shared tail of the set-literal branches in `parse_set_parameter_or_literal`.
    fn finish_set_literal_expr(
        &mut self,
        open: Leaf,
        bindings: Items<Binder>,
        close: Leaf,
    ) -> Result<Expression> {
        let set_term = Term::Set(None, open, bindings, close);
        let term = self.parse_postfix_selection(set_term)?;
        self.continue_operation_from(Expression::Term(term))
    }

    /// Continue parsing operation from a given left expression
    fn continue_operation_from(&mut self, expr: Expression) -> Result<Expression> {
        let expr = if matches!(self.current.value, Token::TQuestion) {
            let question = self.take_and_advance()?;
            let selectors = self.parse_selector_path()?;
            Expression::MemberCheck(Box::new(expr), question, selectors)
        } else if self.is_term_start() {
            let mut app_expr = expr;
            while self.is_term_start() && !self.is_expression_end() {
                let arg = Expression::Term(self.parse_term()?);
                app_expr = Expression::Application(Box::new(app_expr), Box::new(arg));
            }
            app_expr
        } else {
            expr
        };

        self.maybe_parse_binary_operation(expr)
    }

    /// Parse operation or lambda (needs lookahead for :)
    fn parse_operation_or_lambda(&mut self) -> Result<Expression> {
        let expr = self.parse_application()?;

        // Member check (?) is handled in parse_application so that `?` binds tighter than prefix `!`/`-`.

        if matches!(self.current.value, Token::TColon | Token::TAt) {
            return Err(Self::reject_non_parameter_expr(&expr));
        }

        self.maybe_parse_binary_operation(expr)
    }

    /// Parse function application (left-associative)
    /// Application only consumes TERMS, not unary expressions
    fn parse_application(&mut self) -> Result<Expression> {
        // Prefix unary operators recurse so that postfix `?` (handled below) binds tighter.
        match &self.current.value {
            Token::TMinus => {
                let op = self.take_and_advance()?;
                let inner = self.parse_application()?;
                return Ok(Expression::Negation(op, Box::new(inner)));
            }
            Token::TNot => {
                let op = self.take_and_advance()?;
                let inner = self.parse_application()?;
                return Ok(Expression::Inversion(op, Box::new(inner)));
            }
            _ => {}
        }

        let mut expr = Expression::Term(self.parse_term()?);

        // Keep applying while we see more TERMS (not unary ops)
        // IMPORTANT: Don't treat binary operators as term starts even if they could start paths
        while self.is_term_start() && !self.is_binary_op() && !self.is_expression_end() {
            let arg = Expression::Term(self.parse_term()?);
            expr = Expression::Application(Box::new(expr), Box::new(arg));
        }

        // Postfix `?` has higher precedence than prefix `!`/`-`; checking it here ensures
        // `!a ? b` parses as `!(a ? b)`, not `(!a) ? b`.
        if matches!(self.current.value, Token::TQuestion) {
            let question = self.take_and_advance()?;
            let selectors = self.parse_selector_path()?;
            expr = Expression::MemberCheck(Box::new(expr), question, selectors);
        }

        Ok(expr)
    }

    /// Check if current token starts a term
    fn is_term_start(&self) -> bool {
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

    /// Check if current token is a binary operator
    fn is_binary_op(&self) -> bool {
        match self.current.value {
            // TDiv can start a path (e.g., /tmp), so check if it looks like one
            Token::TDiv => !self.looks_like_path(),
            Token::TPlus
            | Token::TMinus
            | Token::TMul
            | Token::TConcat
            | Token::TUpdate
            | Token::TAnd
            | Token::TOr
            | Token::TEqual
            | Token::TUnequal
            | Token::TLess
            | Token::TGreater
            | Token::TLessEqual
            | Token::TGreaterEqual
            | Token::TImplies
            | Token::TPipeForward
            | Token::TPipeBackward => true,
            _ => false,
        }
    }

    /// Check if we're at the end of an expression
    const fn is_expression_end(&self) -> bool {
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

    /// Parse binary operation with precedence climbing
    fn parse_binary_operation(&mut self, mut left: Expression, min_prec: u8) -> Result<Expression> {
        let mut last_comparison_prec: Option<u8> = None;
        let mut last_comparison_op: Option<String> = None;
        while self.is_binary_op() && self.get_precedence() >= min_prec {
            let op_token = self.take_current();
            let is_comparison = is_comparison_operator(&op_token.value);
            let prec = Self::get_precedence_for(&op_token.value);
            let op_string = op_token.value.text().to_string();

            // Check if we're chaining comparison operators at the same precedence level
            // This prevents: 1 < 2 < 3 (both < at precedence 9)
            // But allows: 1 < 2 == 2 > 3 (< and > at precedence 9, == at precedence 8)
            if is_comparison && last_comparison_prec == Some(prec) {
                return Err(Box::new(ParseError {
                    span: op_token.span,
                    kind: crate::error::ErrorKind::ChainedComparison {
                        first_op: last_comparison_op.unwrap_or_else(|| "?".to_string()),
                        second_op: op_string,
                    },
                }));
            }

            let is_right_assoc = Self::is_right_associative(&op_token.value);
            self.advance()?;

            let mut right = match self.parse_application() {
                Ok(expr) => expr,
                Err(e) => {
                    // If we failed to parse the right-hand side and current token is }
                    // (closing an interpolation), provide a more helpful error
                    if matches!(self.current.value, Token::TBraceClose | Token::TInterClose) {
                        return Err(ParseError::invalid(
                            self.current.span,
                            format!(
                                "incomplete expression after '{}' operator",
                                op_token.value.text()
                            ),
                            Some("binary operators require expressions on both sides".to_string()),
                        ));
                    }
                    return Err(e);
                }
            };

            // For right-associative operators, use >= to allow same-precedence operators to bind right
            // For left-associative operators, use > to make them bind left
            while self.is_binary_op()
                && (self.get_precedence() > prec
                    || (self.get_precedence() == prec && is_right_assoc))
            {
                right = self.parse_binary_operation(right, self.get_precedence())?;
            }

            // HACK: nixfmt parses TPlus as left-associative but restructures it to right-associative
            // in the AST. This is needed because some formatting code needs to match on the first
            // operand, and doing that with a left-associative chain is not possible.
            // If we have: (a + b) + c, restructure to: a + (b + c)
            left = if matches!(op_token.value, Token::TPlus) {
                if let Expression::Operation(one, op1, two) = left {
                    if matches!(op1.value, Token::TPlus) {
                        Expression::Operation(
                            one,
                            op1,
                            Box::new(Expression::Operation(two, op_token, Box::new(right))),
                        )
                    } else {
                        Expression::Operation(
                            Box::new(Expression::Operation(one, op1, two)),
                            op_token,
                            Box::new(right),
                        )
                    }
                } else {
                    Expression::Operation(Box::new(left), op_token, Box::new(right))
                }
            } else {
                Expression::Operation(Box::new(left), op_token, Box::new(right))
            };

            if is_comparison {
                last_comparison_prec = Some(prec);
                last_comparison_op = Some(op_string);
            } else {
                last_comparison_prec = None;
                last_comparison_op = None;
            }
        }

        Ok(left)
    }

    /// Get precedence of current operator
    const fn get_precedence(&self) -> u8 {
        Self::get_precedence_for(&self.current.value)
    }

    /// Get precedence for a token (higher = tighter binding)
    /// Precedence follows nixfmt's operator table (Types.hs:570-597)
    /// Note: Operators later in nixfmt's list have LOWER precedence
    const fn get_precedence_for(token: &Token) -> u8 {
        match token {
            // Highest precedence (tightest binding)
            Token::TConcat => 14,               // ++ (line 575 in nixfmt)
            Token::TMul | Token::TDiv => 13,    // * / (lines 576-577)
            Token::TPlus | Token::TMinus => 12, // + - (lines 579-580)
            // Note: Prefix TNot would be at precedence 11 (line 582) but it's handled separately
            Token::TUpdate => 10, // // (line 583)
            Token::TLess | Token::TGreater | Token::TLessEqual | Token::TGreaterEqual => 9, // comparisons (lines 584-587)
            Token::TEqual | Token::TUnequal => 8, // == != (lines 589-590)
            Token::TAnd => 7,                     // && (line 592)
            Token::TOr => 6,                      // || (line 593)
            Token::TImplies => 5,                 // -> (line 594)
            Token::TPipeForward => 4,             // |> (line 595)
            Token::TPipeBackward => 3,            // <| (line 596) - lowest!
            _ => 0,                               // Unknown/not a binary operator
        }
    }

    /// Check if an operator is right-associative
    ///
    /// Right-associative operators per nixfmt Types.hs:
    /// - `TConcat` (++) - line 575: `InfixR`
    /// - `TUpdate` (//) - line 583: `InfixR`
    /// - `TPipeBackward` (<|) - line 596: `InfixR`
    ///
    /// Note: `TPlus` (+) is `InfixL` in the spec and is parsed as left-associative,
    /// but nixfmt uses a HACK to restructure it to right-associative in the AST.
    /// This is handled separately in the `parse_binary_operation` function.
    const fn is_right_associative(token: &Token) -> bool {
        matches!(
            token,
            Token::TConcat | Token::TUpdate | Token::TPipeBackward
        )
    }

    /// Parse a term (atom), including postfix selection
    fn parse_term(&mut self) -> Result<Term> {
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
    fn parse_postfix_selection(&mut self, base_term: Term) -> Result<Term> {
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
            let saved_state = self.save_state();

            let mut or_tok = self.take_current();
            // is_or_token() guarantees this is either KOr or Identifier("or").
            or_tok.value = Token::KOr;
            self.advance()?;

            // Check if the next token can start a term (the default value)
            // If not, backtrack and treat 'or' as an identifier
            if self.is_term_start() {
                let default_term = self.parse_term()?;
                Some((or_tok, Box::new(default_term)))
            } else {
                self.restore_state(saved_state);
                None
            }
        } else {
            None
        };

        if selectors.is_empty() && or_default.is_none() {
            Ok(base_term)
        } else {
            Ok(Term::Selection(Box::new(base_term), selectors, or_default))
        }
    }

    /// Parse a single-token term (identifier, integer, float, env path).
    fn parse_token_term(&mut self) -> Result<Term> {
        let token_ann = self.take_and_advance()?;
        Ok(Term::Token(token_ann))
    }

    /// Parse trivia after manually consuming content (strings, paths, etc.)
    /// and return the trailing comment for the previous construct.
    /// This also stores leading trivia for the next token and advances to it.
    fn parse_trailing_trivia_and_advance(
        &mut self,
    ) -> Result<Option<crate::types::TrailingComment>> {
        let (trail_comment, next_leading) = self.lexer.parse_and_convert_trivia();

        self.lexer.trivia_buffer = next_leading;
        self.current = self.lexer.lexeme()?;

        Ok(trail_comment)
    }

    /// Wrap a raw-content scanner (strings, paths, URIs) in the common
    /// `Ann` prologue/epilogue: capture span and leading trivia, run the
    /// scanner, then collect the trailing comment and advance.
    fn with_raw_ann<T>(&mut self, f: impl FnOnce(&mut Self) -> Result<T>) -> Result<Ann<T>> {
        let span = self.current.span;
        let pre_trivia = std::mem::take(&mut self.current.pre_trivia);
        let value = f(self)?;
        let trail_comment = self.parse_trailing_trivia_and_advance()?;
        Ok(Ann {
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
    const fn take_current(&mut self) -> Ann<Token> {
        let dummy = Ann {
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
    fn take_and_advance(&mut self) -> Result<Ann<Token>> {
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

    /// Parse binary operation if present, otherwise return expression as-is
    fn maybe_parse_binary_operation(&mut self, expr: Expression) -> Result<Expression> {
        if self.is_binary_op() {
            self.parse_binary_operation(expr, 0)
        } else {
            Ok(expr)
        }
    }

    /// Expect a closing delimiter, providing helpful error if not found
    #[allow(clippy::needless_pass_by_value)] // call sites pass `Token::TFoo` literals
    fn expect_closing_delimiter(
        &mut self,
        opening_span: Span,
        opening_char: char,
        closing_token: Token,
    ) -> Result<Ann<Token>> {
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
            if opening_char == '(' && matches!(self.current.value, Token::TComma) {
                return Err(ParseError::invalid(
                    self.current.span,
                    "comma not allowed inside parentheses",
                    Some("Nix doesn't use commas in parenthesized expressions. For function calls, use spaces: f x y. For multiple values, use a list [x y] or set { a = x; b = y; }".to_string()),
                ));
            }

            if opening_char == '{' && matches!(self.current.value, Token::TColon) {
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
                Token::TBraceClose | Token::TBrackClose | Token::TParenClose | Token::TInterClose
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
        if matches!(self.current.value, Token::TSemicolon) {
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
