//! Keyword expression parsing
//!
//! This module handles parsing of Nix's keyword-based control flow expressions:
//! - `let ... in ...` - Let bindings
//! - `if ... then ... else ...` - Conditional expressions
//! - `with ...; ...` - With expressions (scope introduction)
//! - `assert ...; ...` - Assert expressions

use crate::error::Result;
use crate::types::{Expression, Leaf, Token};

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
        self.parse_keyword_semi_expr(
            Token::KWith,
            "'with'",
            "'with' expression",
            Expression::With,
        )
    }

    /// Parse assert expression: assert cond ; expr
    pub(super) fn parse_assert(&mut self) -> Result<Expression> {
        self.parse_keyword_semi_expr(
            Token::KAssert,
            "'assert'",
            "'assert' condition",
            Expression::Assert,
        )
    }

    /// Shared shape for `with` / `assert`: `<kw> expr ; expr`.
    fn parse_keyword_semi_expr(
        &mut self,
        keyword: Token,
        kw_label: &'static str,
        semi_after: &'static str,
        build: fn(Leaf, Box<Expression>, Leaf, Box<Expression>) -> Expression,
    ) -> Result<Expression> {
        let kw = self.expect_token(keyword, kw_label)?;
        let head = self.parse_expression()?;
        let semi = self.expect_semicolon_after(semi_after)?;
        let body = self.parse_expression()?;
        Ok(build(kw, Box::new(head), semi, Box::new(body)))
    }
}
