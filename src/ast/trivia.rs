//! Comments and whitespace attached to tokens.

/// A single trivia element.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TriviaPiece {
    EmptyLine,
    LineComment(Box<str>),
    /// `BlockComment(is_doc`, lines)
    /// `is_doc` = true for /** */ comments
    BlockComment(bool, Box<[Box<str>]>),
    LanguageAnnotation(Box<str>),
}

/// Wrapper around a list of trivia items (comments/whitespace).
///
/// Stored as a boxed slice behind an `Option` so the overwhelmingly common
/// empty case is two zero words and never allocates: every `Annotated<T>` carries
/// one of these, and the parser moves `Annotated` values by value through every
/// production, so the 24→16 byte saving compounds across the whole AST.
/// Trivia runs are built once at lexeme boundaries and then read-only, so a
/// frozen slice (single allocation) fits better than a growable `Vec`.
#[derive(Debug, Clone, Default)]
pub struct Trivia(Option<Box<[TriviaPiece]>>);

impl Trivia {
    /// Empty trivia list (no allocation).
    #[inline]
    pub const fn new() -> Self {
        Self(None)
    }

    /// Single-element trivia list.
    pub fn one(t: TriviaPiece) -> Self {
        Self(Some(Box::new([t])))
    }

    /// Append a trivium.
    ///
    /// Reallocates the backing slice; callers on hot paths should accumulate
    /// into a `Vec<TriviaPiece>` and convert once. Existing call sites only hit
    /// this on comment-bearing tokens, which are rare.
    pub fn push(&mut self, t: TriviaPiece) {
        let mut v: Vec<TriviaPiece> = std::mem::take(self).into();
        v.push(t);
        *self = v.into();
    }

    /// Insert at `idx`. Same reallocation caveat as [`Self::push`].
    pub fn insert(&mut self, idx: usize, t: TriviaPiece) {
        let mut v: Vec<TriviaPiece> = std::mem::take(self).into();
        v.insert(idx, t);
        *self = v.into();
    }

    /// Append all items from `iter`, allocating only if it yields any.
    pub fn extend<I: IntoIterator<Item = TriviaPiece>>(&mut self, iter: I) {
        let mut iter = iter.into_iter();
        if let Some(first) = iter.next() {
            let mut v: Vec<TriviaPiece> = std::mem::take(self).into();
            v.push(first);
            v.extend(iter);
            *self = v.into();
        }
    }

    /// Drop all items, retaining no allocation.
    #[inline]
    pub fn clear(&mut self) {
        self.0 = None;
    }
}

impl PartialEq for Trivia {
    fn eq(&self, other: &Self) -> bool {
        // `None` and `Some(empty)` are observationally identical.
        self[..] == other[..]
    }
}
impl Eq for Trivia {}

impl std::ops::Deref for Trivia {
    type Target = [TriviaPiece];

    #[inline]
    fn deref(&self) -> &Self::Target {
        match &self.0 {
            Some(v) => v,
            None => &[],
        }
    }
}

impl From<Vec<TriviaPiece>> for Trivia {
    fn from(value: Vec<TriviaPiece>) -> Self {
        if value.is_empty() {
            Self(None)
        } else {
            Self(Some(value.into_boxed_slice()))
        }
    }
}

impl From<Trivia> for Vec<TriviaPiece> {
    fn from(val: Trivia) -> Self {
        val.0.map(Self::from).unwrap_or_default()
    }
}

impl IntoIterator for Trivia {
    type Item = TriviaPiece;
    type IntoIter = std::vec::IntoIter<TriviaPiece>;

    fn into_iter(self) -> Self::IntoIter {
        Vec::from(self).into_iter()
    }
}

impl<'a> IntoIterator for &'a Trivia {
    type Item = &'a TriviaPiece;
    type IntoIter = std::slice::Iter<'a, TriviaPiece>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Trailing comment on the same line as a token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrailingComment(pub Box<str>);

/// Haskell `convertTrailing`.
impl From<&TrailingComment> for TriviaPiece {
    fn from(tc: &TrailingComment) -> Self {
        Self::LineComment(format!(" {}", tc.0).into_boxed_str())
    }
}
