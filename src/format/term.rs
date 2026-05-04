use crate::ast::{
    Annotated, Binder, Expression, Item, Items, Leaf, Selector, SimpleSelector, Term, Token,
    TriviaPiece,
};
use crate::doc::{Doc, Elem, Emit, hardline, hardspace, line, linebreak};

use super::Width;
use super::app::emit_app;
use super::string::emit_simple_string;

impl Emit for SimpleSelector {
    fn emit(&self, doc: &mut Doc) {
        match self {
            Self::ID(id) => id.emit(doc),
            Self::String(ann) => {
                ann.emit_with(doc, |d, v| emit_simple_string(d, v));
            }
            Self::Interpol(interp) => interp.emit(doc),
        }
    }
}

impl Emit for Selector {
    fn emit(&self, doc: &mut Doc) {
        if let Some(dot) = &self.dot {
            dot.emit(doc);
        }
        self.selector.emit(doc);
    }
}

impl Emit for Binder {
    fn emit(&self, doc: &mut Doc) {
        match self {
            Self::Inherit {
                kw: inherit,
                from: source,
                attrs: ids,
                semi: semicolon,
            } => {
                // Determine spacing strategy based on original layout
                let same_line = inherit.span.start_line() == semicolon.span.start_line();
                let few_ids = ids.len() < 4;
                let (sep, nosep) = if same_line && few_ids {
                    (line(), linebreak())
                } else {
                    (hardline(), hardline())
                };

                doc.group(|d| {
                    inherit.emit(d);

                    let sep_doc = [sep.clone()];
                    let finish_inherit = |nested: &mut Doc| {
                        if !ids.is_empty() {
                            nested.sep_by(&sep_doc, ids);
                        }
                        nested.push_raw(nosep.clone());
                        semicolon.emit(nested);
                    };

                    match source {
                        None => {
                            d.push_raw(sep.clone());
                            d.nested(finish_inherit);
                        }
                        Some(src) => {
                            d.nested(|nested| {
                                nested.group(|g| {
                                    g.line();
                                    src.emit(g);
                                });
                                nested.push_raw(sep);
                                finish_inherit(nested);
                            });
                        }
                    }
                });
            }
            Self::Assignment {
                path: selectors,
                eq: assign,
                value: expr,
                semi: semicolon,
            } => {
                // Only allow a break after `=` when the key is long/dynamic;
                // for short plain-id keys the extra line buys almost nothing.
                let simple_lhs = selectors.len() <= 4 && selectors.iter().all(Selector::is_simple);
                doc.group(|d| {
                    d.hcat(selectors);
                    d.nested(|inner| {
                        inner.hardspace();
                        assign.emit(inner);
                        if simple_lhs {
                            expr.absorb_rhs(inner);
                        } else {
                            inner.linebreak();
                            inner.priority_group(|g| {
                                expr.absorb_rhs(g);
                            });
                        }
                    });
                    semicolon.emit(d);
                });
            }
        }
    }
}

/// Render an empty bracketed container (`[]`, `{}`), preserving a user-inserted
/// line break between the delimiters. Shared by empty list / set / param-set.
pub(super) fn empty_brackets(doc: &mut Doc, open: &Leaf, close: &Leaf) {
    open.emit(doc);
    if open.span.start_line() == close.span.start_line() {
        doc.hardspace();
    } else {
        doc.hardline();
    }
    close.emit(doc);
}

/// Mirrors `prettyTerm (List ..)` in Nixfmt/Pretty.hs (no surrounding group).
pub(super) fn emit_list(doc: &mut Doc, open: &Leaf, items: &Items<Term>, close: &Leaf) {
    if items.0.is_empty() && open.trail_comment.is_none() && close.pre_trivia.is_empty() {
        empty_brackets(doc, open, close);
    } else {
        render_list(doc, &hardline(), open, items, close);
    }
}

impl Term {
    /// Like [`Emit::emit`] but without the extra outer group around lists.
    /// Used where the caller already provides a surrounding group.
    pub(in crate::format) fn emit_bare(&self, doc: &mut Doc) {
        match self {
            Self::List { open, items, close } => emit_list(doc, open, items, close),
            _ => self.emit(doc),
        }
    }

    /// Like [`Self::emit_bare`] but renders sets in their wide (multi-line)
    /// layout. Used when the term is being absorbed onto a preceding line.
    pub(in crate::format) fn emit_wide(&self, doc: &mut Doc) {
        match self {
            Self::Set {
                rec,
                open,
                items,
                close,
            } => emit_set(doc, Width::Wide, rec.as_ref(), open, items, close),
            Self::List { open, items, close } => emit_list(doc, open, items, close),
            _ => self.emit(doc),
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
    open.emit_head(doc);

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
            open.trail_comment.emit(inner);
            items.emit_sep(inner, item_sep);
        });
    });
    close.emit(doc);
}

/// Format an attribute set with optional rec keyword
/// Based on Haskell prettySet (Pretty.hs:185-205)
pub(super) fn emit_set(
    doc: &mut Doc,
    wide: Width,
    rec: Option<&Annotated<Token>>,
    open: &Annotated<Token>,
    items: &Items<Binder>,
    close: &Annotated<Token>,
) {
    if items.0.is_empty() && open.is_lone() && close.pre_trivia.is_empty() {
        if let Some(rec) = rec {
            rec.emit(doc);
            doc.hardspace();
        }
        empty_brackets(doc, open, close);
        return;
    }

    if let Some(rec) = rec {
        rec.emit(doc);
        doc.hardspace();
    }

    open.emit_head(doc);

    let starts_with_emptyline = match items.0.first() {
        Some(Item::Comments(trivia)) => {
            trivia.iter().any(|t| matches!(t, TriviaPiece::EmptyLine()))
        }
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
            open.trail_comment.emit(inner);
            items.emit(inner);
        });
    });
    close.emit(doc);
}

impl<T: Emit> Emit for Items<T> {
    fn emit(&self, doc: &mut Doc) {
        self.emit_sep(doc, &hardline());
    }
}

impl<T: Emit> Items<T> {
    pub(super) fn emit_sep(&self, doc: &mut Doc, sep: &Elem) {
        let items = &self.0;
        match items.as_slice() {
            [] => {}
            [item] => item.emit(doc),
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
                        && let TriviaPiece::LanguageAnnotation(lang) = &trivia[0]
                        && let Item::Item(string_item) = &items[i + 1]
                    {
                        TriviaPiece::LanguageAnnotation(lang.clone()).emit(doc);
                        doc.hardspace();
                        doc.group(|d| string_item.emit(d));
                        i += 2;
                        continue;
                    }

                    items[i].emit(doc);
                    i += 1;
                }
            }
        }
    }
}

impl Expression {
    /// Render the nested document that appears between parentheses.
    pub(in crate::format) fn emit_paren_body(&self, doc: &mut Doc) {
        match self {
            _ if self.is_absorbable() => {
                doc.group(|inner| self.absorb(inner, Width::Regular));
            }
            Self::Application { .. } => {
                emit_app(doc, true, &[], true, self);
            }
            Self::Term(Term::Selection { base: term, .. }) if term.is_absorbable() => {
                doc.linebreak();
                doc.group(|inner| self.emit(inner));
                doc.linebreak();
            }
            Self::Term(Term::Selection { .. }) => {
                doc.group(|inner| self.emit(inner));
                doc.linebreak();
            }
            _ => {
                doc.linebreak();
                doc.group(|inner| self.emit(inner));
                doc.linebreak();
            }
        }
    }
}

/// Pretty print a parenthesized expression (Haskell `prettyTerm (Parenthesized ...)`).
pub(super) fn emit_paren(
    doc: &mut Doc,
    open: &Annotated<Token>,
    expr: &Expression,
    close: &Annotated<Token>,
) {
    doc.group(|g| {
        // A trailing comment on `(` becomes leading trivia so it renders
        // before the body, not after it on the same line.
        open.move_trailing_comment_up().emit(g);
        g.nested(|nested| {
            expr.emit_paren_body(nested);
            close.pre_trivia.emit(nested);
        });
        close.emit_tail(g);
    });
}
