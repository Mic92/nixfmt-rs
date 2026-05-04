//! AST normalisation for structural comparison.
//!
//! Strips all source-location and trivia information (spans, leading
//! trivia, trailing comments, interleaved comment items) so two ASTs
//! produced from differently-formatted but semantically identical
//! inputs compare equal with `==`.
//!
//! Used by the fuzzing harness to check that `parse → format → parse`
//! is a semantic round-trip.

use crate::ast::{
    Annotated, Binder, Expression, File, Item, Items, Leaf, NixString, ParamAttr, Parameter,
    Selector, SimpleSelector, Span, StringPart, Term, Trailed, Trivia,
};

const ZERO_SPAN: Span = Span::with_lines(0, 0, 1, 1);

pub fn normalize_file(file: &mut File) {
    normalize_whole_expr(file);
}

fn normalize_whole_expr(w: &mut Trailed<Expression>) {
    w.trailing_trivia = Trivia::new();
    normalize_expr(&mut w.value);
}

fn normalize_ann<T>(a: &mut Annotated<T>, f: impl FnOnce(&mut T)) {
    a.pre_trivia = Trivia::new();
    a.span = ZERO_SPAN;
    a.trail_comment = None;
    f(&mut a.value);
}

fn normalize_leaf(l: &mut Leaf) {
    normalize_ann(l, |_| {});
}

fn normalize_items<T>(items: &mut Items<T>, mut f: impl FnMut(&mut T)) {
    items.0.retain_mut(|it| match it {
        Item::Item(v) => {
            f(v);
            true
        }
        Item::Comments(_) => false,
    });
}

fn normalize_string_parts(parts: &mut [StringPart]) {
    for p in parts {
        if let StringPart::Interpolation(w) = p {
            normalize_whole_expr(w);
        }
    }
}

fn normalize_nix_string(s: &mut NixString) {
    normalize_ann(s, |lines| {
        for line in lines {
            normalize_string_parts(line);
        }
    });
}

fn normalize_simple_selector(s: &mut SimpleSelector) {
    match s {
        SimpleSelector::ID(l) => normalize_leaf(l),
        SimpleSelector::Interpol(a) => normalize_ann(a, |sp| {
            if let StringPart::Interpolation(w) = sp {
                normalize_whole_expr(w);
            }
        }),
        SimpleSelector::String(s) => normalize_nix_string(s),
    }
}

fn normalize_selector(s: &mut Selector) {
    if let Some(dot) = &mut s.dot {
        normalize_leaf(dot);
    }
    normalize_simple_selector(&mut s.selector);
}

fn normalize_binder(b: &mut Binder) {
    match b {
        Binder::Inherit {
            kw,
            from,
            attrs,
            semi,
        } => {
            normalize_leaf(kw);
            if let Some(t) = from {
                normalize_term(t);
            }
            for s in attrs {
                normalize_simple_selector(s);
            }
            normalize_leaf(semi);
        }
        Binder::Assignment {
            path,
            eq,
            value,
            semi,
        } => {
            for s in path {
                normalize_selector(s);
            }
            normalize_leaf(eq);
            normalize_expr(value);
            normalize_leaf(semi);
        }
    }
}

fn normalize_term(t: &mut Term) {
    match t {
        Term::Token(l) => normalize_leaf(l),
        Term::SimpleString(s) | Term::IndentedString(s) => normalize_nix_string(s),
        Term::Path(p) => normalize_ann(p, |parts| normalize_string_parts(parts)),
        Term::List { open, items, close } => {
            normalize_leaf(open);
            normalize_items(items, normalize_term);
            normalize_leaf(close);
        }
        Term::Set {
            rec,
            open,
            items,
            close,
        } => {
            if let Some(r) = rec {
                normalize_leaf(r);
            }
            normalize_leaf(open);
            normalize_items(items, normalize_binder);
            normalize_leaf(close);
        }
        Term::Selection {
            base,
            selectors,
            default,
        } => {
            normalize_term(base);
            for s in selectors {
                normalize_selector(s);
            }
            if let Some(d) = default {
                normalize_leaf(&mut d.or_kw);
                normalize_term(&mut d.value);
            }
        }
        Term::Parenthesized { open, expr, close } => {
            normalize_leaf(open);
            normalize_expr(expr);
            normalize_leaf(close);
        }
    }
}

fn normalize_param_attr(p: &mut ParamAttr) {
    match p {
        ParamAttr::Attr {
            name,
            default,
            comma,
        } => {
            normalize_leaf(name);
            if let Some(d) = default.as_mut() {
                normalize_leaf(&mut d.question);
                normalize_expr(&mut d.value);
            }
            // The formatter freely adds/removes trailing commas in pattern
            // sets; treat presence of a comma as non-semantic.
            *comma = None;
        }
        ParamAttr::Ellipsis(l) => normalize_leaf(l),
    }
}

fn normalize_parameter(p: &mut Parameter) {
    match p {
        Parameter::Id(l) => normalize_leaf(l),
        Parameter::Set { open, attrs, close } => {
            normalize_leaf(open);
            for a in attrs {
                normalize_param_attr(a);
            }
            normalize_leaf(close);
        }
        Parameter::Context { lhs, at, rhs } => {
            normalize_parameter(lhs);
            normalize_leaf(at);
            normalize_parameter(rhs);
        }
    }
}

#[allow(clippy::many_single_char_names)] // names mirror Types.hs constructors
fn normalize_expr(e: &mut Expression) {
    match e {
        Expression::Term(t) => normalize_term(t),
        Expression::With {
            kw_with: kw,
            scope: a,
            semi,
            body: b,
        }
        | Expression::Assert {
            kw_assert: kw,
            cond: a,
            semi,
            body: b,
        } => {
            normalize_leaf(kw);
            normalize_expr(a);
            normalize_leaf(semi);
            normalize_expr(b);
        }
        Expression::Let {
            kw_let,
            bindings,
            kw_in,
            body,
        } => {
            normalize_leaf(kw_let);
            normalize_items(bindings, normalize_binder);
            normalize_leaf(kw_in);
            normalize_expr(body);
        }
        Expression::If {
            kw_if,
            cond,
            kw_then,
            then_branch,
            kw_else,
            else_branch,
        } => {
            normalize_leaf(kw_if);
            normalize_expr(cond);
            normalize_leaf(kw_then);
            normalize_expr(then_branch);
            normalize_leaf(kw_else);
            normalize_expr(else_branch);
        }
        Expression::Lambda { param, colon, body } => {
            normalize_parameter(param);
            normalize_leaf(colon);
            normalize_expr(body);
        }
        Expression::Apply { func, arg } => {
            normalize_expr(func);
            normalize_expr(arg);
        }
        Expression::Operation { lhs, op, rhs } => {
            normalize_expr(lhs);
            normalize_leaf(op);
            normalize_expr(rhs);
        }
        Expression::MemberCheck {
            lhs,
            question,
            path,
        } => {
            normalize_expr(lhs);
            normalize_leaf(question);
            for s in path {
                normalize_selector(s);
            }
        }
        Expression::Negation { minus: op, expr: a }
        | Expression::Inversion { bang: op, expr: a } => {
            normalize_leaf(op);
            normalize_expr(a);
        }
    }
}
