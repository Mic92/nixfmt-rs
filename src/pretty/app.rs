use crate::predoc::*;
use crate::types::*;

use super::absorb::{is_absorbable_term, push_absorb_paren};
use super::term::{push_pretty_term_wide, push_render_list};
use super::util::{has_trivia, is_simple_expression, is_simple_term, move_trailing_comment_up};

/// `absorbInner` from Pretty.hs: short lists of simple terms get a soft `line`
/// separator so they may stay on one line; everything else falls back to `pretty`.
fn push_absorb_inner(doc: &mut Doc, arg: &Expression) {
    if let Expression::Term(Term::List(open, items, close)) = arg {
        let all_simple = items.0.iter().all(|item| match item {
            Item::Item(t) => is_simple_term(t),
            Item::Comments(_) => true,
        });
        if items.0.len() <= 6 && all_simple {
            push_render_list(doc, line(), open, items, close);
            return;
        }
    }
    arg.pretty(doc);
}

/// Strip and return the pre-trivia of the first token of an expression,
/// after applying `move_trailing_comment_up` to that token. This is the
/// `mapFirstToken' ((\a -> (a{preTrivia=[]}, preTrivia)) . moveTrailingCommentUp)`
/// invocation from Haskell `prettyApp`.
fn extract_first_comment_expr(expr: &Expression) -> (Expression, Trivia) {
    fn go_ann<T: Clone>(ann: &Ann<T>) -> (Ann<T>, Trivia) {
        let moved = move_trailing_comment_up(ann);
        let trivia = moved.pre_trivia.clone();
        (
            Ann {
                pre_trivia: Trivia::new(),
                ..moved
            },
            trivia,
        )
    }
    fn go_param(p: &Parameter) -> (Parameter, Trivia) {
        match p {
            Parameter::ID(n) => {
                let (n, t) = go_ann(n);
                (Parameter::ID(n), t)
            }
            Parameter::Set(open, attrs, close) => {
                let (open, t) = go_ann(open);
                (Parameter::Set(open, attrs.clone(), close.clone()), t)
            }
            Parameter::Context(first, at, second) => {
                let (first, t) = go_param(first);
                (
                    Parameter::Context(Box::new(first), at.clone(), second.clone()),
                    t,
                )
            }
        }
    }
    fn go_term(term: &Term) -> (Term, Trivia) {
        match term {
            Term::Token(l) => {
                let (l, t) = go_ann(l);
                (Term::Token(l), t)
            }
            Term::SimpleString(s) => {
                let (s, t) = go_ann(s);
                (Term::SimpleString(s), t)
            }
            Term::IndentedString(s) => {
                let (s, t) = go_ann(s);
                (Term::IndentedString(s), t)
            }
            Term::Path(p) => {
                let (p, t) = go_ann(p);
                (Term::Path(p), t)
            }
            Term::List(open, items, close) => {
                let (open, t) = go_ann(open);
                (Term::List(open, items.clone(), close.clone()), t)
            }
            Term::Set(Some(rec), open, items, close) => {
                let (rec, t) = go_ann(rec);
                (
                    Term::Set(Some(rec), open.clone(), items.clone(), close.clone()),
                    t,
                )
            }
            Term::Set(None, open, items, close) => {
                let (open, t) = go_ann(open);
                (Term::Set(None, open, items.clone(), close.clone()), t)
            }
            Term::Selection(inner, sels, def) => {
                let (inner, t) = go_term(inner);
                (
                    Term::Selection(Box::new(inner), sels.clone(), def.clone()),
                    t,
                )
            }
            Term::Parenthesized(open, expr, close) => {
                let (open, t) = go_ann(open);
                (Term::Parenthesized(open, expr.clone(), close.clone()), t)
            }
        }
    }
    match expr {
        Expression::Term(term) => {
            let (term, t) = go_term(term);
            (Expression::Term(term), t)
        }
        Expression::With(kw, e0, semi, e1) => {
            let (kw, t) = go_ann(kw);
            (
                Expression::With(kw, e0.clone(), semi.clone(), e1.clone()),
                t,
            )
        }
        Expression::Let(kw, items, in_, body) => {
            let (kw, t) = go_ann(kw);
            (
                Expression::Let(kw, items.clone(), in_.clone(), body.clone()),
                t,
            )
        }
        Expression::Assert(kw, cond, semi, body) => {
            let (kw, t) = go_ann(kw);
            (
                Expression::Assert(kw, cond.clone(), semi.clone(), body.clone()),
                t,
            )
        }
        Expression::If(kw, e0, then_, e1, else_, e2) => {
            let (kw, t) = go_ann(kw);
            (
                Expression::If(
                    kw,
                    e0.clone(),
                    then_.clone(),
                    e1.clone(),
                    else_.clone(),
                    e2.clone(),
                ),
                t,
            )
        }
        Expression::Abstraction(param, colon, body) => {
            let (param, t) = go_param(param);
            (
                Expression::Abstraction(param, colon.clone(), body.clone()),
                t,
            )
        }
        Expression::Application(g, a) => {
            let (g, t) = extract_first_comment_expr(g);
            (Expression::Application(Box::new(g), a.clone()), t)
        }
        Expression::Operation(l, op, r) => {
            let (l, t) = extract_first_comment_expr(l);
            (Expression::Operation(Box::new(l), op.clone(), r.clone()), t)
        }
        Expression::MemberCheck(e, dot, sels) => {
            let (e, t) = extract_first_comment_expr(e);
            (
                Expression::MemberCheck(Box::new(e), dot.clone(), sels.clone()),
                t,
            )
        }
        Expression::Negation(tok, e) => {
            let (tok, t) = go_ann(tok);
            (Expression::Negation(tok, e.clone()), t)
        }
        Expression::Inversion(tok, e) => {
            let (tok, t) = go_ann(tok);
            (Expression::Inversion(tok, e.clone()), t)
        }
    }
}

/// Walk the function-call chain. Mirrors Haskell `absorbApp` (Pretty.hs).
fn push_absorb_app(doc: &mut Doc, expr: &Expression, indent_function: bool, comment: &Trivia) {
    match expr {
        // Selections must not priority-expand: only the `.`-suffix would move,
        // which looks odd.
        Expression::Application(f, a)
            if matches!(**a, Expression::Term(Term::Selection(_, _, _))) =>
        {
            push_group_ann(doc, GroupAnn::Transparent, |g| {
                push_absorb_app(g, f, indent_function, comment);
            });
            doc.push(line());
            push_nested(doc, |n| {
                push_group_ann(n, GroupAnn::RegularG, |g| push_absorb_inner(g, a));
            });
        }
        // Two consecutive list arguments stay together: if one wraps, both wrap.
        Expression::Application(f, l2)
            if matches!(**l2, Expression::Term(Term::List(_, _, _)))
                && matches!(
                    **f,
                    Expression::Application(_, ref l1)
                        if matches!(**l1, Expression::Term(Term::List(_, _, _)))
                ) =>
        {
            let Expression::Application(f2, l1) = &**f else {
                unreachable!()
            };
            push_group_ann(doc, GroupAnn::Transparent, |outer| {
                push_group_ann(outer, GroupAnn::Transparent, |g| {
                    push_absorb_app(g, f2, indent_function, comment);
                });
                push_nested(outer, |n| {
                    push_group_ann(n, GroupAnn::RegularG, |g| {
                        g.push(line());
                        push_group(g, |inner| push_absorb_inner(inner, l1));
                        g.push(line());
                        push_group(g, |inner| push_absorb_inner(inner, l2));
                    });
                });
            });
        }
        Expression::Application(f, a) => {
            push_group_ann(doc, GroupAnn::Transparent, |g| {
                push_absorb_app(g, f, indent_function, comment);
            });
            doc.push(line());
            push_nested(doc, |n| {
                push_group_ann(n, GroupAnn::Priority, |g| push_absorb_inner(g, a));
            });
        }
        // Base case: the function expression itself.
        _ => {
            if indent_function && comment.0.is_empty() {
                push_nested(doc, |n| {
                    push_group_ann(n, GroupAnn::RegularG, |g| {
                        g.push(line_prime());
                        expr.pretty(g);
                    });
                });
            } else {
                expr.pretty(doc);
            }
        }
    }
}

/// Render the last argument of a function call. Mirrors Haskell `absorbLast`.
fn push_absorb_last(doc: &mut Doc, arg: &Expression) {
    match arg {
        Expression::Term(t) if is_absorbable_term(t) => {
            push_group_ann(doc, GroupAnn::Priority, |g| {
                push_nested(g, |n| t.pretty(n));
            });
        }
        // Parenthesised single-ID-parameter abstraction with absorbable body.
        Expression::Term(Term::Parenthesized(open, inner, close))
            if matches!(
                **inner,
                Expression::Abstraction(Parameter::ID(ref name), ref colon, ref body)
                    if matches!(**body, Expression::Term(ref t) if is_absorbable_term(t))
                        && !has_trivia(open) && !has_trivia(name) && !has_trivia(colon)
            ) =>
        {
            let Expression::Abstraction(Parameter::ID(name), colon, body) = &**inner else {
                unreachable!()
            };
            let Expression::Term(body_term) = &**body else {
                unreachable!()
            };
            push_group_ann(doc, GroupAnn::Priority, |g| {
                push_nested(g, |n| {
                    open.pretty(n);
                    name.pretty(n);
                    colon.pretty(n);
                    n.push(hardspace());
                    push_pretty_term_wide(n, body_term);
                    close.pretty(n);
                });
            });
        }
        // Parenthesised `ident { ... }` application with absorbable body.
        Expression::Term(Term::Parenthesized(open, inner, close))
            if matches!(
                **inner,
                Expression::Application(ref f, ref a)
                    if matches!(**f, Expression::Term(Term::Token(ref ident))
                            if matches!(ident.value, Token::Identifier(_))
                                && !has_trivia(open) && !has_trivia(ident) && !has_trivia(close))
                        && matches!(**a, Expression::Term(ref t) if is_absorbable_term(t))
            ) =>
        {
            let Expression::Application(f, a) = &**inner else {
                unreachable!()
            };
            let Expression::Term(Term::Token(ident)) = &**f else {
                unreachable!()
            };
            let Expression::Term(body_term) = &**a else {
                unreachable!()
            };
            push_group_ann(doc, GroupAnn::Priority, |g| {
                push_nested(g, |n| {
                    open.pretty(n);
                    ident.pretty(n);
                    n.push(hardspace());
                    push_pretty_term_wide(n, body_term);
                    close.pretty(n);
                });
            });
        }
        Expression::Term(Term::Parenthesized(open, expr, close)) => {
            push_absorb_paren(doc, open, expr, close);
        }
        _ => {
            push_group(doc, |g| {
                push_nested(g, |n| arg.pretty(n));
            });
        }
    }
}

/// Render function applications (Haskell `prettyApp indentFunction pre hasPost f a`).
pub(super) fn push_pretty_app(
    doc: &mut Doc,
    indent_function: bool,
    pre: &[DocE],
    has_post: bool,
    expr: &Expression,
) {
    let Expression::Application(f, a) = expr else {
        unreachable!("push_pretty_app requires an Application");
    };

    let (f_without_comment, comment) = extract_first_comment_expr(f);

    let post_hardline = |doc: &mut Doc| {
        if has_post && !comment.0.is_empty() {
            doc.push(hardline());
        }
    };

    comment.pretty(doc);

    // Two trailing list arguments are rendered as a pair of regular groups so
    // they wrap together; lists are never "simple", so renderSimple cannot apply.
    if let (Expression::Application(f2, l1), Expression::Term(Term::List(_, _, _))) =
        (&f_without_comment, &**a)
    {
        if matches!(**l1, Expression::Term(Term::List(_, _, _))) {
            push_group(doc, |g| {
                g.extend_from_slice(pre);
                push_group_ann(g, GroupAnn::Transparent, |inner| {
                    push_absorb_app(inner, f2, indent_function, &comment);
                });
                g.push(line());
                push_nested(g, |n| push_group(n, |gr| push_absorb_inner(gr, l1)));
                g.push(line());
                push_nested(g, |n| push_group(n, |gr| push_absorb_inner(gr, a)));
                if has_post {
                    g.push(line_prime());
                }
            });
            post_hardline(doc);
            return;
        }
    }

    let mut rendered_f: Doc = pre.to_vec();
    push_group_ann(&mut rendered_f, GroupAnn::Transparent, |g| {
        push_absorb_app(g, &f_without_comment, indent_function, &comment);
    });

    // renderSimple
    if is_simple_expression(expr) {
        if let Some(unexpanded) = unexpand_spacing_prime(None, &rendered_f) {
            push_group(doc, |g| {
                g.extend(unexpanded);
                g.push(hardspace());
                push_absorb_last(g, a);
            });
            post_hardline(doc);
            return;
        }
    }

    push_group(doc, |g| {
        g.extend(rendered_f);
        g.push(line());
        push_absorb_last(g, a);
        if has_post {
            g.push(line_prime());
        }
    });
    post_hardline(doc);
}
