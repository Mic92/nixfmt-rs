//! Post-processing pass that splices unformatted regions back into the
//! formatted output when `/*nixfmt:disable*/` / `/*nixfmt:enable*/`
//! directives are present.
//!
//! Directive positions are reported by the lexer (for the original source)
//! and by the renderer (for the formatted output), so this module never
//! re-scans either text and never has to re-implement Nix lexing rules.
//!
//! Directives must be on their own line (only optional leading whitespace).
//! Directives inside strings are ignored; directives inside `${}`
//! interpolations are recognized (real Nix code context). An unclosed
//! `/*nixfmt:disable*/` extends to end of file.

/// `(0-based line, is_disable)` directive position, as reported by the lexer
/// or the renderer.
pub type Directive = (usize, bool);

/// Disable region: line index of `/*nixfmt:disable*/` and optionally the
/// matching `/*nixfmt:enable*/`. `None` means unclosed (extends to EOF).
type Region = (usize, Option<usize>);

/// Splice unformatted regions from `original` back into `formatted`, using
/// directive positions reported by the lexer and renderer. Returns
/// `formatted` unchanged if there are no directive regions.
pub fn apply_directives(
    original: &str,
    original_directives: &[Directive],
    formatted: &str,
    formatted_directives: &[Directive],
) -> String {
    let formatted_regions = collect_regions(formatted_directives);
    if formatted_regions.is_empty() {
        return formatted.to_owned();
    }
    let original_regions = collect_regions(original_directives);

    debug_assert_eq!(
        original_regions.len(),
        formatted_regions.len(),
        "directive region count diverged between original and formatted"
    );

    let original_lines: Vec<&str> = original.lines().collect();
    let formatted_lines: Vec<&str> = formatted.lines().collect();

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

fn bounds((start, end): Region, total: usize) -> (usize, usize) {
    (start, end.unwrap_or(total - 1))
}

/// Pair up `disable`/`enable` directives into regions. Nested disables and
/// lone enables are ignored. Directives are expected in source order.
fn collect_regions(directives: &[Directive]) -> Vec<Region> {
    let mut regions = Vec::new();
    let mut open: Option<usize> = None;
    for &(line, is_disable) in directives {
        match (is_disable, open) {
            (true, None) => open = Some(line),
            (false, Some(start)) => {
                regions.push((start, Some(line)));
                open = None;
            }
            // Nested disable or lone enable: ignore.
            (true, Some(_)) | (false, None) => {}
        }
    }
    if let Some(start) = open {
        regions.push((start, None));
    }
    regions
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_disable_enable() {
        let original = "{\n  a   =   1;\n/*nixfmt:disable*/\n  b    =    2;\n/*nixfmt:enable*/\n  c   =   3;\n}\n";
        let formatted =
            "{\n  a = 1;\n/*nixfmt:disable*/\n  b = 2;\n/*nixfmt:enable*/\n  c = 3;\n}\n";
        let dirs = [(2, true), (4, false)];
        let result = apply_directives(original, &dirs, formatted, &dirs);
        assert!(result.contains("b    =    2;"));
        assert!(result.contains("a = 1;"));
        assert!(result.contains("c = 3;"));
    }

    #[test]
    fn unclosed_disable() {
        let original = "{\n  a = 1;\n/*nixfmt:disable*/\n  b    =    2;\n  c    =    3;\n}\n";
        let formatted = "{\n  a = 1;\n/*nixfmt:disable*/\n  b = 2;\n  c = 3;\n}\n";
        let dirs = [(2, true)];
        let result = apply_directives(original, &dirs, formatted, &dirs);
        assert!(result.contains("b    =    2;"));
        assert!(result.contains("c    =    3;"));
    }

    #[test]
    fn no_directives_passthrough() {
        let text = "{ foo = 1; }\n";
        assert_eq!(apply_directives(text, &[], text, &[]), text);
    }

    #[test]
    fn nested_disable_ignored() {
        let dirs = [(0, true), (1, true), (3, false)];
        assert_eq!(collect_regions(&dirs), vec![(0, Some(3))]);
    }

    #[test]
    fn lone_enable_ignored() {
        assert!(collect_regions(&[(0, false)]).is_empty());
    }
}
