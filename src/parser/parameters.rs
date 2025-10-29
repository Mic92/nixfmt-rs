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
                kind: ErrorKind::UnexpectedToken {
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
    pub(super) fn parse_param_attrs(&mut self) -> Result<Vec<ParamAttr>> {
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
                        kind: ErrorKind::InvalidSyntax {
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
    pub(super) fn check_duplicate_formals(&self, attrs: &[ParamAttr]) -> Result<()> {
        use std::collections::HashSet;

        let mut seen = HashSet::new();

        for attr in attrs {
            if let ParamAttr::ParamAttr(name_leaf, _, _) = attr {
                if let Token::Identifier(name) = &name_leaf.value {
                    if !seen.insert(name.clone()) {
                        // Found a duplicate!
                        return Err(ParseError {
                            span: name_leaf.span,
                            kind: ErrorKind::InvalidSyntax {
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
                            kind: ErrorKind::InvalidSyntax {
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
    pub(super) fn validate_context_parameter(&self, first: &Parameter, second: &Parameter) -> Result<()> {
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
}
