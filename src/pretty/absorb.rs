use crate::predoc::{Doc, DocE, GroupAnn, Pretty, hardspace, line};
use crate::types::{Ann, Binder, Expression, Item, Parameter, Term, Token};

use super::app::push_pretty_app;
use super::op::push_pretty_operation;
use super::term::{push_pretty_term, push_pretty_term_wide};
use super::util::{
    Width, has_trivia, is_lone_ann, items_has_only_comments, split_paren_trivia,
    term_first_token_has_pre_trivia,
};

/// Haskell `isAbsorbable` / `isAbsorbableTerm` (Pretty.hs).
pub(super) fn is_absorbable_term(term: &Term) -> bool {
    match term {
        // Multi-line indented string
        Term::IndentedString(s) if s.value.len() >= 2 => true,
        // Non-empty sets and lists
        Term::Set(_, _, items, _) if !items.0.is_empty() => true,
        Term::List(_, items, _) if !items.0.is_empty() => true,
        // Empty sets and lists if they have a line break
        // https://github.com/NixOS/nixfmt/issues/253
        Term::Set(_, open, items, close)
            if items.0.is_empty() && open.span.start_line != close.span.start_line =>
        {
            true
        }
        Term::List(open, items, close)
            if items.0.is_empty() && open.span.start_line != close.span.start_line =>
        {
            true
        }
        // Lists/sets with only comments are absorbable
        // https://github.com/NixOS/nixfmt/issues/362
        Term::List(open, items, _) if has_trivia(open) || items_has_only_comments(items) => true,
        Term::Set(_, open, items, _) if has_trivia(open) || items_has_only_comments(items) => true,
        // Parenthesized absorbable term, only when the open paren has no trivia
        Term::Parenthesized(open, expr, _) if is_lone_ann(open) => {
            matches!(&**expr, Expression::Term(t) if is_absorbable_term(t))
        }
        _ => false,
    }
}

/// Haskell `isAbsorbableExpr` (Pretty.hs).
pub(super) fn is_absorbable_expr(expr: &Expression) -> bool {
    match expr {
        Expression::Term(t) => is_absorbable_term(t),
        Expression::With(_, _, _, body) => {
            matches!(&**body, Expression::Term(t) if is_absorbable_term(t))
        }
        // Absorb function declarations but only those with simple parameter(s)
        Expression::Abstraction(Parameter::ID(_), _, body) => match &**body {
            Expression::Term(t) => is_absorbable_term(t),
            Expression::Abstraction(_, _, _) => is_absorbable_expr(body),
            _ => false,
        },
        _ => false,
    }
}

/// Exact port of Haskell `absorbExpr` (Pretty.hs).
///
/// Unlike absorbable terms which can be force-absorbed, some expressions may
/// turn out not to be absorbable; in that case they fall through to `pretty`.
pub(super) fn push_absorb_expr(doc: &mut Doc, width: Width, expr: &Expression) {
    match expr {
        Expression::Term(t) if is_absorbable_term(t) => match width {
            Width::Wide => push_pretty_term_wide(doc, t),
            Width::Regular => push_pretty_term(doc, t),
        },
        // With expression with absorbable body: treat as absorbable term via
        // `prettyWith True`.
        Expression::With(with_kw, env, semicolon, body) if matches!(&**body, Expression::Term(t) if is_absorbable_term(t)) =>
        {
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
        Expression::Term(Term::Set(_, _, binders, _))
            if matches!(
                binders.0.as_slice(),
                [Item::Item(Binder::Inherit(_, _, _, _))]
            ) =>
        {
            push_nested_rhs(doc, hardspace(), |inner| {
                push_absorb_expr(inner, Width::Regular, expr);
            });
        }

        // Absorbable expression. Always start on the same line, force-expand attrsets.
        _ if is_absorbable_expr(expr) => {
            push_nested_rhs(doc, hardspace(), |inner| {
                push_absorb_expr(inner, Width::Wide, expr);
            });
        }

        // Parenthesized expression: same special case as for the last argument of
        // a function call.
        Expression::Term(Term::Parenthesized(open, inner, close)) => {
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
        Expression::Application(_, _) => {
            doc.nested(|d| push_pretty_app(d, false, &[line()], false, expr));
        }

        // `with ...;` keeps the leading `line` inside the group so it can collapse
        // together with the body.
        Expression::With(_, _, _, _) => {
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
        Expression::Operation(left, op, _)
            if op.value.is_update_concat_plus()
                && matches!(
                    &**left,
                    Expression::Term(t)
                        if is_absorbable_term(t) && !term_first_token_has_pre_trivia(t)
                ) =>
        {
            doc.hardspace();
            push_pretty_operation(doc, true, expr, op);
        }

        // Case 2: operator has no trivia and RHS is an absorbable term → keep
        // `<lhs> // {` on one line and let only the RHS expand.
        Expression::Operation(left, op, right)
            if is_lone_ann(op)
                && op.value.is_update_concat_plus()
                && matches!(&**right, Expression::Term(t) if is_absorbable_term(t)) =>
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
