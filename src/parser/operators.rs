//! Binary-operator precedence climbing and the precedence/associativity
//! tables that drive it.

use super::Parser;
use crate::ast::{Expression, Token};
use crate::error::{ParseError, Result};

/// Check if a token is a comparison operator
const fn is_comparison_operator(token: &Token) -> bool {
    matches!(
        token,
        Token::Equal
            | Token::Unequal
            | Token::Less
            | Token::Greater
            | Token::LessEqual
            | Token::GreaterEqual
    )
}

impl Parser {
    /// Check if current token is a binary operator
    pub(super) fn is_binary_op(&self) -> bool {
        match self.current.value {
            // Div can start a path (e.g., /tmp), so check if it looks like one
            Token::Div => !self.looks_like_path(),
            Token::Plus
            | Token::Minus
            | Token::Mul
            | Token::Concat
            | Token::Update
            | Token::And
            | Token::Or
            | Token::Equal
            | Token::Unequal
            | Token::Less
            | Token::Greater
            | Token::LessEqual
            | Token::GreaterEqual
            | Token::Implies
            | Token::PipeForward
            | Token::PipeBackward => true,
            _ => false,
        }
    }

    /// Parse binary operation if present, otherwise return expression as-is
    pub(super) fn maybe_parse_binary_operation(&mut self, expr: Expression) -> Result<Expression> {
        if self.is_binary_op() {
            self.parse_binary_operation(expr, 0)
        } else {
            Ok(expr)
        }
    }

    /// Continue parsing operation from a given left expression
    pub(super) fn continue_operation_from(&mut self, expr: Expression) -> Result<Expression> {
        let expr = if matches!(self.current.value, Token::Question) {
            let question = self.take_and_advance()?;
            let selectors = self.parse_selector_path()?;
            Expression::HasAttr {
                lhs: Box::new(expr),
                question,
                path: selectors,
            }
        } else if self.is_term_start() {
            let mut app_expr = expr;
            while self.is_term_start() && !self.is_expression_end() {
                let arg = Expression::Term(self.parse_term()?);
                app_expr = Expression::Apply {
                    func: Box::new(app_expr),
                    arg: Box::new(arg),
                };
            }
            app_expr
        } else {
            expr
        };

        self.maybe_parse_binary_operation(expr)
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
                return Err(ParseError {
                    span: op_token.span,
                    kind: crate::error::ErrorKind::ChainedComparison {
                        first_op: last_comparison_op.unwrap_or_else(|| "?".to_string()),
                        second_op: op_string,
                    },
                });
            }

            let is_right_assoc = Self::is_right_associative(&op_token.value);
            self.advance()?;

            let mut right = match self.parse_application() {
                Ok(expr) => expr,
                Err(e) => {
                    // If we failed to parse the right-hand side and current token is }
                    // (closing an interpolation), provide a more helpful error
                    if matches!(self.current.value, Token::BraceClose | Token::InterClose) {
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

            // HACK: nixfmt parses Plus as left-associative but restructures it to right-associative
            // in the AST. This is needed because some formatting code needs to match on the first
            // operand, and doing that with a left-associative chain is not possible.
            // If we have: (a + b) + c, restructure to: a + (b + c)
            left = if matches!(op_token.value, Token::Plus) {
                if let Expression::Operation {
                    lhs: one,
                    op: op1,
                    rhs: two,
                } = left
                {
                    if matches!(op1.value, Token::Plus) {
                        Expression::Operation {
                            lhs: one,
                            op: op1,
                            rhs: Box::new(Expression::Operation {
                                lhs: two,
                                op: op_token,
                                rhs: Box::new(right),
                            }),
                        }
                    } else {
                        Expression::Operation {
                            lhs: Box::new(Expression::Operation {
                                lhs: one,
                                op: op1,
                                rhs: two,
                            }),
                            op: op_token,
                            rhs: Box::new(right),
                        }
                    }
                } else {
                    Expression::Operation {
                        lhs: Box::new(left),
                        op: op_token,
                        rhs: Box::new(right),
                    }
                }
            } else {
                Expression::Operation {
                    lhs: Box::new(left),
                    op: op_token,
                    rhs: Box::new(right),
                }
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
            Token::Concat => 14,              // ++ (line 575 in nixfmt)
            Token::Mul | Token::Div => 13,    // * / (lines 576-577)
            Token::Plus | Token::Minus => 12, // + - (lines 579-580)
            // Note: Prefix Not would be at precedence 11 (line 582) but it's handled separately
            Token::Update => 10, // // (line 583)
            Token::Less | Token::Greater | Token::LessEqual | Token::GreaterEqual => 9, // comparisons (lines 584-587)
            Token::Equal | Token::Unequal => 8, // == != (lines 589-590)
            Token::And => 7,                    // && (line 592)
            Token::Or => 6,                     // || (line 593)
            Token::Implies => 5,                // -> (line 594)
            Token::PipeForward => 4,            // |> (line 595)
            Token::PipeBackward => 3,           // <| (line 596) - lowest!
            _ => 0,                             // Unknown/not a binary operator
        }
    }

    /// Check if an operator is right-associative
    ///
    /// Right-associative operators per nixfmt Types.hs:
    /// - `Concat` (++) - line 575: `InfixR`
    /// - `Update` (//) - line 583: `InfixR`
    /// - `PipeBackward` (<|) - line 596: `InfixR`
    ///
    /// Note: `Plus` (+) is `InfixL` in the spec and is parsed as left-associative,
    /// but nixfmt uses a HACK to restructure it to right-associative in the AST.
    /// This is handled separately in the `parse_binary_operation` function.
    const fn is_right_associative(token: &Token) -> bool {
        matches!(token, Token::Concat | Token::Update | Token::PipeBackward)
    }
}
