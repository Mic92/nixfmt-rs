use crate::ast::{
    Annotated, Expression, Leaf, ParamAttr, ParamDefault, Parameter, Selector, SimpleSelector,
    Term, Token, TrailingComment, Trivia,
};
use crate::doc::{Doc, Elem, Emit, hardline, line};

use super::term::empty_brackets;

/// Emit `name ? default ,`. Split out so [`render_param_attrs`] can suppress
/// the trailing comma on the last attr without rebuilding a `ParamAttr`.
fn emit_attr(doc: &mut Doc, name: &Leaf, default: Option<&ParamDefault>, comma: Option<&Leaf>) {
    let make_pretty = |d: &mut Doc| {
        name.emit(d);
        if let Some(def) = default {
            d.hardspace();
            d.nested(|inner| {
                def.question.emit(inner);
                def.value.absorb_rhs(inner);
            });
        }
        comma.emit(d);
    };
    if default.is_some() {
        doc.group(make_pretty);
    } else {
        make_pretty(doc);
    }
}

impl Emit for ParamAttr {
    fn emit(&self, doc: &mut Doc) {
        match self {
            Self::Attr {
                name,
                default,
                comma,
            } => emit_attr(doc, name, default.as_ref(), comma.as_ref()),
            Self::Ellipsis(ellipsis) => ellipsis.emit(doc),
        }
    }
}

/// Mirrors `mapLastToken'` in Nixfmt/Types.hs, specialised to taking the
/// trailing comment off the last leaf of an expression.
fn take_last_trail_comment_expr(expr: &mut Expression) -> Option<TrailingComment> {
    const fn sel(s: &mut Selector) -> Option<TrailingComment> {
        match &mut s.selector {
            SimpleSelector::ID(l) => l.trail_comment.take(),
            SimpleSelector::Interpol(l) => l.trail_comment.take(),
            SimpleSelector::String(l) => l.trail_comment.take(),
        }
    }
    fn term(t: &mut Term) -> Option<TrailingComment> {
        match t {
            Term::Token(l) => l.trail_comment.take(),
            Term::SimpleString(l) | Term::IndentedString(l) => l.trail_comment.take(),
            Term::Path(l) => l.trail_comment.take(),
            Term::List { close, .. }
            | Term::Set { close, .. }
            | Term::Parenthesized { close, .. } => close.trail_comment.take(),
            Term::Selection {
                default: Some(d), ..
            } => term(&mut d.value),
            // The parser only builds `Selection` when `selectors` is non-empty.
            Term::Selection {
                selectors,
                default: None,
                ..
            } => sel(selectors.last_mut().expect("≥1 selector")),
        }
    }
    match expr {
        Expression::Term(t) => term(t),
        Expression::With { body, .. }
        | Expression::Let { body, .. }
        | Expression::Assert { body, .. }
        | Expression::If {
            else_branch: body, ..
        }
        | Expression::Lambda { body, .. }
        | Expression::Apply { arg: body, .. }
        | Expression::Operation { rhs: body, .. }
        | Expression::Negation { expr: body, .. }
        | Expression::Not { expr: body, .. } => take_last_trail_comment_expr(body),
        // `parse_selector_path` always pushes at least one selector.
        Expression::HasAttr { path, .. } => sel(path.last_mut().expect("≥1 selector")),
    }
}

/// Mirrors `moveParamAttrComment` in Nixfmt/Pretty.hs.
fn move_param_attr_comment(attr: ParamAttr) -> ParamAttr {
    match attr {
        ParamAttr::Attr {
            mut name,
            default,
            comma: Some(mut comma),
        } if default.is_none() && name.trail_comment.is_some() && !comma.has_trivia() => {
            comma.trail_comment = name.trail_comment.take();
            ParamAttr::Attr {
                name,
                default,
                comma: Some(comma),
            }
        }
        ParamAttr::Attr {
            name,
            mut default,
            comma: Some(mut comma),
        } if default.is_some() && !comma.has_trivia() => {
            if let Some(def) = default.as_mut() {
                comma.trail_comment = take_last_trail_comment_expr(&mut def.value);
            }
            ParamAttr::Attr {
                name,
                default,
                comma: Some(comma),
            }
        }
        other => other,
    }
}

/// Mirrors `moveParamsComments` in Nixfmt/Pretty.hs.
fn move_params_comments(attrs: &[ParamAttr]) -> Vec<ParamAttr> {
    let mut out: Vec<ParamAttr> = attrs.to_vec();
    let mut i = 0;
    while i < out.len() {
        let is_last = i + 1 == out.len();
        let (head, tail) = out[i..].split_first_mut().unwrap();
        match head {
            ParamAttr::Attr {
                comma: Some(comma), ..
            } if comma.trail_comment.is_none() && !is_last => {
                let mut trivia = std::mem::take(&mut comma.pre_trivia);
                match &mut tail[0] {
                    ParamAttr::Attr { name, .. } => {
                        trivia.extend(std::mem::take(&mut name.pre_trivia));
                        name.pre_trivia = trivia;
                    }
                    ParamAttr::Ellipsis(ell) => {
                        trivia.extend(std::mem::take(&mut ell.pre_trivia));
                        ell.pre_trivia = trivia;
                    }
                }
            }
            ParamAttr::Attr {
                name,
                comma: comma @ None,
                ..
            } if is_last => {
                *comma = Some(Annotated {
                    pre_trivia: Trivia::new(),
                    value: Token::Comma,
                    span: name.span,
                    trail_comment: None,
                });
            }
            _ => {}
        }
        i += 1;
    }
    out
}

fn parameter_separator(open: &Leaf, attrs: &[ParamAttr], close: &Leaf) -> Elem {
    if open.span.start_line() != close.span.start_line() {
        return hardline();
    }
    // Allow a compact `{ a, b, ... }` only for at most two plain attrs
    // optionally followed by `...`.
    let plain = match attrs.split_last() {
        Some((last, init)) if last.is_ellipsis() => init,
        _ => attrs,
    };
    if plain.len() <= 2 && plain.iter().all(ParamAttr::has_no_default) {
        line()
    } else {
        hardline()
    }
}

/// Mirrors `handleTrailingComma` in Nixfmt/Pretty.hs.
fn render_param_attrs(attrs: &[ParamAttr]) -> Vec<Doc> {
    let attrs: Vec<ParamAttr> = move_params_comments(attrs)
        .into_iter()
        .map(move_param_attr_comment)
        .collect();

    attrs
        .iter()
        .enumerate()
        .map(|(idx, attr)| {
            let mut rendered = Doc::new();
            let is_last = idx + 1 == attrs.len();

            if is_last
                && let ParamAttr::Attr {
                    name,
                    default,
                    comma: Some(comma),
                } = attr
                && !comma.has_trivia()
            {
                emit_attr(&mut rendered, name, default.as_ref(), None);
                rendered.trailing(",");
                return rendered;
            }

            attr.emit(&mut rendered);
            rendered
        })
        .collect()
}

impl Emit for Parameter {
    fn emit(&self, doc: &mut Doc) {
        match self {
            Self::Id(id) => id.emit(doc),
            Self::Set { open, attrs, close } => {
                let open = open.move_trailing_comment_up();
                if attrs.is_empty() {
                    doc.group(|doc| empty_brackets(doc, &open, close));
                    return;
                }

                let sep = parameter_separator(&open, attrs, close);
                let sep_doc = [sep.clone()];

                doc.group(|doc| {
                    open.emit(doc);
                    doc.push_raw(sep.clone());
                    doc.nested(|inner| {
                        inner.sep_by(&sep_doc, render_param_attrs(attrs));
                    });
                    doc.push_raw(sep);
                    doc.nested(|inner| close.pre_trivia.emit(inner));
                    close.emit_tail(doc);
                });
            }
            Self::Context {
                lhs: left,
                at,
                rhs: right,
            } => {
                left.emit(doc);
                at.emit(doc);
                right.emit(doc);
            }
        }
    }
}
