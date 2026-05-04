use crate::ast::{Annotated, Binder, Expression, Item, Items, Leaf, Term, Token, Trivium};
use crate::predoc::{Doc, Elem, Pretty, hardline, hardspace, line};

use super::Width;
use super::app::pretty_app;

/// Render an empty bracketed container (`[]`, `{}`), preserving a user-inserted
/// line break between the delimiters. Shared by empty list / set / param-set.
pub(super) fn empty_brackets(doc: &mut Doc, open: &Leaf, close: &Leaf) {
    open.pretty(doc);
    if open.span.start_line() == close.span.start_line() {
        doc.hardspace();
    } else {
        doc.hardline();
    }
    close.pretty(doc);
}

/// Mirrors `prettyTerm (List ..)` in Nixfmt/Pretty.hs (no surrounding group).
pub(super) fn pretty_list(doc: &mut Doc, open: &Leaf, items: &Items<Term>, close: &Leaf) {
    if items.0.is_empty() && open.trail_comment.is_none() && close.pre_trivia.is_empty() {
        empty_brackets(doc, open, close);
    } else {
        render_list(doc, &hardline(), open, items, close);
    }
}

impl Term {
    /// Like [`Pretty::pretty`] but without the extra outer group around lists.
    /// Used where the caller already provides a surrounding group.
    pub(in crate::pretty) fn pretty_bare(&self, doc: &mut Doc) {
        match self {
            Self::List { open, items, close } => pretty_list(doc, open, items, close),
            _ => self.pretty(doc),
        }
    }

    /// Like [`Self::pretty_bare`] but renders sets in their wide (multi-line)
    /// layout. Used when the term is being absorbed onto a preceding line.
    pub(in crate::pretty) fn pretty_wide(&self, doc: &mut Doc) {
        match self {
            Self::Set {
                rec,
                open,
                items,
                close,
            } => pretty_set(doc, Width::Wide, rec.as_ref(), open, items, close),
            Self::List { open, items, close } => pretty_list(doc, open, items, close),
            _ => self.pretty(doc),
        }
    }
}

/// `renderList` from Pretty.hs.
pub(super) fn render_list(
    doc: &mut Doc,
    item_sep: &Elem,
    open: &Annotated<Token>,
    items: &Items<Term>,
    close: &Annotated<Token>,
) {
    open.pretty_head(doc);

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
            items.pretty_sep(inner, item_sep);
        });
    });
    close.pretty(doc);
}

/// Format an attribute set with optional rec keyword
/// Based on Haskell prettySet (Pretty.hs:185-205)
pub(super) fn pretty_set(
    doc: &mut Doc,
    wide: Width,
    rec: Option<&Annotated<Token>>,
    open: &Annotated<Token>,
    items: &Items<Binder>,
    close: &Annotated<Token>,
) {
    if items.0.is_empty() && open.is_lone() && close.pre_trivia.is_empty() {
        if let Some(rec) = rec {
            rec.pretty(doc);
            doc.hardspace();
        }
        empty_brackets(doc, open, close);
        return;
    }

    if let Some(rec) = rec {
        rec.pretty(doc);
        doc.hardspace();
    }

    open.pretty_head(doc);

    let starts_with_emptyline = match items.0.first() {
        Some(Item::Comments(trivia)) => trivia.iter().any(|t| matches!(t, Trivium::EmptyLine())),
        _ => false,
    };

    // The different-line check is independent of `items` so an empty set that
    // missed the `is_lone` fast path (pre-trivia on `{`) still preserves the
    // user's line break.
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
            items.pretty(inner);
        });
    });
    close.pretty(doc);
}

impl<T: Pretty> Pretty for Items<T> {
    fn pretty(&self, doc: &mut Doc) {
        self.pretty_sep(doc, &hardline());
    }
}

impl<T: Pretty> Items<T> {
    pub(super) fn pretty_sep(&self, doc: &mut Doc, sep: &Elem) {
        let items = &self.0;
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
}

impl Expression {
    /// Render the nested document that appears between parentheses.
    pub(in crate::pretty) fn pretty_paren_body(&self, doc: &mut Doc) {
        match self {
            _ if self.is_absorbable() => {
                doc.group(|inner| self.absorb(inner, Width::Regular));
            }
            Self::Application { .. } => {
                pretty_app(doc, true, &[], true, self);
            }
            Self::Term(Term::Selection { base: term, .. }) if term.is_absorbable() => {
                doc.linebreak();
                doc.group(|inner| self.pretty(inner));
                doc.linebreak();
            }
            Self::Term(Term::Selection { .. }) => {
                doc.group(|inner| self.pretty(inner));
                doc.linebreak();
            }
            _ => {
                doc.linebreak();
                doc.group(|inner| self.pretty(inner));
                doc.linebreak();
            }
        }
    }
}

/// Pretty print a parenthesized expression (Haskell `prettyTerm (Parenthesized ...)`).
pub(super) fn pretty_paren(
    doc: &mut Doc,
    open: &Annotated<Token>,
    expr: &Expression,
    close: &Annotated<Token>,
) {
    doc.group(|g| {
        // A trailing comment on `(` becomes leading trivia so it renders
        // before the body, not after it on the same line.
        open.move_trailing_comment_up().pretty(g);
        g.nested(|nested| {
            expr.pretty_paren_body(nested);
            close.pre_trivia.pretty(nested);
        });
        close.pretty_tail(g);
    });
}
