use crate::predoc::{
    Doc, DocE, GroupAnn, Pretty, hardline, hardspace, line, line_prime, push_group, push_group_ann,
    push_nested, unexpand_spacing_prime,
};
use crate::types::{Expression, FirstToken, Item, Parameter, Term, Token, Trivia};

use super::absorb::{is_absorbable_term, push_absorb_paren};
use super::term::{push_pretty_term, push_pretty_term_wide, push_render_list};
use super::util::{has_trivia, is_simple_expression, is_simple_term};

/// `absorbInner` from Pretty.hs: short lists of simple terms get a soft `line`
/// separator so they may stay on one line; everything else falls back to `pretty`.
fn push_absorb_inner(doc: &mut Doc, arg: &Expression) {
    if let Expression::Term(Term::List(open, items, close)) = arg {
        let all_simple = items.0.iter().all(|item| match item {
            Item::Item(t) => is_simple_term(t),
            Item::Comments(_) => true,
        });
        if items.0.len() <= 6 && all_simple {
            push_render_list(doc, &line(), open, items, close);
            return;
        }
    }
    arg.pretty(doc);
}

/// Return the pre-trivia of the first token of `expr` after applying
/// `move_trailing_comment_up` to it, without cloning the expression.
/// This is the projection half of Haskell's
/// `mapFirstToken' ((\a -> (a{preTrivia=[]}, preTrivia)) . moveTrailingCommentUp)`.
fn first_token_comment(expr: &Expression) -> Trivia {
    let slot = expr.first_token();
    let mut t = slot.pre_trivia.clone();
    if let Some(tc) = slot.trail_comment {
        t.push(tc.into());
    }
    t
}

/// Rebuild `expr` with the first token's `pre_trivia` and `trail_comment`
/// cleared. Only invoked on the leftmost (non-`Application`) head of a call
/// chain, which is almost always a small `Term`, so the deep clone is cheap.
fn strip_first_comment(expr: &Expression) -> Expression {
    let mut e = expr.clone();
    let slot = e.first_token_mut();
    *slot.pre_trivia = Trivia::new();
    *slot.trail_comment = None;
    e
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
        // Base case: the function expression itself. The first token's
        // pre-trivia/trailing comment was already emitted by `push_pretty_app`,
        // so render the head with that trivia stripped.
        _ => {
            if comment.is_empty() {
                if indent_function {
                    push_nested(doc, |n| {
                        push_group_ann(n, GroupAnn::RegularG, |g| {
                            g.push(line_prime());
                            expr.pretty(g);
                        });
                    });
                } else {
                    expr.pretty(doc);
                }
            } else {
                strip_first_comment(expr).pretty(doc);
            }
        }
    }
}

/// `group' Priority $ nest …`
fn push_priority_nest(doc: &mut Doc, f: impl FnOnce(&mut Doc)) {
    push_group_ann(doc, GroupAnn::Priority, |g| push_nested(g, f));
}

/// Render the last argument of a function call. Mirrors Haskell `absorbLast`.
fn push_absorb_last(doc: &mut Doc, arg: &Expression) {
    match arg {
        Expression::Term(t) if is_absorbable_term(t) => {
            // Haskell: `group' Priority $ nest $ prettyTerm t`. `prettyTerm`
            // (unlike `instance Pretty Term`) does *not* wrap a `List` in an
            // extra group.
            push_priority_nest(doc, |n| push_pretty_term(n, t));
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
            push_priority_nest(doc, |n| {
                open.pretty(n);
                name.pretty(n);
                colon.pretty(n);
                n.push(hardspace());
                push_pretty_term_wide(n, body_term);
                close.pretty(n);
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
            push_priority_nest(doc, |n| {
                open.pretty(n);
                ident.pretty(n);
                n.push(hardspace());
                push_pretty_term_wide(n, body_term);
                close.pretty(n);
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

    let comment = first_token_comment(f);

    let post_hardline = |doc: &mut Doc| {
        if has_post && !comment.is_empty() {
            doc.push(hardline());
        }
    };

    comment.pretty(doc);

    // Two trailing list arguments are rendered as a pair of regular groups so
    // they wrap together; lists are never "simple", so renderSimple cannot apply.
    if let (Expression::Application(f2, l1), Expression::Term(Term::List(_, _, _))) = (&**f, &**a)
        && matches!(**l1, Expression::Term(Term::List(_, _, _)))
    {
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

    let mut rendered_f: Doc = pre.to_vec();
    push_group_ann(&mut rendered_f, GroupAnn::Transparent, |g| {
        push_absorb_app(g, f, indent_function, &comment);
    });

    // renderSimple
    if is_simple_expression(expr)
        && let Some(unexpanded) = unexpand_spacing_prime(None, &rendered_f)
    {
        push_group(doc, |g| {
            g.extend(unexpanded);
            g.push(hardspace());
            push_absorb_last(g, a);
        });
        post_hardline(doc);
        return;
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
