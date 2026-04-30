use crate::predoc::*;
use crate::types::*;

use super::absorb::is_absorbable_term;
use super::term::push_parenthesized_inner;
use super::util::{is_simple_expression, is_spaces};

impl Pretty for StringPart {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            StringPart::TextPart(s) => push_text(doc, s),
            StringPart::Interpolation(whole) => {
                let trailing_empty = whole.trailing_trivia.is_empty();
                let value = &whole.value;

                if trailing_empty
                    && let Expression::Term(term) = value
                    && is_absorbable_term(term)
                {
                    push_group(doc, |g| {
                        push_text(g, "${");
                        term.pretty(g);
                        push_text(g, "}");
                    });
                    return;
                }

                // Simple interpolations (mostly identifiers/selections): force
                // single line regardless of width.
                if trailing_empty && is_simple_expression(value) {
                    push_text(doc, "${");
                    let mut rendered = Doc::new();
                    value.pretty(&mut rendered);
                    match unexpand_spacing_prime(None, &rendered) {
                        Some(compact) => doc.extend(compact),
                        None => doc.extend(rendered),
                    }
                    push_text(doc, "}");
                    return;
                }

                // General case: render the body and, if it fits compactly in
                // ≤30 columns, force it onto this line even past the width limit.
                let mut rendered = Doc::new();
                whole.pretty(&mut rendered);
                push_group(doc, |g| {
                    push_text(g, "${");
                    match unexpand_spacing_prime(Some(30), &rendered) {
                        Some(compact) => g.extend(compact),
                        None => push_nested(g, |n| {
                            n.push(line_prime());
                            n.extend(rendered);
                            n.push(line_prime());
                        }),
                    }
                    push_text(g, "}");
                });
            }
        }
    }
}

impl Pretty for Vec<StringPart> {
    fn pretty(&self, doc: &mut Doc) {
        // When the interpolation is the only thing on the string line (modulo
        // leading whitespace) and carries no trailing trivia, absorb its body
        // instead of surrounding it with `line'`.
        let lone = match self.as_slice() {
            [StringPart::Interpolation(whole)] if whole.trailing_trivia.is_empty() => {
                Some(("", &whole.value))
            }
            [StringPart::TextPart(pre), StringPart::Interpolation(whole)]
                if is_spaces(pre) && whole.trailing_trivia.is_empty() =>
            {
                Some((pre.as_str(), &whole.value))
            }
            _ => None,
        };
        if let Some((pre, expr)) = lone {
            push_text(doc, pre);
            push_offset(doc, text_width(pre), |d| {
                push_group(d, |g| {
                    push_text(g, "${");
                    // Upstream keeps this case split identical to `prettyTerm (Parenthesized …)`.
                    push_nested(g, |n| push_parenthesized_inner(n, expr));
                    push_text(g, "}");
                });
            });
            return;
        }

        match self.as_slice() {
            // Lone interpolation with trailing trivia: always surround with `line'`.
            [StringPart::Interpolation(whole)] => {
                push_group(doc, |g| {
                    push_text(g, "${");
                    push_nested(g, |n| {
                        n.push(line_prime());
                        whole.pretty(n);
                        n.push(line_prime());
                    });
                    push_text(g, "}");
                });
            }
            // If a line is split across multiple code lines due to large
            // interpolations, indent the continuation by the line's leading
            // whitespace so it lines up under the string content.
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
                for part in self {
                    part.pretty(doc);
                }
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
