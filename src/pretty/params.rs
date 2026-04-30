use crate::predoc::{
    Doc, DocE, Pretty, hardline, hardspace, line, push_group, push_nested, push_sep_by,
    push_trailing,
};
use crate::types::{
    Ann, Expression, Leaf, ParamAttr, Parameter, Selector, SimpleSelector, Term, Token,
    TrailingComment, Trivia,
};

use super::absorb::push_absorb_rhs;
use super::util::{is_lone_ann, move_trailing_comment_up, push_empty_brackets};

impl Pretty for ParamAttr {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Self::ParamAttr(name, default, maybe_comma) => {
                let has_default = default.is_some();
                let make_pretty = |d: &mut Doc| {
                    name.pretty(d);

                    if let Some((qmark, def)) = default.as_ref() {
                        d.push(hardspace());
                        push_nested(d, |inner| {
                            qmark.pretty(inner);
                            push_absorb_rhs(inner, def);
                        });
                    }

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
            Self::ParamEllipsis(ellipsis) => ellipsis.pretty(doc),
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
            Term::List(_, _, close)
            | Term::Set(_, _, _, close)
            | Term::Parenthesized(_, _, close) => close.trail_comment.take(),
            Term::Selection(_, _, Some((_, def))) => term(def),
            #[allow(clippy::option_if_let_else)] // map_or_else fights the borrow checker here
            Term::Selection(inner, sels, None) => match sels.last_mut() {
                Some(last) => sel(last),
                None => term(inner),
            },
        }
    }
    match expr {
        Expression::Term(t) => term(t),
        Expression::With(_, _, _, body)
        | Expression::Let(_, _, _, body)
        | Expression::Assert(_, _, _, body)
        | Expression::If(_, _, _, _, _, body)
        | Expression::Abstraction(_, _, body)
        | Expression::Application(_, body)
        | Expression::Operation(_, _, body)
        | Expression::Negation(_, body)
        | Expression::Inversion(_, body) => take_last_trail_comment_expr(body),
        Expression::MemberCheck(_, qmark, sels) => match sels.last_mut() {
            Some(last) => sel(last),
            None => qmark.trail_comment.take(),
        },
    }
}

/// Mirrors `moveParamAttrComment` in Nixfmt/Pretty.hs.
fn move_param_attr_comment(attr: ParamAttr) -> ParamAttr {
    match attr {
        ParamAttr::ParamAttr(mut name, default, Some(mut comma))
            if default.is_none() && name.trail_comment.is_some() && is_lone_ann(&comma) =>
        {
            comma.trail_comment = name.trail_comment.take();
            ParamAttr::ParamAttr(name, default, Some(comma))
        }
        ParamAttr::ParamAttr(name, mut default, Some(mut comma))
            if default.is_some() && is_lone_ann(&comma) =>
        {
            if let Some((_, def)) = default.as_mut() {
                comma.trail_comment = take_last_trail_comment_expr(def);
            }
            ParamAttr::ParamAttr(name, default, Some(comma))
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
            ParamAttr::ParamAttr(_, _, Some(comma))
                if comma.trail_comment.is_none() && !is_last =>
            {
                let mut trivia = std::mem::take(&mut comma.pre_trivia);
                match &mut tail[0] {
                    ParamAttr::ParamAttr(name, _, _) => {
                        trivia.extend(std::mem::take(&mut name.pre_trivia));
                        name.pre_trivia = trivia;
                    }
                    ParamAttr::ParamEllipsis(ell) => {
                        trivia.extend(std::mem::take(&mut ell.pre_trivia));
                        ell.pre_trivia = trivia;
                    }
                }
            }
            ParamAttr::ParamAttr(name, _, comma @ None) if is_last => {
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

fn param_attr_without_default(attr: &ParamAttr) -> bool {
    matches!(attr, ParamAttr::ParamAttr(_, default, _) if default.is_none())
}

const fn param_attr_is_ellipsis(attr: &ParamAttr) -> bool {
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
            let mut rendered = Vec::new();
            let is_last = idx + 1 == attrs.len();

            if is_last
                && let ParamAttr::ParamAttr(name, default, Some(comma)) = attr
                && is_lone_ann(comma)
            {
                ParamAttr::ParamAttr(name.clone(), default.clone(), None).pretty(&mut rendered);
                push_trailing(&mut rendered, ",");
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
            Self::ID(id) => id.pretty(doc),
            Self::Set(open, attrs, close) => {
                let open = move_trailing_comment_up(open);
                if attrs.is_empty() {
                    push_group(doc, |doc| push_empty_brackets(doc, &open, close));
                    return;
                }

                let sep = parameter_separator(&open, attrs, close);
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
                    close.without_pre().pretty(doc);
                });
            }
            Self::Context(left, at, right) => {
                left.pretty(doc);
                at.pretty(doc);
                right.pretty(doc);
            }
        }
    }
}
