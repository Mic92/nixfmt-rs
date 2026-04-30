//! Error formatting with source snippets

use crate::error::{ErrorKind, LabelStyle, ParseError};
use std::fmt::Write;

use super::context::ErrorContext;

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

        let line_num_width = self.calculate_line_num_width(error);
        self.format_header(error, line_num_width, &mut output);
        self.format_snippet(error, line_num_width, &mut output);
        self.format_notes(error, line_num_width, &mut output);

        output
    }

    fn calculate_line_num_width(&self, error: &ParseError) -> usize {
        let pos = self.context.position(error.span.start);
        let error_line_idx = pos.line - 1;
        let end_line =
            (error_line_idx + 1).min(self.context.source.lines().count().saturating_sub(1));
        let max_line_num = end_line + 1;
        max_line_num.to_string().len().max(2)
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
        if let Some(filename) = self.context.filename {
            writeln!(out, "┌─ {}:{}:{}", filename, pos.line, pos.column).unwrap();
        } else {
            writeln!(out, "┌─ line {}:{}", pos.line, pos.column).unwrap();
        }
    }

    fn format_snippet(&self, error: &ParseError, line_num_width: usize, out: &mut String) {
        let pos = self.context.position(error.span.start);
        let error_line_idx = pos.line - 1; // Convert to 0-based

        // Context window: 1 line before, error line, 1 after.
        let start_line = error_line_idx.saturating_sub(1);
        let end_line =
            (error_line_idx + 1).min(self.context.source.lines().count().saturating_sub(1));

        writeln!(out, "{:>width$} │", "", width = line_num_width).unwrap();

        for line_idx in start_line..=end_line {
            let line_num = line_idx + 1;

            let line_start_offset = self.context.line_start(line_idx);
            let (actual_line_num, line_text) = self.context.line_at(line_start_offset);
            assert_eq!(line_num, actual_line_num);

            writeln!(out, "{line_num:>line_num_width$} │ {line_text}").unwrap();

            if line_idx == error_line_idx {
                let error_col = (error.span.start as usize).saturating_sub(line_start_offset);
                let error_len = (error.span.end - error.span.start).max(1) as usize;

                // Visual column in chars, not bytes.
                let visual_col = line_text[..error_col.min(line_text.len())].chars().count();

                write!(out, "{:>width$} │ ", "", width = line_num_width).unwrap();

                for _ in 0..visual_col {
                    out.push(' ');
                }

                // Visual length in chars, not bytes.
                let remaining_text = &line_text[error_col.min(line_text.len())..];
                let visual_len = remaining_text.chars().take(error_len).count().max(1);

                for _ in 0..visual_len {
                    out.push('^');
                }

                writeln!(out).unwrap();
            }
        }
    }

    fn format_notes(&self, error: &ParseError, line_num_width: usize, out: &mut String) {
        let indent = " ".repeat(line_num_width + 1);

        match &error.kind {
            ErrorKind::UnexpectedToken { expected, found } => {
                if expected.len() == 1 && expected[0] == "';'" {
                    writeln!(out, "{indent}= note: missing semicolon after definition").unwrap();
                    writeln!(
                        out,
                        "{indent}= help: add a semicolon at the end of the previous line"
                    )
                    .unwrap();
                } else if expected.len() == 1 && expected[0] == "'}'" {
                    writeln!(
                        out,
                        "{indent}= note: string interpolations must be closed with '}}'"
                    )
                    .unwrap();
                    writeln!(out, "{indent}= help: add '}}' to close the interpolation").unwrap();
                } else if expected.len() == 1 && expected[0] == "'then'" {
                    writeln!(
                        out,
                        "{indent}= note: if expressions require: if <condition> then <expr> else <expr>"
                    )
                    .unwrap();
                    writeln!(out, "{indent}= help: add 'then' after the condition").unwrap();
                } else if expected.len() == 1 && expected[0] == "'else'" {
                    writeln!(
                        out,
                        "{indent}= note: if expressions require: if <condition> then <expr> else <expr>"
                    )
                    .unwrap();
                    writeln!(
                        out,
                        "{indent}= help: add 'else' followed by the alternative expression"
                    )
                    .unwrap();
                } else if expected.len() == 1 && expected[0] == "'in'" {
                    writeln!(
                        out,
                        "{indent}= note: 'in' is required to complete the let expression"
                    )
                    .unwrap();
                    writeln!(
                        out,
                        "{indent}= help: add 'in' followed by the expression body"
                    )
                    .unwrap();
                } else if !expected.is_empty() {
                    let expected_str = if expected.len() == 1 {
                        expected[0].clone()
                    } else {
                        format!("one of {}", expected.join(", "))
                    };
                    writeln!(
                        out,
                        "{indent}= help: expected {expected_str}, but found {found}"
                    )
                    .unwrap();
                }
            }
            ErrorKind::InvalidSyntax {
                hint: Some(hint), ..
            } => {
                writeln!(out, "{indent}= help: {hint}").unwrap();
            }
            ErrorKind::UnclosedDelimiter {
                delimiter,
                opening_span,
            } => {
                let open_pos = self.context.position(opening_span.start);
                writeln!(
                    out,
                    "{}= note: '{}' opened at line {}:{}",
                    indent, delimiter, open_pos.line, open_pos.column
                )
                .unwrap();
                writeln!(
                    out,
                    "{}= help: add closing '{}' to match the opening delimiter",
                    indent,
                    Self::closing_delimiter(*delimiter)
                )
                .unwrap();
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

        for label in &error.labels {
            let pos = self.context.position(label.span.start);
            let prefix = match label.style {
                LabelStyle::Primary => "error",
                LabelStyle::Secondary => "note",
                LabelStyle::Note => "help",
            };
            writeln!(
                out,
                "{}= {}: {} at line {}:{}",
                indent, prefix, label.message, pos.line, pos.column
            )
            .unwrap();
        }
    }

    const fn closing_delimiter(opening: char) -> char {
        match opening {
            '{' => '}',
            '[' => ']',
            '(' => ')',
            _ => opening,
        }
    }
}
