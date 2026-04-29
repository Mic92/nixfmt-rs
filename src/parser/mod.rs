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

use crate::error::{ParseError, Result};
use crate::lexer::Lexer;
use crate::types::*;

pub(crate) struct Parser {
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
fn is_comparison_operator(token: &Token) -> bool {
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
        lexer.start_parse()?;
        let current = lexer.lexeme()?;
        Ok(Parser { lexer, current })
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

                if matches!(self.current.value, Token::TColon) {
                    let colon = self.take_and_advance()?;
                    let body = self.parse_expression()?;
                    Ok(Expression::Abstraction(
                        Parameter::ID(ident),
                        colon,
                        Box::new(body),
                    ))
                } else if matches!(self.current.value, Token::TAt) {
                    // Context parameter: x @ param: body
                    let at_tok = self.take_and_advance()?;
                    let second_param = self.parse_full_parameter()?;

                    if !matches!(self.current.value, Token::TColon) {
                        return Err(Box::new(ParseError {
                            span: at_tok.span,
                            kind: crate::error::ErrorKind::InvalidSyntax {
                                description: "@ is only valid in lambda parameters".to_string(),
                                hint: Some(
                                    "use 'name1 @ name2: body' for function parameters".to_string(),
                                ),
                            },
                            labels: vec![],
                        }));
                    }

                    let first_param = Parameter::ID(ident);
                    self.validate_context_parameter(&first_param, &second_param)?;

                    let colon = self.expect_token_match(|t| matches!(t, Token::TColon))?;
                    let body = self.parse_expression()?;
                    Ok(Expression::Abstraction(
                        Parameter::Context(Box::new(first_param), at_tok, Box::new(second_param)),
                        colon,
                        Box::new(body),
                    ))
                } else {
                    let base_term = Term::Token(ident);
                    let term = self.parse_postfix_selection(base_term)?;
                    let term_expr = Expression::Term(term);
                    self.continue_operation_from(term_expr)
                }
            }
            _ => self.parse_operation_or_lambda(),
        }
    }

    /// Parse { as either set parameter or set literal
    fn parse_set_parameter_or_literal(&mut self) -> Result<Expression> {
        let saved_state = self.save_state();
        let open_brace = self.take_and_advance()?;

        match &self.current.value {
            Token::TBraceClose => {
                // Empty set: {} - could be parameter or literal
                let close_brace = self.take_and_advance()?;

                if matches!(self.current.value, Token::TColon) {
                    // Empty set parameter `{}: body`; keep trivia on the close brace for proper formatting
                    let colon = self.take_and_advance()?;
                    let body = self.parse_expression()?;
                    Ok(Expression::Abstraction(
                        Parameter::Set(open_brace, Vec::new(), close_brace),
                        colon,
                        Box::new(body),
                    ))
                } else if matches!(self.current.value, Token::TAt) {
                    // Context parameter: { } @ param: body
                    let at_tok = self.take_and_advance()?;
                    let second_param = self.parse_full_parameter()?;

                    let first_param = Parameter::Set(open_brace, Vec::new(), close_brace);
                    self.validate_context_parameter(&first_param, &second_param)?;

                    let colon = self.expect_token_match(|t| matches!(t, Token::TColon))?;
                    let body = self.parse_expression()?;
                    Ok(Expression::Abstraction(
                        Parameter::Context(Box::new(first_param), at_tok, Box::new(second_param)),
                        colon,
                        Box::new(body),
                    ))
                } else {
                    // Empty set literal (possibly with comments)
                    // Extract trivia from close brace as comments for set literals
                    let mut close_brace_for_literal = close_brace;
                    let items = if !close_brace_for_literal.pre_trivia.is_empty() {
                        let comments = std::mem::take(&mut close_brace_for_literal.pre_trivia);
                        vec![Item::Comments(comments)]
                    } else {
                        Vec::new()
                    };
                    let set_term =
                        Term::Set(None, open_brace, Items(items), close_brace_for_literal);
                    let term_with_selection = self.parse_postfix_selection(set_term)?;
                    self.continue_operation_from(Expression::Term(term_with_selection))
                }
            }
            Token::Identifier(_) => {
                // Try to parse as parameter attributes first
                // If it fails (sees = or .), parse as bindings
                match self.try_parse_param_attrs()? {
                    Some(attrs) => {
                        self.check_duplicate_formals(&attrs)?;

                        let close_brace =
                            self.expect_token_match(|t| matches!(t, Token::TBraceClose))?;

                        if matches!(self.current.value, Token::TColon) {
                            // Set parameter: { x, y }: body
                            let colon = self.take_and_advance()?;
                            let body = self.parse_expression()?;
                            Ok(Expression::Abstraction(
                                Parameter::Set(open_brace, attrs, close_brace),
                                colon,
                                Box::new(body),
                            ))
                        } else if matches!(self.current.value, Token::TAt) {
                            // Context parameter: { x } @ param: body
                            let at_tok = self.take_and_advance()?;
                            let second_param = self.parse_full_parameter()?;

                            let first_param = Parameter::Set(open_brace, attrs, close_brace);
                            self.validate_context_parameter(&first_param, &second_param)?;

                            let colon = self.expect_token_match(|t| matches!(t, Token::TColon))?;
                            let body = self.parse_expression()?;
                            Ok(Expression::Abstraction(
                                Parameter::Context(
                                    Box::new(first_param),
                                    at_tok,
                                    Box::new(second_param),
                                ),
                                colon,
                                Box::new(body),
                            ))
                        } else {
                            Err(Box::new(ParseError {
                                span: close_brace.span,
                                kind: crate::error::ErrorKind::InvalidSyntax {
                                    description: "set with parameter-like syntax but no colon".to_string(),
                                    hint: Some("use '{ x = ...; }' for set literals or '{ x }: body' for parameters".to_string()),
                                },
                                labels: vec![],
                            }))
                        }
                    }
                    None => {
                        // Failed to parse as parameters (saw `=` or `.`); retry as set literal
                        self.restore_state(saved_state);

                        let open_brace = self.take_and_advance()?;

                        let bindings = self.parse_binders()?;
                        let close_brace =
                            self.expect_token_match(|t| matches!(t, Token::TBraceClose))?;
                        let set_term = Term::Set(None, open_brace, bindings, close_brace);
                        let term_with_selection = self.parse_postfix_selection(set_term)?;
                        self.continue_operation_from(Expression::Term(term_with_selection))
                    }
                }
            }
            Token::TEllipsis => {
                // Definitely a parameter: { ... }
                let attrs = self.parse_param_attrs()?;
                self.check_duplicate_formals(&attrs)?;

                let close_brace = self.expect_token_match(|t| matches!(t, Token::TBraceClose))?;

                if matches!(self.current.value, Token::TColon) {
                    let colon = self.take_and_advance()?;
                    let body = self.parse_expression()?;
                    Ok(Expression::Abstraction(
                        Parameter::Set(open_brace, attrs, close_brace),
                        colon,
                        Box::new(body),
                    ))
                } else if matches!(self.current.value, Token::TAt) {
                    // Context parameter: {...}@args
                    let at_tok = self.take_and_advance()?;
                    let second_param = self.parse_full_parameter()?;

                    let first_param = Parameter::Set(open_brace, attrs, close_brace);
                    self.validate_context_parameter(&first_param, &second_param)?;

                    let colon = self.expect_token_match(|t| matches!(t, Token::TColon))?;
                    let body = self.parse_expression()?;
                    Ok(Expression::Abstraction(
                        Parameter::Context(Box::new(first_param), at_tok, Box::new(second_param)),
                        colon,
                        Box::new(body),
                    ))
                } else {
                    Err(Box::new(ParseError {
                        span: close_brace.span,
                        kind: crate::error::ErrorKind::InvalidSyntax {
                            description: "{ ... } must be followed by ':' or '@'".to_string(),
                            hint: Some("use '{ x }: body' for function parameters".to_string()),
                        },
                        labels: vec![],
                    }))
                }
            }
            _ => {
                // Must be set literal with bindings
                let bindings = self.parse_binders()?;
                let close_brace = self.expect_token_match(|t| matches!(t, Token::TBraceClose))?;
                let set_term = Term::Set(None, open_brace, bindings, close_brace);
                let term_with_selection = self.parse_postfix_selection(set_term)?;
                self.continue_operation_from(Expression::Term(term_with_selection))
            }
        }
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

        if matches!(self.current.value, Token::TAt) {
            let at_tok = self.take_and_advance()?;
            // Parse second part as a PARAMETER (not expression)
            let second_param = self.parse_full_parameter()?;

            if matches!(self.current.value, Token::TColon) {
                let first_param = self.expr_to_parameter(expr)?;
                self.validate_context_parameter(&first_param, &second_param)?;

                let param =
                    Parameter::Context(Box::new(first_param), at_tok, Box::new(second_param));
                let colon = self.expect_token_match(|t| matches!(t, Token::TColon))?;
                let body = self.parse_expression()?;
                return Ok(Expression::Abstraction(param, colon, Box::new(body)));
            } else {
                return Err(Box::new(ParseError::new(
                    at_tok.span,
                    "@ is only valid in lambda parameters",
                )));
            }
        }

        // Member check (?) is handled in parse_application so that `?` binds tighter than prefix `!`/`-`.

        if matches!(self.current.value, Token::TColon) {
            let param = self.expr_to_parameter(expr)?;
            let colon = self.expect_token_match(|t| matches!(t, Token::TColon))?;
            let body = self.parse_expression()?;
            Ok(Expression::Abstraction(param, colon, Box::new(body)))
        } else {
            self.maybe_parse_binary_operation(expr)
        }
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
    fn is_expression_end(&self) -> bool {
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
            let prec = self.get_precedence_for(&op_token.value);
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
                    labels: vec![],
                }));
            }

            let is_right_assoc = self.is_right_associative(&op_token.value);
            self.advance()?;

            let mut right = match self.parse_application() {
                Ok(expr) => expr,
                Err(e) => {
                    // If we failed to parse the right-hand side and current token is }
                    // (closing an interpolation), provide a more helpful error
                    if matches!(self.current.value, Token::TBraceClose | Token::TInterClose) {
                        return Err(Box::new(ParseError {
                            span: self.current.span,
                            kind: crate::error::ErrorKind::InvalidSyntax {
                                description: format!(
                                    "incomplete expression after '{}' operator",
                                    op_token.value.text()
                                ),
                                hint: Some(
                                    "binary operators require expressions on both sides"
                                        .to_string(),
                                ),
                            },
                            labels: vec![],
                        }));
                    } else {
                        return Err(e);
                    }
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
    fn get_precedence(&self) -> u8 {
        self.get_precedence_for(&self.current.value)
    }

    /// Get precedence for a token (higher = tighter binding)
    /// Precedence follows nixfmt's operator table (Types.hs:570-597)
    /// Note: Operators later in nixfmt's list have LOWER precedence
    fn get_precedence_for(&self, token: &Token) -> u8 {
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
    /// - TConcat (++) - line 575: InfixR
    /// - TUpdate (//) - line 583: InfixR
    /// - TPipeBackward (<|) - line 596: InfixR
    ///
    /// Note: TPlus (+) is InfixL in the spec and is parsed as left-associative,
    /// but nixfmt uses a HACK to restructure it to right-associative in the AST.
    /// This is handled separately in the parse_binary_operation function.
    fn is_right_associative(&self, token: &Token) -> bool {
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
            Token::Identifier(_) => self.parse_identifier_term(),
            Token::Integer(_) => self.parse_integer_term(),
            Token::Float(_) => self.parse_float_term(),
            Token::EnvPath(_) => self.parse_env_path_term(),
            Token::TBraceOpen | Token::KRec | Token::KLet => self.parse_set(),
            Token::TBrackOpen => self.parse_list(),
            Token::TParenOpen => self.parse_parenthesized(),
            Token::TDoubleQuote => self.parse_simple_string(),
            Token::TDoubleSingleQuote => self.parse_indented_string(),
            _ => Err(Box::new(ParseError {
                span: self.current.span,
                kind: crate::error::ErrorKind::UnexpectedToken {
                    expected: vec![
                        "identifier".to_string(),
                        "number".to_string(),
                        "string".to_string(),
                        "set".to_string(),
                        "list".to_string(),
                        "path".to_string(),
                    ],
                    found: format!("'{}'", self.current.value.text()),
                },
                labels: vec![],
            })),
        }?;

        self.parse_postfix_selection(base_term)
    }

    /// Parse postfix selection: term.attr.attr or term.attr or term
    fn parse_postfix_selection(&mut self, base_term: Term) -> Result<Term> {
        let mut selectors = Vec::new();

        while matches!(self.current.value, Token::TDot) {
            let saved_state = self.save_state();

            self.advance()?;
            let is_selector_start = matches!(
                self.current.value,
                Token::Identifier(_) | Token::TDoubleQuote | Token::TInterOpen
            );

            if !is_selector_start {
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

    /// Parse identifier as a term (Token)
    fn parse_identifier_term(&mut self) -> Result<Term> {
        let token_ann = self.take_and_advance()?;
        Ok(Term::Token(token_ann))
    }

    /// Parse integer as a term (Token)
    fn parse_integer_term(&mut self) -> Result<Term> {
        let token_ann = self.take_and_advance()?;
        Ok(Term::Token(token_ann))
    }

    /// Parse float as a term (Token)
    fn parse_float_term(&mut self) -> Result<Term> {
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

    /// Advance to next token
    fn advance(&mut self) -> Result<()> {
        self.current = self.lexer.lexeme()?;
        Ok(())
    }

    /// Take current token (consumes it)
    fn take_current(&mut self) -> Ann<Token> {
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

    /// Collect pre_trivia as Comments item if not empty (common pattern)
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
    fn expect_closing_delimiter(
        &mut self,
        opening_span: Span,
        opening_char: char,
        closing_token: Token,
    ) -> Result<Ann<Token>> {
        if self.current.value == closing_token {
            self.take_and_advance()
        } else if matches!(self.current.value, Token::Sof) {
            Err(Box::new(ParseError {
                span: self.current.span,
                kind: crate::error::ErrorKind::UnclosedDelimiter {
                    delimiter: opening_char,
                    opening_span,
                },
                labels: vec![],
            }))
        } else {
            // Special case: comma inside parentheses (common mistake from other languages)
            if opening_char == '(' && matches!(self.current.value, Token::TComma) {
                return Err(Box::new(ParseError {
                    span: self.current.span,
                    kind: crate::error::ErrorKind::InvalidSyntax {
                        description: "comma not allowed inside parentheses".to_string(),
                        hint: Some("Nix doesn't use commas in parenthesized expressions. For function calls, use spaces: f x y. For multiple values, use a list [x y] or set { a = x; b = y; }".to_string()),
                    },
                    labels: vec![],
                }));
            }

            Err(Box::new(ParseError {
                span: self.current.span,
                kind: crate::error::ErrorKind::UnexpectedToken {
                    expected: vec![format!("'{}'", closing_token.text())],
                    found: format!("'{}'", self.current.value.text()),
                },
                labels: vec![],
            }))
        }
    }

    /// Expect specific token, advance if matches
    fn expect_token_match<F>(&mut self, predicate: F) -> Result<Ann<Token>>
    where
        F: Fn(&Token) -> bool,
    {
        if predicate(&self.current.value) {
            self.take_and_advance()
        } else {
            Err(Box::new(ParseError {
                span: self.current.span,
                kind: crate::error::ErrorKind::UnexpectedToken {
                    expected: vec![], // caller doesn't specify what's expected
                    found: format!("'{}'", self.current.value.text()),
                },
                labels: vec![],
            }))
        }
    }

    /// Expect EOF
    fn expect_eof(&self) -> Result<()> {
        if matches!(self.current.value, Token::Sof) {
            Ok(())
        } else {
            Err(Box::new(ParseError {
                span: self.current.span,
                kind: crate::error::ErrorKind::UnexpectedToken {
                    expected: vec!["end of file".to_string()],
                    found: format!("'{}'", self.current.value.text()),
                },
                labels: vec![],
            }))
        }
    }
}
