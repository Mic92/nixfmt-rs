use super::trivia::Trivia;

/// Items with interleaved comments (for lists, sets, let bindings)
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item<T> {
    /// An actual item
    Item(T),
    /// Trivia interleaved in items
    Comments(Trivia),
}

/// Items wrapper (newtype)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Items<T>(pub Vec<Item<T>>);

impl<T> Items<T> {
    /// Haskell `hasOnlyComments` (Pretty.hs): non-empty `Items` containing only comment items.
    pub fn has_only_comments(&self) -> bool {
        !self.0.is_empty() && self.0.iter().all(|i| matches!(i, Item::Comments(_)))
    }
}
