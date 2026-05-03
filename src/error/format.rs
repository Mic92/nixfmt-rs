//! Error formatting with source snippets

use crate::error::{ErrorKind, ParseError};
use crate::types::Span;
use std::fmt::{self, Write as _};

use super::context::ErrorContext;

/// One caret/underline row to render under a source line.
struct Mark {
    span: Span,
    glyph: char,
    label: Option<String>,
}

/// `Display` wrapper that renders a `ParseError` against an `ErrorContext`.
pub struct ErrorDisplay<'a> {
    context: &'a ErrorContext<'a>,
    error: &'a ParseError,
}

/// Build a `Display` value rendering `error` against `context`.
#[must_use]
pub const fn render<'a>(context: &'a ErrorContext<'a>, error: &'a ParseError) -> ErrorDisplay<'a> {
    ErrorDisplay { context, error }
}

impl fmt::Display for ErrorDisplay<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let marks = collect_marks(self.error);
        let lines = lines_to_show(self.context, self.error, &marks);
        let line_num_width = lines.last().map_or(1, |l| l + 1).to_string().len().max(2);

        format_header(self.context, self.error, line_num_width, f)?;
        format_snippet(self.context, &marks, &lines, line_num_width, f)?;
        format_notes(self.error, line_num_width, f)?;
        Ok(())
    }
}

/// Primary span plus any secondary spans (delimiter opener) that should
/// get their own caret row in the snippet.
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

    marks.sort_by_key(|m| m.span.start());
    marks
}

/// 0-based line indices to render: every marked line, plus one line of
/// context either side of the primary error.
fn lines_to_show(context: &ErrorContext<'_>, error: &ParseError, marks: &[Mark]) -> Vec<usize> {
    let last_line = context.source.lines().count().saturating_sub(1);
    let primary = context.position(error.span.start()).line - 1;

    let mut lines: Vec<usize> = marks
        .iter()
        .map(|m| context.position(m.span.start()).line - 1)
        .chain([primary.saturating_sub(1), (primary + 1).min(last_line)])
        .collect();
    lines.sort_unstable();
    lines.dedup();
    lines
}

fn format_header(
    context: &ErrorContext<'_>,
    error: &ParseError,
    line_num_width: usize,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    let pos = context.position(error.span.start());

    write!(f, "Error")?;

    if let Some(code) = error.code() {
        write!(f, "[{code}]")?;
    }

    writeln!(f, ": {}", error.message())?;

    // Align the ┌─ with the │ from line numbers.
    write!(f, "{:>width$} ", "", width = line_num_width)?;
    let col = pos.column + 1; // 0-based -> 1-based for editors
    if let Some(filename) = context.filename {
        writeln!(f, "┌─ {}:{}:{}", filename, pos.line, col)?;
    } else {
        writeln!(f, "┌─ line {}:{}", pos.line, col)?;
    }
    Ok(())
}

fn format_snippet(
    context: &ErrorContext<'_>,
    marks: &[Mark],
    lines: &[usize],
    line_num_width: usize,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    writeln!(f, "{:>line_num_width$} │", "")?;

    let mut prev: Option<usize> = None;
    for &line_idx in lines {
        if prev.is_some_and(|p| line_idx > p + 1) {
            writeln!(f, "{:>line_num_width$} ·", "")?;
        }
        prev = Some(line_idx);

        let line_start = context.line_start(line_idx);
        let (line_num, line_text) = context.line_at(line_start);
        writeln!(f, "{line_num:>line_num_width$} │ {line_text}")?;

        for mark in marks {
            if context.position(mark.span.start()).line - 1 != line_idx {
                continue;
            }
            let (col, len) = visual_span(line_text, line_start, mark.span);
            write!(f, "{:>line_num_width$} │ {:col$}", "", "")?;
            for _ in 0..len {
                f.write_char(mark.glyph)?;
            }
            if let Some(label) = &mark.label {
                write!(f, " {label}")?;
            }
            writeln!(f)?;
        }
    }
    Ok(())
}

fn format_notes(
    error: &ParseError,
    line_num_width: usize,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    let indent = " ".repeat(line_num_width + 1);

    match &error.kind {
        ErrorKind::UnexpectedToken { expected, found } => {
            if let [single] = expected.as_slice()
                && let Some((note, help)) = unexpected_token_hint(single, found)
            {
                writeln!(f, "{indent}= note: {note}")?;
                writeln!(f, "{indent}= help: {help}")?;
            }
            // No fallback: header already states "expected X, found Y".
        }
        ErrorKind::InvalidSyntax {
            hint: Some(hint), ..
        } => {
            writeln!(f, "{indent}= help: {hint}")?;
        }
        ErrorKind::UnclosedDelimiter { delimiter, .. } => {
            let (_, close_tok) = delimiter_pair(*delimiter);
            writeln!(f, "{indent}= help: add closing {close_tok}")?;
        }
        ErrorKind::ChainedComparison { .. } => {
            writeln!(
                f,
                "{indent}= note: comparison operators cannot be chained in Nix"
            )?;
            writeln!(f, "{indent}= help: use parentheses: (a < b) && (b < c)")?;
        }
        ErrorKind::MissingToken { token, after } => {
            writeln!(f, "{indent}= note: {token} is required after {after}")?;
        }
        ErrorKind::InvalidSyntax { hint: None, .. } => {}
    }
    Ok(())
}

/// Character column and width of `span` within `line_text` (bytes -> chars).
fn visual_span(line_text: &str, line_start: usize, span: Span) -> (usize, usize) {
    let byte_col = span.start().saturating_sub(line_start);
    let byte_len = span.len().max(1);

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
pub(super) fn unexpected_token_hint(
    expected: &str,
    found: &str,
) -> Option<(&'static str, &'static str)> {
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
        _ => return None,
    })
}

/// The lexer encodes the `''` opener as a single `'`; expand it back so we
/// don't render `'''`.
pub(super) const fn delimiter_pair(opening: char) -> (&'static str, &'static str) {
    match opening {
        '{' => ("'{'", "'}'"),
        '[' => ("'['", "']'"),
        '(' => ("'('", "')'"),
        '\'' => ("''", "''"),
        '"' => ("'\"'", "'\"'"),
        _ => ("delimiter", "delimiter"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn render_str(src: &str) -> String {
        let err = parse(src).unwrap_err();
        let ctx = ErrorContext::new(src, Some("t.nix"));
        format!("{}", render(&ctx, &err))
    }

    #[test]
    fn unclosed_delimiter_shows_opener_in_snippet() {
        let out = render_str("(1 + 2");
        assert!(out.contains("t.nix:1:7"), "1-based column: {out}");
        assert!(
            out.contains("- unclosed delimiter opened here"),
            "secondary mark: {out}"
        );
    }

    #[test]
    fn unclosed_opener_on_other_line_is_rendered() {
        let out = render_str("{\n  x = 1;\n");
        assert!(out.contains("1 │ {"), "opener line shown: {out}");
        assert!(out.contains("opened here"), "{out}");
    }

    #[test]
    fn indented_string_delimiter_rendered_as_double_quote() {
        let out = render_str("''\nhello\n");
        assert!(out.contains("unclosed indented string"), "{out}");
        assert!(!out.contains("'''"), "must not render triple quote: {out}");
    }
}
