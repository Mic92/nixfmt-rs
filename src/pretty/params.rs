use crate::predoc::{Doc, DocE, Pretty, hardline, line};
use crate::types::{
    Ann, Expression, Leaf, ParamAttr, Parameter, Selector, SimpleSelector, Term, Token,
    TrailingComment, Trivia,
};

use super::absorb::push_absorb_rhs;
use super::util::{is_lone_ann, move_trailing_comment_up, push_empty_brackets};

impl Pretty for ParamAttr {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Self::Attr {
                name,
                default,
                comma: maybe_comma,
            } => {
                let has_default = default.is_some();
                let make_pretty = |d: &mut Doc| {
                    name.pretty(d);

                    if let Some(def) = default.as_ref() {
                        d.hardspace();
                        d.nested(|inner| {
                            def.question.pretty(inner);
                            push_absorb_rhs(inner, &def.value);
                        });
                    }

                    if let Some(comma) = maybe_comma {
                        comma.pretty(d);
                    }
                };

                if has_default {
                    doc.group(make_pretty);
                } else {
                    make_pretty(doc);
                }
            }
            Self::Ellipsis(ellipsis) => ellipsis.pretty(doc),
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
        | Expression::Abstraction { body, .. }
        | Expression::Application { arg: body, .. }
        | Expression::Operation { rhs: body, .. }
        | Expression::Negation { expr: body, .. }
        | Expression::Inversion { expr: body, .. } => take_last_trail_comment_expr(body),
        // `parse_selector_path` always pushes at least one selector.
        Expression::MemberCheck { path, .. } => sel(path.last_mut().expect("≥1 selector")),
    }
}

/// Mirrors `moveParamAttrComment` in Nixfmt/Pretty.hs.
fn move_param_attr_comment(attr: ParamAttr) -> ParamAttr {
    match attr {
        ParamAttr::Attr {
            mut name,
            default,
            comma: Some(mut comma),
        } if default.is_none() && name.trail_comment.is_some() && is_lone_ann(&comma) => {
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
        } if default.is_some() && is_lone_ann(&comma) => {
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
                *comma = Some(Ann {
                    pre_trivia: Trivia::new(),
                    value: Token::TComma,
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

const fn param_attr_without_default(attr: &ParamAttr) -> bool {
    matches!(attr, ParamAttr::Attr { default, .. } if default.is_none())
}

const fn param_attr_is_ellipsis(attr: &ParamAttr) -> bool {
    matches!(attr, ParamAttr::Ellipsis(_))
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
                && is_lone_ann(comma)
            {
                ParamAttr::Attr {
                    name: name.clone(),
                    default: default.clone(),
                    comma: None,
                }
                .pretty(&mut rendered);
                rendered.trailing(",");
                return rendered;
            }

            attr.pretty(&mut rendered);
            rendered
        })
        .collect()
}

impl Pretty for Parameter {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Self::Id(id) => id.pretty(doc),
            Self::Set { open, attrs, close } => {
                let open = move_trailing_comment_up(open);
                if attrs.is_empty() {
                    doc.group(|doc| push_empty_brackets(doc, &open, close));
                    return;
                }

                let sep = parameter_separator(&open, attrs, close);
                let sep_doc = [sep.clone()];

                doc.group(|doc| {
                    open.pretty(doc);
                    doc.push_raw(sep.clone());
                    doc.nested(|inner| {
                        inner.sep_by(&sep_doc, render_param_attrs(attrs));
                    });
                    doc.push_raw(sep);
                    doc.nested(|inner| close.pre_trivia.pretty(inner));
                    close.without_pre().pretty(doc);
                });
            }
            Self::Context {
                lhs: left,
                at,
                rhs: right,
            } => {
                left.pretty(doc);
                at.pretty(doc);
                right.pretty(doc);
            }
        }
    }
}
