//! Keyword expression parsing
//!
//! This module handles parsing of Nix's keyword-based control flow expressions:
//! - `let ... in ...` - Let bindings
//! - `if ... then ... else ...` - Conditional expressions
//! - `with ...; ...` - With expressions (scope introduction)
//! - `assert ...; ...` - Assert expressions

use crate::error::Result;
use crate::types::*;

use super::Parser;

impl Parser {
    /// Parse let expression: let bindings in expr
    pub(super) fn parse_let(&mut self) -> Result<Expression> {
        let let_tok = self.expect_token(Token::KLet, "'let'")?;
        let bindings = self.parse_binders()?;
        let in_tok = self.expect_token(Token::KIn, "'in'")?;
        let body = self.parse_expression()?;

        Ok(Expression::Let(let_tok, bindings, in_tok, Box::new(body)))
    }

    /// Parse if expression: if cond then expr else expr
    pub(super) fn parse_if(&mut self) -> Result<Expression> {
        let if_tok = self.expect_token(Token::KIf, "'if'")?;
        let cond = self.parse_expression()?;
        let then_tok = self.expect_token(Token::KThen, "'then'")?;
        let then_expr = self.parse_expression()?;
        let else_tok = self.expect_token(Token::KElse, "'else'")?;
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
    pub(super) fn parse_with(&mut self) -> Result<Expression> {
        let with_tok = self.expect_token(Token::KWith, "'with'")?;
        let expr1 = self.parse_expression()?;
        let semi = self.expect_semicolon_after("'with' expression")?;
        let expr2 = self.parse_expression()?;

        Ok(Expression::With(
            with_tok,
            Box::new(expr1),
            semi,
            Box::new(expr2),
        ))
    }

    /// Parse assert expression: assert cond ; expr
    pub(super) fn parse_assert(&mut self) -> Result<Expression> {
        let assert_tok = self.expect_token(Token::KAssert, "'assert'")?;
        let cond = self.parse_expression()?;
        let semi = self.expect_semicolon_after("'assert' condition")?;
        let body = self.parse_expression()?;

        Ok(Expression::Assert(
            assert_tok,
            Box::new(cond),
            semi,
            Box::new(body),
        ))
    }
}
