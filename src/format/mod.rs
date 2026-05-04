//! Formatting rules for Nix AST
//!
//! Implements formatting rules from nixfmt's Pretty.hs.
//!
//! Module layout:
//! - [`base`]: `Emit` impls for trivia / `Annotated` / `Item` / `Token`
//! - [`term`]: terms, selectors, binders and bracketed-container helpers
//! - [`stmt`]: `let` / `with` / `if` / `assert` and lambda-body absorption
//! - [`app`], [`op`], [`absorb`], [`params`], [`string`]: per-construct rules
//!
//! `Emit for Term` and `Emit for Expression` stay here as the top-level
//! dispatchers that fan out into those submodules.

use crate::ast::{Annotated, Expression, Parameter, Term, Token};
use crate::doc::{Doc, Emit, line};

mod absorb;
mod app;
mod base;
mod op;
mod params;
mod stmt;
mod string;
mod term;

use app::emit_app;
use op::emit_operation;
use stmt::{emit_if, emit_let, emit_with, insert_into_app};
use string::{emit_indented_string, emit_simple_string};
use term::{emit_list, emit_paren, emit_set};

/// Whether a set/absorbed term should prefer its expanded (multi-line) layout.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Width {
    Regular,
    Wide,
}

impl Emit for Term {
    fn emit(&self, doc: &mut Doc) {
        match self {
            Self::Token(t) => t.emit(doc),
            Self::SimpleString(s) => {
                s.emit_with(doc, |d, v| emit_simple_string(d, v));
            }
            Self::IndentedString(s) => {
                s.emit_with(doc, |d, v| emit_indented_string(d, v));
            }
            Self::Path(p) => p.emit_with(doc, |d, v| {
                for part in v {
                    part.emit(d);
                }
            }),
            Self::Parenthesized { open, expr, close } => {
                emit_paren(doc, open, expr, close);
            }
            Self::List { open, items, close } => {
                doc.group(|g| emit_list(g, open, items, close));
            }
            Self::Set {
                rec,
                open,
                items: binders,
                close,
            } => {
                emit_set(doc, Width::Regular, rec.as_ref(), open, binders, close);
            }
            Self::Selection {
                base: term,
                selectors,
                default,
            } => {
                term.emit(doc);

                // Separator strength depends on how likely a break before the
                // `.` chain is desirable.
                match &**term {
                    // `1.a` would re-lex as float `1.` applied to `a`; keep a
                    // space. Diverges from Haskell nixfmt, which has this bug.
                    Self::Token(Annotated {
                        value: Token::Integer(_),
                        ..
                    }) if !selectors.is_empty() => {
                        doc.hardspace();
                    }
                    Self::Token(_) => {}
                    Self::Parenthesized { .. } => {
                        doc.softbreak();
                    }
                    _ => {
                        doc.linebreak();
                    }
                }

                doc.hcat(selectors.iter().cloned());

                if let Some(d) = default {
                    doc.softline();
                    doc.nested(|inner| {
                        d.or_kw.emit(inner);
                        inner.hardspace();
                        d.value.emit(inner);
                    });
                }
            }
        }
    }
}

impl Emit for Expression {
    // Single shallow match over every Expression variant.
    #[allow(clippy::too_many_lines)]
    fn emit(&self, doc: &mut Doc) {
        match self {
            Self::Term(t) => t.emit(doc),
            Self::Application { .. } => {
                emit_app(doc, false, &[], false, self);
            }
            Self::Operation {
                lhs: left,
                op,
                rhs: right,
            } => {
                emit_operation(doc, self, left, op, right);
            }
            Self::MemberCheck {
                lhs: expr,
                question,
                path: selectors,
            } => {
                expr.emit(doc);
                doc.softline();
                question.emit(doc);
                doc.hardspace();
                for sel in selectors {
                    sel.emit(doc);
                }
            }
            Self::Negation { minus, expr } => {
                minus.emit(doc);
                expr.emit(doc);
            }
            Self::Inversion { bang, expr } => {
                bang.emit(doc);
                expr.emit(doc);
            }
            Self::Let {
                kw_let: let_kw,
                bindings: binders,
                kw_in: in_kw,
                body: expr,
            } => {
                emit_let(doc, let_kw, binders, in_kw, expr);
            }
            Self::If {
                kw_if,
                cond,
                kw_then,
                then_branch,
                kw_else,
                else_branch,
            } => {
                doc.group(|g| {
                    // Only the outermost `if` keyword has its trailing comment
                    // hoisted; nested `else if` keywords keep theirs in place.
                    emit_if(
                        g,
                        line(),
                        &kw_if.move_trailing_comment_up(),
                        cond,
                        kw_then,
                        then_branch,
                        kw_else,
                        else_branch,
                    );
                });
            }
            Self::Assert {
                kw_assert: assert_kw,
                cond,
                semi: semicolon,
                body: expr,
            } => {
                // group $ prettyApp False mempty False (insertIntoApp (Term (Token assert)) cond)
                //       <> ";" <> hardline <> pretty expr
                doc.group(|g| {
                    let assert_term = Self::Term(Term::Token(assert_kw.clone()));
                    let (f, a) = insert_into_app(assert_term, (**cond).clone());
                    let app = Self::Application {
                        func: Box::new(f),
                        arg: Box::new(a),
                    };
                    emit_app(g, false, &[], false, &app);
                    semicolon.emit(g);
                    g.hardline();
                    expr.emit(g);
                });
            }
            Self::With {
                kw_with: with_kw,
                scope: env,
                semi: semicolon,
                body: expr,
            } => {
                emit_with(doc, with_kw, env, semicolon, expr);
            }
            Self::Abstraction {
                param: Parameter::Id(param),
                colon,
                body,
            } => {
                doc.group(|group_doc| {
                    group_doc.linebreak();
                    param.emit(group_doc);
                    colon.emit(group_doc);
                    body.absorb_abs(group_doc, 1);
                });
            }
            Self::Abstraction { param, colon, body } => {
                param.emit(doc);
                colon.emit(doc);
                doc.line();
                // Haskell `Abstraction` (set-param) clause: absorbable body
                // gets `group (prettyTermWide t)`.
                if let Self::Term(t) = &**body
                    && t.is_absorbable()
                {
                    doc.group(|g| t.emit_wide(g));
                    return;
                }
                body.emit(doc);
            }
        }
    }
}
