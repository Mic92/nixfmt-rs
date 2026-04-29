//! AST normalisation for structural comparison.
//!
//! Strips all source-location and trivia information (spans, leading
//! trivia, trailing comments, interleaved comment items) so two ASTs
//! produced from differently-formatted but semantically identical
//! inputs compare equal with `==`.
//!
//! Used by the fuzzing harness to check that `parse → format → parse`
//! is a semantic round-trip.

use crate::types::*;

const ZERO_SPAN: Span = Span {
    start: 0,
    end: 0,
    start_line: 1,
    end_line: 1,
};

pub fn normalize_file(file: &mut File) {
    normalize_whole_expr(file);
}

fn normalize_whole_expr(w: &mut Whole<Expression>) {
    w.trailing_trivia = Trivia::new();
    normalize_expr(&mut w.value);
}

fn normalize_ann<T>(a: &mut Ann<T>, f: impl FnOnce(&mut T)) {
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
        Binder::Inherit(kw, from, sels, semi) => {
            normalize_leaf(kw);
            if let Some(t) = from {
                normalize_term(t);
            }
            for s in sels {
                normalize_simple_selector(s);
            }
            normalize_leaf(semi);
        }
        Binder::Assignment(sels, eq, expr, semi) => {
            for s in sels {
                normalize_selector(s);
            }
            normalize_leaf(eq);
            normalize_expr(expr);
            normalize_leaf(semi);
        }
    }
}

fn normalize_term(t: &mut Term) {
    match t {
        Term::Token(l) => normalize_leaf(l),
        Term::SimpleString(s) | Term::IndentedString(s) => normalize_nix_string(s),
        Term::Path(p) => normalize_ann(p, |parts| normalize_string_parts(parts)),
        Term::List(open, items, close) => {
            normalize_leaf(open);
            normalize_items(items, normalize_term);
            normalize_leaf(close);
        }
        Term::Set(rec, open, items, close) => {
            if let Some(r) = rec {
                normalize_leaf(r);
            }
            normalize_leaf(open);
            normalize_items(items, normalize_binder);
            normalize_leaf(close);
        }
        Term::Selection(base, sels, default) => {
            normalize_term(base);
            for s in sels {
                normalize_selector(s);
            }
            if let Some((kw, t)) = default {
                normalize_leaf(kw);
                normalize_term(t);
            }
        }
        Term::Parenthesized(open, e, close) => {
            normalize_leaf(open);
            normalize_expr(e);
            normalize_leaf(close);
        }
    }
}

fn normalize_param_attr(p: &mut ParamAttr) {
    match p {
        ParamAttr::ParamAttr(name, def, comma) => {
            normalize_leaf(name);
            if let Some((q, e)) = def.as_mut() {
                normalize_leaf(q);
                normalize_expr(e);
            }
            // The formatter freely adds/removes trailing commas in pattern
            // sets; treat presence of a comma as non-semantic.
            *comma = None;
        }
        ParamAttr::ParamEllipsis(l) => normalize_leaf(l),
    }
}

fn normalize_parameter(p: &mut Parameter) {
    match p {
        Parameter::ID(l) => normalize_leaf(l),
        Parameter::Set(open, attrs, close) => {
            normalize_leaf(open);
            for a in attrs {
                normalize_param_attr(a);
            }
            normalize_leaf(close);
        }
        Parameter::Context(a, at, b) => {
            normalize_parameter(a);
            normalize_leaf(at);
            normalize_parameter(b);
        }
    }
}

fn normalize_expr(e: &mut Expression) {
    match e {
        Expression::Term(t) => normalize_term(t),
        Expression::With(kw, a, semi, b) | Expression::Assert(kw, a, semi, b) => {
            normalize_leaf(kw);
            normalize_expr(a);
            normalize_leaf(semi);
            normalize_expr(b);
        }
        Expression::Let(kw, binds, in_kw, body) => {
            normalize_leaf(kw);
            normalize_items(binds, normalize_binder);
            normalize_leaf(in_kw);
            normalize_expr(body);
        }
        Expression::If(i, c, t, a, el, b) => {
            normalize_leaf(i);
            normalize_expr(c);
            normalize_leaf(t);
            normalize_expr(a);
            normalize_leaf(el);
            normalize_expr(b);
        }
        Expression::Abstraction(p, colon, body) => {
            normalize_parameter(p);
            normalize_leaf(colon);
            normalize_expr(body);
        }
        Expression::Application(f, a) => {
            normalize_expr(f);
            normalize_expr(a);
        }
        Expression::Operation(a, op, b) => {
            normalize_expr(a);
            normalize_leaf(op);
            normalize_expr(b);
        }
        Expression::MemberCheck(a, q, sels) => {
            normalize_expr(a);
            normalize_leaf(q);
            for s in sels {
                normalize_selector(s);
            }
        }
        Expression::Negation(op, a) | Expression::Inversion(op, a) => {
            normalize_leaf(op);
            normalize_expr(a);
        }
    }
}
