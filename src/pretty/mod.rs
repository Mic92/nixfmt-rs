//! Pretty-printing for Nix AST
//!
//! Implements formatting rules from nixfmt's Pretty.hs

use crate::predoc::{Doc, GroupAnn, Pretty, hardline, line, linebreak};
use crate::types::{
    Ann, Binder, Expression, Item, Parameter, Selector, SimpleSelector, Term, Token,
    TrailingComment, Trivia, Trivium, Whole,
};

mod absorb;
mod app;
mod op;
mod params;
mod stmt;
mod string;
mod term;
mod util;

use absorb::push_absorb_rhs;
use app::push_pretty_app;
use op::pretty_operation;
use stmt::{insert_into_app, pretty_if, pretty_let, pretty_with, push_absorb_abs};
use string::{push_pretty_indented_string, push_pretty_simple_string};
use term::{
    push_pretty_parenthesized, push_pretty_set, push_pretty_term_list, push_pretty_term_wide,
};
use util::{Width, pretty_ann_with};

impl Pretty for TrailingComment {
    fn pretty(&self, doc: &mut Doc) {
        doc.hardspace()
            .trailing_comment(format!("# {}", self.0))
            .hardline();
    }
}

impl Pretty for Trivium {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Self::EmptyLine() => {
                doc.emptyline();
            }
            Self::LineComment(c) => {
                doc.comment(format!("#{c}")).hardline();
            }
            Self::BlockComment(is_doc, lines) => {
                doc.comment(if *is_doc { "/**" } else { "/*" }).hardline();
                // Indent the comment using offset instead of nest
                doc.offset(2, |offset_doc| {
                    for line in lines {
                        if line.is_empty() {
                            offset_doc.emptyline();
                        } else {
                            offset_doc.comment(&**line).hardline();
                        }
                    }
                });
                doc.comment("*/").hardline();
            }
            Self::LanguageAnnotation(lang) => {
                doc.comment(format!("/* {lang} */")).hardspace();
            }
        }
    }
}

impl Pretty for Trivia {
    fn pretty(&self, doc: &mut Doc) {
        if self.is_empty() {
            return;
        }

        // Special case: single language annotation renders inline
        if self.len() == 1
            && let Trivium::LanguageAnnotation(_) = &self[0]
        {
            self[0].pretty(doc);
            return;
        }

        doc.hardline();
        for trivium in self {
            trivium.pretty(doc);
        }
    }
}

impl<T: Pretty> Pretty for Ann<T> {
    fn pretty(&self, doc: &mut Doc) {
        self.pre_trivia.pretty(doc);
        self.value.pretty(doc);
        self.trail_comment.pretty(doc);
    }
}

impl<T: Pretty> Pretty for Item<T> {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Self::Comments(trivia) => trivia.pretty(doc),
            Self::Item(x) => {
                doc.group(|d| x.pretty(d));
            }
        }
    }
}

impl Pretty for Binder {
    fn pretty(&self, doc: &mut Doc) {
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
                    inherit.pretty(d);

                    let sep_doc = [sep.clone()];
                    let finish_inherit = |nested: &mut Doc| {
                        if !ids.is_empty() {
                            nested.sep_by(&sep_doc, ids.iter().cloned());
                        }
                        nested.push_raw(nosep.clone());
                        semicolon.pretty(nested);
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
                                    src.pretty(g);
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
                    d.hcat(selectors.iter().cloned());
                    d.nested(|inner| {
                        inner.hardspace();
                        assign.pretty(inner);
                        if simple_lhs {
                            push_absorb_rhs(inner, expr);
                        } else {
                            inner.linebreak();
                            inner.group_ann(GroupAnn::Priority, |g| {
                                push_absorb_rhs(g, expr);
                            });
                        }
                    });
                    semicolon.pretty(d);
                });
            }
        }
    }
}

impl Pretty for Token {
    fn pretty(&self, doc: &mut Doc) {
        if let Self::EnvPath(s) = self {
            doc.text(format!("<{s}>"));
            return;
        }
        doc.text(self.text());
    }
}

impl Pretty for SimpleSelector {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Self::ID(id) => id.pretty(doc),
            Self::String(ann) => {
                pretty_ann_with(doc, ann, |d, v| push_pretty_simple_string(d, v));
            }
            Self::Interpol(interp) => interp.pretty(doc),
        }
    }
}

impl Pretty for Selector {
    fn pretty(&self, doc: &mut Doc) {
        if let Some(dot) = &self.dot {
            dot.pretty(doc);
        }
        self.selector.pretty(doc);
    }
}

impl Pretty for Term {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Self::Token(t) => t.pretty(doc),
            Self::SimpleString(s) => {
                pretty_ann_with(doc, s, |d, v| push_pretty_simple_string(d, v));
            }
            Self::IndentedString(s) => {
                pretty_ann_with(doc, s, |d, v| push_pretty_indented_string(d, v));
            }
            Self::Path(p) => pretty_ann_with(doc, p, |d, v| {
                for part in v {
                    part.pretty(d);
                }
            }),
            Self::Parenthesized { open, expr, close } => {
                push_pretty_parenthesized(doc, open, expr, close);
            }
            Self::List { open, items, close } => {
                doc.group(|g| push_pretty_term_list(g, open, items, close));
            }
            Self::Set {
                rec: krec,
                open,
                items: binders,
                close,
            } => {
                push_pretty_set(doc, Width::Regular, krec.as_ref(), open, binders, close);
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
                    Self::Token(Ann {
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
                push_pretty_app(doc, false, &[], false, self);
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
                // group' RegularG $ prettyIf line $ mapFirstToken moveTrailingCommentUp expr
                // The first token of an `If` is always the `if` keyword itself.
                let if_kw_moved = kw_if.move_trailing_comment_up();
                let expr_moved = Self::If {
                    kw_if: if_kw_moved,
                    cond: cond.clone(),
                    kw_then: kw_then.clone(),
                    then_branch: then_branch.clone(),
                    kw_else: kw_else.clone(),
                    else_branch: else_branch.clone(),
                };
                doc.group_ann(GroupAnn::RegularG, |g| {
                    pretty_if(g, line(), &expr_moved);
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
                    push_pretty_app(g, false, &[], false, &app);
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
                    push_absorb_abs(group_doc, 1, body);
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
                    doc.group(|g| push_pretty_term_wide(g, t));
                    return;
                }
                body.pretty(doc);
            }
        }
    }
}

impl<T: Pretty> Pretty for Whole<T> {
    fn pretty(&self, doc: &mut Doc) {
        doc.group(|doc| {
            self.value.pretty(doc);
            self.trailing_trivia.pretty(doc);
        });
        // No trailing Hardline: reference nixfmt's `--ir` output does not emit one.
    }
}
