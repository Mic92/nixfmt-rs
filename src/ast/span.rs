/// Identifier / number text. `CompactString` stores up to 24 bytes inline,
/// which covers virtually every Nix identifier and literal, eliminating the
/// per-token heap allocation that previously dominated parser drop time.
pub type TokenText = compact_str::CompactString;

/// A byte offset range in the source with line information.
///
/// Stored as `u32` so a `Span` is 16 bytes instead of 32; every AST leaf
/// carries one, and the parser moves leaves by value constantly, so the
/// halved width measurably reduces `memmove` traffic. Nix source files are
/// far below 4 GiB, so the narrower offsets are not a practical limitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Span {
    start: u32,      // byte offset
    end: u32,        // byte offset
    start_line: u32, // line number (1-indexed)
    end_line: u32,   // line number (1-indexed)
}

impl Span {
    /// Create a span from byte offsets, with line numbers defaulting to 1.
    #[allow(clippy::cast_possible_truncation)] // source files are < 4 GiB
    pub const fn new(start: usize, end: usize) -> Self {
        Self {
            start: start as u32,
            end: end as u32,
            start_line: 1,
            end_line: 1,
        }
    }

    /// Create a new span with line information
    #[allow(clippy::cast_possible_truncation)]
    pub const fn with_lines(start: usize, end: usize, start_line: usize, end_line: usize) -> Self {
        Self {
            start: start as u32,
            end: end as u32,
            start_line: start_line as u32,
            end_line: end_line as u32,
        }
    }

    /// Create a zero-length span at the given offset
    #[allow(clippy::cast_possible_truncation)]
    pub const fn point(offset: usize) -> Self {
        Self {
            start: offset as u32,
            end: offset as u32,
            start_line: 1,
            end_line: 1,
        }
    }

    /// Start byte offset.
    #[inline]
    pub const fn start(self) -> usize {
        self.start as usize
    }

    /// End byte offset (exclusive).
    #[inline]
    pub const fn end(self) -> usize {
        self.end as usize
    }

    /// Line number of the start offset (1-indexed).
    #[inline]
    pub const fn start_line(self) -> usize {
        self.start_line as usize
    }

    /// Line number of the end offset (1-indexed).
    #[inline]
    pub const fn end_line(self) -> usize {
        self.end_line as usize
    }

    /// Length in bytes.
    #[inline]
    pub const fn len(self) -> usize {
        (self.end - self.start) as usize
    }

    /// True iff the span covers zero bytes.
    #[inline]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    /// Byte range, suitable for slicing the source: `source[span.range()]`.
    #[inline]
    pub const fn range(self) -> std::ops::Range<usize> {
        self.start as usize..self.end as usize
    }
}
