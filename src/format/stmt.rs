use crate::ast::{Binder, Expression, Items, Leaf, Parameter, Trivia, TriviaPiece};
use crate::doc::{Doc, Elem, Pretty, hardline, line};

use super::Width;

impl Expression {
    pub(in crate::format) fn absorb_abs(&self, doc: &mut Doc, depth: usize) {
        match self {
            Self::Abstraction {
                param: Parameter::Id(param),
                colon,
                body,
            } => {
                doc.hardspace();
                param.pretty(doc);
                colon.pretty(doc);
                body.absorb_abs(doc, depth + 1);
            }
            _ if self.is_absorbable() => {
                doc.hardspace();
                doc.priority_group(|pg| self.absorb(pg, Width::Regular));
            }
            _ => {
                let separator = if depth <= 2 { line() } else { hardline() };
                doc.push_raw(separator);
                self.pretty(doc);
            }
        }
    }
}

/// Prepend an expression as the function head of a (possibly nested) application.
/// Mirrors Haskell `insertIntoApp` used by the `Assert` pretty instance.
pub(super) fn insert_into_app(insert: Expression, expr: Expression) -> (Expression, Expression) {
    match expr {
        Expression::Application { func: f, arg: a } => {
            let (f2, a2) = insert_into_app(insert, *f);
            (
                Expression::Application {
                    func: Box::new(f2),
                    arg: Box::new(a2),
                },
                *a,
            )
        }
        other => (insert, other),
    }
}

/// Render a `with` expression.
/// Mirrors Haskell `prettyWith False` (Pretty.hs); the `prettyWith True`
/// path is open-coded inside `absorb_expr`.
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

    let mut moved_trivia_vec: Vec<TriviaPiece> = in_kw.pre_trivia.clone().into();
    if let Some(trailing) = &in_kw.trail_comment {
        moved_trivia_vec.push(trailing.into());
    }
    let moved_trivia: Trivia = moved_trivia_vec.into();

    // letPart = group $ pretty let_ <> hardline <> nest (renderItems hardline binders)
    doc.group(|g| {
        let_kw.pretty(g);
        g.hardline();
        g.nested(|n| binders.pretty(n));
    });
    doc.hardline();
    // inPart = group $ pretty in_ <> hardline <> trivia <> pretty expr
    doc.group(|g| {
        in_kw_clean.pretty(g);
        g.hardline();
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
    doc.group(|g| {
        with.pretty(g);
        g.hardspace();
        g.nested(|n| {
            n.group(|inner| expr0.pretty(inner));
        });
        semicolon.pretty(g);
    });
    doc.line();
    expr1.pretty(doc);
}

/// Recursive renderer for `if`/`else if` chains.
/// Mirrors Haskell `prettyIf` (Pretty.hs, inside the `If` clause).
#[allow(clippy::too_many_arguments)]
pub(super) fn pretty_if(
    doc: &mut Doc,
    sep: Elem,
    if_kw: &Leaf,
    cond: &Expression,
    then_kw: &Leaf,
    then_branch: &Expression,
    else_kw: &Leaf,
    else_branch: &Expression,
) {
    doc.group(|g| {
        if_kw.pretty(g);
        g.line();
        g.nested(|n| cond.pretty(n));
        g.line();
        then_kw.pretty(g);
    });
    doc.surrounded(&[sep], |d| {
        d.nested(|n| {
            n.group(|g| then_branch.pretty(g));
        });
    });
    else_kw.move_trailing_comment_up().pretty(doc);
    doc.hardspace();
    match else_branch {
        Expression::If {
            kw_if,
            cond,
            kw_then,
            then_branch,
            kw_else,
            else_branch,
        } => pretty_if(
            doc,
            hardline(),
            kw_if,
            cond,
            kw_then,
            then_branch,
            kw_else,
            else_branch,
        ),
        x => {
            doc.line();
            doc.nested(|n| {
                n.group(|g| x.pretty(g));
            });
        }
    }
}
