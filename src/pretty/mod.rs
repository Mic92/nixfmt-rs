//! Pretty-printing for Nix AST
//!
//! Implements formatting rules from nixfmt's Pretty.hs

use crate::predoc::*;
use crate::types::*;

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
use op::push_pretty_operation;
use stmt::{insert_into_app, pretty_if, pretty_with, push_absorb_abs};
use string::{push_pretty_indented_string, push_pretty_simple_string};
use term::{push_pretty_items, push_pretty_parenthesized, push_pretty_set, push_pretty_term_list};
use util::{Width, move_trailing_comment_up};

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
            Trivium::EmptyLine() => doc.push(emptyline()),
            Trivium::LineComment(c) => {
                push_comment(doc, format!("#{}", c));
                doc.push(hardline());
            }
            Trivium::BlockComment(is_doc, lines) => {
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
            Trivium::LanguageAnnotation(lang) => {
                push_comment(doc, format!("/* {} */", lang));
                doc.push(hardspace());
            }
        }
    }
}

impl Pretty for Trivia {
    fn pretty(&self, doc: &mut Doc) {
        if self.0.is_empty() {
            return;
        }

        // Special case: single language annotation renders inline
        if self.0.len() == 1 {
            if let Trivium::LanguageAnnotation(_) = &self.0[0] {
                self.0[0].pretty(doc);
                return;
            }
        }

        doc.push(hardline());
        for trivium in &self.0 {
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
            Item::Comments(trivia) => trivia.pretty(doc),
            Item::Item(x) => push_group(doc, |d| x.pretty(d)),
        }
    }
}

impl Pretty for Binder {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Binder::Inherit(inherit, source, ids, semicolon) => {
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
            Binder::Assignment(selectors, assign, expr, semicolon) => {
                push_group(doc, |d| {
                    push_hcat(d, selectors.clone());
                    push_nested(d, |inner| {
                        inner.push(hardspace());
                        assign.pretty(inner);
                        push_absorb_rhs(inner, expr);
                    });
                    semicolon.pretty(d);
                });
            }
        }
    }
}

impl Pretty for Token {
    fn pretty(&self, doc: &mut Doc) {
        use Token::*;
        if let EnvPath(s) = self {
            push_text(doc, format!("<{}>", s));
            return;
        }
        let s = match self {
            Integer(s) => s.as_str(),
            Float(s) => s.as_str(),
            Identifier(s) => s.as_str(),
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
            TBraceClose => "}",
            TBrackOpen => "[",
            TBrackClose => "]",
            TInterOpen => "${",
            TInterClose => "}",
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
            TNegate => "-",
            TUpdate => "//",
            TPlus => "+",
            TMinus => "-",
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
            SimpleSelector::ID(id) => id.pretty(doc),
            SimpleSelector::String(ann) => {
                ann.pre_trivia.pretty(doc);
                push_pretty_simple_string(doc, &ann.value);
                ann.trail_comment.pretty(doc);
            }
            SimpleSelector::Interpol(interp) => interp.pretty(doc),
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
            Term::Token(t) => t.pretty(doc),
            Term::SimpleString(s) => {
                s.pre_trivia.pretty(doc);
                push_pretty_simple_string(doc, &s.value);
                s.trail_comment.pretty(doc);
            }
            Term::IndentedString(s) => {
                s.pre_trivia.pretty(doc);
                push_pretty_indented_string(doc, &s.value);
                s.trail_comment.pretty(doc);
            }
            Term::Path(p) => {
                p.pre_trivia.pretty(doc);
                for part in &p.value {
                    part.pretty(doc);
                }
                p.trail_comment.pretty(doc);
            }
            Term::Parenthesized(open, expr, close) => {
                push_pretty_parenthesized(doc, open, expr, close);
            }
            Term::List(open, items, close) => {
                push_group(doc, |g| push_pretty_term_list(g, open, items, close));
            }
            Term::Set(krec, open, binders, close) => {
                push_pretty_set(doc, Width::Regular, krec, open, binders, close);
            }
            Term::Selection(term, selectors, default) => {
                term.pretty(doc);

                // Separator strength depends on how likely a break before the
                // `.` chain is desirable.
                match &**term {
                    Term::Token(_) => {}
                    Term::Parenthesized(_, _, _) => doc.push(softline_prime()),
                    _ => doc.push(line_prime()),
                };

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
            Expression::Term(t) => t.pretty(doc),
            Expression::Application(_, _) => {
                push_pretty_app(doc, false, &[], false, self);
            }
            Expression::Operation(left, op, right) => {
                // `//`, `++`, `+` with an absorbable RHS get a compact layout
                // (cf. the corresponding clause in `absorbRHS`).
                if let Expression::Term(t) = &**right {
                    if is_absorbable_term(t) && op.value.is_update_concat_plus() {
                        push_group(doc, |inner| {
                            left.pretty(inner);
                            inner.push(line());
                            op.pretty(inner);
                            inner.push(hardspace());
                            push_nested(inner, |rhs_nested| {
                                t.pretty(rhs_nested);
                            });
                        });
                        return;
                    }
                }

                push_pretty_operation(doc, false, self, op);
            }
            Expression::MemberCheck(expr, question, selectors) => {
                expr.pretty(doc);
                doc.push(softline());
                question.pretty(doc);
                doc.push(hardspace());
                for sel in selectors {
                    sel.pretty(doc);
                }
            }
            Expression::Negation(minus, expr) => {
                minus.pretty(doc);
                expr.pretty(doc);
            }
            Expression::Inversion(bang, expr) => {
                bang.pretty(doc);
                expr.pretty(doc);
            }
            Expression::Let(let_kw, binders, in_kw, expr) => {
                // Strip trivia/trailing from `in` and move it down to the body,
                // mirroring the Haskell clause for `Let`.
                let mut in_kw_clean = in_kw.clone();
                in_kw_clean.pre_trivia = Trivia::new();
                in_kw_clean.trail_comment = None;

                // convertTrailing
                let mut moved_trivia_vec: Vec<Trivium> = in_kw.pre_trivia.clone().into();
                if let Some(trailing) = &in_kw.trail_comment {
                    moved_trivia_vec.push(Trivium::LineComment(format!(" {}", trailing.0)));
                }
                let moved_trivia: Trivia = moved_trivia_vec.into();

                // letPart = group $ pretty let_ <> hardline <> letBody
                // letBody = nest $ renderItems hardline binders
                let let_part = |doc: &mut Doc| {
                    push_group(doc, |g| {
                        let_kw.pretty(g);
                        g.push(hardline());
                        push_nested(g, |n| {
                            push_pretty_items(n, binders);
                        });
                    });
                };
                // inPart = group $ pretty in_ <> hardline <> trivia <> pretty expr
                let in_part = |doc: &mut Doc| {
                    push_group(doc, |g| {
                        in_kw_clean.pretty(g);
                        g.push(hardline());
                        moved_trivia.pretty(g);
                        expr.pretty(g);
                    });
                };

                // letPart <> hardline <> inPart
                let_part(doc);
                doc.push(hardline());
                in_part(doc);
            }
            Expression::If(if_kw, _, _, _, _, _) => {
                // group' RegularG $ prettyIf line $ mapFirstToken moveTrailingCommentUp expr
                // The first token of an `If` is always the `if` keyword itself.
                let if_kw_moved = move_trailing_comment_up(if_kw);
                let expr_moved = match self {
                    Expression::If(_, c, t, e0, el, e1) => Expression::If(
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
            Expression::Assert(assert_kw, cond, semicolon, expr) => {
                // group $ prettyApp False mempty False (insertIntoApp (Term (Token assert)) cond)
                //       <> ";" <> hardline <> pretty expr
                push_group(doc, |g| {
                    let assert_term = Expression::Term(Term::Token(assert_kw.clone()));
                    let (f, a) = insert_into_app(assert_term, (**cond).clone());
                    let app = Expression::Application(Box::new(f), Box::new(a));
                    push_pretty_app(g, false, &[], false, &app);
                    semicolon.pretty(g);
                    g.push(hardline());
                    expr.pretty(g);
                });
            }
            Expression::With(with_kw, env, semicolon, expr) => {
                pretty_with(doc, with_kw, env, semicolon, expr);
            }
            Expression::Abstraction(Parameter::ID(param), colon, body) => {
                push_group(doc, |group_doc| {
                    group_doc.push(line_prime());
                    param.pretty(group_doc);
                    colon.pretty(group_doc);
                    push_absorb_abs(group_doc, 1, body);
                });
            }
            Expression::Abstraction(param, colon, body) => {
                param.pretty(doc);
                colon.pretty(doc);
                doc.push(line());
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
