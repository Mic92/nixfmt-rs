use crate::predoc::{Doc, DocE, Pretty, hardline, hardspace, line};
use crate::types::{Ann, Binder, Expression, Item, Items, Leaf, Term, Token, Trivium};

use super::absorb::push_absorb_expr;
use super::app::push_pretty_app;
use super::util::{Width, push_empty_brackets, split_paren_trivia};

/// Mirrors `prettyTerm (List ..)` in Nixfmt/Pretty.hs (no surrounding group).
pub(super) fn push_pretty_term_list(doc: &mut Doc, open: &Leaf, items: &Items<Term>, close: &Leaf) {
    if items.0.is_empty() && open.trail_comment.is_none() && close.pre_trivia.is_empty() {
        push_empty_brackets(doc, open, close);
    } else {
        push_render_list(doc, &hardline(), open, items, close);
    }
}

/// Mirrors Haskell `prettyTerm`: like `impl Pretty for Term` but *without* the
/// extra outer group around `List`.
pub(super) fn push_pretty_term(doc: &mut Doc, term: &Term) {
    match term {
        Term::List { open, items, close } => push_pretty_term_list(doc, open, items, close),
        _ => term.pretty(doc),
    }
}

/// Mirrors `prettyTermWide` in Nixfmt/Pretty.hs.
pub(super) fn push_pretty_term_wide(doc: &mut Doc, term: &Term) {
    match term {
        Term::Set {
            rec: krec,
            open,
            items,
            close,
        } => {
            push_pretty_set(doc, Width::Wide, krec.as_ref(), open, items, close);
        }
        // `prettyTermWide` delegates to `prettyTerm`, which unlike `instance
        // Pretty Term` does *not* wrap lists in an extra group.
        Term::List { open, items, close } => push_pretty_term_list(doc, open, items, close),
        _ => term.pretty(doc),
    }
}

/// `renderList` from Pretty.hs.
pub(super) fn push_render_list(
    doc: &mut Doc,
    item_sep: &DocE,
    open: &Ann<Token>,
    items: &Items<Term>,
    close: &Ann<Token>,
) {
    open.without_trail().pretty(doc);

    let sur = if open.span.start_line() != close.span.start_line()
        || items.has_only_comments()
        || (open.has_trivia() && items.0.is_empty())
    {
        hardline()
    } else if items.0.is_empty() {
        hardspace()
    } else {
        line()
    };

    doc.surrounded(&[sur], |d| {
        d.nested(|inner| {
            open.trail_comment.pretty(inner);
            push_pretty_items_sep(inner, items, item_sep);
        });
    });
    close.pretty(doc);
}

/// Format an attribute set with optional rec keyword
/// Based on Haskell prettySet (Pretty.hs:185-205)
pub(super) fn push_pretty_set(
    doc: &mut Doc,
    wide: Width,
    krec: Option<&Ann<Token>>,
    open: &Ann<Token>,
    items: &Items<Binder>,
    close: &Ann<Token>,
) {
    if items.0.is_empty() && open.is_lone() && close.pre_trivia.is_empty() {
        if let Some(rec) = krec {
            rec.pretty(doc);
            doc.hardspace();
        }
        push_empty_brackets(doc, open, close);
        return;
    }

    if let Some(rec) = krec {
        rec.pretty(doc);
        doc.hardspace();
    }

    open.without_trail().pretty(doc);

    let starts_with_emptyline = match items.0.first() {
        Some(Item::Comments(trivia)) => trivia.iter().any(|t| matches!(t, Trivium::EmptyLine())),
        _ => false,
    };

    // Pretty.hs:226-231. The different-line check is independent of `items`
    // so an empty set that missed the LoneAnn fast path (pre-trivia on `{`)
    // still preserves the user's line break.
    let sep = if (!items.0.is_empty() && (wide == Width::Wide || starts_with_emptyline))
        || open.span.start_line() != close.span.start_line()
    {
        hardline()
    } else {
        line()
    };

    doc.surrounded(&[sep], |d| {
        d.nested(|inner| {
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
                    doc.push_raw(sep.clone());
                }

                // Special case: language annotation comment followed by string item
                if i + 1 < items.len()
                    && let Item::Comments(trivia) = &items[i]
                    && trivia.len() == 1
                    && let Trivium::LanguageAnnotation(lang) = &trivia[0]
                    && let Item::Item(string_item) = &items[i + 1]
                {
                    Trivium::LanguageAnnotation(lang.clone()).pretty(doc);
                    doc.hardspace();
                    doc.group(|d| string_item.pretty(d));
                    i += 2;
                    continue;
                }

                items[i].pretty(doc);
                i += 1;
            }
        }
    }
}

/// Render the nested document that appears between parentheses.
/// Mirrors `inner` in Haskell `prettyTerm (Parenthesized ...)`.
pub(super) fn push_parenthesized_inner(doc: &mut Doc, expr: &Expression) {
    match expr {
        _ if expr.is_absorbable() => {
            doc.group(|inner| {
                push_absorb_expr(inner, Width::Regular, expr);
            });
        }
        Expression::Application { .. } => {
            push_pretty_app(doc, true, &[], true, expr);
        }
        Expression::Term(Term::Selection { base: term, .. }) if term.is_absorbable() => {
            doc.line_prime();
            doc.group(|inner| {
                expr.pretty(inner);
            });
            doc.line_prime();
        }
        Expression::Term(Term::Selection { .. }) => {
            doc.group(|inner| {
                expr.pretty(inner);
            });
            doc.line_prime();
        }
        _ => {
            doc.line_prime();
            doc.group(|inner| {
                expr.pretty(inner);
            });
            doc.line_prime();
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

    doc.group(|g| {
        open.pretty(g);
        g.nested(|nested| {
            push_parenthesized_inner(nested, expr);
            close_pre.pretty(nested);
        });
        close.pretty(g);
    });
}
