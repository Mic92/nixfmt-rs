use crate::ast::{Expression, FirstToken, Item, Parameter, Term, Token, Trivia};
use crate::doc::{Doc, Elem, Emit, GroupKind, line};

use super::absorb::absorb_paren;
use super::term::render_list;

/// `absorbInner` from Pretty.hs: short lists of simple terms get a soft `line`
/// separator so they may stay on one line; everything else falls back to `pretty`.
fn absorb_inner(doc: &mut Doc, arg: &Expression) {
    if let Expression::Term(Term::List { open, items, close }) = arg {
        let all_simple = items.0.iter().all(|item| match item {
            Item::Item(t) => t.is_simple(),
            Item::Comments(_) => true,
        });
        if items.0.len() <= 6 && all_simple {
            render_list(doc, &line(), open, items, close);
            return;
        }
    }
    arg.emit(doc);
}

/// Collect the leading trivia that would precede `expr`'s first token if its
/// trailing comment were hoisted into `pre_trivia`, without mutating `expr`.
fn first_token_comment(expr: &Expression) -> Trivia {
    let slot = expr.first_token();
    let mut t = slot.pre_trivia.clone();
    if let Some(tc) = slot.trail_comment {
        t.push(tc.into());
    }
    t
}

/// Rebuild `expr` with the first token's trivia cleared. Only invoked on the
/// leftmost head of a call chain (a small `Term`), so the deep clone is cheap.
fn strip_first_comment(expr: &Expression) -> Expression {
    let mut e = expr.clone();
    let slot = e.first_token_mut();
    *slot.pre_trivia = Trivia::new();
    *slot.trail_comment = None;
    e
}

/// Walk the function-call chain. Mirrors Haskell `absorbApp` (Pretty.hs).
fn absorb_app(doc: &mut Doc, expr: &Expression, indent_function: bool, comment: &Trivia) {
    let recurse_head = |doc: &mut Doc, head: &Expression| {
        doc.transparent_group(|g| {
            absorb_app(g, head, indent_function, comment);
        });
    };

    let Expression::Application { func: f, arg: a } = expr else {
        // Base case: the function expression itself. The first token's
        // pre-trivia/trailing comment was already emitted by `emit_app`,
        // so render the head with that trivia stripped.
        if !comment.is_empty() {
            strip_first_comment(expr).emit(doc);
        } else if indent_function {
            doc.nested(|n| {
                n.group(|g| {
                    g.linebreak();
                    expr.emit(g);
                });
            });
        } else {
            expr.emit(doc);
        }
        return;
    };

    // Two consecutive list arguments stay together: if one wraps, both wrap.
    if matches!(**a, Expression::Term(Term::List { .. }))
        && let Expression::Application { func: f2, arg: l1 } = &**f
        && matches!(**l1, Expression::Term(Term::List { .. }))
    {
        doc.transparent_group(|outer| {
            recurse_head(outer, f2);
            outer.nested(|n| {
                n.group(|g| {
                    g.line();
                    g.group(|inner| absorb_inner(inner, l1));
                    g.line();
                    g.group(|inner| absorb_inner(inner, a));
                });
            });
        });
        return;
    }

    recurse_head(doc, f);
    doc.line();
    // Selections must not priority-expand: only the `.`-suffix would move,
    // which looks odd.
    let arg_ann = if matches!(**a, Expression::Term(Term::Selection { .. })) {
        GroupKind::Regular
    } else {
        GroupKind::Priority
    };
    doc.nested(|n| {
        n.group_with(arg_ann, |g| absorb_inner(g, a));
    });
}

/// `group' Priority $ nest …`
fn priority_nest(doc: &mut Doc, f: impl FnOnce(&mut Doc)) {
    doc.priority_group(|g| {
        g.nested(f);
    });
}

/// Render the last argument of a function call. Mirrors Haskell `absorbLast`.
fn absorb_last(doc: &mut Doc, arg: &Expression) {
    if let Expression::Term(t) = arg
        && t.is_absorbable()
    {
        return priority_nest(doc, |n| t.emit_bare(n));
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
            && body_term.is_absorbable()
            && !open.has_trivia()
            && !name.has_trivia()
            && !colon.has_trivia()
        {
            return priority_nest(doc, |n| {
                open.emit(n);
                name.emit(n);
                colon.emit(n);
                n.hardspace();
                body_term.emit_wide(n);
                close.emit(n);
            });
        }
        // Parenthesised `ident { ... }` application with absorbable body.
        if let Expression::Application { func: f, arg: a } = &**inner
            && let Expression::Term(Term::Token(ident)) = &**f
            && matches!(ident.value, Token::Identifier(_))
            && let Expression::Term(body_term) = &**a
            && body_term.is_absorbable()
            && !open.has_trivia()
            && !ident.has_trivia()
            && !close.has_trivia()
        {
            return priority_nest(doc, |n| {
                open.emit(n);
                ident.emit(n);
                n.hardspace();
                body_term.emit_wide(n);
                close.emit(n);
            });
        }
        return absorb_paren(doc, open, inner, close);
    }

    doc.group(|g| {
        g.nested(|n| arg.emit(n));
    });
}

/// Render function applications (Haskell `prettyApp indentFunction pre hasPost f a`).
pub(super) fn emit_app(
    doc: &mut Doc,
    indent_function: bool,
    pre: &[Elem],
    has_post: bool,
    expr: &Expression,
) {
    let Expression::Application { func: f, arg: a } = expr else {
        unreachable!("emit_app requires an Application");
    };

    let comment = first_token_comment(f);

    let post_hardline = |doc: &mut Doc| {
        if has_post && !comment.is_empty() {
            doc.hardline();
        }
    };

    comment.emit(doc);

    // Two trailing list arguments are rendered as a pair of regular groups so
    // they wrap together; lists are never "simple", so renderSimple cannot apply.
    if matches!(**a, Expression::Term(Term::List { .. }))
        && let Expression::Application { func: f2, arg: l1 } = &**f
        && matches!(**l1, Expression::Term(Term::List { .. }))
    {
        doc.group(|g| {
            g.0.extend_from_slice(pre);
            g.transparent_group(|inner| {
                absorb_app(inner, f2, indent_function, &comment);
            });
            g.line();
            g.nested(|n| {
                n.group(|gr| absorb_inner(gr, l1));
            });
            g.line();
            g.nested(|n| {
                n.group(|gr| absorb_inner(gr, a));
            });
            if has_post {
                g.linebreak();
            }
        });
        post_hardline(doc);
        return;
    }

    let mut rendered_f = Doc::from(pre.to_vec());
    rendered_f.transparent_group(|g| {
        absorb_app(g, f, indent_function, &comment);
    });

    // renderSimple
    if expr.is_simple()
        && let Some(unexpanded) = rendered_f.try_compact(None)
    {
        doc.group(|g| {
            g.extend(unexpanded);
            g.hardspace();
            absorb_last(g, a);
        });
        post_hardline(doc);
        return;
    }

    doc.group(|g| {
        g.extend(rendered_f);
        g.line();
        absorb_last(g, a);
        if has_post {
            g.linebreak();
        }
    });
    post_hardline(doc);
}
