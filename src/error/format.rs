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
    pub fn new(context: &'a ErrorContext<'a>) -> Self {
        Self { context }
    }

    /// Format a single error
    pub fn format(&self, error: &ParseError) -> String {
        let mut output = String::new();

        // Calculate line number width first (needed for alignment)
        let line_num_width = self.calculate_line_num_width(error);

        // Error header
        self.format_header(error, line_num_width, &mut output);

        // Source snippet with pointer
        self.format_snippet(error, line_num_width, &mut output);

        // Notes and hints
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

        // Show error code if available
        if let Some(code) = error.code() {
            write!(out, "[{}]", code).unwrap();
        }

        // Main message
        writeln!(out, ": {}", error.message()).unwrap();

        // File location - align the ┌─ with the │ from line numbers
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

        // Get context lines (1 before, error line, 1 after)
        let start_line = error_line_idx.saturating_sub(1);
        let end_line =
            (error_line_idx + 1).min(self.context.source.lines().count().saturating_sub(1));

        // Add empty separator line
        writeln!(out, "{:>width$} │", "", width = line_num_width).unwrap();

        // Show context lines
        for line_idx in start_line..=end_line {
            let line_num = line_idx + 1;

            // Get line bounds - need to use line_starts directly
            let line_start_offset = self.context.line_start(line_idx);

            // Convert line_idx to byte offset to use line_at
            let (actual_line_num, line_text) = self.context.line_at(line_start_offset);

            // Sanity check
            assert_eq!(line_num, actual_line_num);

            // Show the line
            writeln!(
                out,
                "{:>width$} │ {}",
                line_num,
                line_text,
                width = line_num_width
            )
            .unwrap();

            // Show pointer only on the error line
            if line_idx == error_line_idx {
                let error_col = error.span.start.saturating_sub(line_start_offset);
                let error_len = (error.span.end - error.span.start).max(1);

                // Calculate visual column (counting chars not bytes)
                let visual_col = line_text[..error_col.min(line_text.len())].chars().count();

                write!(out, "{:>width$} │ ", "", width = line_num_width).unwrap();

                // Spaces before pointer
                for _ in 0..visual_col {
                    out.push(' ');
                }

                // Pointer (calculate visual length in chars not bytes)
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
        // Show hints, suggestions, examples - align with the line numbers
        let indent = " ".repeat(line_num_width + 1);

        match &error.kind {
            ErrorKind::UnexpectedToken { expected, found } => {
                // Special handling for missing semicolons
                if expected.len() == 1 && expected[0] == "';'" {
                    writeln!(out, "{}= note: missing semicolon after definition", indent).unwrap();
                    writeln!(
                        out,
                        "{}= help: add a semicolon at the end of the previous line",
                        indent
                    )
                    .unwrap();
                }
                // Special handling for missing } in interpolation
                else if expected.len() == 1 && expected[0] == "'}'" {
                    writeln!(
                        out,
                        "{}= note: string interpolations must be closed with '}}'",
                        indent
                    )
                    .unwrap();
                    writeln!(out, "{}= help: add '}}' to close the interpolation", indent).unwrap();
                }
                // Special handling for if-then-else
                else if expected.len() == 1 && expected[0] == "'then'" {
                    writeln!(
                        out,
                        "{}= note: if expressions require: if <condition> then <expr> else <expr>",
                        indent
                    )
                    .unwrap();
                    writeln!(out, "{}= help: add 'then' after the condition", indent).unwrap();
                } else if expected.len() == 1 && expected[0] == "'else'" {
                    writeln!(
                        out,
                        "{}= note: if expressions require: if <condition> then <expr> else <expr>",
                        indent
                    )
                    .unwrap();
                    writeln!(
                        out,
                        "{}= help: add 'else' followed by the alternative expression",
                        indent
                    )
                    .unwrap();
                }
                // Special handling for let-in
                else if expected.len() == 1 && expected[0] == "'in'" {
                    writeln!(
                        out,
                        "{}= note: 'in' is required to complete the let expression",
                        indent
                    )
                    .unwrap();
                    writeln!(
                        out,
                        "{}= help: add 'in' followed by the expression body",
                        indent
                    )
                    .unwrap();
                } else if !expected.is_empty() {
                    // Show what was expected
                    let expected_str = if expected.len() == 1 {
                        expected[0].clone()
                    } else {
                        format!("one of {}", expected.join(", "))
                    };
                    writeln!(
                        out,
                        "{}= help: expected {}, but found {}",
                        indent, expected_str, found
                    )
                    .unwrap();
                }
            }
            ErrorKind::InvalidSyntax {
                hint: Some(hint), ..
            } => {
                writeln!(out, "{}= help: {}", indent, hint).unwrap();
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
                    "{}= note: comparison operators cannot be chained in Nix",
                    indent
                )
                .unwrap();
                writeln!(out, "{}= help: use parentheses: (a < b) && (b < c)", indent).unwrap();
            }
            ErrorKind::MissingToken { token, after } => {
                writeln!(
                    out,
                    "{}= note: {} is required after {}",
                    indent, token, after
                )
                .unwrap();
            }
            _ => {}
        }

        // Show secondary labels
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

    fn closing_delimiter(opening: char) -> char {
        match opening {
            '{' => '}',
            '[' => ']',
            '(' => ')',
            _ => opening,
        }
    }
}
