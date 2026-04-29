//! Error context for formatting errors with source snippets

/// Context needed to format errors with source snippets
pub struct ErrorContext<'a> {
    /// The source code
    pub source: &'a str,

    /// Optional filename for display
    pub filename: Option<&'a str>,

    /// Byte offsets of line starts (computed once, shared)
    line_starts: Vec<usize>,
}

impl<'a> ErrorContext<'a> {
    /// Create context from source
    pub fn new(source: &'a str, filename: Option<&'a str>) -> Self {
        let line_starts = compute_line_starts(source);
        Self {
            source,
            filename,
            line_starts,
        }
    }

    /// Convert byte offset to (line, column)
    pub fn position(&self, offset: usize) -> Position {
        let line_idx = line_number(&self.line_starts, offset);
        let line_start = self.line_starts.get(line_idx).copied().unwrap_or(0);

        let column = column_number(self.source, line_start, offset);

        Position {
            line: line_idx + 1, // 1-based line numbers
            column,
        }
    }

    /// Get line containing offset
    pub fn line_at(&self, offset: usize) -> (usize, &str) {
        let line_idx = line_number(&self.line_starts, offset);
        let line_num = line_idx + 1; // 1-based

        let line_start = self.line_starts.get(line_idx).copied().unwrap_or(0);
        let line_end = self
            .line_starts
            .get(line_idx + 1)
            .copied()
            .unwrap_or(self.source.len());

        // Trim trailing newline if present
        let line_end = if line_end > line_start
            && self
                .source
                .as_bytes()
                .get(line_end - 1)
                .is_some_and(|&b| b == b'\n')
        {
            line_end - 1
        } else {
            line_end
        };

        let line_text = &self.source[line_start..line_end];

        (line_num, line_text)
    }

    /// Get the line start offset for a given line index
    pub fn line_start(&self, line_idx: usize) -> usize {
        self.line_starts.get(line_idx).copied().unwrap_or(0)
    }
}

/// Computed position (line, column)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: usize,   // 1-based
    pub column: usize, // 0-based byte offset from line start
}

/// Compute byte offsets of line starts
fn compute_line_starts(source: &str) -> Vec<usize> {
    std::iter::once(0)
        .chain(
            source.match_indices('\n').map(|(i, _)| i + 1), // Start of next line
        )
        .collect()
}

/// Binary search to find line number from byte offset
fn line_number(line_starts: &[usize], offset: usize) -> usize {
    match line_starts.binary_search(&offset) {
        Ok(idx) => idx,
        Err(idx) => idx.saturating_sub(1),
    }
}

/// Get column from byte offset (count UTF-8 characters)
fn column_number(source: &str, line_start: usize, offset: usize) -> usize {
    if offset <= line_start {
        return 0;
    }
    let end = offset.min(source.len());
    source[line_start..end].chars().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_position() {
        let source = "line1\nline2\nline3";
        let ctx = ErrorContext::new(source, None);

        let pos = ctx.position(0);
        assert_eq!(pos.line, 1);

        let pos = ctx.position(6);
        assert_eq!(pos.line, 2);
    }
}
