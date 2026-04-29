use crate::predoc::*;
use crate::types::*;

use super::app::push_pretty_app;
use super::op::push_pretty_operation;
use super::term::push_pretty_term_wide;
use super::util::{
    Width, has_trivia, is_lone_ann, items_has_only_comments, split_paren_trivia,
    term_first_token_has_pre_trivia,
};

/// Haskell `isAbsorbable` / `isAbsorbableTerm` (Pretty.hs).
pub(super) fn is_absorbable_term(term: &Term) -> bool {
    match term {
        // Multi-line indented string
        Term::IndentedString(s) if s.value.len() >= 2 => true,
        Term::Path(_) => false,
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
            Width::Regular => t.pretty(doc),
        },
        // With expression with absorbable body: treat as absorbable term via
        // `prettyWith True`.
        Expression::With(with_kw, env, semicolon, body) if matches!(&**body, Expression::Term(t) if is_absorbable_term(t)) =>
        {
            let Expression::Term(t) = &**body else {
                unreachable!()
            };
            push_group_ann(doc, GroupAnn::RegularG, |g| {
                g.push(line_prime());
                with_kw.pretty(g);
                g.push(hardspace());
                push_nested(g, |n| push_group(n, |gg| env.pretty(gg)));
                semicolon.pretty(g);
                g.push(hardspace());
                push_group_ann(g, GroupAnn::Priority, |pg| push_pretty_term_wide(pg, t));
            });
        }
        _ => expr.pretty(doc),
    }
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
            push_nested(doc, |d| {
                d.push(hardspace());
                push_group(d, |inner| push_absorb_expr(inner, Width::Regular, expr));
            });
        }

        // Absorbable expression. Always start on the same line, force-expand attrsets.
        _ if is_absorbable_expr(expr) => {
            push_nested(doc, |d| {
                d.push(hardspace());
                push_group(d, |inner| push_absorb_expr(inner, Width::Wide, expr));
            });
        }

        // Parenthesized expression: same special case as for the last argument of
        // a function call.
        Expression::Term(Term::Parenthesized(open, inner, close)) => {
            push_nested(doc, |d| {
                d.push(hardspace());
                push_absorb_paren(d, open, inner, close);
            });
        }

        // Not all strings are absorbable, but there is nothing to gain from
        // starting them on a new line; same for paths.
        Expression::Term(Term::SimpleString(_))
        | Expression::Term(Term::IndentedString(_))
        | Expression::Term(Term::Path(_)) => {
            push_nested(doc, |d| {
                d.push(hardspace());
                push_group(d, |inner| expr.pretty(inner));
            });
        }

        // Non-absorbable term: if multi-line, force it onto a new indented line.
        Expression::Term(_) => {
            push_nested(doc, |d| {
                push_group(d, |inner| {
                    inner.push(line());
                    expr.pretty(inner);
                });
            });
        }

        // Function call: absorb if all arguments except the last fit on the line,
        // start on a new line otherwise.
        Expression::Application(_, _) => {
            push_nested(doc, |d| push_pretty_app(d, false, &[line()], false, expr));
        }

        // `with ...;` keeps the leading `line` inside the group so it can collapse
        // together with the body.
        Expression::With(_, _, _, _) => {
            push_nested(doc, |d| {
                push_group(d, |inner| {
                    inner.push(line());
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
            doc.push(hardspace());
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
            push_nested(doc, |d| {
                push_group(d, |g| {
                    g.push(line());
                    left.pretty(g);
                    g.push(line());
                    push_group_ann(g, GroupAnn::Transparent, |tg| {
                        op.pretty(tg);
                        tg.push(hardspace());
                        push_group_ann(tg, GroupAnn::Priority, |pg| {
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
            push_nested(doc, |d| {
                d.push(line());
                push_group(d, |inner| expr.pretty(inner));
            });
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
    push_group_ann(doc, GroupAnn::Priority, |g| {
        push_nested(g, |outer| {
            open.pretty(outer);
            outer.push(line_prime());
            push_group(outer, |inner| {
                push_nested(inner, |body| {
                    // Any trailing comment on `(` is moved down into the body,
                    // mirroring `mapFirstToken (\a -> a{preTrivia = post' <> preTrivia})`.
                    trail.pretty(body);
                    expr.pretty(body);
                    close_pre.pretty(body);
                });
            });
            outer.push(line_prime());
            close.pretty(outer);
        });
    });
}
