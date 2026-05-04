use crate::ast::{Binder, Expression, Items, Leaf, Parameter};
use crate::doc::{Doc, Elem, Emit, hardline, line};

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
                param.emit(doc);
                colon.emit(doc);
                body.absorb_abs(doc, depth + 1);
            }
            _ if self.is_absorbable() => {
                doc.hardspace();
                doc.priority_group(|pg| self.absorb(pg, Width::Regular));
            }
            _ => {
                let separator = if depth <= 2 { line() } else { hardline() };
                doc.push_raw(separator);
                self.emit(doc);
            }
        }
    }
}

/// Render a `with` expression.
/// Mirrors Haskell `prettyWith False` (Pretty.hs); the `prettyWith True`
/// path is open-coded inside `absorb_expr`.
/// `instance Pretty Expression` clause for `Let` (Pretty.hs).
pub(super) fn emit_let(
    doc: &mut Doc,
    let_kw: &Leaf,
    binders: &Items<Binder>,
    in_kw: &Leaf,
    expr: &Expression,
) {
    // Trivia/trailing on `in` are moved down to the body.
    let mut moved_trivia = in_kw.pre_trivia.clone();
    if let Some(trailing) = &in_kw.trail_comment {
        moved_trivia.push(trailing.into());
    }

    // letPart = group $ pretty let_ <> hardline <> nest (renderItems hardline binders)
    doc.group(|g| {
        let_kw.emit(g);
        g.hardline();
        g.nested(|n| binders.emit(n));
    });
    doc.hardline();
    // inPart = group $ pretty in_ <> hardline <> trivia <> pretty expr
    doc.group(|g| {
        in_kw.value.emit(g);
        g.hardline();
        moved_trivia.emit(g);
        expr.emit(g);
    });
}

pub(super) fn emit_with(
    doc: &mut Doc,
    with: &Leaf,
    expr0: &Expression,
    semicolon: &Leaf,
    expr1: &Expression,
) {
    doc.group(|g| {
        with.emit(g);
        g.hardspace();
        g.nested(|n| {
            n.group(|inner| expr0.emit(inner));
        });
        semicolon.emit(g);
    });
    doc.line();
    expr1.emit(doc);
}

/// Recursive renderer for `if`/`else if` chains.
/// Mirrors Haskell `prettyIf` (Pretty.hs, inside the `If` clause).
#[allow(clippy::too_many_arguments)]
pub(super) fn emit_if(
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
        if_kw.emit(g);
        g.line();
        g.nested(|n| cond.emit(n));
        g.line();
        then_kw.emit(g);
    });
    doc.surrounded(&[sep], |d| {
        d.nested(|n| {
            n.group(|g| then_branch.emit(g));
        });
    });
    else_kw.move_trailing_comment_up().emit(doc);
    doc.hardspace();
    match else_branch {
        Expression::If {
            kw_if,
            cond,
            kw_then,
            then_branch,
            kw_else,
            else_branch,
        } => emit_if(
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
                n.group(|g| x.emit(g));
            });
        }
    }
}
