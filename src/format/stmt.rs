use std::borrow::Cow;

use crate::ast::{Binder, Expression, FirstToken, Items, Leaf, Parameter};
use crate::doc::{Doc, Emit, hardline, line};

use super::Width;

/// Haskell `sourceLine tok == firstTokenLine expr`.
pub(super) fn starts_on_line_of(expr: &Expression, tok: &Leaf) -> bool {
    expr.first_token().span.start_line() == tok.span.start_line()
}

/// `instance Pretty Expression` clauses for `Abstraction` (Pretty.hs).
pub(super) fn emit_lambda(doc: &mut Doc, param: &Parameter, colon: &Leaf, body: &Expression) {
    if let Parameter::Id(id) = param {
        doc.group(|g| {
            g.linebreak();
            id.emit(g);
            colon.emit(g);
            body.absorb_lambda(g, 1);
        });
        return;
    }
    param.emit(doc);
    colon.emit(doc);
    let same_line = starts_on_line_of(body, colon);
    match body {
        _ if same_line && param.is_flat_set() && body.is_absorbable() => {
            doc.hardspace();
            doc.priority_group(|g| body.emit(g));
        }
        _ if matches!(param, Parameter::Set { .. }) && !same_line => {
            doc.hardline();
            body.emit(doc);
        }
        Expression::Term(t) if t.is_absorbable() => {
            doc.line();
            doc.group(|g| t.emit_wide(g));
        }
        _ => {
            doc.line();
            body.emit(doc);
        }
    }
}

impl Expression {
    fn absorb_lambda(&self, doc: &mut Doc, depth: usize) {
        match self {
            Self::Lambda {
                param: Parameter::Id(param),
                colon,
                body,
            } => {
                doc.hardspace();
                param.emit(doc);
                colon.emit(doc);
                body.absorb_lambda(doc, depth + 1);
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
    // inPart = group $ pretty in_ <> hardline <> pretty expr'
    doc.group(|g| {
        in_kw.value.emit(g);
        g.hardline();
        if moved_trivia.is_empty() {
            expr.emit(g);
        } else {
            // Prepend to the body's first token so layout matches pass 2,
            // where these re-lex there anyway. Clone is rare (commented `in`).
            let mut expr = expr.clone();
            let slot = expr.first_token_mut();
            moved_trivia.extend(std::mem::take(slot.pre_trivia));
            *slot.pre_trivia = moved_trivia;
            expr.emit(g);
        }
    });
}

/// Mirrors Haskell `prettyWith False` (Pretty.hs); the `prettyWith True`
/// path is open-coded inside `absorb`.
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

/// Recursive renderer for `if`/`else if` chains; `nested` is set for the
/// `else if` continuation. Mirrors Haskell `prettyIf` (Pretty.hs).
pub(super) fn emit_if(doc: &mut Doc, expr: &Expression, nested: bool) {
    let Expression::If {
        kw_if,
        cond,
        kw_then,
        then_branch,
        kw_else,
        else_branch,
    } = expr
    else {
        doc.line();
        doc.nested(|n| {
            n.group(|g| expr.emit(g));
        });
        return;
    };
    // Only the outermost `if` has its trailing comment hoisted.
    let (kw_if, sep) = if nested {
        (Cow::Borrowed(kw_if), hardline())
    } else {
        (Cow::Owned(kw_if.move_trailing_comment_up()), line())
    };
    doc.group(|g| {
        kw_if.emit(g);
        g.line();
        g.nested(|n| cond.emit(n));
        g.line();
        kw_then.emit(g);
    });
    doc.surrounded(&[sep], |d| {
        d.nested(|n| {
            n.group(|g| then_branch.emit(g));
        });
    });
    kw_else.move_trailing_comment_up().emit(doc);
    doc.hardspace();
    emit_if(doc, else_branch, true);
}
