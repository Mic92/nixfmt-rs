use crate::ast::{
    Annotated, Binder, Expression, FirstToken, Item, Items, Parameter, Term, Token, Trivia,
};
use crate::doc::{Doc, Elem, Emit, hardspace, line};

use super::Width;
use super::app::{AppCtx, emit_app};
use super::op::emit_operation_chain;

impl Term {
    /// Haskell `isAbsorbable` / `isAbsorbableTerm` (Pretty.hs).
    pub(super) fn is_absorbable(&self) -> bool {
        match self {
            // `len() >= 2` means the indented string spans multiple lines.
            Self::IndentedString(s) => s.value.len() >= 2,
            Self::Set {
                open, items, close, ..
            } => is_absorbable_braces(open, items, close),
            Self::List { open, items, close } => is_absorbable_braces(open, items, close),
            // `open` trivia is hoisted before `(` so never affects body
            // absorption; checking it flipped this between passes (patch 0005).
            Self::Parenthesized { expr, .. } => {
                matches!(&**expr, Expression::Term(t) if t.is_absorbable())
            }
            _ => false,
        }
    }
}

/// Shared absorbability rule for `[ ... ]` and `{ ... }`: absorb when the
/// braces enclose anything at all: items/comments (NixOS/nixfmt#362),
/// trivia on the opener, or a user-inserted line break (NixOS/nixfmt#253).
fn is_absorbable_braces<T>(
    open: &Annotated<Token>,
    items: &Items<T>,
    close: &Annotated<Token>,
) -> bool {
    !items.0.is_empty() || open.has_trivia() || open.span.start_line() != close.span.start_line()
}

impl Expression {
    /// Haskell `isAbsorbableExpr` (Pretty.hs).
    pub(super) fn is_absorbable(&self) -> bool {
        match self {
            Self::Term(t) => t.is_absorbable(),
            Self::With { body, .. } => {
                matches!(&**body, Self::Term(t) if t.is_absorbable())
            }
            // Absorb function declarations but only those with simple parameter(s)
            Self::Lambda {
                param: Parameter::Id(_),
                body,
                ..
            } => match &**body {
                Self::Term(t) => t.is_absorbable(),
                Self::Lambda { .. } => body.is_absorbable(),
                _ => false,
            },
            _ => false,
        }
    }
}

impl Expression {
    /// Render this expression "absorbed" onto the preceding line: an
    /// absorbable term is emitted bare/wide, a `with ... ; <absorbable>` body
    /// gets the compact single-group treatment, and anything else falls back
    /// to the regular [`Emit`] impl.
    pub(in crate::format) fn absorb(&self, doc: &mut Doc, width: Width) {
        match self {
            Self::Term(t) if t.is_absorbable() => match width {
                Width::Wide => t.emit_wide(doc),
                Width::Regular => t.emit_bare(doc),
            },
            Self::With {
                kw_with,
                scope,
                semi,
                body,
            } => match &**body {
                Self::Term(t) if t.is_absorbable() => {
                    doc.group(|g| {
                        g.linebreak();
                        kw_with.emit(g);
                        g.hardspace();
                        g.nested(|n| {
                            n.group(|gg| scope.emit(gg));
                        });
                        semi.emit(g);
                        g.hardspace();
                        g.priority_group(|pg| t.emit_wide(pg));
                    });
                }
                _ => self.emit(doc),
            },
            _ => self.emit(doc),
        }
    }
}

/// `nest $ lead <> group …`
fn nested_rhs(doc: &mut Doc, lead: Elem, f: impl FnOnce(&mut Doc)) {
    doc.nested(|d| {
        d.push_raw(lead);
        d.group(f);
    });
}

impl Expression {
    /// Format this expression as the right-hand side of an assignment or
    /// function-parameter default value.
    ///
    /// Match arms mirror Haskell `absorbRHS` one-to-one and in order so
    /// behavioural differences against the reference are easy to locate.
    pub(in crate::format) fn absorb_rhs(&self, doc: &mut Doc) {
        match self {
            // Exception to the absorbable-expr case below: do not force-expand attrsets
            // that only contain a single `inherit` statement.
            Self::Term(Term::Set { items: binders, .. })
                if matches!(binders.0.as_slice(), [Item::Item(Binder::Inherit { .. })]) =>
            {
                nested_rhs(doc, hardspace(), |inner| self.absorb(inner, Width::Regular));
            }

            // Absorbable expression. Always start on the same line, force-expand attrsets.
            _ if self.is_absorbable() => {
                nested_rhs(doc, hardspace(), |inner| self.absorb(inner, Width::Wide));
            }

            // Parenthesized expression: same special case as for the last argument of
            // a function call.
            Self::Term(Term::Parenthesized {
                open,
                expr: inner,
                close,
            }) => {
                doc.nested(|d| {
                    d.hardspace();
                    absorb_paren(d, open, inner, close);
                });
            }

            // Not all strings are absorbable, but there is nothing to gain from
            // starting them on a new line; same for paths.
            Self::Term(Term::SimpleString(_) | Term::IndentedString(_) | Term::Path(_)) => {
                nested_rhs(doc, hardspace(), |inner| self.emit(inner));
            }

            // Non-absorbable term: if multi-line, force it onto a new indented line.
            Self::Term(_) => {
                doc.nested(|d| {
                    d.group(|inner| {
                        inner.line();
                        self.emit(inner);
                    });
                });
            }

            // Function call: absorb if all arguments except the last fit on the line,
            // start on a new line otherwise.
            Self::Apply { .. } => {
                doc.nested(|d| emit_app(d, AppCtx::RHS, self));
            }

            // `with ...;` keeps the leading `line` inside the group so it can collapse
            // together with the body.
            Self::With { .. } => {
                doc.nested(|d| {
                    d.group(|inner| {
                        inner.line();
                        self.emit(inner);
                    });
                });
            }

            // Special-case `//`, `++` and `+` to be more compact in some situations.
            // Case 1: LHS is an absorbable term without leading trivia → unindent the
            // concatenation chain (https://github.com/NixOS/nixfmt/issues/228).
            Self::Operation { lhs: left, op, .. }
                if op.value.is_update_concat_plus()
                    && matches!(
                        &**left,
                        Self::Term(t)
                            if t.is_absorbable() && t.first_token().pre_trivia.is_empty()
                    ) =>
            {
                doc.hardspace();
                emit_operation_chain(doc, true, self, op);
            }

            // Case 2: operator has no trivia and RHS is an absorbable term → keep
            // `<lhs> // {` on one line and let only the RHS expand.
            Self::Operation {
                lhs: left,
                op,
                rhs: right,
            } if !op.has_trivia()
                && op.value.is_update_concat_plus()
                && matches!(&**right, Self::Term(t) if t.is_absorbable()) =>
            {
                let Self::Term(t) = &**right else {
                    unreachable!()
                };
                doc.nested(|d| {
                    d.group(|g| {
                        g.line();
                        left.emit(g);
                        g.line();
                        g.transparent_group(|tg| {
                            op.emit(tg);
                            tg.hardspace();
                            tg.priority_group(|pg| t.emit_wide(pg));
                        });
                    });
                });
            }

            // Everything else:
            // - fits on one line → keep it there
            // - fits with a newline after `=` → do that
            // - otherwise start on a new line and expand fully
            _ => {
                nested_rhs(doc, line(), |inner| self.emit(inner));
            }
        }
    }
}

/// Render parenthesized expression in a Priority group (Haskell `absorbParen`).
pub(super) fn absorb_paren(
    doc: &mut Doc,
    open: &Annotated<Token>,
    expr: &Expression,
    close: &Annotated<Token>,
) {
    doc.priority_group(|g| {
        g.nested(|outer| {
            open.emit_head(outer);
            outer.linebreak();
            outer.group(|inner| {
                inner.nested(|body| {
                    // Any trailing comment on `(` is moved down into the body
                    // as a leading line comment so it indents with the
                    // expression rather than hugging the paren.
                    if let Some(tc) = &open.trail_comment {
                        Trivia::one(tc.into()).emit(body);
                    }
                    expr.emit(body);
                    close.pre_trivia.emit(body);
                });
            });
            outer.linebreak();
            close.emit_tail(outer);
        });
    });
}
