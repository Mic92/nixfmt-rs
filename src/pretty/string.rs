use crate::predoc::*;
use crate::types::*;

use super::absorb::is_absorbable_term;
use super::util::{is_simple_expression, is_spaces, text_width};

impl Pretty for StringPart {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            StringPart::TextPart(s) => push_text(doc, s),
            StringPart::Interpolation(whole) => {
                let trailing_empty = whole.trailing_trivia.0.is_empty();
                let value = &whole.value;

                let absorbable_term = if trailing_empty {
                    match value {
                        Expression::Term(term) if is_absorbable_term(term) => Some(term),
                        _ => None,
                    }
                } else {
                    None
                };

                if let Some(term) = absorbable_term {
                    push_group(doc, |group_doc| {
                        push_text(group_doc, "${");
                        term.pretty(group_doc);
                        push_text(group_doc, "}");
                    });
                    return;
                }

                if trailing_empty && is_simple_expression(value) {
                    push_text(doc, "${");
                    value.pretty(doc);
                    push_text(doc, "}");
                    return;
                }

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
            // Single interpolation with leading whitespace: offset by the
            // leading-space width so wrapped `${ ... }` lines up under the `$`.
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
            _ => {
                push_hcat(doc, self.clone());
            }
        }
    }
}

/// Format a simple string (with double quotes)
pub(super) fn push_pretty_simple_string(doc: &mut Doc, parts: &[Vec<StringPart>]) {
    push_group(doc, |d| {
        push_text(d, "\"");
        // Use literal \n instead of newline() to avoid indentation
        let newline_doc = vec![DocE::Text(0, 0, TextAnn::RegularT, "\n".to_string())];
        push_sep_by(d, &newline_doc, parts.to_vec());
        push_text(d, "\"");
    });
}

/// Format an indented string (with '')
pub(super) fn push_pretty_indented_string(doc: &mut Doc, parts: &[Vec<StringPart>]) {
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
