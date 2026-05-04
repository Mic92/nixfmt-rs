use crate::predoc::{Doc, DocE, Pretty, TextAnn, newline, text_width};
use crate::types::{Expression, StringPart};

use super::term::push_parenthesized_inner;

fn is_spaces(s: &str) -> bool {
    s.chars().all(char::is_whitespace)
}

/// Wrap content in `${ ... }` with a group: try to compact it onto one line
/// (within `max_width` columns), otherwise break with `linebreak`.
fn push_interpolation_braces(doc: &mut Doc, max_width: i32, body: Doc) {
    doc.group(|g| {
        g.text("${");
        match body.try_compact(Some(max_width)) {
            Some(compact) => g.extend(compact),
            None => {
                g.nested(|n| {
                    n.linebreak();
                    n.extend(body);
                    n.linebreak();
                });
            }
        }
        g.text("}");
    });
}

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
                    && term.is_absorbable()
                {
                    doc.group(|g| {
                        g.text("${");
                        term.pretty(g);
                        g.text("}");
                    });
                    return;
                }

                if trailing_empty && value.is_simple() {
                    doc.text("${");
                    let mut rendered = Doc::new();
                    value.pretty(&mut rendered);
                    match rendered.try_compact(None) {
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
                push_interpolation_braces(doc, 30, rendered);
            }
        }
    }
}

impl Pretty for Vec<StringPart> {
    fn pretty(&self, doc: &mut Doc) {
        // When the interpolation is the only thing on the string line (modulo
        // leading whitespace) and carries no trailing trivia, absorb its body
        // instead of surrounding it with `linebreak`.
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
                    g.nested(|n| push_parenthesized_inner(n, expr));
                    g.text("}");
                });
            });
            return;
        }

        match self.as_slice() {
            // Lone interpolation with trailing trivia: always surround with `linebreak`.
            [StringPart::Interpolation(whole)] => {
                let mut rendered = Doc::new();
                whole.pretty(&mut rendered);
                push_interpolation_braces(doc, 0, rendered);
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
        // Literal \n avoids the indentation that newline() would inject
        let newline_doc = [DocE::Text(0, 0, TextAnn::Regular, "\n".to_string())];
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
            d.linebreak();
        }
        d.nested(|inner| {
            inner.sep_by(&[newline()], parts.iter().cloned());
        });
        d.text("''");
    });
}
