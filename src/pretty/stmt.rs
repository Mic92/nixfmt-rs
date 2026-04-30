use crate::predoc::{
    Doc, DocE, GroupAnn, Pretty, hardline, hardspace, line, push_group, push_group_ann,
    push_nested, push_surrounded,
};
use crate::types::{Binder, Expression, Items, Leaf, Parameter, Trivia, Trivium};

use super::term::push_pretty_items;

use super::absorb::{is_absorbable_expr, push_absorb_expr};
use super::util::{Width, move_trailing_comment_up};

pub(super) fn push_absorb_abs(doc: &mut Doc, depth: usize, expr: &Expression) {
    match expr {
        Expression::Abstraction(Parameter::ID(param), colon, body) => {
            doc.push(hardspace());
            param.pretty(doc);
            colon.pretty(doc);
            push_absorb_abs(doc, depth + 1, body);
        }
        _ if is_absorbable_expr(expr) => {
            doc.push(hardspace());
            push_group_ann(doc, GroupAnn::Priority, |priority_group| {
                push_absorb_expr(priority_group, Width::Regular, expr);
            });
        }
        _ => {
            let separator = if depth <= 2 { line() } else { hardline() };
            doc.push(separator);
            expr.pretty(doc);
        }
    }
}

/// Prepend an expression as the function head of a (possibly nested) application.
/// Mirrors Haskell `insertIntoApp` used by the `Assert` pretty instance.
pub(super) fn insert_into_app(insert: Expression, expr: Expression) -> (Expression, Expression) {
    match expr {
        Expression::Application(f, a) => {
            let (f2, a2) = insert_into_app(insert, *f);
            (Expression::Application(Box::new(f2), Box::new(a2)), *a)
        }
        other => (insert, other),
    }
}

/// Render a `with` expression.
/// Mirrors Haskell `prettyWith False` (Pretty.hs); the `prettyWith True`
/// path is open-coded inside `push_absorb_expr`.
/// `instance Pretty Expression` clause for `Let` (Pretty.hs).
pub(super) fn pretty_let(
    doc: &mut Doc,
    let_kw: &Leaf,
    binders: &Items<Binder>,
    in_kw: &Leaf,
    expr: &Expression,
) {
    // Strip trivia/trailing from `in` and move it down to the body.
    let mut in_kw_clean = in_kw.clone();
    in_kw_clean.pre_trivia = Trivia::new();
    in_kw_clean.trail_comment = None;

    // convertTrailing
    let mut moved_trivia_vec: Vec<Trivium> = in_kw.pre_trivia.clone().into();
    if let Some(trailing) = &in_kw.trail_comment {
        moved_trivia_vec.push(trailing.into());
    }
    let moved_trivia: Trivia = moved_trivia_vec.into();

    // letPart = group $ pretty let_ <> hardline <> nest (renderItems hardline binders)
    push_group(doc, |g| {
        let_kw.pretty(g);
        g.push(hardline());
        push_nested(g, |n| push_pretty_items(n, binders));
    });
    doc.push(hardline());
    // inPart = group $ pretty in_ <> hardline <> trivia <> pretty expr
    push_group(doc, |g| {
        in_kw_clean.pretty(g);
        g.push(hardline());
        moved_trivia.pretty(g);
        expr.pretty(g);
    });
}

pub(super) fn pretty_with(
    doc: &mut Doc,
    with: &Leaf,
    expr0: &Expression,
    semicolon: &Leaf,
    expr1: &Expression,
) {
    push_group(doc, |g| {
        with.pretty(g);
        g.push(hardspace());
        push_nested(g, |n| {
            push_group(n, |inner| expr0.pretty(inner));
        });
        semicolon.pretty(g);
    });
    doc.push(line());
    expr1.pretty(doc);
}

/// Recursive renderer for `if`/`else if` chains.
/// Mirrors Haskell `prettyIf` (Pretty.hs, inside the `If` clause).
pub(super) fn pretty_if(doc: &mut Doc, sep: DocE, expr: &Expression) {
    match expr {
        Expression::If(if_kw, cond, then_kw, expr0, else_kw, expr1) => {
            push_group(doc, |g| {
                if_kw.pretty(g);
                g.push(line());
                push_nested(g, |n| cond.pretty(n));
                g.push(line());
                then_kw.pretty(g);
            });
            push_surrounded(doc, &vec![sep], |d| {
                push_nested(d, |n| {
                    push_group(n, |g| expr0.pretty(g));
                });
            });
            move_trailing_comment_up(else_kw).pretty(doc);
            doc.push(hardspace());
            pretty_if(doc, hardline(), expr1);
        }
        x => {
            doc.push(line());
            push_nested(doc, |n| {
                push_group(n, |g| x.pretty(g));
            });
        }
    }
}
