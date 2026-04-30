//! Error formatting with source snippets

use crate::error::{ErrorKind, ParseError};
use crate::types::Span;
use std::fmt::Write;

use super::context::ErrorContext;

/// One caret/underline row to render under a source line.
struct Mark {
    span: Span,
    glyph: char,
    label: Option<String>,
}

/// Rich error formatter with source snippets
pub struct ErrorFormatter<'a> {
    context: &'a ErrorContext<'a>,
}

impl<'a> ErrorFormatter<'a> {
    /// Create a formatter that resolves spans against `context`.
    #[must_use]
    pub const fn new(context: &'a ErrorContext<'a>) -> Self {
        Self { context }
    }

    /// Format a single error
    #[must_use]
    pub fn format(&self, error: &ParseError) -> String {
        let mut output = String::new();

        let marks = Self::collect_marks(error);
        let lines = self.lines_to_show(error, &marks);
        let line_num_width = lines.last().map_or(1, |l| l + 1).to_string().len().max(2);

        self.format_header(error, line_num_width, &mut output);
        self.format_snippet(&marks, &lines, line_num_width, &mut output);
        Self::format_notes(error, line_num_width, &mut output);

        output
    }

    /// Primary span plus any secondary spans (delimiter opener, labels) that
    /// should get their own caret row in the snippet.
    fn collect_marks(error: &ParseError) -> Vec<Mark> {
        let mut marks = vec![Mark {
            span: error.span,
            glyph: '^',
            label: None,
        }];

        if let ErrorKind::UnclosedDelimiter { opening_span, .. } = &error.kind {
            marks.push(Mark {
                span: *opening_span,
                glyph: '-',
                label: Some("unclosed delimiter opened here".to_string()),
            });
        }

        for label in &error.labels {
            marks.push(Mark {
                span: label.span,
                glyph: '-',
                label: Some(label.message.clone()),
            });
        }

        marks.sort_by_key(|m| m.span.start);
        marks
    }

    /// 0-based line indices to render: every marked line, plus one line of
    /// context either side of the primary error.
    fn lines_to_show(&self, error: &ParseError, marks: &[Mark]) -> Vec<usize> {
        let last_line = self.context.source.lines().count().saturating_sub(1);
        let primary = self.context.position(error.span.start).line - 1;

        let mut lines: Vec<usize> = marks
            .iter()
            .map(|m| self.context.position(m.span.start).line - 1)
            .chain([primary.saturating_sub(1), (primary + 1).min(last_line)])
            .collect();
        lines.sort_unstable();
        lines.dedup();
        lines
    }

    fn format_header(&self, error: &ParseError, line_num_width: usize, out: &mut String) {
        let pos = self.context.position(error.span.start);

        write!(out, "Error").unwrap();

        if let Some(code) = error.code() {
            write!(out, "[{code}]").unwrap();
        }

        writeln!(out, ": {}", error.message()).unwrap();

        // Align the ┌─ with the │ from line numbers.
        write!(out, "{:>width$} ", "", width = line_num_width).unwrap();
        let col = pos.column + 1; // 0-based -> 1-based for editors
        if let Some(filename) = self.context.filename {
            writeln!(out, "┌─ {}:{}:{}", filename, pos.line, col).unwrap();
        } else {
            writeln!(out, "┌─ line {}:{}", pos.line, col).unwrap();
        }
    }

    fn format_snippet(
        &self,
        marks: &[Mark],
        lines: &[usize],
        line_num_width: usize,
        out: &mut String,
    ) {
        writeln!(out, "{:>line_num_width$} │", "").unwrap();

        let mut prev: Option<usize> = None;
        for &line_idx in lines {
            if prev.is_some_and(|p| line_idx > p + 1) {
                writeln!(out, "{:>line_num_width$} ·", "").unwrap();
            }
            prev = Some(line_idx);

            let line_start = self.context.line_start(line_idx);
            let (line_num, line_text) = self.context.line_at(line_start);
            writeln!(out, "{line_num:>line_num_width$} │ {line_text}").unwrap();

            for mark in marks {
                if self.context.position(mark.span.start).line - 1 != line_idx {
                    continue;
                }
                let (col, len) = visual_span(line_text, line_start, mark.span);
                write!(out, "{:>line_num_width$} │ {:col$}", "", "").unwrap();
                for _ in 0..len {
                    out.push(mark.glyph);
                }
                if let Some(label) = &mark.label {
                    write!(out, " {label}").unwrap();
                }
                writeln!(out).unwrap();
            }
        }
    }

    fn format_notes(error: &ParseError, line_num_width: usize, out: &mut String) {
        let indent = " ".repeat(line_num_width + 1);

        match &error.kind {
            ErrorKind::UnexpectedToken { expected, found } => {
                if let [single] = expected.as_slice()
                    && let Some((note, help)) = unexpected_token_hint(single, found)
                {
                    writeln!(out, "{indent}= note: {note}").unwrap();
                    writeln!(out, "{indent}= help: {help}").unwrap();
                }
                // No fallback: header already states "expected X, found Y".
            }
            ErrorKind::InvalidSyntax {
                hint: Some(hint), ..
            } => {
                writeln!(out, "{indent}= help: {hint}").unwrap();
            }
            ErrorKind::UnclosedDelimiter { delimiter, .. } => {
                let (_, close_tok) = Self::delimiter_pair(*delimiter);
                writeln!(out, "{indent}= help: add closing {close_tok}").unwrap();
            }
            ErrorKind::ChainedComparison { .. } => {
                writeln!(
                    out,
                    "{indent}= note: comparison operators cannot be chained in Nix"
                )
                .unwrap();
                writeln!(out, "{indent}= help: use parentheses: (a < b) && (b < c)").unwrap();
            }
            ErrorKind::MissingToken { token, after } => {
                writeln!(out, "{indent}= note: {token} is required after {after}").unwrap();
            }
            _ => {}
        }
    }
}

/// Character column and width of `span` within `line_text` (bytes -> chars).
fn visual_span(line_text: &str, line_start: usize, span: Span) -> (usize, usize) {
    let byte_col = (span.start as usize).saturating_sub(line_start);
    let byte_len = (span.end - span.start).max(1) as usize;

    let clamped_col = byte_col.min(line_text.len());
    let col = line_text[..clamped_col].chars().count();
    let len = line_text[clamped_col..]
        .chars()
        .take(byte_len)
        .count()
        .max(1);
    (col, len)
}

/// Canned `(note, help)` text for the common single-expected-token errors.
///
/// Keyed on the *expected* token; `found` lets us special-case a few
/// confusable pairs without guessing about parser context we don't have here.
fn unexpected_token_hint(expected: &str, found: &str) -> Option<(&'static str, &'static str)> {
    Some(match expected {
        "';'" => (
            "missing semicolon after definition",
            "add a semicolon at the end of the previous line",
        ),
        // `'}'` closes both attrsets and interpolations; only hint when the
        // found token disambiguates.
        "'}'" if found == "'in'" => (
            "'in' is only valid inside 'let ... in ...' expressions",
            "did you mean to start with 'let' instead of '{'?",
        ),
        "'}'" if found == "':'" => (
            "':' is not used for attribute assignment in Nix",
            "use '=' to assign a value: name = ...;",
        ),
        "'&&'" => (
            "single '&' is not a valid operator in Nix",
            "did you mean '&&' (logical and)?",
        ),
        "'then'" => (
            "if expressions require: if <condition> then <expr> else <expr>",
            "add 'then' after the condition",
        ),
        "'else'" => (
            "if expressions require: if <condition> then <expr> else <expr>",
            "add 'else' followed by the alternative expression",
        ),
        "'in'" => (
            "'in' is required to complete the let expression",
            "add 'in' followed by the expression body",
        ),
        "'='" => (
            "attribute paths must be followed by '= <value>;'",
            "add '= ...' to assign a value",
        ),
        "'{'" => (
            "'rec' must be followed by an attribute set",
            "write 'rec { ... }'",
        ),
        _ => return None,
    })
}

impl ErrorFormatter<'_> {
    /// The lexer encodes the `''` opener as a single `'`; expand it back so we
    /// don't render `'''`.
    const fn delimiter_pair(opening: char) -> (&'static str, &'static str) {
        match opening {
            '{' => ("'{'", "'}'"),
            '[' => ("'['", "']'"),
            '(' => ("'('", "')'"),
            '\'' => ("''", "''"),
            '"' => ("'\"'", "'\"'"),
            _ => ("delimiter", "delimiter"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn render(src: &str) -> String {
        let err = parse(src).unwrap_err();
        let ctx = ErrorContext::new(src, Some("t.nix"));
        ErrorFormatter::new(&ctx).format(&err)
    }

    #[test]
    fn unclosed_delimiter_shows_opener_in_snippet() {
        let out = render("(1 + 2");
        assert!(out.contains("t.nix:1:7"), "1-based column: {out}");
        assert!(
            out.contains("- unclosed delimiter opened here"),
            "secondary mark: {out}"
        );
    }

    #[test]
    fn unclosed_opener_on_other_line_is_rendered() {
        let out = render("{\n  x = 1;\n");
        assert!(out.contains("1 │ {"), "opener line shown: {out}");
        assert!(out.contains("opened here"), "{out}");
    }

    #[test]
    fn indented_string_delimiter_rendered_as_double_quote() {
        let out = render("''\nhello\n");
        assert!(out.contains("unclosed indented string"), "{out}");
        assert!(!out.contains("'''"), "must not render triple quote: {out}");
    }
}
