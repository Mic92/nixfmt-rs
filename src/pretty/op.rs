use crate::predoc::{Doc, Pretty, hardspace, line, push_group, push_nested, softline};
use crate::types::{Expression, Leaf, Token};

use super::absorb::is_absorbable_term;
use super::app::push_pretty_app;
use super::term::push_pretty_term_wide;

fn flatten_operation_chain<'a>(
    target: &'a Leaf,
    expr: &'a Expression,
    current_op: Option<&'a Leaf>,
    out: &mut Vec<(Option<&'a Leaf>, &'a Expression)>,
) {
    match expr {
        Expression::Operation(left, op_leaf, right) if op_leaf.value == target.value => {
            flatten_operation_chain(target, left, current_op, out);
            flatten_operation_chain(target, right, Some(op_leaf), out);
        }
        _ => out.push((current_op, expr)),
    }
}

fn push_absorb_operation(doc: &mut Doc, expr: &Expression) {
    match expr {
        Expression::Term(term) if is_absorbable_term(term) => {
            doc.push(hardspace());
            term.pretty(doc);
        }
        Expression::Operation(_, _, _) => {
            push_group(doc, |group_doc| {
                group_doc.push(line());
                expr.pretty(group_doc);
            });
        }
        Expression::Application(_, _) => {
            push_group(doc, |g| push_pretty_app(g, false, &[line()], false, expr));
        }
        _ => {
            doc.push(hardspace());
            expr.pretty(doc);
        }
    }
}

/// `instance Pretty Expression` clause for `Operation` (Pretty.hs).
pub(super) fn pretty_operation(
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
        Token::TLess
            | Token::TGreater
            | Token::TLessEqual
            | Token::TGreaterEqual
            | Token::TEqual
            | Token::TUnequal
    ) {
        left.pretty(doc);
        doc.push(softline());
        op.pretty(doc);
        doc.push(hardspace());
        right.pretty(doc);
        return;
    }

    // `//`, `++`, `+` with an absorbable RHS get a compact layout
    // (cf. the corresponding clause in `absorbRHS`).
    if let Expression::Term(t) = right
        && is_absorbable_term(t)
        && op.value.is_update_concat_plus()
    {
        push_group(doc, |inner| {
            left.pretty(inner);
            inner.push(line());
            op.pretty(inner);
            inner.push(hardspace());
            push_nested(inner, |n| t.pretty(n));
        });
        return;
    }

    push_pretty_operation(doc, false, whole, op);
}

pub(super) fn push_pretty_operation(
    doc: &mut Doc,
    force_first_term_wide: bool,
    operation: &Expression,
    op: &Leaf,
) {
    let mut parts: Vec<(Option<&Leaf>, &Expression)> = Vec::new();
    flatten_operation_chain(op, operation, None, &mut parts);

    push_group(doc, |group_doc| {
        for (maybe_op, expr) in &parts {
            match maybe_op {
                None => match expr {
                    Expression::Term(term) if force_first_term_wide && is_absorbable_term(term) => {
                        push_pretty_term_wide(group_doc, term);
                    }
                    _ => expr.pretty(group_doc),
                },
                Some(op_leaf) => {
                    group_doc.push(line());
                    op_leaf.pretty(group_doc);
                    push_nested(group_doc, |nested| {
                        push_absorb_operation(nested, expr);
                    });
                }
            }
        }
    });
}
