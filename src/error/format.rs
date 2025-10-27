//! Error formatting with source snippets

use crate::error::{ErrorKind, LabelStyle, ParseError};
use std::fmt::Write;

use super::context::ErrorContext;

/// Rich error formatter with source snippets
pub struct ErrorFormatter<'a> {
    context: &'a ErrorContext<'a>,
    #[allow(dead_code)]
    use_color: bool,
}

impl<'a> ErrorFormatter<'a> {
    pub fn new(context: &'a ErrorContext<'a>) -> Self {
        Self {
            context,
            use_color: false, // TODO: detect terminal support
        }
    }

    /// Format a single error
    pub fn format(&self, error: &ParseError) -> String {
        let mut output = String::new();

        // Error header
        self.format_header(error, &mut output);

        // Source snippet with pointer
        self.format_snippet(error, &mut output);

        // Notes and hints
        self.format_notes(error, &mut output);

        output
    }

    fn format_header(&self, error: &ParseError, out: &mut String) {
        let pos = self.context.position(error.span.start);

        write!(out, "Error").unwrap();

        // Show error code if available
        if let Some(code) = error.code() {
            write!(out, "[{}]", code).unwrap();
        }

        // Main message
        writeln!(out, ": {}", error.message()).unwrap();

        // File location
        if let Some(filename) = self.context.filename {
            writeln!(out, "  ┌─ {}:{}:{}", filename, pos.line, pos.column).unwrap();
        } else {
            writeln!(out, "  ┌─ line {}:{}", pos.line, pos.column).unwrap();
        }
    }

    fn format_snippet(&self, error: &ParseError, out: &mut String) {
        let pos = self.context.position(error.span.start);
        let (line_num, line_text) = self.context.line_at(error.span.start);

        // Calculate line number width for alignment
        let line_num_width = line_num.to_string().len().max(2);

        // Show the line
        writeln!(
            out,
            "{:>width$} │ {}",
            line_num,
            line_text,
            width = line_num_width
        )
        .unwrap();

        // Show pointer to error location
        let line_start = self.context.line_start(pos.line - 1);
        let error_col = error.span.start.saturating_sub(line_start);
        let error_len = (error.span.end - error.span.start).max(1);

        // Calculate visual column (counting chars not bytes)
        let visual_col = line_text[..error_col.min(line_text.len())]
            .chars()
            .count();

        write!(
            out,
            "{:>width$} │ ",
            "",
            width = line_num_width
        )
        .unwrap();

        // Spaces before pointer
        for _ in 0..visual_col {
            out.push(' ');
        }

        // Pointer
        for _ in 0..error_len.min(line_text.len() - error_col).max(1) {
            out.push('^');
        }

        writeln!(out).unwrap();
    }

    fn format_notes(&self, error: &ParseError, out: &mut String) {
        // Show hints, suggestions, examples
        match &error.kind {
            ErrorKind::UnknownIdentifier { suggestions, .. } => {
                if !suggestions.is_empty() {
                    writeln!(out, "  = help: did you mean '{}'?", suggestions[0]).unwrap();
                }
            }
            ErrorKind::InvalidSyntax { hint: Some(hint), .. } => {
                writeln!(out, "  = help: {}", hint).unwrap();
            }
            ErrorKind::UnclosedDelimiter { opening_span, .. } => {
                let open_pos = self.context.position(opening_span.start);
                writeln!(
                    out,
                    "  = note: delimiter opened at line {}:{}",
                    open_pos.line, open_pos.column
                )
                .unwrap();
            }
            ErrorKind::ChainedComparison { .. } => {
                writeln!(
                    out,
                    "  = help: use parentheses to clarify: (a < b) && (b < c)"
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
                "  = {}: {} at line {}:{}",
                prefix, label.message, pos.line, pos.column
            )
            .unwrap();
        }
    }
}
