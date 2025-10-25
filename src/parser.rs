//! Hand-written recursive descent parser for Nix
//!
//! Ports parsing logic from nixfmt's Parser.hs

use crate::error::{ParseError, Result};
use crate::lexer::Lexer;
use crate::types::*;

pub(crate) struct Parser {
    lexer: Lexer,
    /// Current token
    current: Ann<Token>,
}

impl Parser {
    pub(crate) fn new(source: &str) -> Result<Self> {
        let mut lexer = Lexer::new(source);
        lexer.start_parse()?;

        // Parse first token
        let current = lexer.lexeme()?;

        Ok(Parser { lexer, current })
    }

    /// Parse a complete Nix file
    pub(crate) fn parse_file(&mut self) -> Result<File> {
        let expr = self.parse_expression()?;

        // Expect EOF
        self.expect_eof()?;

        // Get trailing trivia
        let trailing_trivia = self.lexer.finish_parse();

        Ok(Whole {
            value: expr,
            trailing_trivia,
        })
    }

    /// Parse an expression (top-level)
    fn parse_expression(&mut self) -> Result<Expression> {
        // Match Haskell's order: try operation, then abstraction, then keywords
        match &self.current.value {
            Token::KLet => self.parse_let(),
            Token::KIf => self.parse_if(),
            Token::KWith => self.parse_with(),
            Token::KAssert => self.parse_assert(),
            _ => {
                // Try parsing as abstraction first (if it looks like parameters)
                // Otherwise parse as operation
                self.parse_abstraction_or_operation()
            }
        }
    }

    /// Parse abstraction or operation (handles ambiguity)
    fn parse_abstraction_or_operation(&mut self) -> Result<Expression> {
        // Simple approach: Check what token we start with
        match &self.current.value {
            Token::TBraceOpen => {
                // Could be set literal OR set parameter
                // Parse it as set parameter if possible, otherwise as set literal
                self.parse_set_parameter_or_literal()
            }
            Token::Identifier(_) => {
                // Could be identifier parameter OR identifier term
                // Parse identifier and check for : or @
                let ident = self.take_current();
                self.advance()?;

                if matches!(self.current.value, Token::TColon) {
                    // It's a lambda: x: body
                    let colon = self.take_current();
                    self.advance()?;
                    let body = self.parse_expression()?;
                    Ok(Expression::Abstraction(
                        Parameter::IDParameter(ident),
                        colon,
                        Box::new(body),
                    ))
                } else if matches!(self.current.value, Token::TAt) {
                    // Context parameter: x @ param: body
                    let at_tok = self.take_current();
                    self.advance()?;
                    let second_param = self.parse_full_parameter()?;

                    // Check for colon - if not present, give helpful error
                    if !matches!(self.current.value, Token::TColon) {
                        return Err(ParseError::new(
                            at_tok.source_line,
                            "@ is only valid in lambda parameters",
                        ));
                    }

                    let colon = self.expect_token_match(|t| matches!(t, Token::TColon))?;
                    let body = self.parse_expression()?;
                    Ok(Expression::Abstraction(
                        Parameter::ContextParameter(
                            Box::new(Parameter::IDParameter(ident)),
                            at_tok,
                            Box::new(second_param),
                        ),
                        colon,
                        Box::new(body),
                    ))
                } else {
                    // It's just an identifier term - check for selection
                    let base_term = Term::Token(ident);
                    let term = self.parse_postfix_selection(base_term)?;
                    let term_expr = Expression::Term(term);
                    self.continue_operation_from(term_expr)
                }
            }
            _ => {
                // Parse normally as operation
                self.parse_operation_or_lambda()
            }
        }
    }

    /// Parse { as either set parameter or set literal
    fn parse_set_parameter_or_literal(&mut self) -> Result<Expression> {
        // Save complete lexer state before consuming {
        let saved_state = self.lexer.save_state();
        let saved_current = self.current.clone();

        let open_brace = self.take_current();
        self.advance()?;

        // Look at next token to decide
        match &self.current.value {
            Token::TBraceClose => {
                // Empty set: {} - could be parameter or literal
                // Check if followed by :
                let close_brace = self.take_current();
                self.advance()?;

                if matches!(self.current.value, Token::TColon) {
                    // Empty set parameter: {}: body
                    let colon = self.take_current();
                    self.advance()?;
                    let body = self.parse_expression()?;
                    Ok(Expression::Abstraction(
                        Parameter::SetParameter(open_brace, Vec::new(), close_brace),
                        colon,
                        Box::new(body),
                    ))
                } else {
                    // Empty set literal
                    let set_term = Term::Set(None, open_brace, Items(Vec::new()), close_brace);
                    self.continue_operation_from(Expression::Term(set_term))
                }
            }
            Token::Identifier(_) => {
                // Try to parse as parameter attributes first
                // If it fails (sees = or .), parse as bindings
                match self.parse_param_attrs() {
                    Ok(attrs) => {
                        // Successfully parsed as parameter attributes
                        let close_brace =
                            self.expect_token_match(|t| matches!(t, Token::TBraceClose))?;

                        // Check if followed by : or @
                        if matches!(self.current.value, Token::TColon) {
                            // Set parameter: { x, y }: body
                            let colon = self.take_current();
                            self.advance()?;
                            let body = self.parse_expression()?;
                            Ok(Expression::Abstraction(
                                Parameter::SetParameter(open_brace, attrs, close_brace),
                                colon,
                                Box::new(body),
                            ))
                        } else if matches!(self.current.value, Token::TAt) {
                            // Context parameter: { x } @ param: body
                            let at_tok = self.take_current();
                            self.advance()?;
                            let second_param = self.parse_full_parameter()?;
                            let colon = self.expect_token_match(|t| matches!(t, Token::TColon))?;
                            let body = self.parse_expression()?;
                            Ok(Expression::Abstraction(
                                Parameter::ContextParameter(
                                    Box::new(Parameter::SetParameter(
                                        open_brace,
                                        attrs,
                                        close_brace,
                                    )),
                                    at_tok,
                                    Box::new(second_param),
                                ),
                                colon,
                                Box::new(body),
                            ))
                        } else {
                            // Not a parameter - must be invalid
                            Err(ParseError::new(
                                close_brace.source_line,
                                "set with parameter-like syntax but no : - expected = for bindings",
                            ))
                        }
                    }
                    Err(_) => {
                        // Failed to parse as parameters (saw = or .)
                        // Restore complete state
                        self.lexer.restore_state(saved_state);
                        self.current = saved_current;

                        // Now parse as set literal
                        let open_brace = self.take_current();
                        self.advance()?;

                        let bindings = self.parse_binders()?;
                        let close_brace =
                            self.expect_token_match(|t| matches!(t, Token::TBraceClose))?;
                        let set_term = Term::Set(None, open_brace, bindings, close_brace);
                        self.continue_operation_from(Expression::Term(set_term))
                    }
                }
            }
            Token::TEllipsis => {
                // Definitely a parameter: { ... }
                let attrs = self.parse_param_attrs()?;
                let close_brace = self.expect_token_match(|t| matches!(t, Token::TBraceClose))?;

                if matches!(self.current.value, Token::TColon) {
                    let colon = self.take_current();
                    self.advance()?;
                    let body = self.parse_expression()?;
                    Ok(Expression::Abstraction(
                        Parameter::SetParameter(open_brace, attrs, close_brace),
                        colon,
                        Box::new(body),
                    ))
                } else {
                    Err(ParseError::new(
                        close_brace.source_line,
                        "{ ... } must be followed by :",
                    ))
                }
            }
            _ => {
                // Must be set literal with bindings
                let bindings = self.parse_binders()?;
                let close_brace = self.expect_token_match(|t| matches!(t, Token::TBraceClose))?;
                let set_term = Term::Set(None, open_brace, bindings, close_brace);
                self.continue_operation_from(Expression::Term(set_term))
            }
        }
    }

    /// Continue parsing operation from a given left expression
    fn continue_operation_from(&mut self, expr: Expression) -> Result<Expression> {
        // Check for member check (?), binary operations, or application
        if matches!(self.current.value, Token::TQuestion) {
            let question = self.take_current();
            self.advance()?;
            let selectors = self.parse_selector_path()?;
            let expr = Expression::MemberCheck(Box::new(expr), question, selectors);

            if self.is_binary_op() {
                self.parse_binary_operation(expr, 0)
            } else {
                Ok(expr)
            }
        } else if self.is_binary_op() {
            self.parse_binary_operation(expr, 0)
        } else if self.is_term_start() {
            // Application
            let mut app_expr = expr;
            while self.is_term_start() && !self.is_expression_end() {
                let arg = self.parse_term_expr()?;
                app_expr = Expression::Application(Box::new(app_expr), Box::new(arg));
            }

            if self.is_binary_op() {
                self.parse_binary_operation(app_expr, 0)
            } else {
                Ok(app_expr)
            }
        } else {
            Ok(expr)
        }
    }

    /// Parse operation or lambda (needs lookahead for :)
    fn parse_operation_or_lambda(&mut self) -> Result<Expression> {
        // Try to parse initial term/application
        let mut expr = self.parse_application()?;

        // Check for @ (context parameter) - special case
        if matches!(self.current.value, Token::TAt) {
            // This is a context parameter pattern: param @ param
            let at_tok = self.take_current();
            self.advance()?;

            // Parse second part as a PARAMETER (not expression)
            let second_param = self.parse_full_parameter()?;

            // Now we MUST see a colon for this to be valid
            if matches!(self.current.value, Token::TColon) {
                let first_param = self.expr_to_parameter(expr)?;
                let param = Parameter::ContextParameter(
                    Box::new(first_param),
                    at_tok,
                    Box::new(second_param),
                );
                let colon = self.expect_token_match(|t| matches!(t, Token::TColon))?;
                let body = self.parse_expression()?;
                return Ok(Expression::Abstraction(param, colon, Box::new(body)));
            } else {
                return Err(ParseError::new(
                    at_tok.source_line,
                    "@ is only valid in lambda parameters",
                ));
            }
        }

        // Check for member check (? operator) - postfix
        if matches!(self.current.value, Token::TQuestion) {
            let question = self.take_current();
            self.advance()?;

            // Parse selector path
            let selectors = self.parse_selector_path()?;

            expr = Expression::MemberCheck(Box::new(expr), question, selectors);
        }

        // Check if there's a colon (lambda)
        if matches!(self.current.value, Token::TColon) {
            // It's a lambda! Convert expr back to parameter
            let param = self.expr_to_parameter(expr)?;
            let colon = self.expect_token_match(|t| matches!(t, Token::TColon))?;
            let body = self.parse_expression()?;
            Ok(Expression::Abstraction(param, colon, Box::new(body)))
        } else if self.is_binary_op() {
            // It's a binary operation
            self.parse_binary_operation(expr, 0)
        } else {
            // Just return the expression
            Ok(expr)
        }
    }

    /// Parse selector path: .attr or .attr.attr
    fn parse_selector_path(&mut self) -> Result<Vec<Selector>> {
        let mut selectors = Vec::new();

        // First selector (no dot)
        let first_sel = self.parse_simple_selector()?;
        selectors.push(Selector {
            dot: None,
            selector: first_sel,
        });

        // Additional selectors with dots
        while matches!(self.current.value, Token::TDot) {
            let dot = self.take_current();
            self.advance()?;

            let simple_sel = self.parse_simple_selector()?;
            selectors.push(Selector {
                dot: Some(dot),
                selector: simple_sel,
            });
        }

        Ok(selectors)
    }

    /// Convert expression to parameter (for lambda detection)
    fn expr_to_parameter(&self, expr: Expression) -> Result<Parameter> {
        match expr {
            Expression::Term(Term::Token(ann)) => {
                if matches!(ann.value, Token::Identifier(_)) {
                    Ok(Parameter::IDParameter(ann))
                } else {
                    Err(ParseError::new(ann.source_line, "expected parameter"))
                }
            }
            Expression::Term(Term::Set(None, open, items, close)) => {
                // Convert set literal to set parameter
                // This happens for: { x, y }: or { x ? 1 }: patterns
                let attrs = self.items_to_param_attrs(items)?;
                Ok(Parameter::SetParameter(open, attrs, close))
            }
            _ => Err(ParseError::new(
                Pos(1),
                "complex parameters not yet supported",
            )),
        }
    }

    /// Convert Items<Binder> to Vec<ParamAttr>
    fn items_to_param_attrs(&self, items: Items<Binder>) -> Result<Vec<ParamAttr>> {
        let mut attrs = Vec::new();

        for item in items.0 {
            match item {
                Item::Item(binder) => {
                    // Convert binder to param attr
                    match binder {
                        Binder::Assignment(mut sels, _eq, expr, comma_or_semi) => {
                            // Should be single identifier selector
                            if sels.len() == 1 {
                                if let Some(Selector {
                                    dot: None,
                                    selector: SimpleSelector::IDSelector(name),
                                }) = sels.pop()
                                {
                                    // Check if expr indicates a default (x ? default pattern)
                                    // For simplicity, we'll treat any assignment as x ? default
                                    let default = Some((
                                        Ann::new(Token::TQuestion, name.source_line), // Fake ? token
                                        expr,
                                    ));
                                    let comma = Some(comma_or_semi);
                                    attrs.push(ParamAttr::ParamAttr(
                                        name,
                                        Box::new(default),
                                        comma,
                                    ));
                                } else {
                                    return Err(ParseError::new(
                                        Pos(1),
                                        "invalid parameter attribute",
                                    ));
                                }
                            } else {
                                return Err(ParseError::new(Pos(1), "invalid parameter selector"));
                            }
                        }
                        Binder::Inherit(_, _, _, dots) => {
                            // Might be ellipsis
                            attrs.push(ParamAttr::ParamEllipsis(dots));
                        }
                    }
                }
                Item::Comments(_) => {
                    // Skip comments in conversion
                }
            }
        }

        Ok(attrs)
    }

    /// Parse parameter for lambda (handles all parameter types)
    fn parse_full_parameter(&mut self) -> Result<Parameter> {
        // Check for set parameter or context parameter
        if matches!(self.current.value, Token::TBraceOpen) {
            self.parse_set_or_context_parameter()
        } else if matches!(self.current.value, Token::Identifier(_)) {
            // Could be identifier or context parameter (id @ pattern)
            let ident = self.take_current();
            self.advance()?;

            if matches!(self.current.value, Token::TAt) {
                // Context parameter: id @ pattern
                let at_tok = self.take_current();
                self.advance()?;
                let second = self.parse_full_parameter()?;
                Ok(Parameter::ContextParameter(
                    Box::new(Parameter::IDParameter(ident)),
                    at_tok,
                    Box::new(second),
                ))
            } else {
                Ok(Parameter::IDParameter(ident))
            }
        } else {
            Err(ParseError::new(
                self.current.source_line,
                "expected parameter",
            ))
        }
    }

    /// Parse set parameter or context parameter starting with {
    fn parse_set_or_context_parameter(&mut self) -> Result<Parameter> {
        let open_brace = self.expect_token_match(|t| matches!(t, Token::TBraceOpen))?;

        // Parse parameter attributes
        let attrs = self.parse_param_attrs()?;

        let close_brace = self.expect_token_match(|t| matches!(t, Token::TBraceClose))?;

        let set_param = Parameter::SetParameter(open_brace, attrs, close_brace);

        // Check for @ (context parameter)
        if matches!(self.current.value, Token::TAt) {
            let at_tok = self.take_current();
            self.advance()?;
            let second = self.parse_full_parameter()?;
            Ok(Parameter::ContextParameter(
                Box::new(set_param),
                at_tok,
                Box::new(second),
            ))
        } else {
            Ok(set_param)
        }
    }

    /// Parse parameter attributes: x, y, z ? 1, ...
    /// Returns Err if this looks like bindings (sees = or .) instead
    fn parse_param_attrs(&mut self) -> Result<Vec<ParamAttr>> {
        let mut attrs = Vec::new();

        while !matches!(self.current.value, Token::TBraceClose | Token::SOF) {
            if matches!(self.current.value, Token::TEllipsis) {
                // Ellipsis
                let dots = self.take_current();
                self.advance()?;
                attrs.push(ParamAttr::ParamEllipsis(dots));

                // Optional comma after ellipsis
                if matches!(self.current.value, Token::TComma) {
                    self.advance()?;
                }
                break; // Ellipsis must be last
            } else if matches!(self.current.value, Token::Identifier(_)) {
                let name = self.take_current();
                self.advance()?;

                // Check what follows the identifier
                if matches!(self.current.value, Token::TAssign | Token::TDot) {
                    // This is a binding (a = ...), not a parameter!
                    return Err(ParseError::new(
                        name.source_line,
                        "not a parameter - looks like binding",
                    ));
                }

                // Check for ? default
                let default = if matches!(self.current.value, Token::TQuestion) {
                    let q = self.take_current();
                    self.advance()?;
                    let def_expr = self.parse_expression()?;
                    Some((q, def_expr))
                } else {
                    None
                };

                // Check for comma
                let comma = if matches!(self.current.value, Token::TComma) {
                    let c = self.take_current();
                    self.advance()?;
                    Some(c)
                } else {
                    None
                };

                attrs.push(ParamAttr::ParamAttr(name, Box::new(default), comma));
            } else {
                break;
            }
        }

        Ok(attrs)
    }

    /// Parse let expression: let bindings in expr
    fn parse_let(&mut self) -> Result<Expression> {
        let let_tok = self.expect_token_match(|t| matches!(t, Token::KLet))?;
        let bindings = self.parse_binders()?;
        let in_tok = self.expect_token_match(|t| matches!(t, Token::KIn))?;
        let body = self.parse_expression()?;

        Ok(Expression::Let(let_tok, bindings, in_tok, Box::new(body)))
    }

    /// Parse if expression: if cond then expr else expr
    fn parse_if(&mut self) -> Result<Expression> {
        let if_tok = self.expect_token_match(|t| matches!(t, Token::KIf))?;
        let cond = self.parse_expression()?;
        let then_tok = self.expect_token_match(|t| matches!(t, Token::KThen))?;
        let then_expr = self.parse_expression()?;
        let else_tok = self.expect_token_match(|t| matches!(t, Token::KElse))?;
        let else_expr = self.parse_expression()?;

        Ok(Expression::If(
            if_tok,
            Box::new(cond),
            then_tok,
            Box::new(then_expr),
            else_tok,
            Box::new(else_expr),
        ))
    }

    /// Parse with expression: with expr ; expr
    fn parse_with(&mut self) -> Result<Expression> {
        let with_tok = self.expect_token_match(|t| matches!(t, Token::KWith))?;
        let expr1 = self.parse_expression()?;
        let semi = self.expect_token_match(|t| matches!(t, Token::TSemicolon))?;
        let expr2 = self.parse_expression()?;

        Ok(Expression::With(
            with_tok,
            Box::new(expr1),
            semi,
            Box::new(expr2),
        ))
    }

    /// Parse assert expression: assert cond ; expr
    fn parse_assert(&mut self) -> Result<Expression> {
        let assert_tok = self.expect_token_match(|t| matches!(t, Token::KAssert))?;
        let cond = self.parse_expression()?;
        let semi = self.expect_token_match(|t| matches!(t, Token::TSemicolon))?;
        let body = self.parse_expression()?;

        Ok(Expression::Assert(
            assert_tok,
            Box::new(cond),
            semi,
            Box::new(body),
        ))
    }

    /// Parse function application (left-associative)
    /// Application only consumes TERMS, not unary expressions
    fn parse_application(&mut self) -> Result<Expression> {
        // Check for prefix unary operators BEFORE parsing application
        match &self.current.value {
            Token::TMinus => {
                let op = self.take_current();
                self.advance()?;
                let expr = self.parse_application()?; // Right-associative
                return Ok(Expression::Negation(op, Box::new(expr)));
            }
            Token::TNot => {
                let op = self.take_current();
                self.advance()?;
                let expr = self.parse_application()?; // Right-associative
                return Ok(Expression::Inversion(op, Box::new(expr)));
            }
            _ => {}
        }

        // Parse first term
        let mut expr = self.parse_term_expr()?;

        // Keep applying while we see more TERMS (not unary ops)
        // IMPORTANT: Don't treat binary operators as term starts even if they could start paths
        while self.is_term_start() && !self.is_binary_op() && !self.is_expression_end() {
            let arg = self.parse_term_expr()?;
            expr = Expression::Application(Box::new(expr), Box::new(arg));
        }

        Ok(expr)
    }

    /// Parse term as expression
    fn parse_term_expr(&mut self) -> Result<Expression> {
        let term = self.parse_term()?;
        Ok(Expression::Term(term))
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
        matches!(
            self.current.value,
            Token::TPlus
                | Token::TMinus
                | Token::TMul
                | Token::TDiv
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
                | Token::TPipeBackward
        )
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
                | Token::SOF
        )
    }

    /// Parse binary operation with precedence climbing
    fn parse_binary_operation(&mut self, mut left: Expression, min_prec: u8) -> Result<Expression> {
        let mut last_was_comparison = false;
        while self.is_binary_op() && self.get_precedence() >= min_prec {
            let op_token = self.take_current();
            let is_comparison = Self::is_comparison_operator(&op_token.value);
            if last_was_comparison && is_comparison {
                return Err(ParseError::new(
                    op_token.source_line,
                    "comparison operators cannot be chained",
                ));
            }
            let prec = self.get_precedence_for(&op_token.value);
            let is_right_assoc = self.is_right_associative(&op_token.value);
            self.advance()?;

            let mut right = self.parse_application()?;

            // For right-associative operators, use >= to allow same-precedence operators to bind right
            // For left-associative operators, use > to make them bind left
            while self.is_binary_op()
                && (self.get_precedence() > prec
                    || (self.get_precedence() == prec && is_right_assoc))
            {
                right = self.parse_binary_operation(right, self.get_precedence())?;
            }

            left = Expression::Operation(Box::new(left), op_token, Box::new(right));
            last_was_comparison = is_comparison;
        }

        Ok(left)
    }

    /// Get precedence of current operator
    fn get_precedence(&self) -> u8 {
        self.get_precedence_for(&self.current.value)
    }

    /// Get precedence for a token (higher = tighter binding)
    fn get_precedence_for(&self, token: &Token) -> u8 {
        match token {
            Token::TOr => 1,
            Token::TAnd => 2,
            Token::TEqual | Token::TUnequal => 3,
            Token::TLess | Token::TGreater | Token::TLessEqual | Token::TGreaterEqual => 4,
            Token::TUpdate => 5,
            Token::TPlus | Token::TMinus => 6,
            Token::TMul | Token::TDiv => 7,
            Token::TConcat => 8,
            Token::TImplies => 9,
            _ => 0,
        }
    }

    /// Check if an operator is right-associative
    ///
    /// Right-associative operators: ++, //, +
    /// (Note: + follows nixfmt's AST conversion hack where it's
    /// parsed as right-associative for formatting purposes, even though
    /// it's defined as InfixL in nixfmt's Types.hs)
    fn is_right_associative(&self, token: &Token) -> bool {
        matches!(token, Token::TConcat | Token::TUpdate | Token::TPlus)
    }

    /// Parse binders (for let and attribute sets)
    fn parse_binders(&mut self) -> Result<Items<Binder>> {
        let mut items = Vec::new();

        while !matches!(
            self.current.value,
            Token::KIn | Token::TBraceClose | Token::SOF
        ) {
            // Check for comments before binding
            if !self.current.pre_trivia.is_empty() {
                items.push(Item::Comments(std::mem::take(&mut self.current.pre_trivia)));
            }

            let binder = self.parse_binder()?;
            items.push(Item::Item(binder));
        }

        if matches!(self.current.value, Token::KIn | Token::TBraceClose)
            && !self.current.pre_trivia.is_empty()
        {
            items.push(Item::Comments(std::mem::take(&mut self.current.pre_trivia)));
        }

        Ok(Items(items))
    }

    /// Parse a single binder (inherit or assignment)
    fn parse_binder(&mut self) -> Result<Binder> {
        if matches!(self.current.value, Token::KInherit) {
            self.parse_inherit()
        } else {
            self.parse_assignment()
        }
    }

    /// Parse inherit statement: inherit [ (expr) ] names... ;
    fn parse_inherit(&mut self) -> Result<Binder> {
        let inherit_tok = self.expect_token_match(|t| matches!(t, Token::KInherit))?;

        // Check for optional (expr)
        let from = if matches!(self.current.value, Token::TParenOpen) {
            let open = self.take_current();
            self.advance()?;
            let expr = self.parse_expression()?;
            let close = self.expect_token_match(|t| matches!(t, Token::TParenClose))?;
            Some(Term::Parenthesized(open, Box::new(expr), close))
        } else {
            None
        };

        // Parse selectors (identifiers, strings, interpolations)
        let mut selectors = Vec::new();
        while self.is_simple_selector_start() {
            let sel = self.parse_simple_selector()?;
            selectors.push(sel);
        }

        let semi = self.expect_token_match(|t| matches!(t, Token::TSemicolon))?;

        Ok(Binder::Inherit(inherit_tok, from, selectors, semi))
    }

    /// Parse assignment: selector = expr ;
    fn parse_assignment(&mut self) -> Result<Binder> {
        // Parse left-hand side (selector path)
        let mut selectors = Vec::new();

        // Parse at least one selector
        let first_sel = self.parse_selector()?;
        selectors.push(first_sel);

        // Parse additional selectors if present
        while matches!(self.current.value, Token::TDot) {
            let dot = self.take_current();
            self.advance()?;

            // Parse simple selector after dot
            let simple_sel = self.parse_simple_selector()?;
            selectors.push(Selector {
                dot: Some(dot),
                selector: simple_sel,
            });
        }

        let eq = self.expect_token_match(|t| matches!(t, Token::TAssign))?;
        let expr = self.parse_expression()?;
        let semi = self.expect_token_match(|t| matches!(t, Token::TSemicolon))?;

        Ok(Binder::Assignment(selectors, eq, expr, semi))
    }

    /// Parse a selector (with optional dot)
    fn parse_selector(&mut self) -> Result<Selector> {
        let simple_sel = self.parse_simple_selector()?;
        Ok(Selector {
            dot: None,
            selector: simple_sel,
        })
    }

    /// Parse simple selector (identifier, string, or interpolation)
    fn parse_simple_selector(&mut self) -> Result<SimpleSelector> {
        match &self.current.value {
            Token::Identifier(_) => {
                let ident = self.take_current();
                self.advance()?;
                Ok(SimpleSelector::IDSelector(ident))
            }
            Token::TDoubleQuote => {
                let string = self.parse_simple_string_literal()?;
                Ok(SimpleSelector::StringSelector(string))
            }
            Token::TInterOpen => {
                let interpol = self.parse_selector_interpolation()?;
                Ok(SimpleSelector::InterpolSelector(interpol))
            }
            _ => Err(ParseError::new(
                self.current.source_line,
                "expected selector",
            )),
        }
    }

    /// Parse a term (atom), including postfix selection
    fn parse_term(&mut self) -> Result<Term> {
        // Check for paths first (they can start with identifiers)
        if self.looks_like_path() {
            return self.parse_path();
        }

        let base_term = match &self.current.value {
            Token::Identifier(_) => self.parse_identifier_term(),
            Token::Integer(_) => self.parse_integer_term(),
            Token::Float(_) => self.parse_float_term(),
            Token::EnvPath(_) => self.parse_env_path_term(),
            Token::TBraceOpen | Token::KRec => self.parse_set(),
            Token::TBrackOpen => self.parse_list(),
            Token::TParenOpen => self.parse_parenthesized(),
            Token::TDoubleQuote => self.parse_simple_string(),
            Token::TDoubleSingleQuote => self.parse_indented_string(),
            _ => Err(ParseError::new(
                self.current.source_line,
                format!("unexpected token: {:?}", self.current.value),
            )),
        }?;

        // Check for selection (.attr) or or-default
        self.parse_postfix_selection(base_term)
    }

    /// Check if current position starts a path
    /// Must check BEFORE consuming any tokens
    fn looks_like_path(&self) -> bool {
        // Paths start with: ~/, ./, ../, or /
        match &self.current.value {
            Token::TDot => {
                // Could be ./ or ../
                self.lexer.peek() == Some('/')
                    || (self.lexer.peek() == Some('.') && self.lexer.peek_ahead(1) == Some('/'))
            }
            Token::TDiv => {
                // / at start - this is a path IF it's followed immediately by path chars
                // Path chars are: alphanumeric, ., _, -, +, ~, or ${ for interpolation
                // Key: there must be NO whitespace between / and the path char
                match self.lexer.peek() {
                    Some(c) if c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '+' | '~') => {
                        true
                    }
                    Some('$') => self.lexer.peek_ahead(1) == Some('{'), // interpolation ${
                    _ => false,
                }
            }
            Token::TTilde => {
                // ~ - check if followed by /
                self.lexer.peek() == Some('/')
            }
            _ => false,
        }
    }

    /// Parse path: ./foo/bar or ~/foo or /foo
    /// Based on Haskell's path parser
    fn parse_path(&mut self) -> Result<Term> {
        let start_pos = self.current.source_line;
        let pre_trivia = self.current.pre_trivia.clone();
        let mut parts = Vec::new();

        // Handle the prefix that was already tokenized
        // NOTE: Don't call self.advance() here - we need to read raw chars from lexer
        match &self.current.value {
            Token::TDot => {
                // ./ or ../
                // The lexer is positioned just after the '.' character
                if self.lexer.peek() == Some('.') {
                    // ../
                    parts.push(StringPart::TextPart("..".to_string()));
                    self.lexer.advance();
                } else {
                    // ./
                    parts.push(StringPart::TextPart(".".to_string()));
                }
                // Now expect /
                if self.lexer.peek() == Some('/') {
                    self.lexer.advance();
                    if let Some(StringPart::TextPart(text)) = parts.last_mut() {
                        text.push('/');
                    }
                }
            }
            Token::TDiv => {
                // Absolute path /
                // The lexer is positioned just after the '/' character
                // Don't call self.advance() - just start with "/"
                parts.push(StringPart::TextPart("/".to_string()));
            }
            Token::TTilde => {
                // ~/
                // The lexer is positioned just after the '~' character
                parts.push(StringPart::TextPart("~".to_string()));
                if self.lexer.peek() == Some('/') {
                    self.lexer.advance();
                    if let Some(StringPart::TextPart(text)) = parts.last_mut() {
                        text.push('/');
                    }
                }
            }
            _ => {}
        }

        // Parse rest of path
        loop {
            match self.lexer.peek() {
                Some('$') if self.lexer.peek_ahead(1) == Some('{') => {
                    // Interpolation in path
                    let (interp, trailing_ws) = self.parse_string_interpolation()?;
                    parts.push(interp);
                    if let Some(ws) = trailing_ws {
                        if !ws.is_empty() {
                            parts.push(StringPart::TextPart(ws));
                        }
                    }
                }
                Some(ch) if ch.is_alphanumeric() || "._-+".contains(ch) => {
                    // Path text (not / here, that's handled specially)
                    let text = self.parse_path_part()?;
                    if !text.is_empty() {
                        // Append to last TextPart if it exists, otherwise create new one
                        if let Some(StringPart::TextPart(last_text)) = parts.last_mut() {
                            last_text.push_str(&text);
                        } else {
                            parts.push(StringPart::TextPart(text));
                        }
                    }
                }
                Some('/') => {
                    // Path separator
                    self.lexer.advance();
                    if let Some(StringPart::TextPart(text)) = parts.last_mut() {
                        text.push('/');
                    } else {
                        parts.push(StringPart::TextPart("/".to_string()));
                    }
                }
                _ => break,
            }
        }

        // Re-sync parser
        self.current = self.lexer.lexeme()?;

        let ann = Ann {
            pre_trivia,
            source_line: start_pos,
            value: parts,
            trail_comment: None,
        };

        Ok(Term::Path(ann))
    }

    /// Parse path text component (without /)
    /// Based on Haskell's pathText
    fn parse_path_part(&mut self) -> Result<String> {
        let mut text = String::new();

        while let Some(ch) = self.lexer.peek() {
            if ch.is_alphanumeric() || "._-+~".contains(ch) {
                text.push(ch);
                self.lexer.advance();
            } else if ch == '$' && self.lexer.peek_ahead(1) == Some('{') {
                // Interpolation coming
                break;
            } else if ch == '/' {
                // Don't consume / here - it's handled in the main loop
                break;
            } else {
                break;
            }
        }

        Ok(text)
    }

    /// Parse postfix selection: term.attr.attr or term.attr or term
    fn parse_postfix_selection(&mut self, base_term: Term) -> Result<Term> {
        let mut selectors = Vec::new();

        // Parse .attr chains
        while matches!(self.current.value, Token::TDot) {
            let dot_token = self.current.clone();
            let saved_state = self.lexer.save_state();

            self.advance()?;
            let is_selector_start = matches!(
                self.current.value,
                Token::Identifier(_) | Token::TDoubleQuote | Token::TInterOpen
            );

            if !is_selector_start {
                self.lexer.restore_state(saved_state);
                self.current = dot_token;
                break;
            }

            let simple_sel = self.parse_simple_selector()?;
            selectors.push(Selector {
                dot: Some(dot_token),
                selector: simple_sel,
            });
        }

        // Check for 'or' default (only valid if we have at least one selector)
        let or_default = if !selectors.is_empty() && self.is_or_token() {
            let mut or_tok = self.take_current();
            if matches!(
                &or_tok.value,
                Token::Identifier(name) if name == "or"
            ) {
                or_tok.value = Token::KOr;
            }
            self.advance()?;
            let default_term = self.parse_term()?;
            Some((or_tok, Box::new(default_term)))
        } else {
            None
        };

        if !selectors.is_empty() {
            Ok(Term::Selection(Box::new(base_term), selectors, or_default))
        } else {
            Ok(base_term)
        }
    }

    /// Parse EnvPath as a term (Token)
    fn parse_env_path_term(&mut self) -> Result<Term> {
        let token_ann = self.take_current();
        self.advance()?;
        Ok(Term::Token(token_ann))
    }

    /// Parse simple string literal and return annotated string structure
    fn parse_simple_string_literal(&mut self) -> Result<Ann<Vec<Vec<StringPart>>>> {
        let open_quote_pos = self.current.source_line;
        let pre_trivia = self.current.pre_trivia.clone();

        // DON'T advance - just verify we're at a quote
        if !matches!(self.current.value, Token::TDoubleQuote) {
            return Err(ParseError::new(open_quote_pos, "expected opening quote"));
        }

        let _opening_quote = self.take_current();
        // DON'T call advance() - parse raw characters directly

        // Now parse string content directly from lexer.input
        let mut parts = Vec::new();

        loop {
            match self.lexer.peek() {
                Some('"') => {
                    // End of string
                    break;
                }
                None => {
                    return Err(ParseError::new(open_quote_pos, "unclosed string"));
                }
                Some('$') if self.lexer.peek_ahead(1) == Some('{') => {
                    // Interpolation
                    let (interp, trailing_ws) = self.parse_string_interpolation()?;
                    parts.push(interp);
                    if let Some(ws) = trailing_ws {
                        if !ws.is_empty() {
                            parts.push(StringPart::TextPart(ws));
                        }
                    }
                }
                _ => {
                    // Text part
                    let text = self.parse_simple_string_part()?;
                    if !text.is_empty() {
                        parts.push(StringPart::TextPart(text));
                    }
                }
            }
        }

        // Consume closing "
        self.lexer.advance();
        self.current = self.lexer.lexeme()?;

        let lines = fix_simple_string(parts);

        Ok(Ann {
            pre_trivia,
            source_line: open_quote_pos,
            value: lines,
            trail_comment: None,
        })
    }

    /// Parse simple string: "..."
    /// Parses string content directly from source (not tokens!)
    fn parse_simple_string(&mut self) -> Result<Term> {
        let ann = self.parse_simple_string_literal()?;
        Ok(Term::SimpleString(ann))
    }

    /// Parse a text part in a simple string (handles escapes)
    /// Based on Haskell's simpleStringPart
    fn parse_simple_string_part(&mut self) -> Result<String> {
        let mut text = String::new();

        loop {
            match self.lexer.peek() {
                Some('"') | None => break,
                Some('$') if self.lexer.peek_ahead(1) == Some('{') => break,
                Some('\\') => {
                    // Escape sequence
                    self.lexer.advance(); // consume \
                    match self.lexer.peek() {
                        Some('n') => {
                            text.push_str("\\n"); // Keep escaped form
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
                            // Keep as \x
                            text.push('\\');
                            text.push(ch);
                            self.lexer.advance();
                        }
                        None => break,
                    }
                }
                Some('$') if self.lexer.peek_ahead(1) == Some('$') => {
                    // $$ -> single $
                    text.push_str("$$"); // Keep as $$
                    self.lexer.advance();
                    self.lexer.advance();
                }
                Some('$') => {
                    // Lone $
                    text.push('$');
                    self.lexer.advance();
                }
                Some(ch) => {
                    text.push(ch);
                    self.lexer.advance();
                }
            }
        }

        Ok(text)
    }

    /// Parse string interpolation: ${expr}
    fn parse_string_interpolation(&mut self) -> Result<(StringPart, Option<String>)> {
        // Consume ${
        self.lexer.advance(); // $
        self.lexer.advance(); // {

        // Re-sync parser
        self.current = self.lexer.lexeme()?;

        // Parse expression
        let expr = self.parse_expression()?;

        // Verify we're at }
        if !matches!(self.current.value, Token::TBraceClose) {
            return Err(ParseError::new(
                self.current.source_line,
                format!(
                    "expected }} in interpolation, found {:?}",
                    self.current.value
                ),
            ));
        }

        // The } token was already consumed by the lexer when creating TBraceClose
        // So lexer.pos is already AFTER the }
        // DON'T call advance() or lexer.advance() - just continue from current lexer.pos

        // Now lexer.pos is right after }, and we can continue parsing string content
        // DON'T resync current - we'll continue with raw parsing

        let mut back = self.lexer.pos;
        while back > 0 {
            let ch = self.lexer.input[back - 1];
            if ch == ' ' || ch == '\t' {
                back -= 1;
            } else {
                break;
            }
        }
        let trailing_ws = if back < self.lexer.pos {
            Some(
                self.lexer.input[back..self.lexer.pos]
                    .iter()
                    .collect::<String>(),
            )
        } else {
            None
        };

        Ok((
            StringPart::Interpolation(Box::new(Whole {
                value: expr,
                trailing_trivia: Trivia::new(),
            })),
            trailing_ws,
        ))
    }

    /// Parse indented string: ''...''
    /// Based on Haskell's indentedString parser
    fn parse_indented_string(&mut self) -> Result<Term> {
        let open_quote_pos = self.current.source_line;
        let pre_trivia = self.current.pre_trivia.clone();

        // Take the opening '' token (don't advance - just take it)
        let _opening = self.take_current();
        // Now lexer.pos is right after the '' token

        // Parse lines (separated by \n)
        let mut lines = Vec::new();
        lines.push(self.parse_indented_string_line()?);

        // Parse additional lines
        while self.lexer.peek() == Some('\n') {
            self.lexer.advance(); // consume \n
            lines.push(self.parse_indented_string_line()?);
        }

        // Expect closing ''
        if self.lexer.peek() != Some('\'') || self.lexer.peek_ahead(1) != Some('\'') {
            return Err(ParseError::new(open_quote_pos, "unclosed indented string"));
        }
        self.lexer.advance(); // '
        self.lexer.advance(); // '

        // Re-sync
        self.current = self.lexer.lexeme()?;

        let lines = fix_indented_string(lines);

        let ann = Ann {
            pre_trivia,
            source_line: open_quote_pos,
            value: lines,
            trail_comment: None,
        };

        Ok(Term::IndentedString(ann))
    }

    /// Parse one line of an indented string
    /// Based on Haskell's indentedLine
    fn parse_indented_string_line(&mut self) -> Result<Vec<StringPart>> {
        let mut parts = Vec::new();

        loop {
            match self.lexer.peek() {
                Some('\'') if self.lexer.peek_ahead(1) == Some('\'') => {
                    // Could be end or escape
                    if matches!(
                        self.lexer.peek_ahead(2),
                        Some('$') | Some('\'') | Some('\\')
                    ) {
                        // Escape sequence: parse it
                        let text = self.parse_indented_string_part()?;
                        if !text.is_empty() {
                            parts.push(StringPart::TextPart(text));
                        }
                    } else {
                        // End of string
                        break;
                    }
                }
                Some('$') if self.lexer.peek_ahead(1) == Some('{') => {
                    // Interpolation
                    let (interp, trailing_ws) = self.parse_string_interpolation()?;
                    parts.push(interp);
                    if let Some(ws) = trailing_ws {
                        if !ws.is_empty() {
                            parts.push(StringPart::TextPart(ws));
                        }
                    }
                }
                Some('\n') | None => {
                    // End of line
                    break;
                }
                _ => {
                    // Regular text
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
            match self.lexer.peek() {
                None | Some('\n') => break,
                Some('\'') if self.lexer.peek_ahead(1) == Some('\'') => {
                    // Check for escape sequences
                    match self.lexer.peek_ahead(2) {
                        Some('$') => {
                            // ''$ -> $
                            text.push_str("''$");
                            self.lexer.advance();
                            self.lexer.advance();
                            self.lexer.advance();
                        }
                        Some('\'') => {
                            // ''' -> '
                            text.push_str("'''");
                            self.lexer.advance();
                            self.lexer.advance();
                            self.lexer.advance();
                        }
                        Some('\\') => {
                            // ''\ escapes next char
                            text.push_str("''\\");
                            self.lexer.advance();
                            self.lexer.advance();
                            self.lexer.advance();
                            if let Some(ch) = self.lexer.peek() {
                                text.push(ch);
                                self.lexer.advance();
                            }
                        }
                        _ => {
                            // Not an escape, end of string
                            break;
                        }
                    }
                }
                Some('$') if self.lexer.peek_ahead(1) == Some('{') => break,
                Some('$') if self.lexer.peek_ahead(1) == Some('$') => {
                    // $$ in indented string
                    text.push_str("$$");
                    self.lexer.advance();
                    self.lexer.advance();
                }
                Some('$') => {
                    // Lone $
                    text.push('$');
                    self.lexer.advance();
                }
                Some('\'') if self.lexer.peek_ahead(1) != Some('\'') => {
                    // Single '
                    text.push('\'');
                    self.lexer.advance();
                }
                Some(ch) => {
                    text.push(ch);
                    self.lexer.advance();
                }
            }
        }

        Ok(text)
    }

    /// Parse identifier as a term (Token)
    fn parse_identifier_term(&mut self) -> Result<Term> {
        let token_ann = self.take_current();
        self.advance()?;
        Ok(Term::Token(token_ann))
    }

    /// Parse integer as a term (Token)
    fn parse_integer_term(&mut self) -> Result<Term> {
        let token_ann = self.take_current();
        self.advance()?;
        Ok(Term::Token(token_ann))
    }

    /// Parse float as a term (Token)
    fn parse_float_term(&mut self) -> Result<Term> {
        let token_ann = self.take_current();
        self.advance()?;
        Ok(Term::Token(token_ann))
    }

    /// Parse attribute set: { ... } or rec { ... }
    fn parse_set(&mut self) -> Result<Term> {
        // Check for 'rec' keyword
        let rec_tok = if matches!(self.current.value, Token::KRec) {
            let tok = self.take_current();
            self.advance()?;
            Some(tok)
        } else {
            None
        };

        let open_brace = self.expect_token_match(|t| matches!(t, Token::TBraceOpen))?;

        // Parse bindings
        let bindings = self.parse_binders()?;

        let close_brace = self.expect_token_match(|t| matches!(t, Token::TBraceClose))?;

        Ok(Term::Set(rec_tok, open_brace, bindings, close_brace))
    }

    /// Parse list: [ ... ]
    fn parse_list(&mut self) -> Result<Term> {
        let open_bracket = self.expect_token_match(|t| matches!(t, Token::TBrackOpen))?;

        // Parse list items (terms separated by whitespace)
        let items = self.parse_list_items()?;

        let close_bracket = self.expect_token_match(|t| matches!(t, Token::TBrackClose))?;

        Ok(Term::List(open_bracket, items, close_bracket))
    }

    /// Parse list items (terms)
    fn parse_list_items(&mut self) -> Result<Items<Term>> {
        let mut items = Vec::new();

        while !matches!(self.current.value, Token::TBrackClose | Token::SOF) {
            // Check for comments before item
            if !self.current.pre_trivia.is_empty() {
                items.push(Item::Comments(std::mem::take(&mut self.current.pre_trivia)));
            }

            // Parse a term
            let term = self.parse_term()?;
            items.push(Item::Item(term));
        }

        if matches!(self.current.value, Token::TBrackClose) && !self.current.pre_trivia.is_empty() {
            items.push(Item::Comments(std::mem::take(&mut self.current.pre_trivia)));
        }

        Ok(Items(items))
    }

    /// Parse parenthesized expression: ( expr )
    fn parse_parenthesized(&mut self) -> Result<Term> {
        let open_paren = self.expect_token_match(|t| matches!(t, Token::TParenOpen))?;

        let expr = self.parse_expression()?;

        let close_paren = self.expect_token_match(|t| matches!(t, Token::TParenClose))?;

        Ok(Term::Parenthesized(open_paren, Box::new(expr), close_paren))
    }

    // Helper methods

    /// Advance to next token
    fn advance(&mut self) -> Result<()> {
        self.current = self.lexer.lexeme()?;
        Ok(())
    }

    /// Take current token (consumes it)
    fn take_current(&mut self) -> Ann<Token> {
        // Create a dummy token to replace current
        let dummy = Ann {
            pre_trivia: Trivia::new(),
            source_line: Pos(1),
            value: Token::SOF,
            trail_comment: None,
        };
        std::mem::replace(&mut self.current, dummy)
    }

    /// Expect specific token, advance if matches
    fn expect_token_match<F>(&mut self, predicate: F) -> Result<Ann<Token>>
    where
        F: Fn(&Token) -> bool,
    {
        if predicate(&self.current.value) {
            let token = self.take_current();
            self.advance()?;
            Ok(token)
        } else {
            Err(ParseError::new(
                self.current.source_line,
                format!("unexpected token: {:?}", self.current.value),
            ))
        }
    }

    /// Expect EOF
    fn expect_eof(&self) -> Result<()> {
        if matches!(self.current.value, Token::SOF) {
            Ok(())
        } else {
            Err(ParseError::new(
                self.current.source_line,
                format!("expected end of file, found: {:?}", self.current.value),
            ))
        }
    }

    /// Check if the current token can begin a simple selector
    fn is_simple_selector_start(&self) -> bool {
        matches!(
            self.current.value,
            Token::Identifier(_) | Token::TDoubleQuote | Token::TInterOpen
        )
    }

    /// Parse ${expr} interpolation used in selectors
    fn parse_selector_interpolation(&mut self) -> Result<Ann<StringPart>> {
        let open = self.take_current();
        debug_assert!(matches!(open.value, Token::TInterOpen));
        self.advance()?;

        let expr = self.parse_expression()?;
        let close = self.expect_token_match(|t| matches!(t, Token::TBraceClose))?;

        Ok(Ann {
            pre_trivia: open.pre_trivia,
            source_line: open.source_line,
            value: StringPart::Interpolation(Box::new(Whole {
                value: expr,
                trailing_trivia: close.pre_trivia.clone(),
            })),
            trail_comment: close.trail_comment,
        })
    }

    /// Check if the current token represents the `or` keyword (identifier or actual keyword)
    fn is_or_token(&self) -> bool {
        matches!(self.current.value, Token::KOr)
            || matches!(
                &self.current.value,
                Token::Identifier(name) if name == "or"
            )
    }

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
}

fn fix_simple_string(parts: Vec<StringPart>) -> Vec<Vec<StringPart>> {
    split_lines(parts).into_iter().map(normalize_line).collect()
}

fn fix_indented_string(lines: Vec<Vec<StringPart>>) -> Vec<Vec<StringPart>> {
    let lines = fix_first_line(lines);
    let lines = fix_last_line(lines);
    let lines = strip_indentation(lines);
    let lines = lines.into_iter().flat_map(split_lines).collect::<Vec<_>>();
    lines.into_iter().map(normalize_line).collect()
}

fn split_lines(parts: Vec<StringPart>) -> Vec<Vec<StringPart>> {
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

fn normalize_line(line: Vec<StringPart>) -> Vec<StringPart> {
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

fn is_spaces(text: &str) -> bool {
    text.chars().all(|c| c == ' ')
}

fn is_empty_line(line: &[StringPart]) -> bool {
    match line {
        [] => true,
        [StringPart::TextPart(text)] => is_spaces(text),
        _ => false,
    }
}

fn fix_first_line(mut lines: Vec<Vec<StringPart>>) -> Vec<Vec<StringPart>> {
    if lines.is_empty() {
        return lines;
    }

    let first = normalize_line(lines[0].clone());
    if is_empty_line(&first) && lines.len() > 1 {
        lines.remove(0);
    } else {
        lines[0] = first;
    }
    lines
}

fn fix_last_line(mut lines: Vec<Vec<StringPart>>) -> Vec<Vec<StringPart>> {
    match lines.len() {
        0 => lines,
        1 => {
            let last = normalize_line(lines[0].clone());
            if is_empty_line(&last) {
                vec![Vec::new()]
            } else {
                vec![last]
            }
        }
        _ => {
            let last_index = lines.len() - 1;
            let last = normalize_line(lines[last_index].clone());
            lines[last_index] = if is_empty_line(&last) {
                Vec::new()
            } else {
                last
            };
            lines
        }
    }
}

fn line_head(line: &[StringPart]) -> Option<String> {
    match line.first() {
        None => None,
        Some(StringPart::TextPart(text)) => Some(text.clone()),
        Some(StringPart::Interpolation(_)) => Some(String::new()),
    }
}

fn common_indentation(heads: Vec<String>) -> Option<String> {
    if heads.is_empty() {
        return None;
    }

    let mut prefix: String = heads[0].chars().take_while(|c| *c == ' ').collect();
    for head in heads.iter().skip(1) {
        let candidate: String = head.chars().take_while(|c| *c == ' ').collect();
        let mut new_prefix = String::new();
        for (a, b) in prefix.chars().zip(candidate.chars()) {
            if a == b {
                new_prefix.push(a);
            } else {
                break;
            }
        }
        prefix = new_prefix;
        if prefix.is_empty() {
            break;
        }
    }
    Some(prefix)
}

fn strip_parts(indentation: &str, mut line: Vec<StringPart>) -> Vec<StringPart> {
    if indentation.is_empty() {
        return line;
    }

    if let Some(StringPart::TextPart(text)) = line.first_mut() {
        if let Some(stripped) = text.strip_prefix(indentation) {
            *text = stripped.to_string();
        }
    }
    line
}

fn strip_indentation(lines: Vec<Vec<StringPart>>) -> Vec<Vec<StringPart>> {
    let heads: Vec<String> = lines.iter().filter_map(|line| line_head(line)).collect();

    match common_indentation(heads) {
        None => lines.into_iter().map(|_| Vec::new()).collect(),
        Some(indentation) => lines
            .into_iter()
            .map(|line| strip_parts(&indentation, line))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_int() {
        let mut parser = Parser::new("42").unwrap();
        let file = parser.parse_file().unwrap();

        // Check it's a Term(Token(Integer))
        match &file.value {
            Expression::Term(Term::Token(ann)) => {
                assert!(matches!(&ann.value, Token::Integer(s) if s == "42"));
            }
            _ => panic!("expected Term(Token(Integer))"),
        }
    }

    #[test]
    fn test_parse_identifier() {
        let mut parser = Parser::new("foo").unwrap();
        let file = parser.parse_file().unwrap();

        match &file.value {
            Expression::Term(Term::Token(ann)) => {
                assert!(matches!(&ann.value, Token::Identifier(s) if s == "foo"));
            }
            _ => panic!("expected Term(Token(Identifier))"),
        }
    }

    #[test]
    fn test_parse_empty_set() {
        let mut parser = Parser::new("{}").unwrap();
        let file = parser.parse_file().unwrap();

        match &file.value {
            Expression::Term(Term::Set(None, _, items, _)) => {
                assert_eq!(items.0.len(), 0);
            }
            _ => panic!("expected empty set"),
        }
    }

    #[test]
    fn test_parse_empty_list() {
        let mut parser = Parser::new("[]").unwrap();
        let file = parser.parse_file().unwrap();

        match &file.value {
            Expression::Term(Term::List(_, items, _)) => {
                assert_eq!(items.0.len(), 0);
            }
            _ => panic!("expected empty list"),
        }
    }

    #[test]
    fn test_parse_parenthesized() {
        let mut parser = Parser::new("(42)").unwrap();
        let file = parser.parse_file().unwrap();

        match &file.value {
            Expression::Term(Term::Parenthesized(_, expr, _)) => match expr.as_ref() {
                Expression::Term(Term::Token(ann)) => {
                    assert!(matches!(&ann.value, Token::Integer(s) if s == "42"));
                }
                _ => panic!("expected integer inside parens"),
            },
            _ => panic!("expected parenthesized expression"),
        }
    }

    #[test]
    fn test_simple_string_trailing_space_preserved() {
        let file = crate::parse("\"outer ${\"inner ${x}\"} end\"").unwrap();
        match file.value {
            Expression::Term(Term::SimpleString(ann)) => {
                let line = &ann.value[0];
                match &line[2] {
                    StringPart::TextPart(text) => assert_eq!(text, " end"),
                    _ => panic!("expected trailing text part"),
                }
            }
            _ => panic!("unexpected parse result"),
        }
    }

    #[test]
    fn test_parse_set_with_binding() {
        let mut parser = Parser::new("{ a = 1; }").unwrap();
        let file = parser.parse_file().unwrap();

        match &file.value {
            Expression::Term(Term::Set(None, _, bindings, _)) => {
                // Should have one binding
                assert!(bindings.0.len() > 0);
            }
            _ => panic!("expected set with bindings"),
        }
    }

    #[test]
    fn test_parse_binary_op() {
        let mut parser = Parser::new("1 + 2").unwrap();
        let file = parser.parse_file().unwrap();

        match &file.value {
            Expression::Operation(_, op, _) => {
                assert!(matches!(op.value, Token::TPlus));
            }
            _ => panic!("expected operation"),
        }
    }

    #[test]
    fn test_parse_let_in() {
        let mut parser = Parser::new("let a = 1; in a").unwrap();
        let file = parser.parse_file().unwrap();

        match &file.value {
            Expression::Let(_, _, _, _) => {
                // Success
            }
            _ => panic!("expected let expression"),
        }
    }

    #[test]
    fn test_parse_if_then_else() {
        let mut parser = Parser::new("if true then 1 else 2").unwrap();
        let file = parser.parse_file().unwrap();

        match &file.value {
            Expression::If(_, _, _, _, _, _) => {
                // Success
            }
            _ => panic!("expected if expression"),
        }
    }

    #[test]
    fn test_parse_lambda() {
        let mut parser = Parser::new("x: x").unwrap();
        let file = parser.parse_file().unwrap();

        match &file.value {
            Expression::Abstraction(_, _, _) => {
                // Success
            }
            _ => panic!("expected lambda expression"),
        }
    }

    #[test]
    fn test_parse_application() {
        let mut parser = Parser::new("f x").unwrap();
        let file = parser.parse_file().unwrap();

        match &file.value {
            Expression::Application(_, _) => {
                // Success
            }
            _ => panic!("expected application"),
        }
    }

    #[test]
    fn test_parse_list_with_items() {
        let mut parser = Parser::new("[1 2 3]").unwrap();
        let file = parser.parse_file().unwrap();

        match &file.value {
            Expression::Term(Term::List(_, items, _)) => {
                // Should have 3 items
                assert!(items.0.len() >= 3);
            }
            _ => panic!("expected list with items"),
        }
    }

    #[test]
    fn test_parse_empty_string() {
        let mut parser = Parser::new(r#""""#).unwrap();
        let file = parser.parse_file().unwrap();

        match &file.value {
            Expression::Term(Term::SimpleString(_)) => {
                // Success
            }
            _ => panic!("expected simple string"),
        }
    }

    #[test]
    fn test_parse_rec_set() {
        let mut parser = Parser::new("rec { a = 1; }").unwrap();
        let file = parser.parse_file().unwrap();

        match &file.value {
            Expression::Term(Term::Set(Some(_), _, _, _)) => {
                // Success - has rec token
            }
            _ => panic!("expected rec set"),
        }
    }

    #[test]
    fn test_parse_negation() {
        let mut parser = Parser::new("-5").unwrap();
        let file = parser.parse_file().unwrap();

        match &file.value {
            Expression::Negation(_, _) => {
                // Success
            }
            _ => panic!("expected negation"),
        }
    }

    #[test]
    fn test_parse_double_negation() {
        let mut parser = Parser::new("- -5").unwrap();
        let file = parser.parse_file().unwrap();

        match &file.value {
            Expression::Negation(_, inner) => {
                match inner.as_ref() {
                    Expression::Negation(_, _) => {
                        // Success - double negation
                    }
                    _ => panic!("expected nested negation"),
                }
            }
            _ => panic!("expected negation"),
        }
    }

    #[test]
    fn test_parse_env_path() {
        let mut parser = Parser::new("<nixpkgs>").unwrap();
        let file = parser.parse_file().unwrap();

        match &file.value {
            Expression::Term(Term::Token(ann)) => {
                assert!(matches!(&ann.value, Token::EnvPath(s) if s == "nixpkgs"));
            }
            _ => panic!("expected env path"),
        }
    }

    #[test]
    fn test_parse_subtraction_not_application() {
        // f -5 should parse as (f - 5), NOT f(-5)
        let mut parser = Parser::new("f -5").unwrap();
        let file = parser.parse_file().unwrap();

        match &file.value {
            Expression::Operation(_, op, _) => {
                assert!(matches!(op.value, Token::TMinus));
            }
            _ => panic!("expected operation (subtraction), not application"),
        }
    }

    #[test]
    fn test_parse_application_with_parens() {
        // f (-5) should parse as Application(f, Negation(5))
        let mut parser = Parser::new("f (-5)").unwrap();
        let file = parser.parse_file().unwrap();

        match &file.value {
            Expression::Application(_, arg) => {
                match arg.as_ref() {
                    Expression::Term(Term::Parenthesized(_, inner, _)) => {
                        match inner.as_ref() {
                            Expression::Negation(_, _) => {
                                // Success
                            }
                            _ => panic!("expected negation inside parens"),
                        }
                    }
                    _ => panic!("expected parenthesized negation as argument"),
                }
            }
            _ => panic!("expected application"),
        }
    }

    #[test]
    fn test_parse_selection() {
        let mut parser = Parser::new("pkgs.gcc").unwrap();
        let file = parser.parse_file().unwrap();

        match &file.value {
            Expression::Term(Term::Selection(_base, sels, None)) => {
                // Should have one selector
                assert!(sels.len() == 1);
            }
            _ => panic!("expected selection"),
        }
    }

    #[test]
    fn test_parse_selection_chain() {
        let mut parser = Parser::new("a.b.c").unwrap();
        let file = parser.parse_file().unwrap();

        match &file.value {
            Expression::Term(Term::Selection(_, sels, None)) => {
                // Should have two selectors (b and c)
                assert!(sels.len() == 2);
            }
            _ => panic!("expected selection chain"),
        }
    }

    #[test]
    fn test_parse_selection_with_default() {
        let mut parser = Parser::new("x.y or z").unwrap();
        let file = parser.parse_file().unwrap();

        match &file.value {
            Expression::Term(Term::Selection(_, sels, Some(_))) => {
                assert!(sels.len() == 1);
            }
            _ => panic!("expected selection with or-default"),
        }
    }

    #[test]
    fn test_parse_member_check() {
        let mut parser = Parser::new("x ? y").unwrap();
        let file = parser.parse_file().unwrap();

        match &file.value {
            Expression::MemberCheck(_, _, sels) => {
                assert!(sels.len() == 1);
            }
            _ => panic!("expected member check"),
        }
    }

    #[test]
    fn test_parse_member_check_chain() {
        let mut parser = Parser::new("x ? y.z").unwrap();
        let file = parser.parse_file().unwrap();

        match &file.value {
            Expression::MemberCheck(_, _, sels) => {
                assert!(sels.len() == 2);
            }
            _ => panic!("expected member check with selector chain"),
        }
    }
}
