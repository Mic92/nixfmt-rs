//! `--message-format=json` rendering for the CLI.
//!
//! One JSON object per line on stderr. Shape mirrors an LSP `Diagnostic` so a
//! thin wrapper (or nil/nixd shelling out) can forward it directly:
//!
//! ```text
//! {
//!   "file": "default.nix",
//!   "severity": "error",
//!   "code": "E002",
//!   "message": "unclosed delimiter '{'",
//!   "range": {"start":{"line":0,"character":0},"end":{"line":0,"character":1}},
//!   "byteRange": {"start":0,"end":1},
//!   "help": "add closing '}'",
//!   "relatedInformation": [{"message":"...","range":{...},"byteRange":{...}}],
//!   "rendered": "Error[E002]: ...\n..."
//! }
//! ```
//!
//! `range` is 0-based line / character (Unicode scalars from line start);
//! `byteRange` gives raw byte offsets for consumers that need exact addressing.

use std::fmt::Write;
use std::ops::Range;

use nixfmt_rs::ParseError;

pub fn parse_error(source: &str, file: &str, error: &ParseError) -> String {
    let lines = LineIndex::new(source);
    let mut o = Obj::new();

    o.str("file", file);
    o.str("severity", "error");
    if let Some(code) = error.code() {
        o.str("code", code);
    }
    o.str("message", &error.message());

    let span = error.byte_range();
    o.raw("range", &lines.range(&span));
    o.raw("byteRange", &byte_range(&span));

    if let Some(help) = error.help() {
        o.str("help", &help);
    }

    let related = error.related();
    if !related.is_empty() {
        let mut arr = String::from("[");
        for (i, (span, msg)) in related.iter().enumerate() {
            if i > 0 {
                arr.push(',');
            }
            let mut r = Obj::new();
            r.str("message", msg);
            r.raw("range", &lines.range(span));
            r.raw("byteRange", &byte_range(span));
            arr.push_str(&r.finish());
        }
        arr.push(']');
        o.raw("relatedInformation", &arr);
    }

    o.str(
        "rendered",
        &nixfmt_rs::format_error(source, Some(file), error),
    );
    o.finish()
}

pub fn message(file: Option<&str>, severity: &str, msg: &str) -> String {
    let mut o = Obj::new();
    if let Some(f) = file {
        o.str("file", f);
    }
    o.str("severity", severity);
    o.str("message", msg);
    o.finish()
}

/// Byte → (0-based line, 0-based char column) lookup.
struct LineIndex<'a> {
    source: &'a str,
    starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    fn new(source: &'a str) -> Self {
        let starts = std::iter::once(0)
            .chain(source.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        Self { source, starts }
    }

    fn position(&self, offset: usize) -> (usize, usize) {
        let offset = offset.min(self.source.len());
        let line = match self.starts.binary_search(&offset) {
            Ok(i) => i,
            Err(i) => i.saturating_sub(1),
        };
        let start = self.starts[line];
        let col = self.source[start..offset].chars().count();
        (line, col)
    }
}

impl LineIndex<'_> {
    fn range(&self, span: &Range<usize>) -> String {
        let (sl, sc) = self.position(span.start);
        let (el, ec) = self.position(span.end);
        format!(
            "{{\"start\":{{\"line\":{sl},\"character\":{sc}}},\
             \"end\":{{\"line\":{el},\"character\":{ec}}}}}"
        )
    }
}

fn byte_range(span: &Range<usize>) -> String {
    format!("{{\"start\":{},\"end\":{}}}", span.start, span.end)
}

/// Tiny JSON object builder; avoids a `serde` dep for a debug stream.
struct Obj {
    buf: String,
}

impl Obj {
    fn new() -> Self {
        Self {
            buf: String::from("{"),
        }
    }
    fn key(&mut self, k: &str) {
        if self.buf.len() > 1 {
            self.buf.push(',');
        }
        self.buf.push('"');
        self.buf.push_str(k);
        self.buf.push_str("\":");
    }
    fn str(&mut self, k: &str, v: &str) {
        self.key(k);
        push_json_str(&mut self.buf, v);
    }
    fn raw(&mut self, k: &str, v: &str) {
        self.key(k);
        self.buf.push_str(v);
    }
    fn finish(mut self) -> String {
        self.buf.push('}');
        self.buf
    }
}

fn push_json_str(out: &mut String, s: &str) {
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => write!(out, "\\u{:04x}", c as u32).unwrap(),
            c => out.push(c),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(src: &str, name: &str) -> String {
        let err = nixfmt_rs::parse(src).unwrap_err();
        parse_error(src, name, &err)
    }

    #[test]
    fn unclosed_brace_has_related_and_help() {
        let json = render("{\n  x = 1;\n", "t.nix");
        assert!(json.starts_with('{') && json.ends_with('}'));
        assert!(json.contains(r#""file":"t.nix""#), "{json}");
        assert!(json.contains(r#""code":"E002""#), "{json}");
        assert!(json.contains(r#""help":"add closing '}'""#), "{json}");
        assert!(json.contains(r#""relatedInformation":["#), "{json}");
        assert!(json.contains(r#""start":{"line":2"#), "{json}");
        assert!(
            json.contains(r#""byteRange":{"start":0,"end":1}"#),
            "{json}"
        );
    }

    #[test]
    fn unexpected_token_carries_help() {
        let json = render("let x = 1\ny = 2; in x", "t.nix");
        assert!(json.contains(r#""code":"E001""#), "{json}");
        assert!(
            json.contains(r#""help":"add a semicolon at the end of the previous line""#),
            "{json}"
        );
    }

    #[test]
    fn rendered_field_is_escaped_single_line() {
        let json = render("(1 + 2", "t.nix");
        assert!(json.contains(r#""rendered":""#));
        assert!(json.contains("\\n"), "{json}");
        assert!(!json.contains('\n'), "{json}");
    }

    #[test]
    fn json_escaping_handles_quotes_and_controls() {
        let mut s = String::new();
        push_json_str(&mut s, "a\"b\\c\n\u{1}");
        assert_eq!(s, r#""a\"b\\c\n\u0001""#);
    }

    #[test]
    fn message_only_object_is_minimal() {
        assert_eq!(
            message(Some("a.nix"), "warning", "not formatted"),
            r#"{"file":"a.nix","severity":"warning","message":"not formatted"}"#
        );
        assert_eq!(
            message(None, "error", "boom"),
            r#"{"severity":"error","message":"boom"}"#
        );
    }
}
