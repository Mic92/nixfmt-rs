use crate::predoc::{Doc, Pretty};
use crate::types::Leaf;

/// Whether a set/absorbed term should prefer its expanded (multi-line) layout.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Width {
    Regular,
    Wide,
}

pub(super) fn is_spaces(s: &str) -> bool {
    s.chars().all(char::is_whitespace)
}

/// Render an empty bracketed container (`[]`, `{}`), preserving a user-inserted
/// line break between the delimiters. Shared by empty list / set / param-set.
pub(super) fn push_empty_brackets(doc: &mut Doc, open: &Leaf, close: &Leaf) {
    open.pretty(doc);
    if open.span.start_line() == close.span.start_line() {
        doc.hardspace();
    } else {
        doc.hardline();
    }
    close.pretty(doc);
}
