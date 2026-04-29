//! Parameter parsing utilities
//!
//! This module handles parsing of function parameters in Nix, including:
//! - Simple identifiers: `x:`
//! - Set patterns: `{x, y, z}:`
//! - Set patterns with defaults: `{x, y ? 1, z ? 2}:`
//! - Context parameters: `args@{x, y}:` or `{x, y}@args:`

use crate::error::{ErrorKind, ParseError, Result};
use crate::types::*;

use super::Parser;

impl Parser {
    /// Parse a full parameter (including context parameters)
    pub(super) fn parse_full_parameter(&mut self) -> Result<Parameter> {
        if matches!(self.current.value, Token::TBraceOpen) {
            self.parse_set_or_context_parameter()
        } else if matches!(self.current.value, Token::Identifier(_)) {
            let ident = self.take_and_advance()?;

            if matches!(self.current.value, Token::TAt) {
                // Context parameter: id @ pattern
                let at_tok = self.take_and_advance()?;
                let second = self.parse_full_parameter()?;

                let first_param = Parameter::ID(ident);
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
            Err(Box::new(ParseError {
                span: self.current.span,
                kind: ErrorKind::UnexpectedToken {
                    expected: vec!["identifier".to_string(), "set pattern".to_string()],
                    found: format!("'{}'", self.current.value.text()),
                },
                labels: vec![],
            }))
        }
    }

    /// Parse set parameter or context parameter starting with {
    fn parse_set_or_context_parameter(&mut self) -> Result<Parameter> {
        let open_brace = self.expect_token_match(|t| matches!(t, Token::TBraceOpen))?;
        let attrs = self.parse_param_attrs()?;
        self.check_duplicate_formals(&attrs)?;
        let close_brace = self.expect_token_match(|t| matches!(t, Token::TBraceClose))?;

        let set_param = Parameter::Set(open_brace, attrs, close_brace);

        if matches!(self.current.value, Token::TAt) {
            let at_tok = self.take_and_advance()?;
            let second = self.parse_full_parameter()?;
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
    pub(super) fn parse_param_attrs(&mut self) -> Result<Vec<ParamAttr>> {
        match self.try_parse_param_attrs()? {
            Some(attrs) => Ok(attrs),
            None => Err(Box::new(ParseError {
                span: self.current.span,
                kind: ErrorKind::InvalidSyntax {
                    description: "not a parameter - looks like binding".to_string(),
                    hint: Some("parameters cannot have '=' or '.'".to_string()),
                },
                labels: vec![],
            })),
        }
    }

    /// Like [`parse_param_attrs`] but returns `Ok(None)` (instead of an
    /// allocated `ParseError`) when the input turns out to be attribute
    /// bindings rather than a parameter list. The set/parameter disambiguation
    /// in `parse_set_parameter_or_literal` hits this for every `{ x = ...; }`
    /// literal, so the "not a parameter" signal must be allocation-free.
    pub(super) fn try_parse_param_attrs(&mut self) -> Result<Option<Vec<ParamAttr>>> {
        let mut attrs = Vec::new();

        while !matches!(self.current.value, Token::TBraceClose | Token::Sof) {
            if matches!(self.current.value, Token::TEllipsis) {
                let dots = self.take_and_advance()?;
                attrs.push(ParamAttr::ParamEllipsis(dots));

                if matches!(self.current.value, Token::TComma) {
                    self.advance()?;
                }
                break; // Ellipsis must be last
            } else if matches!(self.current.value, Token::Identifier(_)) {
                let name = self.take_and_advance()?;

                if matches!(self.current.value, Token::TAssign | Token::TDot) {
                    // This is a binding (a = ...), not a parameter!
                    return Ok(None);
                }

                let default = if matches!(self.current.value, Token::TQuestion) {
                    let q = self.take_and_advance()?;
                    let def_expr = self.parse_expression()?;
                    Some((q, def_expr))
                } else {
                    None
                };

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

        Ok(Some(attrs))
    }

    /// Check for duplicate formal parameters
    /// Validates that no parameter name appears more than once in the attrs list
    pub(super) fn check_duplicate_formals(&self, attrs: &[ParamAttr]) -> Result<()> {
        use std::collections::HashSet;

        let mut seen: HashSet<&str> = HashSet::new();

        for attr in attrs {
            if let ParamAttr::ParamAttr(name_leaf, _, _) = attr {
                if let Token::Identifier(name) = &name_leaf.value {
                    if !seen.insert(name.as_str()) {
                        return Err(Box::new(ParseError {
                            span: name_leaf.span,
                            kind: ErrorKind::InvalidSyntax {
                                description: format!(
                                    "duplicate formal function argument '{}'",
                                    name
                                ),
                                hint: None,
                            },
                            labels: vec![],
                        }));
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
                        return Err(Box::new(ParseError {
                            span: name_leaf.span,
                            kind: ErrorKind::InvalidSyntax {
                                description: format!(
                                    "duplicate formal function argument '{}'",
                                    name
                                ),
                                hint: None,
                            },
                            labels: vec![],
                        }));
                    }
                }
            }
        }

        Ok(())
    }

    /// Validate context parameter: check that pattern name doesn't shadow a formal
    /// For id@{formals} or {formals}@id patterns
    pub(super) fn validate_context_parameter(
        &self,
        first: &Parameter,
        second: &Parameter,
    ) -> Result<()> {
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
            _ => {}
        }

        Ok(())
    }

    /// Convert expression to parameter (for lambda detection)
    pub(super) fn expr_to_parameter(&self, expr: Expression) -> Result<Parameter> {
        match expr {
            Expression::Term(Term::Token(ann)) => {
                if matches!(ann.value, Token::Identifier(_)) {
                    Ok(Parameter::ID(ann))
                } else {
                    Err(Box::new(ParseError {
                        span: ann.span,
                        kind: ErrorKind::UnexpectedToken {
                            expected: vec!["identifier".to_string()],
                            found: format!("'{}'", ann.value.text()),
                        },
                        labels: vec![],
                    }))
                }
            }
            Expression::Term(Term::Set(None, open, items, close)) => {
                // Convert set literal to set parameter
                // This happens for: { x, y }: or { x ? 1 }: patterns
                let attrs = self.items_to_param_attrs(items)?;
                Ok(Parameter::Set(open, attrs, close))
            }
            _ => Err(Box::new(ParseError {
                span: Span::point(0),
                kind: ErrorKind::InvalidSyntax {
                    description: "complex parameters not yet supported".to_string(),
                    hint: Some("use simple identifiers or set patterns as parameters".to_string()),
                },
                labels: vec![],
            })),
        }
    }

    /// Convert Items<Binder> to Vec<ParamAttr>
    pub(super) fn items_to_param_attrs(&self, items: Items<Binder>) -> Result<Vec<ParamAttr>> {
        let mut attrs = Vec::new();

        for item in items.0 {
            match item {
                Item::Item(binder) => {
                    match binder {
                        Binder::Assignment(mut sels, _eq, expr, comma_or_semi) => {
                            if sels.len() == 1 {
                                if let Some(Selector {
                                    dot: None,
                                    selector: SimpleSelector::ID(name),
                                }) = sels.pop()
                                {
                                    // Treat any assignment as `x ? default`
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
                                    return Err(Box::new(ParseError {
                                        span: Span::point(0),
                                        kind: ErrorKind::InvalidSyntax {
                                            description: "invalid parameter attribute".to_string(),
                                            hint: Some(
                                                "expected 'name' or 'name ? default'".to_string(),
                                            ),
                                        },
                                        labels: vec![],
                                    }));
                                }
                            } else {
                                return Err(Box::new(ParseError {
                                    span: Span::point(0),
                                    kind: ErrorKind::InvalidSyntax {
                                        description: "invalid parameter selector".to_string(),
                                        hint: Some(
                                            "expected identifier in parameter pattern".to_string(),
                                        ),
                                    },
                                    labels: vec![],
                                }));
                            }
                        }
                        Binder::Inherit(_, _, _, dots) => {
                            attrs.push(ParamAttr::ParamEllipsis(dots));
                        }
                    }
                }
                Item::Comments(_) => {}
            }
        }

        Ok(attrs)
    }
}
