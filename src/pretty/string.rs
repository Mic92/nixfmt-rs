use crate::predoc::{Doc, DocE, Pretty, TextAnn, newline, text_width, unexpand_spacing_prime};
use crate::types::{Expression, StringPart};

use super::absorb::is_absorbable_term;
use super::term::push_parenthesized_inner;
use super::util::{is_simple_expression, is_spaces};

impl Pretty for StringPart {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Self::TextPart(s) => {
                doc.text(&**s);
            }
            Self::Interpolation(whole) => {
                let trailing_empty = whole.trailing_trivia.is_empty();
                let value = &whole.value;

                if trailing_empty
                    && let Expression::Term(term) = value
                    && is_absorbable_term(term)
                {
                    doc.group(|g| {
                        g.text("${");
                        term.pretty(g);
                        g.text("}");
                    });
                    return;
                }

                // Simple interpolations (mostly identifiers/selections): force
                // single line regardless of width.
                if trailing_empty && is_simple_expression(value) {
                    doc.text("${");
                    let mut rendered = Doc::new();
                    value.pretty(&mut rendered);
                    match unexpand_spacing_prime(None, &rendered) {
                        Some(compact) => doc.extend(compact),
                        None => doc.extend(rendered),
                    }
                    doc.text("}");
                    return;
                }

                // General case: render the body and, if it fits compactly in
                // ≤30 columns, force it onto this line even past the width limit.
                let mut rendered = Doc::new();
                whole.pretty(&mut rendered);
                doc.group(|g| {
                    g.text("${");
                    match unexpand_spacing_prime(Some(30), &rendered) {
                        Some(compact) => g.extend(compact),
                        None => {
                            g.nested(|n| {
                                n.line_prime();
                                n.extend(rendered);
                                n.line_prime();
                            });
                        }
                    }
                    g.text("}");
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
                Some((&**pre, &whole.value))
            }
            _ => None,
        };
        if let Some((pre, expr)) = lone {
            doc.text(pre);
            doc.offset(text_width(pre), |d| {
                d.group(|g| {
                    g.text("${");
                    // Upstream keeps this case split identical to `prettyTerm (Parenthesized …)`.
                    g.nested(|n| push_parenthesized_inner(n, expr));
                    g.text("}");
                });
            });
            return;
        }

        match self.as_slice() {
            // Lone interpolation with trailing trivia: always surround with `line'`.
            [StringPart::Interpolation(whole)] => {
                doc.group(|g| {
                    g.text("${");
                    g.nested(|n| {
                        n.line_prime();
                        whole.pretty(n);
                        n.line_prime();
                    });
                    g.text("}");
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
                doc.text(&**t);
                doc.offset(indentation, |d| {
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
    doc.group(|d| {
        d.text("\"");
        // Use literal \n instead of newline() to avoid indentation
        let newline_doc = [DocE::Text(0, 0, TextAnn::RegularT, "\n".to_string())];
        d.sep_by(&newline_doc, parts.iter().cloned());
        d.text("\"");
    });
}

/// Format an indented string (with '')
pub(super) fn push_pretty_indented_string(doc: &mut Doc, parts: &[Vec<StringPart>]) {
    doc.group(|d| {
        d.text("''");
        // For multi-line strings, add a potential line break after opening ''
        if parts.len() > 1 {
            d.line_prime();
        }
        d.nested(|inner| {
            inner.sep_by(&[newline()], parts.iter().cloned());
        });
        d.text("''");
    });
}
