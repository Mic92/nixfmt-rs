//! Parameter parsing utilities
//!
//! This module handles parsing of function parameters in Nix, including:
//! - Simple identifiers: `x:`
//! - Set patterns: `{x, y, z}:`
//! - Set patterns with defaults: `{x, y ? 1, z ? 2}:`
//! - Context parameters: `args@{x, y}:` or `{x, y}@args:`

use crate::error::{ParseError, Result};
use crate::types::{Expression, ParamAttr, Parameter, Span, Term, Token};

use super::Parser;

/// Scan a parameter attribute list for the first identifier formal whose name
/// satisfies `pred`.
fn find_formal<'a>(
    attrs: &'a [ParamAttr],
    mut pred: impl FnMut(&'a str) -> bool,
) -> Option<(Span, &'a str)> {
    for attr in attrs {
        if let ParamAttr::ParamAttr(name_leaf, _, _) = attr
            && let Token::Identifier(name) = &name_leaf.value
            && pred(name.as_str())
        {
            return Some((name_leaf.span, name.as_str()));
        }
    }
    None
}

/// Build the "duplicate formal function argument" error for `name` at `span`.
fn duplicate_formal_error(span: Span, name: &str) -> ParseError {
    ParseError::invalid(
        span,
        format!("duplicate formal function argument '{name}'"),
        None,
    )
}

impl Parser {
    /// Parse a full parameter (including context parameters)
    /// Parse the part of a context parameter that follows `@`.
    ///
    /// Nix only allows `id @ { formals }` or `{ formals } @ id`, so the second
    /// half is fully determined by the first: an identifier must be followed by
    /// a set pattern and vice versa. Enforcing that here (instead of accepting
    /// any parameter and validating afterwards) rejects `a@b@{}` / `a@b` /
    /// `{}@{}` with the same "expected '{'" / "expected identifier" pointing at
    /// the offending token that `nix-instantiate --parse` produces.
    pub(super) fn parse_context_second(&mut self, first: &Parameter) -> Result<Parameter> {
        match first {
            Parameter::ID(name) => {
                let open = self.expect_token(Token::TBraceOpen, "'{'")?;
                let attrs = self.parse_param_attrs()?;
                Self::check_duplicate_formals(&attrs)?;
                if let Token::Identifier(n) = &name.value {
                    Self::check_pattern_shadows_formal(n, &attrs)?;
                }
                let close = self.expect_token(Token::TBraceClose, "'}'")?;
                Ok(Parameter::Set(open, attrs, close))
            }
            Parameter::Set(_, attrs, _) => {
                if !matches!(self.current.value, Token::Identifier(_)) {
                    return Err(ParseError::unexpected(
                        self.current.span,
                        vec!["identifier".to_string()],
                        format!("'{}'", self.current.value.text()),
                    ));
                }
                let name = self.take_and_advance()?;
                if let Token::Identifier(n) = &name.value {
                    Self::check_pattern_shadows_formal(n, attrs)?;
                }
                Ok(Parameter::ID(name))
            }
            Parameter::Context(..) => unreachable!("callers pass ID or Set"),
        }
    }

    /// Parse parameter attributes: x, y, z ? 1, ...
    /// Returns Err if this looks like bindings (sees = or .) instead
    pub(super) fn parse_param_attrs(&mut self) -> Result<Vec<ParamAttr>> {
        match self.try_parse_param_attrs()? {
            Some(attrs) => Ok(attrs),
            None => Err(ParseError::invalid(
                self.current.span,
                "not a parameter - looks like binding",
                Some("parameters cannot have '=' or '.'".to_string()),
            )),
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
    pub(super) fn check_duplicate_formals(attrs: &[ParamAttr]) -> Result<()> {
        use std::collections::HashSet;
        let mut seen: HashSet<&str> = HashSet::new();
        match find_formal(attrs, |name| !seen.insert(name)) {
            Some((span, name)) => Err(duplicate_formal_error(span, name)),
            None => Ok(()),
        }
    }

    /// Check if pattern name shadows a formal parameter
    /// For args@{x, y}: the pattern name 'args' must not appear in the formals
    fn check_pattern_shadows_formal(pattern_name: &str, attrs: &[ParamAttr]) -> Result<()> {
        match find_formal(attrs, |name| name == pattern_name) {
            Some((span, name)) => Err(duplicate_formal_error(span, name)),
            None => Ok(()),
        }
    }

    /// Called from `parse_operation_or_lambda` when `:`/`@` follows an
    /// expression whose head is neither an identifier nor `{` (those are
    /// diverted earlier in `parse_abstraction_or_operation`), so `expr` can
    /// never be a valid lambda parameter.
    pub(super) fn reject_non_parameter_expr(expr: &Expression) -> ParseError {
        if let Expression::Term(Term::Token(ann)) = expr {
            return ParseError::unexpected(
                ann.span,
                vec!["identifier".to_string()],
                format!("'{}'", ann.value.text()),
            );
        }
        ParseError::invalid(
            Span::point(0),
            "expression before ':' / '@' is not a valid lambda parameter",
            Some("use a simple identifier or '{ ... }' set pattern".to_string()),
        )
    }
}
