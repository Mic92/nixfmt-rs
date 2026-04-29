use crate::predoc::*;
use crate::types::*;

use super::absorb::{is_absorbable_expr, is_absorbable_term, push_absorb_expr};
use super::app::push_pretty_app;
use super::util::{has_trivia, is_lone_ann, items_has_only_comments, split_paren_trivia};

/// Mirrors `prettyTerm (List ..)` in Nixfmt/Pretty.hs (no surrounding group).
pub(super) fn push_pretty_term_list(doc: &mut Doc, open: &Leaf, items: &Items<Term>, close: &Leaf) {
    if items.0.is_empty() && open.trail_comment.is_none() && close.pre_trivia.0.is_empty() {
        open.pretty(doc);
        if open.span.start_line != close.span.start_line {
            doc.push(hardline());
        } else {
            doc.push(hardspace());
        }
        close.pretty(doc);
    } else {
        push_render_list(doc, hardline(), open, items, close);
    }
}

/// Mirrors `prettyTermWide` in Nixfmt/Pretty.hs.
pub(super) fn push_pretty_term_wide(doc: &mut Doc, term: &Term) {
    match term {
        Term::Set(krec, open, items, close) => push_pretty_set(doc, true, krec, open, items, close),
        // `prettyTermWide` delegates to `prettyTerm`, which unlike `instance
        // Pretty Term` does *not* wrap lists in an extra group.
        Term::List(open, items, close) => push_pretty_term_list(doc, open, items, close),
        _ => term.pretty(doc),
    }
}

/// `renderList` from Pretty.hs.
pub(super) fn push_render_list(
    doc: &mut Doc,
    item_sep: DocE,
    open: &Ann<Token>,
    items: &Items<Term>,
    close: &Ann<Token>,
) {
    let open_clean = Ann {
        trail_comment: None,
        ..open.clone()
    };
    open_clean.pretty(doc);

    let sur = if open.span.start_line != close.span.start_line
        || items_has_only_comments(items)
        || (has_trivia(open) && items.0.is_empty())
    {
        hardline()
    } else if items.0.is_empty() {
        hardspace()
    } else {
        line()
    };

    push_surrounded(doc, &vec![sur], |d| {
        push_nested(d, |inner| {
            open.trail_comment.pretty(inner);
            push_pretty_items_sep(inner, items, &item_sep);
        });
    });
    close.pretty(doc);
}

/// Format an attribute set with optional rec keyword
/// Based on Haskell prettySet (Pretty.hs:185-205)
pub(super) fn push_pretty_set(
    doc: &mut Doc,
    wide: bool,
    krec: &Option<Ann<Token>>,
    open: &Ann<Token>,
    items: &Items<Binder>,
    close: &Ann<Token>,
) {
    if items.0.is_empty() && is_lone_ann(open) && close.pre_trivia.0.is_empty() {
        if let Some(rec) = krec {
            rec.pretty(doc);
            doc.push(hardspace());
        }
        open.pretty(doc);
        // If the braces are on different lines, keep them like that
        doc.push(if open.span.start_line != close.span.start_line {
            hardline()
        } else {
            hardspace()
        });
        close.pretty(doc);
        return;
    }

    if let Some(rec) = krec {
        rec.pretty(doc);
        doc.push(hardspace());
    }

    let open_without_trail = Ann {
        pre_trivia: open.pre_trivia.clone(),
        span: open.span,
        trail_comment: None,
        value: open.value.clone(),
    };
    open_without_trail.pretty(doc);

    let starts_with_emptyline = match items.0.first() {
        Some(Item::Comments(trivia)) => trivia.0.iter().any(|t| matches!(t, Trivium::EmptyLine())),
        _ => false,
    };

    let braces_on_different_lines = open.span.start_line != close.span.start_line;

    // Hardline separator forces multi-line layout when the input was already
    // multi-line or starts with an empty line (Pretty.hs:200-205).
    let sep = if !items.0.is_empty() && (wide || starts_with_emptyline || braces_on_different_lines)
    {
        vec![hardline()]
    } else {
        vec![line()]
    };

    push_surrounded(doc, &sep, |d| {
        push_nested(d, |inner| {
            open.trail_comment.pretty(inner);
            push_pretty_items(inner, items);
        });
    });
    close.pretty(doc);
}

/// Haskell `prettyItems` (Pretty.hs:108-120).
pub(super) fn push_pretty_items<T: Pretty>(doc: &mut Doc, items: &Items<T>) {
    push_pretty_items_sep(doc, items, &hardline());
}

fn push_pretty_items_sep<T: Pretty>(doc: &mut Doc, items: &Items<T>, sep: &DocE) {
    let items = &items.0;
    match items.as_slice() {
        [] => {}
        [item] => item.pretty(doc),
        items => {
            let mut i = 0;
            while i < items.len() {
                if i > 0 {
                    doc.push(sep.clone());
                }

                // Special case: language annotation comment followed by string item
                if i + 1 < items.len() {
                    if let Item::Comments(trivia) = &items[i] {
                        if trivia.0.len() == 1 {
                            if let Trivium::LanguageAnnotation(lang) = &trivia.0[0] {
                                if let Item::Item(string_item) = &items[i + 1] {
                                    Trivium::LanguageAnnotation(lang.clone()).pretty(doc);
                                    doc.push(hardspace());
                                    push_group(doc, |d| string_item.pretty(d));
                                    i += 2;
                                    continue;
                                }
                            }
                        }
                    }
                }

                items[i].pretty(doc);
                i += 1;
            }
        }
    }
}

/// Render the nested document that appears between parentheses.
/// Mirrors `inner` in Haskell `prettyTerm (Parenthesized ...)`.
fn push_parenthesized_inner(doc: &mut Doc, expr: &Expression) {
    match expr {
        _ if is_absorbable_expr(expr) => {
            push_group(doc, |inner| {
                push_absorb_expr(inner, false, expr);
            });
        }
        Expression::Application(_, _) => {
            push_pretty_app(doc, true, &[], true, expr);
        }
        Expression::Term(Term::Selection(term, _, _)) if is_absorbable_term(term) => {
            doc.push(line_prime());
            push_group(doc, |inner| {
                expr.pretty(inner);
            });
            doc.push(line_prime());
        }
        Expression::Term(Term::Selection(_, _, _)) => {
            push_group(doc, |inner| {
                expr.pretty(inner);
            });
            doc.push(line_prime());
        }
        _ => {
            doc.push(line_prime());
            push_group(doc, |inner| {
                expr.pretty(inner);
            });
            doc.push(line_prime());
        }
    }
}

/// Pretty print a parenthesized expression (Haskell `prettyTerm (Parenthesized ...)`).
pub(super) fn push_pretty_parenthesized(
    doc: &mut Doc,
    open: &Ann<Token>,
    expr: &Expression,
    close: &Ann<Token>,
) {
    let (mut open, trail, close_pre, close) = split_paren_trivia(open, close);
    // moveTrailingCommentUp: a trailing comment on `(` becomes its own pre-trivia.
    open.pre_trivia.extend(trail);

    push_group(doc, |g| {
        open.pretty(g);
        push_nested(g, |nested| {
            push_parenthesized_inner(nested, expr);
            close_pre.pretty(nested);
        });
        close.pretty(g);
    });
}
