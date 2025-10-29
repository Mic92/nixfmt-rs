//! Hand-written recursive descent parser for Nix
//!
//! Ports parsing logic from nixfmt's Parser.hs

#[cfg(test)]
mod tests;

mod string_processing;

use crate::error::{ParseError, Result};
use crate::lexer::Lexer;
use crate::types::*;

/// Characters allowed in URI schemes (in addition to alphanumeric)
/// Based on nixfmt's schemeChar: "-.+"
const URI_SCHEME_SPECIAL_CHARS: &[char] = &['-', '.', '+'];
/// Characters allowed in URIs (in addition to alphanumeric)
/// Based on nixfmt's uriChar: "~!@$%&*-=_+:',./?"
const URI_SPECIAL_CHARS: &[char] = &[
    '~', '!', '@', '$', '%', '&', '*', '-', '=', '_', '+', ':', '\'', ',', '.', '/', '?',
];

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
            Token::KLet => {
                // Check if this is old-style 'let { }' or modern 'let ... in ...'
                // Old-style: let is followed by {
                // Modern: let is followed by bindings
                if self.lexer.peek() == Some('{') {
                    // Old-style let { } - parse as operation/term
                    self.parse_abstraction_or_operation()
                } else {
                    // Modern let ... in ...
                    self.parse_let()
                }
            }
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
                // Check if this is a URI (identifier followed by : and URI chars)
                // Must check BEFORE lambda parameter check (which also looks for :)
                if self.looks_like_uri() {
                    return self.parse_operation_or_lambda();
                }

                // Check if this might be a path (identifier followed by /)
                // But NOT the // operator (update)
                // If so, parse it normally as an operation (which will handle the path)
                if self.lexer.peek() == Some('/') && self.lexer.peek_ahead(1) != Some('/') {
                    return self.parse_operation_or_lambda();
                }

                // Could be identifier parameter OR identifier term
                // Parse identifier and check for : or @
                let ident = self.take_and_advance()?;

                if matches!(self.current.value, Token::TColon) {
                    // It's a lambda: x: body
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

                    // Check for colon - if not present, give helpful error
                    if !matches!(self.current.value, Token::TColon) {
                        return Err(ParseError {
                            span: at_tok.span,
                            kind: crate::error::ErrorKind::InvalidSyntax {
                                description: "@ is only valid in lambda parameters".to_string(),
                                hint: Some(
                                    "use 'name1 @ name2: body' for function parameters".to_string(),
                                ),
                            },
                            labels: vec![],
                        });
                    }

                    // Validate that pattern name doesn't shadow a formal
                    let first_param = Parameter::ID(ident.clone());
                    self.validate_context_parameter(&first_param, &second_param)?;

                    let colon = self.expect_token_match(|t| matches!(t, Token::TColon))?;
                    let body = self.parse_expression()?;
                    Ok(Expression::Abstraction(
                        Parameter::Context(Box::new(first_param), at_tok, Box::new(second_param)),
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
        // Save state for potential backtracking
        let saved_state = self.save_state();

        let open_brace = self.take_and_advance()?;

        // Look at next token to decide
        match &self.current.value {
            Token::TBraceClose => {
                // Empty set: {} - could be parameter or literal
                // But first check if there are comments in the pre_trivia of the close brace
                let mut close_brace = self.take_current();
                let items = if !close_brace.pre_trivia.is_empty() {
                    // There are comments inside the empty set, extract them as a Comments item
                    let comments = std::mem::take(&mut close_brace.pre_trivia);
                    vec![Item::Comments(comments)]
                } else {
                    Vec::new()
                };

                self.advance()?;

                if matches!(self.current.value, Token::TColon) {
                    // Empty set parameter: {}: body
                    // Note: parameters can't have Comments items, so this should be empty
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

                    // Validate that pattern name doesn't shadow a formal
                    let first_param =
                        Parameter::Set(open_brace.clone(), Vec::new(), close_brace.clone());
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
                    let set_term = Term::Set(None, open_brace, Items(items), close_brace);
                    let term_with_selection = self.parse_postfix_selection(set_term)?;
                    self.continue_operation_from(Expression::Term(term_with_selection))
                }
            }
            Token::Identifier(_) => {
                // Try to parse as parameter attributes first
                // If it fails (sees = or .), parse as bindings
                match self.parse_param_attrs() {
                    Ok(attrs) => {
                        // Successfully parsed as parameter attributes

                        // Check for duplicate formal parameters
                        self.check_duplicate_formals(&attrs)?;

                        let close_brace =
                            self.expect_token_match(|t| matches!(t, Token::TBraceClose))?;

                        // Check if followed by : or @
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

                            // Validate that pattern name doesn't shadow a formal
                            let first_param = Parameter::Set(
                                open_brace.clone(),
                                attrs.clone(),
                                close_brace.clone(),
                            );
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
                            // Not a parameter - must be invalid
                            Err(ParseError {
                                span: close_brace.span,
                                kind: crate::error::ErrorKind::InvalidSyntax {
                                    description: "set with parameter-like syntax but no colon".to_string(),
                                    hint: Some("use '{ x = ...; }' for set literals or '{ x }: body' for parameters".to_string()),
                                },
                                labels: vec![],
                            })
                        }
                    }
                    Err(_) => {
                        // Failed to parse as parameters (saw = or .)
                        // Restore state to try parsing as set literal
                        self.restore_state(saved_state);

                        // Now parse as set literal
                        let open_brace = self.take_current();
                        self.advance()?;

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

                // Check for duplicate formal parameters
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

                    // Validate that pattern name doesn't shadow a formal
                    let first_param =
                        Parameter::Set(open_brace.clone(), attrs.clone(), close_brace.clone());
                    self.validate_context_parameter(&first_param, &second_param)?;

                    let colon = self.expect_token_match(|t| matches!(t, Token::TColon))?;
                    let body = self.parse_expression()?;
                    Ok(Expression::Abstraction(
                        Parameter::Context(Box::new(first_param), at_tok, Box::new(second_param)),
                        colon,
                        Box::new(body),
                    ))
                } else {
                    Err(ParseError {
                        span: close_brace.span,
                        kind: crate::error::ErrorKind::InvalidSyntax {
                            description: "{ ... } must be followed by ':' or '@'".to_string(),
                            hint: Some("use '{ x }: body' for function parameters".to_string()),
                        },
                        labels: vec![],
                    })
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
        // Check for member check (?), binary operations, or application
        let expr = if matches!(self.current.value, Token::TQuestion) {
            let question = self.take_current();
            self.advance()?;
            let selectors = self.parse_selector_path()?;
            Expression::MemberCheck(Box::new(expr), question, selectors)
        } else if self.is_term_start() {
            // Application
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
        // Try to parse initial term/application
        let expr = self.parse_application()?;

        // Check for @ (context parameter) - special case
        if matches!(self.current.value, Token::TAt) {
            // This is a context parameter pattern: param @ param
            let at_tok = self.take_and_advance()?;

            // Parse second part as a PARAMETER (not expression)
            let second_param = self.parse_full_parameter()?;

            // Now we MUST see a colon for this to be valid
            if matches!(self.current.value, Token::TColon) {
                let first_param = self.expr_to_parameter(expr)?;

                // Validate that pattern name doesn't shadow a formal
                self.validate_context_parameter(&first_param, &second_param)?;

                let param =
                    Parameter::Context(Box::new(first_param), at_tok, Box::new(second_param));
                let colon = self.expect_token_match(|t| matches!(t, Token::TColon))?;
                let body = self.parse_expression()?;
                return Ok(Expression::Abstraction(param, colon, Box::new(body)));
            } else {
                return Err(ParseError::new(
                    at_tok.span,
                    "@ is only valid in lambda parameters",
                ));
            }
        }

        // NOTE: Member check (? operator) is now handled in parse_application
        // This ensures correct precedence: ? has higher precedence than prefix ! and -

        // Check if there's a colon (lambda)
        if matches!(self.current.value, Token::TColon) {
            // It's a lambda! Convert expr back to parameter
            let param = self.expr_to_parameter(expr)?;
            let colon = self.expect_token_match(|t| matches!(t, Token::TColon))?;
            let body = self.parse_expression()?;
            Ok(Expression::Abstraction(param, colon, Box::new(body)))
        } else {
            // Check for binary operation
            self.maybe_parse_binary_operation(expr)
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
                    Ok(Parameter::ID(ann))
                } else {
                    Err(ParseError {
                        span: ann.span,
                        kind: crate::error::ErrorKind::UnexpectedToken {
                            expected: vec!["identifier".to_string()],
                            found: format!("'{}'", ann.value.text()),
                        },
                        labels: vec![],
                    })
                }
            }
            Expression::Term(Term::Set(None, open, items, close)) => {
                // Convert set literal to set parameter
                // This happens for: { x, y }: or { x ? 1 }: patterns
                let attrs = self.items_to_param_attrs(items)?;
                Ok(Parameter::Set(open, attrs, close))
            }
            _ => Err(ParseError {
                span: Span::point(0),
                kind: crate::error::ErrorKind::InvalidSyntax {
                    description: "complex parameters not yet supported".to_string(),
                    hint: Some("use simple identifiers or set patterns as parameters".to_string()),
                },
                labels: vec![],
            }),
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
                                    selector: SimpleSelector::ID(name),
                                }) = sels.pop()
                                {
                                    // Check if expr indicates a default (x ? default pattern)
                                    // For simplicity, we'll treat any assignment as x ? default
                                    let default = Some((
                                        Ann::new(Token::TQuestion, name.span), // Fake ? token
                                        expr,
                                    ));
                                    let comma = Some(comma_or_semi);
                                    attrs.push(ParamAttr::ParamAttr(
                                        name,
                                        Box::new(default),
                                        comma,
                                    ));
                                } else {
                                    return Err(ParseError {
                                        span: Span::point(0),
                                        kind: crate::error::ErrorKind::InvalidSyntax {
                                            description: "invalid parameter attribute".to_string(),
                                            hint: Some(
                                                "expected 'name' or 'name ? default'".to_string(),
                                            ),
                                        },
                                        labels: vec![],
                                    });
                                }
                            } else {
                                return Err(ParseError {
                                    span: Span::point(0),
                                    kind: crate::error::ErrorKind::InvalidSyntax {
                                        description: "invalid parameter selector".to_string(),
                                        hint: Some(
                                            "expected identifier in parameter pattern".to_string(),
                                        ),
                                    },
                                    labels: vec![],
                                });
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
            let ident = self.take_and_advance()?;

            if matches!(self.current.value, Token::TAt) {
                // Context parameter: id @ pattern
                let at_tok = self.take_and_advance()?;
                let second = self.parse_full_parameter()?;

                // Validate that pattern name doesn't shadow a formal
                let first_param = Parameter::ID(ident.clone());
                self.validate_context_parameter(&first_param, &second)?;

                Ok(Parameter::Context(
                    Box::new(first_param),
                    at_tok,
                    Box::new(second),
                ))
            } else {
                Ok(Parameter::ID(ident))
            }
        } else {
            Err(ParseError {
                span: self.current.span,
                kind: crate::error::ErrorKind::UnexpectedToken {
                    expected: vec!["identifier".to_string(), "set pattern".to_string()],
                    found: format!("'{}'", self.current.value.text()),
                },
                labels: vec![],
            })
        }
    }

    /// Parse set parameter or context parameter starting with {
    fn parse_set_or_context_parameter(&mut self) -> Result<Parameter> {
        let open_brace = self.expect_token_match(|t| matches!(t, Token::TBraceOpen))?;

        // Parse parameter attributes
        let attrs = self.parse_param_attrs()?;

        // Check for duplicate formal parameters
        self.check_duplicate_formals(&attrs)?;

        let close_brace = self.expect_token_match(|t| matches!(t, Token::TBraceClose))?;

        let set_param = Parameter::Set(open_brace, attrs, close_brace);

        // Check for @ (context parameter)
        if matches!(self.current.value, Token::TAt) {
            let at_tok = self.take_and_advance()?;
            let second = self.parse_full_parameter()?;

            // Validate that pattern name doesn't shadow a formal
            self.validate_context_parameter(&set_param, &second)?;

            Ok(Parameter::Context(
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

        while !matches!(self.current.value, Token::TBraceClose | Token::Sof) {
            if matches!(self.current.value, Token::TEllipsis) {
                // Ellipsis
                let dots = self.take_and_advance()?;
                attrs.push(ParamAttr::ParamEllipsis(dots));

                // Optional comma after ellipsis
                if matches!(self.current.value, Token::TComma) {
                    self.advance()?;
                }
                break; // Ellipsis must be last
            } else if matches!(self.current.value, Token::Identifier(_)) {
                let name = self.take_and_advance()?;

                // Check what follows the identifier
                if matches!(self.current.value, Token::TAssign | Token::TDot) {
                    // This is a binding (a = ...), not a parameter!
                    return Err(ParseError {
                        span: name.span,
                        kind: crate::error::ErrorKind::InvalidSyntax {
                            description: "not a parameter - looks like binding".to_string(),
                            hint: Some("parameters cannot have '=' or '.'".to_string()),
                        },
                        labels: vec![],
                    });
                }

                // Check for ? default
                let default = if matches!(self.current.value, Token::TQuestion) {
                    let q = self.take_and_advance()?;
                    let def_expr = self.parse_expression()?;
                    Some((q, def_expr))
                } else {
                    None
                };

                // Check for comma
                let comma = if matches!(self.current.value, Token::TComma) {
                    Some(self.take_and_advance()?)
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

    /// Check for duplicate formal parameters
    /// Validates that no parameter name appears more than once in the attrs list
    fn check_duplicate_formals(&self, attrs: &[ParamAttr]) -> Result<()> {
        use std::collections::HashSet;

        let mut seen = HashSet::new();

        for attr in attrs {
            if let ParamAttr::ParamAttr(name_leaf, _, _) = attr {
                if let Token::Identifier(name) = &name_leaf.value {
                    if !seen.insert(name.clone()) {
                        // Found a duplicate!
                        return Err(ParseError {
                            span: name_leaf.span,
                            kind: crate::error::ErrorKind::InvalidSyntax {
                                description: format!(
                                    "duplicate formal function argument '{}'",
                                    name
                                ),
                                hint: None,
                            },
                            labels: vec![],
                        });
                    }
                }
            }
        }

        Ok(())
    }

    /// Check if pattern name shadows a formal parameter
    /// For args@{x, y}: the pattern name 'args' must not appear in the formals
    fn check_pattern_shadows_formal(&self, pattern_name: &str, attrs: &[ParamAttr]) -> Result<()> {
        for attr in attrs {
            if let ParamAttr::ParamAttr(name_leaf, _, _) = attr {
                if let Token::Identifier(name) = &name_leaf.value {
                    if name == pattern_name {
                        return Err(ParseError {
                            span: name_leaf.span,
                            kind: crate::error::ErrorKind::InvalidSyntax {
                                description: format!(
                                    "duplicate formal function argument '{}'",
                                    name
                                ),
                                hint: None,
                            },
                            labels: vec![],
                        });
                    }
                }
            }
        }

        Ok(())
    }

    /// Validate context parameter: check that pattern name doesn't shadow a formal
    /// For id@{formals} or {formals}@id patterns
    fn validate_context_parameter(&self, first: &Parameter, second: &Parameter) -> Result<()> {
        // Extract the pattern name and formals from the context parameter
        match (first, second) {
            // Case 1: args@{x, y, z} - pattern name is first, set is second
            (Parameter::ID(pattern_leaf), Parameter::Set(_, attrs, _)) => {
                if let Token::Identifier(pattern_name) = &pattern_leaf.value {
                    self.check_pattern_shadows_formal(pattern_name, attrs)?;
                }
            }
            // Case 2: {x, y, z}@args - set is first, pattern name is second
            (Parameter::Set(_, attrs, _), Parameter::ID(pattern_leaf)) => {
                if let Token::Identifier(pattern_name) = &pattern_leaf.value {
                    self.check_pattern_shadows_formal(pattern_name, attrs)?;
                }
            }
            _ => {
                // Other combinations are not relevant for this check
            }
        }

        Ok(())
    }

    /// Parse let expression: let bindings in expr
    fn parse_let(&mut self) -> Result<Expression> {
        let let_tok = self.expect_token_match(|t| matches!(t, Token::KLet))?;
        let bindings = self.parse_binders()?;

        // Expect 'in' keyword
        let in_tok = if matches!(self.current.value, Token::KIn) {
            let tok = self.take_current();
            self.advance()?;
            tok
        } else {
            return Err(ParseError {
                span: self.current.span,
                kind: crate::error::ErrorKind::UnexpectedToken {
                    expected: vec!["'in'".to_string()],
                    found: format!("'{}'", self.current.value.text()),
                },
                labels: vec![],
            });
        };

        let body = self.parse_expression()?;

        Ok(Expression::Let(let_tok, bindings, in_tok, Box::new(body)))
    }

    /// Parse if expression: if cond then expr else expr
    fn parse_if(&mut self) -> Result<Expression> {
        let if_tok = self.expect_token_match(|t| matches!(t, Token::KIf))?;
        let cond = self.parse_expression()?;

        // Expect 'then' with a helpful error message
        let then_tok = if matches!(self.current.value, Token::KThen) {
            let tok = self.take_current();
            self.advance()?;
            tok
        } else {
            return Err(ParseError {
                span: self.current.span,
                kind: crate::error::ErrorKind::UnexpectedToken {
                    expected: vec!["'then'".to_string()],
                    found: format!("'{}'", self.current.value.text()),
                },
                labels: vec![],
            });
        };

        let then_expr = self.parse_expression()?;

        // Expect 'else' with a helpful error message
        let else_tok = if matches!(self.current.value, Token::KElse) {
            let tok = self.take_current();
            self.advance()?;
            tok
        } else {
            return Err(ParseError {
                span: self.current.span,
                kind: crate::error::ErrorKind::UnexpectedToken {
                    expected: vec!["'else'".to_string()],
                    found: format!("'{}'", self.current.value.text()),
                },
                labels: vec![],
            });
        };

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

        // Expect semicolon after with expression
        let semi = if matches!(self.current.value, Token::TSemicolon) {
            let tok = self.take_current();
            self.advance()?;
            tok
        } else {
            return Err(ParseError {
                span: self.current.span,
                kind: crate::error::ErrorKind::MissingToken {
                    token: "';'".to_string(),
                    after: "'with' expression".to_string(),
                },
                labels: vec![],
            });
        };

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

        // Expect semicolon after assert condition
        let semi = if matches!(self.current.value, Token::TSemicolon) {
            let tok = self.take_current();
            self.advance()?;
            tok
        } else {
            return Err(ParseError {
                span: self.current.span,
                kind: crate::error::ErrorKind::MissingToken {
                    token: "';'".to_string(),
                    after: "'assert' condition".to_string(),
                },
                labels: vec![],
            });
        };

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
        // Check for prefix unary operators
        // For chained unary operators (like --5), we need to recurse
        // But we need to parse ? (postfix) before applying ! or - (prefix) due to precedence
        match &self.current.value {
            Token::TMinus => {
                let op = self.take_and_advance()?;
                // Recursively parse to handle chained unary operators
                let inner = self.parse_application()?;
                return Ok(Expression::Negation(op, Box::new(inner)));
            }
            Token::TNot => {
                let op = self.take_and_advance()?;
                // Recursively parse to handle chained unary operators
                let inner = self.parse_application()?;
                return Ok(Expression::Inversion(op, Box::new(inner)));
            }
            _ => {}
        }

        // Parse first term
        let mut expr = Expression::Term(self.parse_term()?);

        // Keep applying while we see more TERMS (not unary ops)
        // IMPORTANT: Don't treat binary operators as term starts even if they could start paths
        while self.is_term_start() && !self.is_binary_op() && !self.is_expression_end() {
            let arg = Expression::Term(self.parse_term()?);
            expr = Expression::Application(Box::new(expr), Box::new(arg));
        }

        // Check for member check (? operator) - postfix, higher precedence than prefix !/-
        // This is the KEY fix: we check for ? here, AFTER application but within the recursive call
        // This ensures: !a ? b parses as !(a ? b), not (!a) ? b
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
            let is_comparison = Self::is_comparison_operator(&op_token.value);
            let prec = self.get_precedence_for(&op_token.value);
            let op_string = op_token.value.text().to_string(); // User-friendly format

            // Check if we're chaining comparison operators at the same precedence level
            // This prevents: 1 < 2 < 3 (both < at precedence 9)
            // But allows: 1 < 2 == 2 > 3 (< and > at precedence 9, == at precedence 8)
            if is_comparison && last_comparison_prec == Some(prec) {
                return Err(ParseError {
                    span: op_token.span,
                    kind: crate::error::ErrorKind::ChainedComparison {
                        first_op: last_comparison_op.unwrap_or_else(|| "?".to_string()),
                        second_op: op_string,
                    },
                    labels: vec![],
                });
            }

            let is_right_assoc = self.is_right_associative(&op_token.value);
            self.advance()?;

            // Try to parse the right-hand side, providing better error for incomplete expressions
            let mut right = match self.parse_application() {
                Ok(expr) => expr,
                Err(e) => {
                    // If we failed to parse the right-hand side and current token is }
                    // (closing an interpolation), provide a more helpful error
                    if matches!(self.current.value, Token::TBraceClose | Token::TInterClose) {
                        return Err(ParseError {
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
                        });
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
                        // Restructure: Operation(one, op1, Operation(two, op2, three))
                        Expression::Operation(
                            one,
                            op1,
                            Box::new(Expression::Operation(two, op_token, Box::new(right))),
                        )
                    } else {
                        // Left is an operation but not TPlus, create normal operation
                        Expression::Operation(
                            Box::new(Expression::Operation(one, op1, two)),
                            op_token,
                            Box::new(right),
                        )
                    }
                } else {
                    // Left is not an operation, create normal operation
                    Expression::Operation(Box::new(left), op_token, Box::new(right))
                }
            } else {
                // Not TPlus, create normal operation
                Expression::Operation(Box::new(left), op_token, Box::new(right))
            };

            // Track the precedence and operator of comparison operators to prevent chaining at the same level
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

    /// Parse binders (for let and attribute sets)
    fn parse_binders(&mut self) -> Result<Items<Binder>> {
        let mut items = Vec::new();

        while !matches!(
            self.current.value,
            Token::KIn | Token::TBraceClose | Token::Sof
        ) {
            // Check for comments before binding
            self.collect_trivia_as_comments(&mut items);

            let binder = self.parse_binder()?;
            items.push(Item::Item(binder));
        }

        if matches!(self.current.value, Token::KIn | Token::TBraceClose) {
            self.collect_trivia_as_comments(&mut items);
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

        // Check for common mistake: attribute path followed by semicolon (forgot = and value)
        if matches!(self.current.value, Token::TSemicolon) {
            return Err(ParseError {
                span: self.current.span,
                kind: crate::error::ErrorKind::UnexpectedToken {
                    expected: vec!["'='".to_string()],
                    found: "';'".to_string(),
                },
                labels: vec![],
            });
        }

        let eq = self.expect_token_match(|t| matches!(t, Token::TAssign))?;
        let expr = self.parse_expression()?;

        // Get the end of the expression for error reporting
        // Special case: if the expression is an Application, the user likely forgot
        // a semicolon and the parser treated the next line as a function argument.
        // Point to the end of the LEFT side (the function) instead of the RIGHT side.
        let expr_end_span = match &expr {
            Expression::Application(func, _arg) => Self::expr_end_span(func),
            _ => Self::expr_end_span(&expr),
        };

        // Expect semicolon with helpful error message
        if matches!(self.current.value, Token::TSemicolon) {
            let semi = self.take_current();
            self.advance()?;
            Ok(Binder::Assignment(selectors, eq, expr, semi))
        } else if matches!(self.current.value, Token::Sof) {
            // EOF found - check if this is an unclosed nested set
            // If the expression is a set, the closing brace might have belonged to an outer scope
            if let Expression::Term(Term::Set(_, open_brace, _, close_brace)) = &expr {
                Err(ParseError {
                    span: close_brace.span,
                    kind: crate::error::ErrorKind::UnclosedDelimiter {
                        delimiter: '{',
                        opening_span: open_brace.span,
                    },
                    labels: vec![],
                })
            } else {
                // Generic EOF error
                Err(ParseError {
                    span: expr_end_span,
                    kind: crate::error::ErrorKind::UnexpectedToken {
                        expected: vec!["';'".to_string()],
                        found: "'end of file'".to_string(),
                    },
                    labels: vec![],
                })
            }
        } else {
            // Missing semicolon - point to the END of the expression
            Err(ParseError {
                span: expr_end_span,
                kind: crate::error::ErrorKind::UnexpectedToken {
                    expected: vec!["';'".to_string()],
                    found: format!("'{}'", self.current.value.text()),
                },
                labels: vec![],
            })
        }
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
                Ok(SimpleSelector::ID(ident))
            }
            Token::TDoubleQuote => {
                let string = self.parse_simple_string_literal()?;
                Ok(SimpleSelector::String(string))
            }
            Token::TInterOpen => {
                let interpol = self.parse_selector_interpolation()?;
                Ok(SimpleSelector::Interpol(interpol))
            }
            _ => Err(ParseError {
                span: self.current.span,
                kind: crate::error::ErrorKind::UnexpectedToken {
                    expected: vec![
                        "identifier".to_string(),
                        "string".to_string(),
                        "interpolation".to_string(),
                    ],
                    found: format!("'{}'", self.current.value.text()),
                },
                labels: vec![],
            }),
        }
    }

    /// Parse a term (atom), including postfix selection
    fn parse_term(&mut self) -> Result<Term> {
        // Check for URIs first (they look like identifiers followed by ":")
        if self.looks_like_uri() {
            return self.parse_uri();
        }

        // Check for paths (they can start with identifiers)
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
            _ => Err(ParseError {
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
            }),
        }?;

        // Check for selection (.attr) or or-default
        self.parse_postfix_selection(base_term)
    }

    /// Check if character at given offset starts valid path content
    /// Valid path content: alphanumeric, ., _, -, +, ~, or ${ for interpolation
    fn is_path_content_at(&self, offset: usize) -> bool {
        match self.lexer.peek_ahead(offset) {
            Some(c) if c.is_alphanumeric() || matches!(c, '.' | '_' | '-' | '+' | '~') => true,
            Some('$') => self.lexer.peek_ahead(offset + 1) == Some('{'), // interpolation ${
            _ => false,
        }
    }

    /// Check if there's whitespace before current token
    /// Used to distinguish paths from operators: "a/b" (path) vs "a / b" (division)
    fn has_preceding_whitespace(&self) -> bool {
        self.lexer.recent_hspace > 0 || self.lexer.recent_newlines > 0
    }

    /// Check if current position starts a path
    /// Must check BEFORE consuming any tokens
    fn looks_like_path(&self) -> bool {
        match &self.current.value {
            // identifier/ → path (no space), identifier /path → application (space before /)
            Token::Identifier(_) => {
                self.lexer.peek() == Some('/')
                    && self.lexer.peek_ahead(1) != Some('/') // not //
                    && self.is_path_content_at(1)
                    && !self.has_preceding_whitespace()
            }

            // ./ or ../
            Token::TDot => match (self.lexer.peek(), self.lexer.peek_ahead(1)) {
                (Some('/'), _) => self.is_path_content_at(1), // ./
                (Some('.'), Some('/')) => self.is_path_content_at(2), // ../
                _ => false,
            },

            // /path → path (no space before), expr /path → division (space before)
            Token::TDiv => self.is_path_content_at(0) && !self.has_preceding_whitespace(),

            // ~/
            Token::TTilde => self.lexer.peek() == Some('/') && self.is_path_content_at(1),

            _ => false,
        }
    }

    /// Parse path: ./foo/bar or ~/foo or /foo
    /// Based on Haskell's path parser
    fn parse_path(&mut self) -> Result<Term> {
        let start_pos = self.current.span;
        let pre_trivia = self.current.pre_trivia.clone();
        let mut parts = Vec::new();

        // Handle the prefix that was already tokenized
        // NOTE: Don't call self.advance() here - we need to read raw chars from lexer
        match &self.current.value {
            Token::Identifier(ident) => {
                // Path starting with identifier (e.g., common/file.nix, foo-bar/baz.nix)
                // The identifier has already been consumed by the lexer
                parts.push(StringPart::TextPart(ident.clone()));
            }
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
                    let interp = self.parse_string_interpolation()?;
                    parts.push(interp);
                }
                Some(ch) if ch.is_alphanumeric() || matches!(ch, '.' | '_' | '-' | '+') => {
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

        // Validate: paths cannot end with a trailing slash
        // This matches nixfmt's requirement that pathTraversal must have content after the slash
        if let Some(StringPart::TextPart(text)) = parts.last() {
            if text.ends_with('/') {
                // Point to the trailing slash, not the start of the path
                let current_pos = self.lexer.current_pos().start;
                let slash_pos = Span {
                    start: current_pos.saturating_sub(1),
                    end: current_pos,
                };
                return Err(ParseError {
                    span: slash_pos,
                    kind: crate::error::ErrorKind::InvalidSyntax {
                        description: "path cannot end with a trailing slash".to_string(),
                        hint: Some(
                            "remove the trailing '/' or add more path components".to_string(),
                        ),
                    },
                    labels: vec![],
                });
            }
        }

        let trail_comment = self.parse_trailing_trivia_and_advance()?;

        let ann = Ann {
            pre_trivia,
            span: start_pos,
            value: parts,
            trail_comment,
        };

        Ok(Term::Path(ann))
    }

    /// Check if character is valid in URI scheme
    /// Based on nixfmt's schemeChar: "-.+" + alphanumeric
    fn is_scheme_char(c: char) -> bool {
        c.is_alphanumeric() || URI_SCHEME_SPECIAL_CHARS.contains(&c)
    }

    /// Check if character is valid in URI
    /// Based on nixfmt's uriChar: "~!@$%&*-=_+:',./?" + alphanumeric
    fn is_uri_char(c: char) -> bool {
        c.is_alphanumeric() || URI_SPECIAL_CHARS.contains(&c)
    }

    /// Check if current position starts a URI
    /// Pattern: scheme_chars ":" uri_chars (e.g., http://example.com)
    fn looks_like_uri(&self) -> bool {
        // Must be an identifier
        let Token::Identifier(scheme) = &self.current.value else {
            return false;
        };

        // All characters in scheme must be valid scheme chars
        if !scheme.chars().all(Self::is_scheme_char) {
            return false;
        }

        // Must be followed by ":"
        if self.lexer.peek() != Some(':') {
            return false;
        }

        // Must be followed by at least one URI char after ":"
        matches!(self.lexer.peek_ahead(1), Some(c) if Self::is_uri_char(c))
    }

    /// Parse URI as a SimpleString
    /// Based on nixfmt's uri parser
    fn parse_uri(&mut self) -> Result<Term> {
        let start_pos = self.current.span;
        let pre_trivia = self.current.pre_trivia.clone();

        // Get the scheme (already tokenized as identifier)
        let Token::Identifier(scheme) = &self.current.value else {
            return Err(ParseError {
                span: start_pos,
                kind: crate::error::ErrorKind::UnexpectedToken {
                    expected: vec!["identifier".to_string()],
                    found: format!("'{}'", self.current.value.text()),
                },
                labels: vec![],
            });
        };

        let mut uri_text = scheme.clone();

        // Expect ":"
        if self.lexer.peek() != Some(':') {
            return Err(ParseError {
                span: self.lexer.current_pos(),
                kind: crate::error::ErrorKind::MissingToken {
                    token: "':'".to_string(),
                    after: "URI scheme".to_string(),
                },
                labels: vec![],
            });
        }
        self.lexer.advance();
        uri_text.push(':');

        // Parse URI characters
        while let Some(ch) = self.lexer.peek() {
            if Self::is_uri_char(ch) {
                uri_text.push(ch);
                self.lexer.advance();
            } else {
                break;
            }
        }

        // Parse trailing trivia and advance
        let trail_comment = self.parse_trailing_trivia_and_advance()?;

        // Wrap as SimpleString
        let parts = vec![vec![StringPart::TextPart(uri_text)]];
        let ann = Ann {
            pre_trivia,
            span: start_pos,
            value: parts,
            trail_comment,
        };

        Ok(Term::SimpleString(ann))
    }

    /// Parse path text component (without /)
    /// Based on Haskell's pathText
    fn parse_path_part(&mut self) -> Result<String> {
        let mut text = String::new();

        while let Some(ch) = self.lexer.peek() {
            if ch.is_alphanumeric() || matches!(ch, '.' | '_' | '-' | '+' | '~') {
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

        // Check for 'or' default
        // The 'or' operator is syntactically allowed on any term,
        // but semantically only makes sense with selectors (e.g., `foo.bar or default`).
        // When there are no selectors, nixfmt parses and then discards the 'or' clause.
        // This makes sense: `fold or []` means "lookup fold, use [] if not found",
        // but simple variable lookups either succeed or error - there's no "not found" case.
        let or_default = if self.is_or_token() {
            // Save state in case we need to backtrack
            let saved_state = self.save_state();

            let mut or_tok = self.take_current();
            if matches!(
                &or_tok.value,
                Token::Identifier(name) if name == "or"
            ) {
                or_tok.value = Token::KOr;
            }
            self.advance()?;

            // Check if the next token can start a term (the default value)
            // If not, backtrack and treat 'or' as an identifier
            if self.is_term_start() {
                let default_term = self.parse_term()?;
                Some((or_tok, Box::new(default_term)))
            } else {
                // Backtrack: restore parser state
                self.restore_state(saved_state);
                None
            }
        } else {
            None
        };

        // Return Selection only if we have selectors
        // If there are no selectors, discard any 'or' default (matching nixfmt behavior)
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
        let open_quote_pos = self.current.span;
        let pre_trivia = self.current.pre_trivia.clone();

        // DON'T advance - just verify we're at a quote
        if !matches!(self.current.value, Token::TDoubleQuote) {
            return Err(ParseError {
                span: open_quote_pos,
                kind: crate::error::ErrorKind::UnexpectedToken {
                    expected: vec!["'\"'".to_string()],
                    found: format!("'{}'", self.current.value.text()),
                },
                labels: vec![],
            });
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
                    return Err(ParseError {
                        span: self.lexer.current_pos(),
                        kind: crate::error::ErrorKind::UnclosedDelimiter {
                            delimiter: '"',
                            opening_span: open_quote_pos,
                        },
                        labels: vec![],
                    });
                }
                Some('$') if self.lexer.peek_ahead(1) == Some('{') => {
                    // Interpolation
                    let interp = self.parse_string_interpolation()?;
                    parts.push(interp);
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

        let trail_comment = self.parse_trailing_trivia_and_advance()?;
        let lines = string_processing::process_simple(parts);

        Ok(Ann {
            pre_trivia,
            span: open_quote_pos,
            value: lines,
            trail_comment,
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
    fn parse_string_interpolation(&mut self) -> Result<StringPart> {
        // Consume ${
        self.lexer.advance(); // $
        self.lexer.advance(); // {

        // Re-sync parser
        self.current = self.lexer.lexeme()?;

        // Check for empty interpolation ${}
        if matches!(self.current.value, Token::TBraceClose) {
            return Err(ParseError {
                span: self.current.span,
                kind: crate::error::ErrorKind::InvalidSyntax {
                    description: "empty interpolation expression".to_string(),
                    hint: Some(
                        "string interpolations require an expression inside ${...}".to_string(),
                    ),
                },
                labels: vec![],
            });
        }

        // Parse expression, catching errors to provide better messages for common mistakes
        let expr = match self.parse_expression() {
            Ok(e) => e,
            Err(err) => {
                // If we got an UnclosedDelimiter error for a quote, it's likely because
                // the user forgot to close the interpolation with }, and the expression
                // parser treated the closing quote of the outer string as a new string start
                if let crate::error::ErrorKind::UnclosedDelimiter {
                    delimiter: '"',
                    opening_span,
                } = &err.kind
                {
                    // Check if the "opening" position is likely the closing quote of outer string
                    // by seeing if there's a quote at that position
                    return Err(ParseError {
                        span: *opening_span,
                        kind: crate::error::ErrorKind::UnexpectedToken {
                            expected: vec!["'}'".to_string()],
                            found: "'\"'".to_string(),
                        },
                        labels: vec![],
                    });
                }
                return Err(err);
            }
        };

        // Verify we're at }
        if !matches!(self.current.value, Token::TBraceClose) {
            return Err(ParseError {
                span: self.current.span,
                kind: crate::error::ErrorKind::UnexpectedToken {
                    expected: vec!["'}'".to_string()],
                    found: format!("'{}'", self.current.value.text()),
                },
                labels: vec![],
            });
        }

        // The } token was already consumed by the lexer when creating TBraceClose
        // So lexer.pos is already AFTER the }
        // DON'T call advance() or lexer.advance() - just continue from current lexer.pos

        // Now lexer.pos is right after }, and we can continue parsing string content
        // DON'T resync current - we'll continue with raw parsing

        self.lexer.rewind_trivia();
        Ok(StringPart::Interpolation(Box::new(Whole {
            value: expr,
            trailing_trivia: Trivia::new(),
        })))
    }

    /// Parse indented string: ''...''
    /// Based on Haskell's indentedString parser
    fn parse_indented_string(&mut self) -> Result<Term> {
        let open_quote_pos = self.current.span;
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
            return Err(ParseError {
                span: self.lexer.current_pos(),
                kind: crate::error::ErrorKind::UnclosedDelimiter {
                    delimiter: '\'', // represents ''
                    opening_span: open_quote_pos,
                },
                labels: vec![],
            });
        }
        self.lexer.advance(); // '
        self.lexer.advance(); // '

        let trail_comment = self.parse_trailing_trivia_and_advance()?;
        let lines = string_processing::process_indented(lines);

        let ann = Ann {
            pre_trivia,
            span: open_quote_pos,
            value: lines,
            trail_comment,
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
                    let interp = self.parse_string_interpolation()?;
                    parts.push(interp);
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

    /// Parse attribute set: { ... } or rec { ... } or let { ... }
    fn parse_set(&mut self) -> Result<Term> {
        // Check for 'rec' or 'let' keyword
        let prefix_tok = if matches!(self.current.value, Token::KRec | Token::KLet) {
            let tok = self.take_current();
            self.advance()?;
            Some(tok)
        } else {
            None
        };

        let open_brace = self.expect_token_match(|t| matches!(t, Token::TBraceOpen))?;
        let opening_span = open_brace.span;

        // Parse bindings
        let bindings = self.parse_binders()?;

        let close_brace = self.expect_closing_delimiter(opening_span, '{', Token::TBraceClose)?;

        Ok(Term::Set(prefix_tok, open_brace, bindings, close_brace))
    }

    /// Parse list: [ ... ]
    fn parse_list(&mut self) -> Result<Term> {
        let open_bracket = self.expect_token_match(|t| matches!(t, Token::TBrackOpen))?;
        let opening_span = open_bracket.span;

        // Parse list items (terms separated by whitespace)
        let items = self.parse_list_items()?;

        let close_bracket = self.expect_closing_delimiter(opening_span, '[', Token::TBrackClose)?;

        Ok(Term::List(open_bracket, items, close_bracket))
    }

    /// Parse list items (terms)
    fn parse_list_items(&mut self) -> Result<Items<Term>> {
        let mut items = Vec::new();

        while !matches!(self.current.value, Token::TBrackClose | Token::Sof) {
            // Check for commas (not valid in Nix lists)
            if matches!(self.current.value, Token::TComma) {
                return Err(ParseError {
                    span: self.current.span,
                    kind: crate::error::ErrorKind::InvalidSyntax {
                        description: "commas are not used to separate list elements in Nix"
                            .to_string(),
                        hint: Some("use spaces to separate list elements: [1 2 3]".to_string()),
                    },
                    labels: vec![],
                });
            }

            // Check for mismatched closing delimiters before trying to parse
            if matches!(
                self.current.value,
                Token::TBraceClose | Token::TParenClose | Token::TInterClose
            ) {
                return Err(ParseError {
                    span: self.current.span,
                    kind: crate::error::ErrorKind::InvalidSyntax {
                        description: format!(
                            "mismatched delimiter: expected ']', found '{}'",
                            self.current.value.text()
                        ),
                        hint: Some(format!(
                            "change '{}' to ']' to match the opening bracket",
                            self.current.value.text()
                        )),
                    },
                    labels: vec![],
                });
            }

            // Check for comments before item
            self.collect_trivia_as_comments(&mut items);

            // Parse a term
            let term = self.parse_term()?;
            items.push(Item::Item(term));
        }

        if matches!(self.current.value, Token::TBrackClose) {
            self.collect_trivia_as_comments(&mut items);
        }

        Ok(Items(items))
    }

    /// Parse parenthesized expression: ( expr )
    fn parse_parenthesized(&mut self) -> Result<Term> {
        let open_paren = self.expect_token_match(|t| matches!(t, Token::TParenOpen))?;
        let opening_span = open_paren.span;

        let expr = self.parse_expression()?;

        let close_paren = self.expect_closing_delimiter(opening_span, '(', Token::TParenClose)?;

        Ok(Term::Parenthesized(open_paren, Box::new(expr), close_paren))
    }

    // Helper methods

    /// Parse trivia after manually consuming content (strings, paths, etc.)
    /// and return the trailing comment for the previous construct.
    /// This also stores leading trivia for the next token and advances to it.
    fn parse_trailing_trivia_and_advance(
        &mut self,
    ) -> Result<Option<crate::types::TrailingComment>> {
        // Parse trivia after the construct and split into trailing/leading
        let parsed_trivia = self.lexer.parse_trivia();
        let next_col = self.lexer.column;
        let (trail_comment, next_leading) = crate::lexer::convert_trivia(parsed_trivia, next_col);

        // Store the leading trivia for the next token
        self.lexer.trivia_buffer = next_leading;

        // Now get the next token
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
        // Create a dummy token to replace current
        let dummy = Ann {
            pre_trivia: Trivia::new(),
            span: Span::point(0),
            value: Token::Sof,
            trail_comment: None,
        };
        std::mem::replace(&mut self.current, dummy)
    }

    /// Take current token and advance to next (common pattern)
    fn take_and_advance(&mut self) -> Result<Ann<Token>> {
        let token = self.take_current();
        self.advance()?;
        Ok(token)
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

    /// Get the ending span of an expression (the span of its last/rightmost token)
    fn expr_end_span(expr: &Expression) -> Span {
        match expr {
            Expression::Term(term) => Self::term_end_span(term),
            Expression::With(_, _, _, expr) => Self::expr_end_span(expr),
            Expression::Let(_, _, _, expr) => Self::expr_end_span(expr),
            Expression::Assert(_, _, _, expr) => Self::expr_end_span(expr),
            Expression::If(_, _, _, _, _, expr) => Self::expr_end_span(expr),
            Expression::Abstraction(_, _, expr) => Self::expr_end_span(expr),
            Expression::Application(_, expr) => Self::expr_end_span(expr),
            Expression::Operation(_, _, right) => Self::expr_end_span(right),
            Expression::MemberCheck(_, _, selectors) => {
                // Last selector's span
                if let Some(last) = selectors.last() {
                    Self::simple_selector_end_span(&last.selector)
                } else {
                    // No selectors - shouldn't happen
                    Span::point(0)
                }
            }
            Expression::Negation(_, expr) => Self::expr_end_span(expr),
            Expression::Inversion(_, expr) => Self::expr_end_span(expr),
        }
    }

    /// Get the ending span of a term
    fn term_end_span(term: &Term) -> Span {
        match term {
            Term::Token(leaf) => leaf.span,
            Term::SimpleString(s) | Term::IndentedString(s) => s.span,
            Term::Path(p) => p.span,
            Term::List(_, _, close) => close.span,
            Term::Set(_, _, _, close) => close.span,
            Term::Selection(base, selectors, default) => {
                // Return the rightmost element: default > last selector > base
                if let Some((_, default_expr)) = default {
                    Self::term_end_span(default_expr)
                } else if let Some(last) = selectors.last() {
                    Self::simple_selector_end_span(&last.selector)
                } else {
                    Self::term_end_span(base)
                }
            }
            Term::Parenthesized(_, _, close) => close.span,
        }
    }

    /// Get the ending span of a simple selector
    fn simple_selector_end_span(sel: &SimpleSelector) -> Span {
        match sel {
            SimpleSelector::ID(leaf) => leaf.span,
            SimpleSelector::Interpol(ann) => ann.span,
            SimpleSelector::String(s) => s.span,
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
            let token = self.take_current();
            self.advance()?;
            Ok(token)
        } else if matches!(self.current.value, Token::Sof) {
            // EOF reached without closing delimiter
            Err(ParseError {
                span: self.current.span,
                kind: crate::error::ErrorKind::UnclosedDelimiter {
                    delimiter: opening_char,
                    opening_span,
                },
                labels: vec![],
            })
        } else {
            // Special case: comma inside parentheses (common mistake from other languages)
            if opening_char == '(' && matches!(self.current.value, Token::TComma) {
                return Err(ParseError {
                    span: self.current.span,
                    kind: crate::error::ErrorKind::InvalidSyntax {
                        description: "comma not allowed inside parentheses".to_string(),
                        hint: Some("Nix doesn't use commas in parenthesized expressions. For function calls, use spaces: f x y. For multiple values, use a list [x y] or set { a = x; b = y; }".to_string()),
                    },
                    labels: vec![],
                });
            }

            // Some other token found
            Err(ParseError {
                span: self.current.span,
                kind: crate::error::ErrorKind::UnexpectedToken {
                    expected: vec![format!("'{}'", closing_token.text())],
                    found: format!("'{}'", self.current.value.text()),
                },
                labels: vec![],
            })
        }
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
            Err(ParseError {
                span: self.current.span,
                kind: crate::error::ErrorKind::UnexpectedToken {
                    expected: vec![], // caller doesn't specify what's expected
                    found: format!("'{}'", self.current.value.text()),
                },
                labels: vec![],
            })
        }
    }

    /// Expect EOF
    fn expect_eof(&self) -> Result<()> {
        if matches!(self.current.value, Token::Sof) {
            Ok(())
        } else {
            Err(ParseError {
                span: self.current.span,
                kind: crate::error::ErrorKind::UnexpectedToken {
                    expected: vec!["end of file".to_string()],
                    found: format!("'{}'", self.current.value.text()),
                },
                labels: vec![],
            })
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
            span: open.span,
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
            || matches!(&self.current.value, Token::Identifier(name) if name == "or")
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
