use crate::ast::{Expression, Leaf, Token};
use crate::doc::{Doc, Emit};

use super::app::{AppCtx, emit_app};

fn flatten_operation_chain<'a>(
    target: &'a Leaf,
    expr: &'a Expression,
    current_op: Option<&'a Leaf>,
    out: &mut Vec<(Option<&'a Leaf>, &'a Expression)>,
) {
    match expr {
        Expression::Operation {
            lhs: left,
            op: op_leaf,
            rhs: right,
        } if op_leaf.value == target.value => {
            flatten_operation_chain(target, left, current_op, out);
            flatten_operation_chain(target, right, Some(op_leaf), out);
        }
        _ => out.push((current_op, expr)),
    }
}

fn absorb_operation(doc: &mut Doc, expr: &Expression) {
    match expr {
        Expression::Term(term) if term.is_absorbable() => {
            doc.hardspace();
            term.emit(doc);
        }
        Expression::Operation { .. } => {
            doc.group(|group_doc| {
                group_doc.line();
                expr.emit(group_doc);
            });
        }
        Expression::Apply { .. } => {
            doc.group(|g| emit_app(g, AppCtx::RHS, expr));
        }
        _ => {
            doc.hardspace();
            expr.emit(doc);
        }
    }
}

/// `instance Pretty Expression` clause for `Operation` (Pretty.hs).
pub(super) fn emit_operation(
    doc: &mut Doc,
    whole: &Expression,
    left: &Expression,
    op: &Leaf,
    right: &Expression,
) {
    // Non-chainable comparison operators: `softline` lets the op stay on the
    // LHS's last line whenever the remainder fits.
    if matches!(
        op.value,
        Token::Less
            | Token::Greater
            | Token::LessEqual
            | Token::GreaterEqual
            | Token::Equal
            | Token::Unequal
    ) {
        left.emit(doc);
        doc.softline();
        op.emit(doc);
        doc.hardspace();
        right.emit(doc);
        return;
    }

    // `//`, `++`, `+` with an absorbable RHS get a compact layout
    // (cf. the corresponding clause in `absorbRHS`).
    if let Expression::Term(t) = right
        && t.is_absorbable()
        && op.value.is_update_concat_plus()
    {
        doc.group(|inner| {
            left.emit(inner);
            inner.line();
            op.emit(inner);
            inner.hardspace();
            inner.nested(|n| t.emit(n));
        });
        return;
    }

    emit_operation_chain(doc, false, whole, op);
}

pub(super) fn emit_operation_chain(
    doc: &mut Doc,
    force_first_term_wide: bool,
    operation: &Expression,
    op: &Leaf,
) {
    let mut parts: Vec<(Option<&Leaf>, &Expression)> = Vec::new();
    flatten_operation_chain(op, operation, None, &mut parts);

    doc.group(|group_doc| {
        for (maybe_op, expr) in &parts {
            match maybe_op {
                None => match expr {
                    Expression::Term(term) if force_first_term_wide && term.is_absorbable() => {
                        term.emit_wide(group_doc);
                    }
                    _ => expr.emit(group_doc),
                },
                Some(op_leaf) => {
                    group_doc.line();
                    op_leaf.emit(group_doc);
                    group_doc.nested(|nested| {
                        absorb_operation(nested, expr);
                    });
                }
            }
        }
    });
}
