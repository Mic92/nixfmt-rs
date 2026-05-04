use crate::predoc::{Doc, DocE, GroupAnn, Pretty, hardspace, line};
use crate::types::{Ann, Binder, Expression, FirstToken, Item, Parameter, Term, Token};

use super::app::push_pretty_app;
use super::op::push_pretty_operation;
use super::term::{push_pretty_term, push_pretty_term_wide};
use super::util::{Width, split_paren_trivia};

impl Term {
    /// Haskell `isAbsorbable` / `isAbsorbableTerm` (Pretty.hs).
    pub(super) fn is_absorbable(&self) -> bool {
        match self {
            // Multi-line indented string
            Self::IndentedString(s) if s.value.len() >= 2 => true,
            // Non-empty sets and lists
            Self::Set { items, .. } if !items.0.is_empty() => true,
            Self::List { items, .. } if !items.0.is_empty() => true,
            // Empty sets and lists if they have a line break
            // https://github.com/NixOS/nixfmt/issues/253
            Self::Set {
                open, items, close, ..
            } if items.0.is_empty() && open.span.start_line() != close.span.start_line() => true,
            Self::List { open, items, close }
                if items.0.is_empty() && open.span.start_line() != close.span.start_line() =>
            {
                true
            }
            // Lists/sets with only comments are absorbable
            // https://github.com/NixOS/nixfmt/issues/362
            Self::List { open, items, .. } if open.has_trivia() || items.has_only_comments() => {
                true
            }
            Self::Set { open, items, .. } if open.has_trivia() || items.has_only_comments() => true,
            // Parenthesized absorbable term, only when the open paren has no trivia
            Self::Parenthesized { open, expr, .. } if open.is_lone() => {
                matches!(&**expr, Expression::Term(t) if t.is_absorbable())
            }
            _ => false,
        }
    }
}

impl Expression {
    /// Haskell `isAbsorbableExpr` (Pretty.hs).
    pub(super) fn is_absorbable(&self) -> bool {
        match self {
            Self::Term(t) => t.is_absorbable(),
            Self::With { body, .. } => {
                matches!(&**body, Self::Term(t) if t.is_absorbable())
            }
            // Absorb function declarations but only those with simple parameter(s)
            Self::Abstraction {
                param: Parameter::Id(_),
                body,
                ..
            } => match &**body {
                Self::Term(t) => t.is_absorbable(),
                Self::Abstraction { .. } => body.is_absorbable(),
                _ => false,
            },
            _ => false,
        }
    }
}

/// Exact port of Haskell `absorbExpr` (Pretty.hs).
///
/// Unlike absorbable terms which can be force-absorbed, some expressions may
/// turn out not to be absorbable; in that case they fall through to `pretty`.
pub(super) fn push_absorb_expr(doc: &mut Doc, width: Width, expr: &Expression) {
    match expr {
        Expression::Term(t) if t.is_absorbable() => match width {
            Width::Wide => push_pretty_term_wide(doc, t),
            Width::Regular => push_pretty_term(doc, t),
        },
        // With expression with absorbable body: treat as absorbable term via
        // `prettyWith True`.
        Expression::With {
            kw_with: with_kw,
            scope: env,
            semi: semicolon,
            body,
        } if matches!(&**body, Expression::Term(t) if t.is_absorbable()) => {
            let Expression::Term(t) = &**body else {
                unreachable!()
            };
            doc.group_ann(GroupAnn::RegularG, |g| {
                g.line_prime();
                with_kw.pretty(g);
                g.hardspace();
                g.nested(|n| {
                    n.group(|gg| env.pretty(gg));
                });
                semicolon.pretty(g);
                g.hardspace();
                g.group_ann(GroupAnn::Priority, |pg| push_pretty_term_wide(pg, t));
            });
        }
        _ => expr.pretty(doc),
    }
}

/// `nest $ lead <> group …`
fn push_nested_rhs(doc: &mut Doc, lead: DocE, f: impl FnOnce(&mut Doc)) {
    doc.nested(|d| {
        d.push_raw(lead);
        d.group(f);
    });
}

/// Format the right-hand side of an assignment or function-parameter default value.
///
/// This mirrors Haskell `absorbRHS` (Pretty.hs ~ line 657) one-to-one: each match
/// arm corresponds to exactly one Haskell `case` arm, in the same order, so that
/// behavioural differences against the reference implementation are easy to locate.
pub(super) fn push_absorb_rhs(doc: &mut Doc, expr: &Expression) {
    match expr {
        // Exception to the absorbable-expr case below: do not force-expand attrsets
        // that only contain a single `inherit` statement.
        Expression::Term(Term::Set { items: binders, .. })
            if matches!(binders.0.as_slice(), [Item::Item(Binder::Inherit { .. })]) =>
        {
            push_nested_rhs(doc, hardspace(), |inner| {
                push_absorb_expr(inner, Width::Regular, expr);
            });
        }

        // Absorbable expression. Always start on the same line, force-expand attrsets.
        _ if expr.is_absorbable() => {
            push_nested_rhs(doc, hardspace(), |inner| {
                push_absorb_expr(inner, Width::Wide, expr);
            });
        }

        // Parenthesized expression: same special case as for the last argument of
        // a function call.
        Expression::Term(Term::Parenthesized {
            open,
            expr: inner,
            close,
        }) => {
            doc.nested(|d| {
                d.hardspace();
                push_absorb_paren(d, open, inner, close);
            });
        }

        // Not all strings are absorbable, but there is nothing to gain from
        // starting them on a new line; same for paths.
        Expression::Term(Term::SimpleString(_) | Term::IndentedString(_) | Term::Path(_)) => {
            push_nested_rhs(doc, hardspace(), |inner| expr.pretty(inner));
        }

        // Non-absorbable term: if multi-line, force it onto a new indented line.
        Expression::Term(_) => {
            doc.nested(|d| {
                d.group(|inner| {
                    inner.line();
                    expr.pretty(inner);
                });
            });
        }

        // Function call: absorb if all arguments except the last fit on the line,
        // start on a new line otherwise.
        Expression::Application { .. } => {
            doc.nested(|d| push_pretty_app(d, false, &[line()], false, expr));
        }

        // `with ...;` keeps the leading `line` inside the group so it can collapse
        // together with the body.
        Expression::With { .. } => {
            doc.nested(|d| {
                d.group(|inner| {
                    inner.line();
                    expr.pretty(inner);
                });
            });
        }

        // Special-case `//`, `++` and `+` to be more compact in some situations.
        // Case 1: LHS is an absorbable term without leading trivia → unindent the
        // concatenation chain (https://github.com/NixOS/nixfmt/issues/228).
        Expression::Operation { lhs: left, op, .. }
            if op.value.is_update_concat_plus()
                && matches!(
                    &**left,
                    Expression::Term(t)
                        if t.is_absorbable() && t.first_token().pre_trivia.is_empty()
                ) =>
        {
            doc.hardspace();
            push_pretty_operation(doc, true, expr, op);
        }

        // Case 2: operator has no trivia and RHS is an absorbable term → keep
        // `<lhs> // {` on one line and let only the RHS expand.
        Expression::Operation {
            lhs: left,
            op,
            rhs: right,
        } if op.is_lone()
            && op.value.is_update_concat_plus()
            && matches!(&**right, Expression::Term(t) if t.is_absorbable()) =>
        {
            let Expression::Term(t) = &**right else {
                unreachable!()
            };
            doc.nested(|d| {
                d.group(|g| {
                    g.line();
                    left.pretty(g);
                    g.line();
                    g.group_ann(GroupAnn::Transparent, |tg| {
                        op.pretty(tg);
                        tg.hardspace();
                        tg.group_ann(GroupAnn::Priority, |pg| {
                            push_pretty_term_wide(pg, t);
                        });
                    });
                });
            });
        }

        // Everything else:
        // - fits on one line → keep it there
        // - fits with a newline after `=` → do that
        // - otherwise start on a new line and expand fully
        _ => {
            push_nested_rhs(doc, line(), |inner| expr.pretty(inner));
        }
    }
}

/// Render parenthesized expression in a Priority group (Haskell `absorbParen`).
pub(super) fn push_absorb_paren(
    doc: &mut Doc,
    open: &Ann<Token>,
    expr: &Expression,
    close: &Ann<Token>,
) {
    let (open, trail, close_pre, close) = split_paren_trivia(open, close);
    doc.group_ann(GroupAnn::Priority, |g| {
        g.nested(|outer| {
            open.pretty(outer);
            outer.line_prime();
            outer.group(|inner| {
                inner.nested(|body| {
                    // Any trailing comment on `(` is moved down into the body,
                    // mirroring `mapFirstToken (\a -> a{preTrivia = post' <> preTrivia})`.
                    trail.pretty(body);
                    expr.pretty(body);
                    close_pre.pretty(body);
                });
            });
            outer.line_prime();
            close.pretty(outer);
        });
    });
}
