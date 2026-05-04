//! Pretty-printing for Nix AST
//!
//! Implements formatting rules from nixfmt's Pretty.hs.
//!
//! Module layout:
//! - [`base`]: `Pretty` impls for trivia / `Annotated` / `Item` / `Token`
//! - [`term`]: terms, selectors, binders and bracketed-container helpers
//! - [`stmt`]: `let` / `with` / `if` / `assert` and lambda-body absorption
//! - [`app`], [`op`], [`absorb`], [`params`], [`string`]: per-construct rules
//!
//! `Pretty for Term` and `Pretty for Expression` stay here as the top-level
//! dispatchers that fan out into those submodules.

use crate::ast::{Annotated, Expression, Parameter, Term, Token};
use crate::predoc::{Doc, Pretty, line};

mod absorb;
mod app;
mod base;
mod op;
mod params;
mod stmt;
mod string;
mod term;

use app::pretty_app;
use op::pretty_operation;
use stmt::{insert_into_app, pretty_if, pretty_let, pretty_with};
use string::{pretty_indented_string, pretty_simple_string};
use term::{pretty_list, pretty_paren, pretty_set};

/// Whether a set/absorbed term should prefer its expanded (multi-line) layout.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Width {
    Regular,
    Wide,
}

impl Pretty for Term {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Self::Token(t) => t.pretty(doc),
            Self::SimpleString(s) => {
                s.pretty_with(doc, |d, v| pretty_simple_string(d, v));
            }
            Self::IndentedString(s) => {
                s.pretty_with(doc, |d, v| pretty_indented_string(d, v));
            }
            Self::Path(p) => p.pretty_with(doc, |d, v| {
                for part in v {
                    part.pretty(d);
                }
            }),
            Self::Parenthesized { open, expr, close } => {
                pretty_paren(doc, open, expr, close);
            }
            Self::List { open, items, close } => {
                doc.group(|g| pretty_list(g, open, items, close));
            }
            Self::Set {
                rec,
                open,
                items: binders,
                close,
            } => {
                pretty_set(doc, Width::Regular, rec.as_ref(), open, binders, close);
            }
            Self::Selection {
                base: term,
                selectors,
                default,
            } => {
                term.pretty(doc);

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
                        d.or_kw.pretty(inner);
                        inner.hardspace();
                        d.value.pretty(inner);
                    });
                }
            }
        }
    }
}

impl Pretty for Expression {
    // Single shallow match over every Expression variant.
    #[allow(clippy::too_many_lines)]
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Self::Term(t) => t.pretty(doc),
            Self::Application { .. } => {
                pretty_app(doc, false, &[], false, self);
            }
            Self::Operation {
                lhs: left,
                op,
                rhs: right,
            } => {
                pretty_operation(doc, self, left, op, right);
            }
            Self::MemberCheck {
                lhs: expr,
                question,
                path: selectors,
            } => {
                expr.pretty(doc);
                doc.softline();
                question.pretty(doc);
                doc.hardspace();
                for sel in selectors {
                    sel.pretty(doc);
                }
            }
            Self::Negation { minus, expr } => {
                minus.pretty(doc);
                expr.pretty(doc);
            }
            Self::Inversion { bang, expr } => {
                bang.pretty(doc);
                expr.pretty(doc);
            }
            Self::Let {
                kw_let: let_kw,
                bindings: binders,
                kw_in: in_kw,
                body: expr,
            } => {
                pretty_let(doc, let_kw, binders, in_kw, expr);
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
                    pretty_if(
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
                    pretty_app(g, false, &[], false, &app);
                    semicolon.pretty(g);
                    g.hardline();
                    expr.pretty(g);
                });
            }
            Self::With {
                kw_with: with_kw,
                scope: env,
                semi: semicolon,
                body: expr,
            } => {
                pretty_with(doc, with_kw, env, semicolon, expr);
            }
            Self::Abstraction {
                param: Parameter::Id(param),
                colon,
                body,
            } => {
                doc.group(|group_doc| {
                    group_doc.linebreak();
                    param.pretty(group_doc);
                    colon.pretty(group_doc);
                    body.absorb_abs(group_doc, 1);
                });
            }
            Self::Abstraction { param, colon, body } => {
                param.pretty(doc);
                colon.pretty(doc);
                doc.line();
                // Haskell `Abstraction` (set-param) clause: absorbable body
                // gets `group (prettyTermWide t)`.
                if let Self::Term(t) = &**body
                    && t.is_absorbable()
                {
                    doc.group(|g| t.pretty_wide(g));
                    return;
                }
                body.pretty(doc);
            }
        }
    }
}
