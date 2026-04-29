//! Pretty-printing for Nix AST
//!
//! Implements formatting rules from nixfmt's Pretty.hs

use crate::predoc::*;
use crate::types::*;

// Helper functions

fn has_trivia<T>(ann: &Ann<T>) -> bool {
    !ann.pre_trivia.0.is_empty() || ann.trail_comment.is_some()
}

/// Exact port of Haskell `isAbsorbable` (Pretty.hs).
fn is_absorbable_term(term: &Term) -> bool {
    match term {
        // Multi-line indented string
        Term::IndentedString(s) if s.value.len() >= 2 => true,
        Term::Path(_) => false,
        // Non-empty sets and lists
        Term::Set(_, _, items, _) if !items.0.is_empty() => true,
        Term::List(_, items, _) if !items.0.is_empty() => true,
        // Empty sets and lists if they have a line break
        // https://github.com/NixOS/nixfmt/issues/253
        Term::Set(_, open, items, close)
            if items.0.is_empty() && open.span.start_line != close.span.start_line =>
        {
            true
        }
        Term::List(open, items, close)
            if items.0.is_empty() && open.span.start_line != close.span.start_line =>
        {
            true
        }
        // Lists/sets with only comments are absorbable
        // https://github.com/NixOS/nixfmt/issues/362
        Term::List(open, items, _) if has_trivia(open) || items_has_only_comments(items) => true,
        Term::Set(_, open, items, _) if has_trivia(open) || items_has_only_comments(items) => true,
        // Parenthesized absorbable term, only when the open paren has no trivia
        Term::Parenthesized(open, expr, _) if !has_trivia(open) => {
            matches!(&**expr, Expression::Term(t) if is_absorbable_term(t))
        }
        _ => false,
    }
}

/// Exact port of Haskell `isAbsorbableExpr` (Pretty.hs).
fn is_absorbable_expr(expr: &Expression) -> bool {
    match expr {
        Expression::Term(t) => is_absorbable_term(t),
        Expression::With(_, _, _, body) => {
            matches!(&**body, Expression::Term(t) if is_absorbable_term(t))
        }
        // Absorb function declarations but only those with simple parameter(s)
        Expression::Abstraction(Parameter::ID(_), _, body) => match &**body {
            Expression::Term(t) => is_absorbable_term(t),
            Expression::Abstraction(_, _, _) => is_absorbable_expr(body),
            _ => false,
        },
        _ => false,
    }
}

/// Exact port of Haskell `prettyTermWide` (Pretty.hs).
fn push_pretty_term_wide(doc: &mut Doc, term: &Term) {
    if let Term::Set(krec, open, items, close) = term {
        push_pretty_set(doc, true, krec, open, items, close);
    } else {
        term.pretty(doc);
    }
}

/// Exact port of Haskell `absorbExpr` (Pretty.hs).
///
/// Unlike absorbable terms which can be force-absorbed, some expressions may
/// turn out not to be absorbable; in that case they fall through to `pretty`.
fn push_absorb_expr(doc: &mut Doc, force_wide: bool, expr: &Expression) {
    match expr {
        Expression::Term(t) if is_absorbable_term(t) => {
            if force_wide {
                push_pretty_term_wide(doc, t);
            } else {
                t.pretty(doc);
            }
        }
        // With expression with absorbable body: treat as absorbable term via
        // `prettyWith True`.
        Expression::With(with_kw, env, semicolon, body) if matches!(&**body, Expression::Term(t) if is_absorbable_term(t)) =>
        {
            let Expression::Term(t) = &**body else {
                unreachable!()
            };
            push_group_ann(doc, GroupAnn::RegularG, |g| {
                g.push(line_prime());
                with_kw.pretty(g);
                g.push(hardspace());
                push_nested(g, |n| push_group(n, |gg| env.pretty(gg)));
                semicolon.pretty(g);
                g.push(hardspace());
                push_group_ann(g, GroupAnn::Priority, |pg| push_pretty_term_wide(pg, t));
            });
        }
        _ => expr.pretty(doc),
    }
}

/// Format the right-hand side of an assignment or function-parameter default value.
///
/// This mirrors Haskell `absorbRHS` (Pretty.hs ~ line 657) one-to-one: each match
/// arm corresponds to exactly one Haskell `case` arm, in the same order, so that
/// behavioural differences against the reference implementation are easy to locate.
fn push_absorb_rhs(doc: &mut Doc, expr: &Expression) {
    match expr {
        // Exception to the absorbable-expr case below: do not force-expand attrsets
        // that only contain a single `inherit` statement.
        Expression::Term(Term::Set(_, _, binders, _))
            if matches!(
                binders.0.as_slice(),
                [Item::Item(Binder::Inherit(_, _, _, _))]
            ) =>
        {
            push_nested(doc, |d| {
                d.push(hardspace());
                push_group(d, |inner| push_absorb_expr(inner, false, expr));
            });
        }

        // Absorbable expression. Always start on the same line, force-expand attrsets.
        _ if is_absorbable_expr(expr) => {
            push_nested(doc, |d| {
                d.push(hardspace());
                push_group(d, |inner| push_absorb_expr(inner, true, expr));
            });
        }

        // Parenthesized expression: same special case as for the last argument of
        // a function call.
        Expression::Term(Term::Parenthesized(open, inner, close)) => {
            push_nested(doc, |d| {
                d.push(hardspace());
                push_absorb_paren(d, open, inner, close);
            });
        }

        // Not all strings are absorbable, but there is nothing to gain from
        // starting them on a new line; same for paths.
        Expression::Term(Term::SimpleString(_))
        | Expression::Term(Term::IndentedString(_))
        | Expression::Term(Term::Path(_)) => {
            push_nested(doc, |d| {
                d.push(hardspace());
                push_group(d, |inner| expr.pretty(inner));
            });
        }

        // Non-absorbable term: if multi-line, force it onto a new indented line.
        Expression::Term(_) => {
            push_nested(doc, |d| {
                push_group(d, |inner| {
                    inner.push(line());
                    expr.pretty(inner);
                });
            });
        }

        // Function call: absorb if all arguments except the last fit on the line,
        // start on a new line otherwise.
        Expression::Application(_, _) => {
            push_nested(doc, |d| push_pretty_app(d, false, &[line()], false, expr));
        }

        // `with ...;` keeps the leading `line` inside the group so it can collapse
        // together with the body.
        Expression::With(_, _, _, _) => {
            push_nested(doc, |d| {
                push_group(d, |inner| {
                    inner.push(line());
                    expr.pretty(inner);
                });
            });
        }

        // Special-case `//`, `++` and `+` to be more compact in some situations.
        // Case 1: LHS is an absorbable term → unindent the concatenation chain.
        Expression::Operation(left, op, _)
            if op.value.is_update_concat_plus()
                && matches!(&**left, Expression::Term(t) if is_absorbable_term(t)) =>
        {
            // hardspace <> prettyOp True expr op
            doc.push(hardspace());
            push_pretty_operation(doc, true, expr, op);
        }

        // Everything else:
        // - fits on one line → keep it there
        // - fits with a newline after `=` → do that
        // - otherwise start on a new line and expand fully
        _ => {
            push_nested(doc, |d| {
                d.push(line());
                push_group(d, |inner| expr.pretty(inner));
            });
        }
    }
}

/// Calculate the display width of a text string (simple character count for now)
fn text_width(s: &str) -> usize {
    s.chars().count()
}

/// Check if a string contains only whitespace
fn is_spaces(s: &str) -> bool {
    s.chars().all(|c| c.is_whitespace())
}

fn is_lone_ann<T>(ann: &Ann<T>) -> bool {
    ann.pre_trivia.0.is_empty() && ann.trail_comment.is_none()
}

fn is_simple_selector(selector: &Selector) -> bool {
    matches!(selector.selector, SimpleSelector::ID(_))
}

fn is_simple_term(term: &Term) -> bool {
    match term {
        Term::SimpleString(s) | Term::IndentedString(s) => is_lone_ann(s),
        Term::Path(p) => is_lone_ann(p),
        Term::Token(leaf)
            if is_lone_ann(leaf)
                && matches!(
                    leaf.value,
                    Token::Identifier(_) | Token::Integer(_) | Token::Float(_) | Token::EnvPath(_)
                ) =>
        {
            true
        }
        Term::Selection(term, selectors, def) => {
            is_simple_term(term) && selectors.iter().all(is_simple_selector) && def.is_none()
        }
        Term::Parenthesized(open, expr, close) => {
            is_lone_ann(open) && is_lone_ann(close) && is_simple_expression(expr)
        }
        _ => false,
    }
}

fn application_arity(expr: &Expression) -> usize {
    match expr {
        Expression::Application(f, _) => 1 + application_arity(f),
        _ => 0,
    }
}

/// Shared trivia juggling for parenthesized rendering: strips the opening
/// token's trailing comment (returned as `Trivia`) and the closing token's
/// leading trivia so callers can re-emit them inside the nested body.
fn split_paren_trivia(
    open: &Ann<Token>,
    close: &Ann<Token>,
) -> (Ann<Token>, Trivia, Trivia, Ann<Token>) {
    let mut open = open.clone();
    let trail: Trivia = open
        .trail_comment
        .take()
        .map(|tc| vec![Trivium::LineComment(format!(" {}", tc.0))])
        .unwrap_or_default()
        .into();
    let mut close = close.clone();
    let close_pre = std::mem::replace(&mut close.pre_trivia, Trivia::new());
    (open, trail, close_pre, close)
}

/// Render parenthesized expression in a Priority group (Haskell `absorbParen`).
fn push_absorb_paren(doc: &mut Doc, open: &Ann<Token>, expr: &Expression, close: &Ann<Token>) {
    let (open, trail, close_pre, close) = split_paren_trivia(open, close);
    push_group_ann(doc, GroupAnn::Priority, |g| {
        push_nested(g, |outer| {
            open.pretty(outer);
            outer.push(line_prime());
            push_group(outer, |inner| {
                push_nested(inner, |body| {
                    // Any trailing comment on `(` is moved down into the body,
                    // mirroring `mapFirstToken (\a -> a{preTrivia = post' <> preTrivia})`.
                    trail.pretty(body);
                    expr.pretty(body);
                    close_pre.pretty(body);
                });
            });
            outer.push(line_prime());
            close.pretty(outer);
        });
    });
}

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

/// `renderList` from Pretty.hs.
fn push_render_list(
    doc: &mut Doc,
    item_sep: DocE,
    open: &Ann<Token>,
    items: &Items<Term>,
    close: &Ann<Token>,
) {
    let open_clean = Ann {
        trail_comment: None,
        ..open.clone()
    };
    open_clean.pretty(doc);

    let sur = if open.span.start_line != close.span.start_line
        || items_has_only_comments(items)
        || (!is_lone_ann(open) && items.0.is_empty())
    {
        hardline()
    } else if items.0.is_empty() {
        hardspace()
    } else {
        line()
    };

    push_surrounded(doc, &vec![sur], |d| {
        push_nested(d, |inner| {
            open.trail_comment.pretty(inner);
            push_pretty_items_sep(inner, items, &item_sep);
        });
    });
    close.pretty(doc);
}

fn items_has_only_comments<T>(items: &Items<T>) -> bool {
    !items.0.is_empty() && items.0.iter().all(|i| matches!(i, Item::Comments(_)))
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
fn push_pretty_app(
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

fn flatten_operation_chain<'a>(
    target: &'a Leaf,
    expr: &'a Expression,
    current_op: Option<&'a Leaf>,
    out: &mut Vec<(Option<&'a Leaf>, &'a Expression)>,
) {
    match expr {
        Expression::Operation(left, op_leaf, right) if op_leaf.value == target.value => {
            flatten_operation_chain(target, left, current_op, out);
            flatten_operation_chain(target, right, Some(op_leaf), out);
        }
        _ => out.push((current_op, expr)),
    }
}

fn push_absorb_operation(doc: &mut Doc, expr: &Expression) {
    match expr {
        Expression::Term(term) if is_absorbable_term(term) => {
            doc.push(hardspace());
            term.pretty(doc);
        }
        Expression::Operation(_, _, _) => {
            push_group(doc, |group_doc| {
                group_doc.push(line());
                expr.pretty(group_doc);
            });
        }
        Expression::Application(_, _) => {
            push_group(doc, |g| push_pretty_app(g, false, &[line()], false, expr));
        }
        _ => {
            doc.push(hardspace());
            expr.pretty(doc);
        }
    }
}

fn push_pretty_operation(
    doc: &mut Doc,
    force_first_term_wide: bool,
    operation: &Expression,
    op: &Leaf,
) {
    let mut parts: Vec<(Option<&Leaf>, &Expression)> = Vec::new();
    flatten_operation_chain(op, operation, None, &mut parts);

    push_group(doc, |group_doc| {
        for (maybe_op, expr) in parts.iter() {
            match maybe_op {
                None => match expr {
                    Expression::Term(term) if force_first_term_wide && is_absorbable_term(term) => {
                        push_pretty_term_wide(group_doc, term);
                    }
                    _ => expr.pretty(group_doc),
                },
                Some(op_leaf) => {
                    group_doc.push(line());
                    op_leaf.pretty(group_doc);
                    push_nested(group_doc, |nested| {
                        push_absorb_operation(nested, expr);
                    });
                }
            }
        }
    });
}

fn push_absorb_abs(doc: &mut Doc, depth: usize, expr: &Expression) {
    match expr {
        Expression::Abstraction(Parameter::ID(param), colon, body) => {
            doc.push(hardspace());
            param.pretty(doc);
            colon.pretty(doc);
            push_absorb_abs(doc, depth + 1, body);
        }
        _ if is_absorbable_expr(expr) => {
            doc.push(hardspace());
            push_group_ann(doc, GroupAnn::Priority, |priority_group| {
                push_absorb_expr(priority_group, false, expr);
            });
        }
        _ => {
            let separator = if depth <= 2 { line() } else { hardline() };
            doc.push(separator);
            expr.pretty(doc);
        }
    }
}

/// Move a trailing comment on a token into its leading trivia.
/// Mirrors Haskell `moveTrailingCommentUp` (Pretty.hs).
fn move_trailing_comment_up<T: Clone>(ann: &Ann<T>) -> Ann<T> {
    let mut out = ann.clone();
    if let Some(tc) = out.trail_comment.take() {
        out.pre_trivia
            .push(Trivium::LineComment(format!(" {}", tc.0)));
    }
    out
}

/// Prepend an expression as the function head of a (possibly nested) application.
/// Mirrors Haskell `insertIntoApp` used by the `Assert` pretty instance.
fn insert_into_app(insert: Expression, expr: Expression) -> (Expression, Expression) {
    match expr {
        Expression::Application(f, a) => {
            let (f2, a2) = insert_into_app(insert, *f);
            (Expression::Application(Box::new(f2), Box::new(a2)), *a)
        }
        other => (insert, other),
    }
}

/// Render a `with` expression.
/// Mirrors Haskell `prettyWith` (Pretty.hs).
fn pretty_with(
    doc: &mut Doc,
    absorb: bool,
    with: &Leaf,
    expr0: &Expression,
    semicolon: &Leaf,
    expr1: &Expression,
) {
    if absorb {
        if let Expression::Term(t) = expr1 {
            // group' RegularG $ line' <> with <> hardspace <> nest (group expr0) <> ";"
            //   <> hardspace <> group' Priority (prettyTermWide expr1)
            push_group_ann(doc, GroupAnn::RegularG, |g| {
                g.push(line_prime());
                with.pretty(g);
                g.push(hardspace());
                push_nested(g, |n| {
                    push_group(n, |inner| expr0.pretty(inner));
                });
                semicolon.pretty(g);
                g.push(hardspace());
                push_group_ann(g, GroupAnn::Priority, |p| match t {
                    Term::Set(krec, open, items, close) => {
                        push_pretty_set(p, true, krec, open, items, close);
                    }
                    _ => t.pretty(p),
                });
            });
            return;
        }
    }
    // group (with <> hardspace <> nest (group expr0) <> ";") <> line <> expr1
    push_group(doc, |g| {
        with.pretty(g);
        g.push(hardspace());
        push_nested(g, |n| {
            push_group(n, |inner| expr0.pretty(inner));
        });
        semicolon.pretty(g);
    });
    doc.push(line());
    expr1.pretty(doc);
}

/// Recursive renderer for `if`/`else if` chains.
/// Mirrors Haskell `prettyIf` (Pretty.hs, inside the `If` clause).
fn pretty_if(doc: &mut Doc, sep: DocE, expr: &Expression) {
    match expr {
        Expression::If(if_kw, cond, then_kw, expr0, else_kw, expr1) => {
            // group (if <> line <> nest cond <> line <> then)
            push_group(doc, |g| {
                if_kw.pretty(g);
                g.push(line());
                push_nested(g, |n| cond.pretty(n));
                g.push(line());
                then_kw.pretty(g);
            });
            // surroundWith sep (nest $ group expr0)
            push_surrounded(doc, &vec![sep], |d| {
                push_nested(d, |n| {
                    push_group(n, |g| expr0.pretty(g));
                });
            });
            // else (with trailing comment moved up) <> hardspace <> recurse with hardline
            move_trailing_comment_up(else_kw).pretty(doc);
            doc.push(hardspace());
            pretty_if(doc, hardline(), expr1);
        }
        x => {
            // line <> nest (group x)
            doc.push(line());
            push_nested(doc, |n| {
                push_group(n, |g| x.pretty(g));
            });
        }
    }
}

fn is_simple_expression(expr: &Expression) -> bool {
    match expr {
        Expression::Term(term) => is_simple_term(term),
        Expression::Application(f, a) => {
            if application_arity(expr) >= 3 {
                return false;
            }
            is_simple_expression(f) && is_simple_expression(a)
        }
        _ => false,
    }
}

/// Render the nested document that appears between parentheses.
/// Mirrors `inner` in Haskell `prettyTerm (Parenthesized ...)`.
fn push_parenthesized_inner(doc: &mut Doc, expr: &Expression) {
    match expr {
        _ if is_absorbable_expr(expr) => {
            push_group(doc, |inner| {
                push_absorb_expr(inner, false, expr);
            });
        }
        Expression::Application(_, _) => {
            push_pretty_app(doc, true, &[], true, expr);
        }
        Expression::Term(Term::Selection(term, _, _)) if is_absorbable_term(term) => {
            doc.push(line_prime());
            push_group(doc, |inner| {
                expr.pretty(inner);
            });
            doc.push(line_prime());
        }
        Expression::Term(Term::Selection(_, _, _)) => {
            push_group(doc, |inner| {
                expr.pretty(inner);
            });
            doc.push(line_prime());
        }
        _ => {
            doc.push(line_prime());
            push_group(doc, |inner| {
                expr.pretty(inner);
            });
            doc.push(line_prime());
        }
    }
}

/// Pretty print a parenthesized expression (Haskell `prettyTerm (Parenthesized ...)`).
fn push_pretty_parenthesized(
    doc: &mut Doc,
    open: &Ann<Token>,
    expr: &Expression,
    close: &Ann<Token>,
) {
    let (mut open, trail, close_pre, close) = split_paren_trivia(open, close);
    // moveTrailingCommentUp: a trailing comment on `(` becomes its own pre-trivia.
    open.pre_trivia.extend(trail);

    push_group(doc, |g| {
        open.pretty(g);
        push_nested(g, |nested| {
            push_parenthesized_inner(nested, expr);
            close_pre.pretty(nested);
        });
        close.pretty(g);
    });
}

// Pretty instances

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

// Pretty for Item - wraps items in groups, passes through comments
impl<T: Pretty> Pretty for Item<T> {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Item::Comments(trivia) => trivia.pretty(doc),
            Item::Item(x) => push_group(doc, |d| x.pretty(d)),
        }
    }
}

/// Format an attribute set with optional rec keyword
/// Based on Haskell prettySet (Pretty.hs:185-205)
fn push_pretty_set(
    doc: &mut Doc,
    wide: bool,
    krec: &Option<Ann<Token>>,
    open: &Ann<Token>,
    items: &Items<Binder>,
    close: &Ann<Token>,
) {
    // Empty attribute set
    if items.0.is_empty() && !has_trivia(open) && close.pre_trivia.0.is_empty() {
        // Pretty print optional `rec` keyword with hardspace
        if let Some(rec) = krec {
            rec.pretty(doc);
            doc.push(hardspace());
        }
        open.pretty(doc);
        // If the braces are on different lines, keep them like that
        doc.push(if open.span.start_line != close.span.start_line {
            hardline()
        } else {
            hardspace()
        });
        close.pretty(doc);
        return;
    }

    // General set with items
    // Pretty print optional `rec` keyword with hardspace
    if let Some(rec) = krec {
        rec.pretty(doc);
        doc.push(hardspace());
    }

    // Open brace without trailing comment
    let open_without_trail = Ann {
        pre_trivia: open.pre_trivia.clone(),
        span: open.span,
        trail_comment: None,
        value: open.value.clone(),
    };
    open_without_trail.pretty(doc);

    // Separator: prefer hardline before close when items start with an empty line
    let starts_with_emptyline = match items.0.first() {
        Some(Item::Comments(trivia)) => trivia.0.iter().any(|t| matches!(t, Trivium::EmptyLine())),
        _ => false,
    };

    // Check if braces are on different lines (matches Pretty.hs:203)
    let braces_on_different_lines = open.span.start_line != close.span.start_line;

    // Separator: use hardline if wide, or when starting with an empty line,
    // or when braces are on different lines; else use line
    // This matches Pretty.hs:200-205
    let sep = if !items.0.is_empty() && (wide || starts_with_emptyline || braces_on_different_lines)
    {
        vec![hardline()]
    } else {
        vec![line()]
    };

    push_surrounded(doc, &sep, |d| {
        push_nested(d, |inner| {
            open.trail_comment.pretty(inner);
            push_pretty_items(inner, items);
        });
    });
    close.pretty(doc);
}

/// Format a list of items with interleaved comments
/// Based on Haskell prettyItems (Pretty.hs:108-120)
fn push_pretty_items<T: Pretty>(doc: &mut Doc, items: &Items<T>) {
    push_pretty_items_sep(doc, items, &hardline());
}

fn push_pretty_items_sep<T: Pretty>(doc: &mut Doc, items: &Items<T>, sep: &DocE) {
    let items = &items.0;
    match items.as_slice() {
        [] => {}
        [item] => item.pretty(doc),
        items => {
            let mut i = 0;
            while i < items.len() {
                if i > 0 {
                    doc.push(sep.clone());
                }

                // Special case: language annotation comment followed by string item
                if i + 1 < items.len() {
                    if let Item::Comments(trivia) = &items[i] {
                        if trivia.0.len() == 1 {
                            if let Trivium::LanguageAnnotation(lang) = &trivia.0[0] {
                                if let Item::Item(string_item) = &items[i + 1] {
                                    // Language annotation + string on same line
                                    Trivium::LanguageAnnotation(lang.clone()).pretty(doc);
                                    doc.push(hardspace());
                                    push_group(doc, |d| string_item.pretty(d));
                                    i += 2;
                                    continue;
                                }
                            }
                        }
                    }
                }

                items[i].pretty(doc);
                i += 1;
            }
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
        // Handle EnvPath separately since it needs formatting
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
                // TODO: implement prettySimpleString
                push_text(doc, "\"...\"");
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

impl Pretty for StringPart {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            StringPart::TextPart(s) => push_text(doc, s),
            StringPart::Interpolation(whole) => {
                let trailing_empty = whole.trailing_trivia.0.is_empty();
                let value = &whole.value;

                // Check for absorbable term (e.g., sets, lists)
                let absorbable_term = if trailing_empty {
                    match value {
                        Expression::Term(term) if is_absorbable_term(term) => Some(term),
                        _ => None,
                    }
                } else {
                    None
                };

                // Handle absorbable term: ${ { ... } } or ${ [ ... ] }
                if let Some(term) = absorbable_term {
                    push_group(doc, |group_doc| {
                        push_text(group_doc, "${");
                        term.pretty(group_doc);
                        push_text(group_doc, "}");
                    });
                    return;
                }

                // Handle simple value: ${ x } without trailing trivia
                if trailing_empty && is_simple_expression(value) {
                    push_text(doc, "${");
                    value.pretty(doc);
                    push_text(doc, "}");
                    return;
                }

                // Handle complex interpolation with line breaks
                push_group(doc, |group_doc| {
                    push_text(group_doc, "${");
                    push_nested(group_doc, |nested| {
                        nested.push(line_prime());
                        whole.pretty(nested);
                        nested.push(line_prime());
                    });
                    push_text(group_doc, "}");
                });
            }
        }
    }
}

impl Pretty for Vec<StringPart> {
    fn pretty(&self, doc: &mut Doc) {
        match self.as_slice() {
            // Handle special case: single interpolation with leading whitespace
            [StringPart::TextPart(pre), StringPart::Interpolation(whole)]
                if is_spaces(pre) && whole.trailing_trivia.0.is_empty() =>
            {
                let indentation = text_width(pre);
                push_text(doc, pre);
                push_offset(doc, indentation, |d| {
                    push_group_ann(d, GroupAnn::RegularG, |g| {
                        push_text(g, "${");
                        push_nested(g, |inner| {
                            inner.push(line_prime());
                            push_group(inner, |ig| whole.value.pretty(ig));
                            inner.push(line_prime());
                        });
                        push_text(g, "}");
                    });
                });
            }
            // Handle leading TextPart with offset
            [StringPart::TextPart(t), rest @ ..] => {
                let indentation = text_width(
                    &t.chars()
                        .take_while(|c| c.is_whitespace())
                        .collect::<String>(),
                );
                push_text(doc, t);
                push_offset(doc, indentation, |d| {
                    for part in rest {
                        part.pretty(d);
                    }
                });
            }
            // Default: just concatenate
            _ => {
                push_hcat(doc, self.clone());
            }
        }
    }
}

/// Format a simple string (with double quotes)
fn push_pretty_simple_string(doc: &mut Doc, parts: &[Vec<StringPart>]) {
    push_group(doc, |d| {
        push_text(d, "\"");
        // Use literal \n instead of newline() to avoid indentation
        let newline_doc = vec![DocE::Text(0, 0, TextAnn::RegularT, "\n".to_string())];
        push_sep_by(d, &newline_doc, parts.to_vec());
        push_text(d, "\"");
    });
}

/// Format an indented string (with '')
fn push_pretty_indented_string(doc: &mut Doc, parts: &[Vec<StringPart>]) {
    push_group(doc, |d| {
        push_text(d, "''");
        // For multi-line strings, add a potential line break after opening ''
        if parts.len() > 1 {
            d.push(line_prime());
        }
        push_nested(d, |inner| {
            let newline_doc = vec![newline()];
            push_sep_by(inner, &newline_doc, parts.to_vec());
        });
        push_text(d, "''");
    });
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
                // Path is Ann<Vec<StringPart>>
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
                // Lists are always wrapped in a group (matches Haskell: pretty l@List{} = group $ prettyTerm l)
                push_group(doc, |g| {
                    // Empty list fast path (Haskell: prettyTerm (List paropen@Ann{trailComment = Nothing} (Items []) parclose@Ann{preTrivia = []}))
                    if items.0.is_empty()
                        && open.trail_comment.is_none()
                        && close.pre_trivia.0.is_empty()
                    {
                        open.pretty(g);
                        // If the brackets are on different lines, keep them like that
                        if open.span.start_line != close.span.start_line {
                            g.push(hardline());
                        } else {
                            g.push(hardspace());
                        }
                        close.pretty(g);
                        return;
                    }

                    // General list (Haskell: prettyTerm (List ..) = renderList hardline ..)
                    push_render_list(g, hardline(), open, items, close);
                });
            }
            Term::Set(krec, open, binders, close) => {
                push_pretty_set(doc, false, krec, open, binders, close);
            }
            Term::Selection(term, selectors, default) => {
                term.pretty(doc);

                // Add separator based on term type
                match &**term {
                    // If it is an ident, keep it all together
                    Term::Token(_) => {}
                    // If it is a parenthesized expression, maybe add a line break
                    Term::Parenthesized(_, _, _) => doc.push(softline_prime()),
                    // Otherwise, very likely add a line break
                    _ => doc.push(line_prime()),
                };

                // Add selectors
                push_hcat(doc, selectors.clone());

                // Add optional "or default" clause
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
                // Special case: absorbable RHS with update/concat/plus operator
                // Based on Haskell Pretty.hs:665-667:
                // nest $ group' RegularG $ line <> pretty l <> line <>
                //   group' Transparent (pretty op <> hardspace <> group' Priority (prettyTermWide t))
                if let Expression::Term(t) = &**right {
                    if is_absorbable_term(t) && op.value.is_update_concat_plus() {
                        // Based on Haskell Pretty.hs:665-667:
                        // nest $ group' RegularG $ line <> pretty l <> line <>
                        //   group' Transparent (pretty op <> hardspace <> group' Priority (prettyTermWide t))
                        // Note: The IR output shows all groups as RegularG, so we use push_group throughout
                        push_group(doc, |inner| {
                            // Left operand - Lists now wrap themselves in groups automatically
                            left.pretty(inner);
                            inner.push(line());
                            // Operator and RHS
                            op.pretty(inner);
                            inner.push(hardspace());
                            // RHS term with nesting - Lists wrap themselves, Sets don't
                            push_nested(inner, |rhs_nested| {
                                t.pretty(rhs_nested);
                            });
                        });
                        return;
                    }
                }

                // Default case
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
                pretty_with(doc, false, with_kw, env, semicolon, expr);
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

impl Pretty for ParamAttr {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            ParamAttr::ParamAttr(name, default, maybe_comma) => {
                let has_default = default.is_some();
                let make_pretty = |d: &mut Doc| {
                    name.pretty(d);

                    // If there's a default value (? expr)
                    if let Some((qmark, def)) = default.as_ref() {
                        d.push(hardspace());
                        push_nested(d, |inner| {
                            qmark.pretty(inner);
                            push_absorb_rhs(inner, def);
                        });
                    }

                    // Add optional comma
                    if let Some(comma) = maybe_comma {
                        comma.pretty(d);
                    }
                };

                if has_default {
                    push_group(doc, make_pretty);
                } else {
                    make_pretty(doc);
                }
            }
            ParamAttr::ParamEllipsis(ellipsis) => ellipsis.pretty(doc),
        }
    }
}

fn param_attr_without_default(attr: &ParamAttr) -> bool {
    matches!(attr, ParamAttr::ParamAttr(_, default, _) if default.is_none())
}

fn param_attr_is_ellipsis(attr: &ParamAttr) -> bool {
    matches!(attr, ParamAttr::ParamEllipsis(_))
}

fn parameter_separator(open: &Leaf, attrs: &[ParamAttr], close: &Leaf) -> DocE {
    if open.span.start_line != close.span.start_line {
        return hardline();
    }

    match attrs {
        [attr] if param_attr_is_ellipsis(attr) => line(),
        [attr] if param_attr_without_default(attr) => line(),
        [a, b] if param_attr_without_default(a) && param_attr_is_ellipsis(b) => line(),
        [a, b] if param_attr_without_default(a) && param_attr_without_default(b) => line(),
        [a, b, c]
            if param_attr_without_default(a)
                && param_attr_without_default(b)
                && param_attr_is_ellipsis(c) =>
        {
            line()
        }
        _ => hardline(),
    }
}

fn render_param_attrs(attrs: &[ParamAttr]) -> Vec<Doc> {
    attrs
        .iter()
        .enumerate()
        .map(|(idx, attr)| {
            let mut rendered = Vec::new();
            let is_last = idx + 1 == attrs.len();

            if is_last {
                if let ParamAttr::ParamAttr(name, default, _) = attr {
                    ParamAttr::ParamAttr(name.clone(), default.clone(), None).pretty(&mut rendered);
                    push_trailing(&mut rendered, ",");
                    return rendered;
                }
            }

            attr.pretty(&mut rendered);
            rendered
        })
        .collect()
}

impl Pretty for Parameter {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Parameter::ID(id) => id.pretty(doc),
            Parameter::Set(open, attrs, close) => {
                if attrs.is_empty() {
                    let sep = if open.span.start_line != close.span.start_line {
                        hardline()
                    } else {
                        hardspace()
                    };

                    push_group(doc, |doc| {
                        open.pretty(doc);
                        doc.push(sep);
                        close.pretty(doc);
                    });
                    return;
                }

                let sep = parameter_separator(open, attrs, close);
                let sep_doc = vec![sep.clone()];

                push_group(doc, |doc| {
                    open.pretty(doc);
                    doc.push(sep.clone());
                    let sep_after = sep.clone();
                    push_nested(doc, |inner| {
                        let attr_docs = render_param_attrs(attrs);
                        push_sep_by(inner, &sep_doc, attr_docs);
                    });
                    doc.push(sep_after);
                    push_nested(doc, |inner| close.pre_trivia.pretty(inner));
                    Ann {
                        pre_trivia: Trivia(vec![]),
                        ..close.clone()
                    }
                    .pretty(doc);
                });
            }
            Parameter::Context(left, at, right) => {
                // Render without spacing - fixup will merge adjacent text elements
                // This matches nixfmt reference: pretty param1 <> pretty at <> pretty param2
                left.pretty(doc);
                at.pretty(doc);
                right.pretty(doc);
            }
        }
    }
}

impl<T: Pretty> Pretty for Whole<T> {
    fn pretty(&self, doc: &mut Doc) {
        // Wrap the entire content in a group
        // This matches nixfmt's: pretty (Whole x finalTrivia) = group $ pretty x <> pretty finalTrivia
        push_group(doc, |doc| {
            self.value.pretty(doc);
            self.trailing_trivia.pretty(doc);
        });
        // Do not force a final Hardline; reference nixfmt IR does not
        // add a trailing newline at the top level in --ir output.
        // Keeping parity avoids extra Spacing Hardline in diffs.
    }
}
