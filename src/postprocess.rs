//! Post-processing pass that splices unformatted regions back into the
//! formatted output when `/*nixfmt:disable*/` / `/*nixfmt:enable*/`
//! directives are present. Port of `Nixfmt.Postprocess`.
//!
//! Directives must be on their own line (only optional leading whitespace).
//! Directives inside strings are ignored; directives inside `${}`
//! interpolations are recognized (real Nix code context).
//! An unclosed `/*nixfmt:disable*/` extends to end of file.

/// Given the original source and the formatted output, find matching
/// directive pairs in both texts and replace the formatted regions with the
/// corresponding raw original text.
pub fn apply_directives(original: &str, formatted: &str) -> String {
    let formatted_lines: Vec<&str> = formatted.lines().collect();
    let formatted_regions = find_regions(&formatted_lines);
    if formatted_regions.is_empty() {
        return formatted.to_owned();
    }

    let original_lines: Vec<&str> = original.lines().collect();
    let original_regions = find_regions(&original_lines);

    debug_assert_eq!(
        original_regions.len(),
        formatted_regions.len(),
        "directive region count diverged between original and formatted"
    );

    let mut result = Vec::with_capacity(formatted_lines.len());
    let mut fmt_idx = 0;
    for (orig, fmt) in original_regions.iter().zip(&formatted_regions) {
        let (fmt_start, fmt_end) = bounds(*fmt, formatted_lines.len());
        let (orig_start, orig_end) = bounds(*orig, original_lines.len());
        result.extend_from_slice(&formatted_lines[fmt_idx..fmt_start]);
        result.extend_from_slice(&original_lines[orig_start..=orig_end]);
        fmt_idx = fmt_end + 1;
    }
    result.extend_from_slice(&formatted_lines[fmt_idx..]);

    let mut out = result.join("\n");
    if formatted.ends_with('\n') {
        out.push('\n');
    }
    out
}

/// Disable region: line index of `/*nixfmt:disable*/` and optionally the
/// matching `/*nixfmt:enable*/`. `None` means unclosed (extends to EOF).
type Region = (usize, Option<usize>);

fn bounds((start, end): Region, total: usize) -> (usize, usize) {
    (start, end.unwrap_or(total - 1))
}

/// Lexical context the scanner is currently inside.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Frame {
    Normal,
    String,
    MultiString,
    BlockComment,
}

fn find_regions(lines: &[&str]) -> Vec<Region> {
    let mut state = vec![Frame::Normal];
    let mut regions = Vec::new();
    let mut open: Option<usize> = None;

    for (idx, line) in lines.iter().enumerate() {
        if matches!(state.last(), Some(Frame::Normal) | None) {
            match line.trim() {
                "/*nixfmt:disable*/" if open.is_none() => open = Some(idx),
                "/*nixfmt:enable*/" => {
                    if let Some(start) = open.take() {
                        regions.push((start, Some(idx)));
                    }
                }
                _ => {}
            }
        }
        advance_state(&mut state, line.as_bytes());
    }

    if let Some(start) = open {
        regions.push((start, None));
    }
    regions
}

/// Advance the context stack across one line. All delimiters are ASCII, so
/// byte-wise scanning is safe; multi-byte UTF-8 (`>= 0x80`) never matches.
fn advance_state(state: &mut Vec<Frame>, line: &[u8]) {
    let mut i = 0;
    while i < line.len() {
        let two = (line[i], line.get(i + 1).copied());
        match state.last().copied().unwrap_or(Frame::Normal) {
            Frame::Normal => match two {
                (b'#', _) => break,
                (b'"', _) => {
                    state.push(Frame::String);
                    i += 1;
                }
                (b'\'', Some(b'\'')) => {
                    state.push(Frame::MultiString);
                    i += 2;
                }
                (b'/', Some(b'*')) => {
                    state.push(Frame::BlockComment);
                    i += 2;
                }
                (b'{', _) => {
                    state.push(Frame::Normal);
                    i += 1;
                }
                (b'}', _) => {
                    if state.len() > 1 {
                        state.pop();
                    }
                    i += 1;
                }
                _ => i += 1,
            },
            Frame::String => match two {
                (b'\\', Some(_)) => i += 2,
                (b'$', Some(b'{')) => {
                    state.push(Frame::Normal);
                    i += 2;
                }
                (b'"', _) => {
                    state.pop();
                    i += 1;
                }
                _ => i += 1,
            },
            Frame::MultiString => match two {
                (b'\'', Some(b'\'')) => match line.get(i + 2) {
                    Some(b'\'' | b'\\') => i += 3,
                    Some(b'$') if line.get(i + 3) == Some(&b'{') => i += 4,
                    _ => {
                        state.pop();
                        i += 2;
                    }
                },
                (b'$', Some(b'{')) => {
                    state.push(Frame::Normal);
                    i += 2;
                }
                _ => i += 1,
            },
            Frame::BlockComment => match two {
                (b'*', Some(b'/')) => {
                    state.pop();
                    i += 2;
                }
                _ => i += 1,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_disable_enable() {
        let original = "{\n  a   =   1;\n/*nixfmt:disable*/\n  b    =    2;\n/*nixfmt:enable*/\n  c   =   3;\n}\n";
        let formatted =
            "{\n  a = 1;\n/*nixfmt:disable*/\n  b = 2;\n/*nixfmt:enable*/\n  c = 3;\n}\n";
        let result = apply_directives(original, formatted);
        assert!(result.contains("b    =    2;"));
        assert!(result.contains("a = 1;"));
        assert!(result.contains("c = 3;"));
    }

    #[test]
    fn unclosed_disable() {
        let original = "{\n  a = 1;\n/*nixfmt:disable*/\n  b    =    2;\n  c    =    3;\n}\n";
        let formatted = "{\n  a = 1;\n/*nixfmt:disable*/\n  b = 2;\n  c = 3;\n}\n";
        let result = apply_directives(original, formatted);
        assert!(result.contains("b    =    2;"));
        assert!(result.contains("c    =    3;"));
    }

    #[test]
    fn no_directives_passthrough() {
        let text = "{ foo = 1; }\n";
        assert_eq!(apply_directives(text, text), text);
    }

    #[test]
    fn directive_in_string_ignored() {
        let lines = vec!["{", "  x = ''", "    /*nixfmt:disable*/", "  '';", "}"];
        assert!(find_regions(&lines).is_empty());
    }

    #[test]
    fn directive_in_interpolation_recognized() {
        let lines = vec![
            "  x = \"${",
            "/*nixfmt:disable*/",
            "    1 + 2",
            "/*nixfmt:enable*/",
            "  }\";",
        ];
        assert_eq!(find_regions(&lines), vec![(1, Some(3))]);
    }

    #[test]
    fn nested_disable_ignored() {
        let lines = vec![
            "/*nixfmt:disable*/",
            "/*nixfmt:disable*/",
            "x",
            "/*nixfmt:enable*/",
        ];
        assert_eq!(find_regions(&lines), vec![(0, Some(3))]);
    }

    #[test]
    fn lone_enable_ignored() {
        let lines = vec!["/*nixfmt:enable*/", "x"];
        assert!(find_regions(&lines).is_empty());
    }
}
