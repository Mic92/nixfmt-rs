use crate::predoc::{Doc, DocE, GroupAnn, Pretty, line, unexpand_spacing_prime};
use crate::types::{Expression, FirstToken, Item, Parameter, Term, Token, Trivia};

use super::absorb::{is_absorbable_term, push_absorb_paren};
use super::term::{push_pretty_term, push_pretty_term_wide, push_render_list};
use super::util::{has_trivia, is_simple_expression, is_simple_term};

const fn is_list_arg(e: &Expression) -> bool {
    matches!(e, Expression::Term(Term::List { .. }))
}

const fn is_selection_arg(e: &Expression) -> bool {
    matches!(e, Expression::Term(Term::Selection { .. }))
}

/// `absorbInner` from Pretty.hs: short lists of simple terms get a soft `line`
/// separator so they may stay on one line; everything else falls back to `pretty`.
fn push_absorb_inner(doc: &mut Doc, arg: &Expression) {
    if let Expression::Term(Term::List { open, items, close }) = arg {
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
    let recurse_head = |doc: &mut Doc, head: &Expression| {
        doc.group_ann(GroupAnn::Transparent, |g| {
            push_absorb_app(g, head, indent_function, comment);
        });
    };

    let Expression::Application { func: f, arg: a } = expr else {
        // Base case: the function expression itself. The first token's
        // pre-trivia/trailing comment was already emitted by `push_pretty_app`,
        // so render the head with that trivia stripped.
        if !comment.is_empty() {
            strip_first_comment(expr).pretty(doc);
        } else if indent_function {
            doc.nested(|n| {
                n.group(|g| {
                    g.line_prime();
                    expr.pretty(g);
                });
            });
        } else {
            expr.pretty(doc);
        }
        return;
    };

    // Two consecutive list arguments stay together: if one wraps, both wrap.
    if is_list_arg(a)
        && let Expression::Application { func: f2, arg: l1 } = &**f
        && is_list_arg(l1)
    {
        doc.group_ann(GroupAnn::Transparent, |outer| {
            recurse_head(outer, f2);
            outer.nested(|n| {
                n.group(|g| {
                    g.line();
                    g.group(|inner| push_absorb_inner(inner, l1));
                    g.line();
                    g.group(|inner| push_absorb_inner(inner, a));
                });
            });
        });
        return;
    }

    recurse_head(doc, f);
    doc.line();
    // Selections must not priority-expand: only the `.`-suffix would move,
    // which looks odd.
    let arg_ann = if is_selection_arg(a) {
        GroupAnn::RegularG
    } else {
        GroupAnn::Priority
    };
    doc.nested(|n| {
        n.group_ann(arg_ann, |g| push_absorb_inner(g, a));
    });
}

/// `group' Priority $ nest …`
fn push_priority_nest(doc: &mut Doc, f: impl FnOnce(&mut Doc)) {
    doc.group_ann(GroupAnn::Priority, |g| {
        g.nested(f);
    });
}

/// Render the last argument of a function call. Mirrors Haskell `absorbLast`.
fn push_absorb_last(doc: &mut Doc, arg: &Expression) {
    if let Expression::Term(t) = arg
        && is_absorbable_term(t)
    {
        // Haskell: `group' Priority $ nest $ prettyTerm t`. `prettyTerm`
        // (unlike `instance Pretty Term`) does *not* wrap a `List` in an
        // extra group.
        return push_priority_nest(doc, |n| push_pretty_term(n, t));
    }

    if let Expression::Term(Term::Parenthesized {
        open,
        expr: inner,
        close,
    }) = arg
    {
        // Parenthesised single-ID-parameter abstraction with absorbable body.
        if let Expression::Abstraction {
            param: Parameter::Id(name),
            colon,
            body,
        } = &**inner
            && let Expression::Term(body_term) = &**body
            && is_absorbable_term(body_term)
            && !has_trivia(open)
            && !has_trivia(name)
            && !has_trivia(colon)
        {
            return push_priority_nest(doc, |n| {
                open.pretty(n);
                name.pretty(n);
                colon.pretty(n);
                n.hardspace();
                push_pretty_term_wide(n, body_term);
                close.pretty(n);
            });
        }
        // Parenthesised `ident { ... }` application with absorbable body.
        if let Expression::Application { func: f, arg: a } = &**inner
            && let Expression::Term(Term::Token(ident)) = &**f
            && matches!(ident.value, Token::Identifier(_))
            && let Expression::Term(body_term) = &**a
            && is_absorbable_term(body_term)
            && !has_trivia(open)
            && !has_trivia(ident)
            && !has_trivia(close)
        {
            return push_priority_nest(doc, |n| {
                open.pretty(n);
                ident.pretty(n);
                n.hardspace();
                push_pretty_term_wide(n, body_term);
                close.pretty(n);
            });
        }
        return push_absorb_paren(doc, open, inner, close);
    }

    doc.group(|g| {
        g.nested(|n| arg.pretty(n));
    });
}

/// Render function applications (Haskell `prettyApp indentFunction pre hasPost f a`).
pub(super) fn push_pretty_app(
    doc: &mut Doc,
    indent_function: bool,
    pre: &[DocE],
    has_post: bool,
    expr: &Expression,
) {
    let Expression::Application { func: f, arg: a } = expr else {
        unreachable!("push_pretty_app requires an Application");
    };

    let comment = first_token_comment(f);

    let post_hardline = |doc: &mut Doc| {
        if has_post && !comment.is_empty() {
            doc.hardline();
        }
    };

    comment.pretty(doc);

    // Two trailing list arguments are rendered as a pair of regular groups so
    // they wrap together; lists are never "simple", so renderSimple cannot apply.
    if is_list_arg(a)
        && let Expression::Application { func: f2, arg: l1 } = &**f
        && is_list_arg(l1)
    {
        doc.group(|g| {
            g.0.extend_from_slice(pre);
            g.group_ann(GroupAnn::Transparent, |inner| {
                push_absorb_app(inner, f2, indent_function, &comment);
            });
            g.line();
            g.nested(|n| {
                n.group(|gr| push_absorb_inner(gr, l1));
            });
            g.line();
            g.nested(|n| {
                n.group(|gr| push_absorb_inner(gr, a));
            });
            if has_post {
                g.line_prime();
            }
        });
        post_hardline(doc);
        return;
    }

    let mut rendered_f = Doc::from(pre.to_vec());
    rendered_f.group_ann(GroupAnn::Transparent, |g| {
        push_absorb_app(g, f, indent_function, &comment);
    });

    // renderSimple
    if is_simple_expression(expr)
        && let Some(unexpanded) = unexpand_spacing_prime(None, &rendered_f)
    {
        doc.group(|g| {
            g.extend(unexpanded);
            g.hardspace();
            push_absorb_last(g, a);
        });
        post_hardline(doc);
        return;
    }

    doc.group(|g| {
        g.extend(rendered_f);
        g.line();
        push_absorb_last(g, a);
        if has_post {
            g.line_prime();
        }
    });
    post_hardline(doc);
}
