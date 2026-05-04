//! Byte-level cursor mechanics for [`Lexer`]: peeking, advancing, bulk
//! scanning, and the `mark`/`reset` snapshotting used for short-range
//! backtracking. Nothing here knows about tokens or trivia.

use super::{Lexer, LexerPos};

/// Update `line`/`column` to account for having advanced over `slice`.
/// Nix source is overwhelmingly ASCII, so the no-newline ASCII case is the
/// fast path; only count chars when non-ASCII bytes are present.
///
/// Free function rather than `&mut self` so callers may borrow
/// `self.source` for `slice` while mutating the two counters.
#[inline]
fn bump_line_col(line: &mut usize, column: &mut usize, slice: &str) {
    match memchr::memrchr(b'\n', slice.as_bytes()) {
        None => {
            *column += if slice.is_ascii() {
                slice.len()
            } else {
                slice.chars().count()
            };
        }
        Some(last_nl) => {
            *line += memchr::memchr_iter(b'\n', slice.as_bytes()).count();
            let tail = &slice[last_nl + 1..];
            *column = if tail.is_ascii() {
                tail.len()
            } else {
                tail.chars().count()
            };
        }
    }
}

impl Lexer {
    /// Remaining input from the cursor.
    #[inline]
    pub(super) fn rest(&self) -> &str {
        &self.source[self.byte_pos..]
    }

    /// Peek at current byte without consuming (None at EOF).
    #[inline]
    pub(crate) fn peek_byte(&self) -> Option<u8> {
        self.source.as_bytes().get(self.byte_pos).copied()
    }

    /// Peek at current character without consuming
    #[inline]
    pub(crate) fn peek(&self) -> Option<char> {
        let b = self.peek_byte()?;
        if b < 0x80 {
            Some(b as char)
        } else {
            self.rest().chars().next()
        }
    }

    /// Peek ahead n characters
    #[inline]
    pub(crate) fn peek_ahead(&self, n: usize) -> Option<char> {
        // `n` is at most 3 in practice, so a short char walk is fine.
        self.rest().chars().nth(n)
    }

    /// Check whether the upcoming input matches `s` byte-for-byte.
    /// Replaces open-coded `peek() == Some(a) && peek_ahead(1) == Some(b)` ladders.
    #[inline]
    pub(crate) fn at(&self, s: &str) -> bool {
        self.source.as_bytes()[self.byte_pos..].starts_with(s.as_bytes())
    }

    /// Advance `n` characters.
    #[inline]
    pub(crate) fn advance_by(&mut self, n: usize) {
        for _ in 0..n {
            self.advance();
        }
    }

    /// Snapshot the cursor (position only, no trivia).
    #[inline]
    pub(super) const fn mark(&self) -> LexerPos {
        LexerPos {
            byte_pos: self.byte_pos,
            line: self.line,
            column: self.column,
        }
    }

    /// Restore the cursor from a snapshot taken by `mark()`.
    #[inline]
    pub(super) const fn reset(&mut self, mark: LexerPos) {
        self.byte_pos = mark.byte_pos;
        self.line = mark.line;
        self.column = mark.column;
    }

    /// Run `f`; on `None`, rewind cursor (`byte_pos/line/column`) only.
    /// Does NOT restore `trivia_buffer`/`recent_*` — callers must not mutate
    /// those inside `f`.
    #[inline]
    pub(super) fn try_with_cursor<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Option<T>,
    ) -> Option<T> {
        let mark = self.mark();
        let r = f(self);
        if r.is_none() {
            self.reset(mark);
        }
        r
    }

    /// Consume and return current character
    #[inline]
    pub(crate) fn advance(&mut self) -> Option<char> {
        let b = self.peek_byte()?;
        if b < 0x80 {
            self.byte_pos += 1;
            if b == b'\n' {
                self.line += 1;
                self.column = 0;
            } else {
                self.column += 1;
            }
            Some(b as char)
        } else {
            let ch = self.rest().chars().next()?;
            self.byte_pos += ch.len_utf8();
            self.column += 1;
            Some(ch)
        }
    }

    /// Advance past the longest prefix containing none of the three given
    /// bytes and return it. Newlines inside the run update `line`/`column`.
    /// SIMD-accelerated via `memchr3`, used for string-body scanning.
    #[inline]
    pub(crate) fn scan_until3(&mut self, a: u8, b: u8, c: u8) -> &str {
        let rest = &self.source.as_bytes()[self.byte_pos..];
        let len = memchr::memchr3(a, b, c, rest).unwrap_or(rest.len());
        if len == 0 {
            return "";
        }
        let start = self.byte_pos;
        let end = start + len;
        self.byte_pos = end;
        bump_line_col(&mut self.line, &mut self.column, &self.source[start..end]);
        &self.source[start..end]
    }

    /// Move the cursor to absolute byte offset `target` (which must be on a
    /// char boundary and `>= self.byte_pos`), updating `line`/`column` from
    /// the skipped slice. Used after a `memchr` jump.
    pub(super) fn seek_to(&mut self, target: usize) {
        debug_assert!(target >= self.byte_pos);
        let start = self.byte_pos;
        self.byte_pos = target;
        bump_line_col(
            &mut self.line,
            &mut self.column,
            &self.source[start..target],
        );
    }

    /// Bulk-advance over the next `len` bytes of source, which must contain no
    /// `\n`. Updates `column` by the number of *chars* in that slice.
    /// Returns the consumed text. Used by string/comment scanners after a
    /// `memchr` hit so the per-char `advance()` loop is skipped for the run.
    #[inline]
    pub(super) fn advance_bytes_no_newline(&mut self, len: usize) -> &str {
        let start = self.byte_pos;
        let end = start + len;
        debug_assert!(!self.source.as_bytes()[start..end].contains(&b'\n'));
        self.byte_pos = end;
        bump_line_col(&mut self.line, &mut self.column, &self.source[start..end]);
        &self.source[start..end]
    }

    /// Consume the longest run of ASCII bytes satisfying `pred` and return it
    /// as a `&str` borrow into `self.source`. `pred` must never accept `b'\n'`
    /// (so `column` can be bumped by byte count without line tracking).
    #[inline]
    pub(super) fn take_ascii_while(&mut self, pred: impl Fn(u8) -> bool) -> &str {
        let bytes = self.source.as_bytes();
        let start = self.byte_pos;
        let mut i = start;
        while i < bytes.len() && pred(bytes[i]) {
            i += 1;
        }
        self.byte_pos = i;
        self.column += i - start;
        &self.source[start..i]
    }

    /// Check if we're at end of input
    #[inline]
    pub(super) fn is_eof(&self) -> bool {
        self.byte_pos >= self.source.len()
    }
}
