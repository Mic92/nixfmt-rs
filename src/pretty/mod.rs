//! Pretty-printing for Nix AST
//!
//! Implements formatting rules from nixfmt's Pretty.hs

use crate::predoc::{
    Doc, GroupAnn, Pretty, emptyline, hardline, hardspace, line, line_prime, push_comment,
    push_group, push_group_ann, push_hcat, push_nested, push_offset, push_sep_by, push_text,
    push_trailing_comment, softline, softline_prime,
};
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

use absorb::{is_absorbable_term, push_absorb_rhs};
use app::push_pretty_app;
use op::pretty_operation;
use stmt::{insert_into_app, pretty_if, pretty_let, pretty_with, push_absorb_abs};
use string::{push_pretty_indented_string, push_pretty_simple_string};
use term::{
    push_pretty_parenthesized, push_pretty_set, push_pretty_term_list, push_pretty_term_wide,
};
use util::{Width, is_simple_selector, move_trailing_comment_up, pretty_ann_with};

impl Pretty for TrailingComment {
    fn pretty(&self, doc: &mut Doc) {
        doc.push(hardspace());
        push_trailing_comment(doc, format!("# {}", self.0));
        doc.push(hardline());
    }
}

impl Pretty for Trivium {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Self::EmptyLine() => doc.push(emptyline()),
            Self::LineComment(c) => {
                push_comment(doc, format!("#{c}"));
                doc.push(hardline());
            }
            Self::BlockComment(is_doc, lines) => {
                push_comment(doc, if *is_doc { "/**" } else { "/*" });
                doc.push(hardline());
                // Indent the comment using offset instead of nest
                push_offset(doc, 2, |offset_doc| {
                    for line in lines {
                        if line.is_empty() {
                            offset_doc.push(emptyline());
                        } else {
                            push_comment(offset_doc, line);
                            offset_doc.push(hardline());
                        }
                    }
                });
                push_comment(doc, "*/");
                doc.push(hardline());
            }
            Self::LanguageAnnotation(lang) => {
                push_comment(doc, format!("/* {lang} */"));
                doc.push(hardspace());
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

        doc.push(hardline());
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
            Self::Item(x) => push_group(doc, |d| x.pretty(d)),
        }
    }
}

impl Pretty for Binder {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Self::Inherit(inherit, source, ids, semicolon) => {
                // Determine spacing strategy based on original layout
                let same_line = inherit.span.start_line == semicolon.span.start_line;
                let few_ids = ids.len() < 4;
                let (sep, nosep) = if same_line && few_ids {
                    (line(), line_prime())
                } else {
                    (hardline(), hardline())
                };

                push_group(doc, |d| {
                    inherit.pretty(d);

                    let sep_doc = vec![sep.clone()];
                    let finish_inherit = |nested: &mut Doc| {
                        if !ids.is_empty() {
                            push_sep_by(nested, &sep_doc, ids.clone());
                        }
                        nested.push(nosep.clone());
                        semicolon.pretty(nested);
                    };

                    match source {
                        None => {
                            d.push(sep.clone());
                            push_nested(d, finish_inherit);
                        }
                        Some(src) => {
                            push_nested(d, |nested| {
                                push_group(nested, |g| {
                                    g.push(line());
                                    src.pretty(g);
                                });
                                nested.push(sep);
                                finish_inherit(nested);
                            });
                        }
                    }
                });
            }
            Self::Assignment(selectors, assign, expr, semicolon) => {
                // Only allow a break after `=` when the key is long/dynamic;
                // for short plain-id keys the extra line buys almost nothing.
                let simple_lhs = selectors.len() <= 4 && selectors.iter().all(is_simple_selector);
                push_group(doc, |d| {
                    push_hcat(d, selectors.clone());
                    push_nested(d, |inner| {
                        inner.push(hardspace());
                        assign.pretty(inner);
                        if simple_lhs {
                            push_absorb_rhs(inner, expr);
                        } else {
                            inner.push(line_prime());
                            push_group_ann(inner, GroupAnn::Priority, |g| {
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
        use Token::{
            EnvPath, Float, Identifier, Integer, KAssert, KElse, KIf, KIn, KInherit, KLet, KOr,
            KRec, KThen, KWith, Sof, TAnd, TAssign, TAt, TBraceClose, TBraceOpen, TBrackClose,
            TBrackOpen, TColon, TComma, TConcat, TDiv, TDot, TDoubleQuote, TDoubleSingleQuote,
            TEllipsis, TEqual, TGreater, TGreaterEqual, TImplies, TInterClose, TInterOpen, TLess,
            TLessEqual, TMinus, TMul, TNegate, TNot, TOr, TParenClose, TParenOpen, TPipeBackward,
            TPipeForward, TPlus, TQuestion, TSemicolon, TTilde, TUnequal, TUpdate,
        };
        if let EnvPath(s) = self {
            push_text(doc, format!("<{s}>"));
            return;
        }
        let s = match self {
            Integer(s) | Float(s) | Identifier(s) => s.as_str(),
            EnvPath(_) => unreachable!("EnvPath handled above"),
            KAssert => "assert",
            KElse => "else",
            KIf => "if",
            KIn => "in",
            KInherit => "inherit",
            KLet => "let",
            KOr => "or",
            KRec => "rec",
            KThen => "then",
            KWith => "with",
            TBraceOpen => "{",
            TBraceClose | TInterClose => "}",
            TBrackOpen => "[",
            TBrackClose => "]",
            TInterOpen => "${",
            TParenOpen => "(",
            TParenClose => ")",
            TAssign => "=",
            TAt => "@",
            TColon => ":",
            TComma => ",",
            TDot => ".",
            TDoubleQuote => "\"",
            TDoubleSingleQuote => "''",
            TEllipsis => "...",
            TQuestion => "?",
            TSemicolon => ";",
            TConcat => "++",
            TNegate | TMinus => "-",
            TUpdate => "//",
            TPlus => "+",
            TMul => "*",
            TDiv => "/",
            TAnd => "&&",
            TOr => "||",
            TEqual => "==",
            TGreater => ">",
            TGreaterEqual => ">=",
            TImplies => "->",
            TLess => "<",
            TLessEqual => "<=",
            TNot => "!",
            TUnequal => "!=",
            TPipeForward => "|>",
            TPipeBackward => "<|",
            Sof => "",
            TTilde => "~",
        };
        push_text(doc, s);
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
            Self::Parenthesized(open, expr, close) => {
                push_pretty_parenthesized(doc, open, expr, close);
            }
            Self::List(open, items, close) => {
                push_group(doc, |g| push_pretty_term_list(g, open, items, close));
            }
            Self::Set(krec, open, binders, close) => {
                push_pretty_set(doc, Width::Regular, krec.as_ref(), open, binders, close);
            }
            Self::Selection(term, selectors, default) => {
                term.pretty(doc);

                // Separator strength depends on how likely a break before the
                // `.` chain is desirable.
                match &**term {
                    // `1.a` would re-lex as float `1.` applied to `a`; keep a
                    // space. Diverges from Haskell nixfmt, which has this bug.
                    Self::Token(Ann {
                        value: Token::Integer(_),
                        ..
                    }) if !selectors.is_empty() => doc.push(hardspace()),
                    Self::Token(_) => {}
                    Self::Parenthesized(_, _, _) => doc.push(softline_prime()),
                    _ => doc.push(line_prime()),
                }

                push_hcat(doc, selectors.clone());

                if let Some((or_kw, def)) = default {
                    doc.push(softline());
                    push_nested(doc, |inner| {
                        or_kw.pretty(inner);
                        inner.push(hardspace());
                        def.pretty(inner);
                    });
                }
            }
        }
    }
}

impl Pretty for Expression {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Self::Term(t) => t.pretty(doc),
            Self::Application(_, _) => {
                push_pretty_app(doc, false, &[], false, self);
            }
            Self::Operation(left, op, right) => {
                pretty_operation(doc, self, left, op, right);
            }
            Self::MemberCheck(expr, question, selectors) => {
                expr.pretty(doc);
                doc.push(softline());
                question.pretty(doc);
                doc.push(hardspace());
                for sel in selectors {
                    sel.pretty(doc);
                }
            }
            Self::Negation(minus, expr) => {
                minus.pretty(doc);
                expr.pretty(doc);
            }
            Self::Inversion(bang, expr) => {
                bang.pretty(doc);
                expr.pretty(doc);
            }
            Self::Let(let_kw, binders, in_kw, expr) => {
                pretty_let(doc, let_kw, binders, in_kw, expr);
            }
            Self::If(if_kw, _, _, _, _, _) => {
                // group' RegularG $ prettyIf line $ mapFirstToken moveTrailingCommentUp expr
                // The first token of an `If` is always the `if` keyword itself.
                let if_kw_moved = move_trailing_comment_up(if_kw);
                let expr_moved = match self {
                    Self::If(_, c, t, e0, el, e1) => Self::If(
                        if_kw_moved,
                        c.clone(),
                        t.clone(),
                        e0.clone(),
                        el.clone(),
                        e1.clone(),
                    ),
                    _ => unreachable!(),
                };
                push_group_ann(doc, GroupAnn::RegularG, |g| {
                    pretty_if(g, line(), &expr_moved);
                });
            }
            Self::Assert(assert_kw, cond, semicolon, expr) => {
                // group $ prettyApp False mempty False (insertIntoApp (Term (Token assert)) cond)
                //       <> ";" <> hardline <> pretty expr
                push_group(doc, |g| {
                    let assert_term = Self::Term(Term::Token(assert_kw.clone()));
                    let (f, a) = insert_into_app(assert_term, (**cond).clone());
                    let app = Self::Application(Box::new(f), Box::new(a));
                    push_pretty_app(g, false, &[], false, &app);
                    semicolon.pretty(g);
                    g.push(hardline());
                    expr.pretty(g);
                });
            }
            Self::With(with_kw, env, semicolon, expr) => {
                pretty_with(doc, with_kw, env, semicolon, expr);
            }
            Self::Abstraction(Parameter::ID(param), colon, body) => {
                push_group(doc, |group_doc| {
                    group_doc.push(line_prime());
                    param.pretty(group_doc);
                    colon.pretty(group_doc);
                    push_absorb_abs(group_doc, 1, body);
                });
            }
            Self::Abstraction(param, colon, body) => {
                param.pretty(doc);
                colon.pretty(doc);
                doc.push(line());
                // Haskell `Abstraction` (set-param) clause: absorbable body
                // gets `group (prettyTermWide t)`.
                if let Self::Term(t) = &**body
                    && is_absorbable_term(t)
                {
                    push_group(doc, |g| push_pretty_term_wide(g, t));
                    return;
                }
                body.pretty(doc);
            }
        }
    }
}

impl<T: Pretty> Pretty for Whole<T> {
    fn pretty(&self, doc: &mut Doc) {
        push_group(doc, |doc| {
            self.value.pretty(doc);
            self.trailing_trivia.pretty(doc);
        });
        // No trailing Hardline: reference nixfmt's `--ir` output does not emit one.
    }
}
