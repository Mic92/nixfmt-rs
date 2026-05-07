//! Intermediate representation and renderer
//!
//! Implements the Wadler/Leijen-style pretty-printing algorithm
//! from nixfmt's Predoc.hs

mod builder;
mod render;

pub use builder::{hardline, hardspace, line, linebreak, newline};
pub use render::RenderConfig;

/// Spacing types for layout
///
/// Sequential spacings are reduced to a single spacing by taking the maximum.
/// This means that e.g. a Space followed by an Emptyline results in just an Emptyline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Spacing {
    /// Line break or nothing (soft)
    Softbreak,
    /// Line break or nothing
    Break,
    /// Always a space
    Hardspace,
    /// Line break or space (soft)
    Softspace,
    /// Line break or space
    Space,
    /// Always a line break
    Hardline,
    /// Two line breaks (blank line)
    Emptyline,
    /// n line breaks
    Newlines(usize),
}

impl Spacing {
    /// Combine two adjacent spacings into one, taking the stronger (per the
    /// `Ord` impl) and accumulating `Newlines` counts.
    pub(super) fn merge(self, other: Self) -> Self {
        use Spacing::{Break, Emptyline, Hardspace, Newlines, Softbreak, Softspace, Space};

        let (lo, hi) = if self <= other {
            (self, other)
        } else {
            (other, self)
        };

        match (lo, hi) {
            (Break, Softspace | Hardspace) => Space,
            (Softbreak, Hardspace) => Softspace,
            (Newlines(x), Newlines(y)) => Newlines(x + y),
            (Emptyline, Newlines(x)) => Newlines(x + 2),
            (Hardspace, Newlines(x)) => Newlines(x),
            (_, Newlines(x)) => Newlines(x + 1),
            _ => hi,
        }
    }
}

/// Group annotation
///
/// Controls how groups are expanded during layout
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupKind {
    /// Regular group - expand if doesn't fit
    Regular,
    /// Priority group - try to keep compressed longer
    /// Used to compact things left and right of multiline elements
    Priority,
    /// Transparent group - handled by parent
    /// Priority children are associated with the parent's parent
    Transparent,
}

/// Text annotation
///
/// Controls how text contributes to line length calculations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextKind {
    /// Regular text
    Regular,
    /// Comment (doesn't count towards line length limits)
    Comment,
    /// Trailing comment (single-line comment at end of line)
    TrailingComment,
    /// Trailing text (only rendered in expanded groups)
    Trailing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Elem {
    /// (`nesting_depth`, offset, annotation, text)
    Text(usize, usize, TextKind, String),
    Spacing(Spacing),
    Group(GroupKind, Doc),
    /// Indentation delta marker (nest, offset). Emitted in begin/end pairs by
    /// [`Doc::nested`]/[`Doc::offset`] and folded into `Text` during `fixup`, so
    /// the renderer never sees it.
    Nest(isize, isize),
}

/// A document under construction.
///
/// Wraps a `Vec<Elem>` and exposes builder methods (`text`, `group`, `nested`,
/// …) defined in [`builder`]. The inner `Vec` is `pub(crate)` so the renderer
/// and fixup pass can perform in-place `Vec` surgery without going through the
/// builder API; everything outside `doc` should treat `Doc` as opaque and
/// use the methods.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Doc(pub(crate) Vec<Elem>);

impl Doc {
    pub const fn new() -> Self {
        Self(Vec::new())
    }
    /// Escape hatch for pushing a pre-built [`Elem`]. Prefer the typed
    /// builder methods (`text`, `hardline`, …) where one exists.
    pub fn push_raw(&mut self, e: Elem) -> &mut Self {
        self.0.push(e);
        self
    }
}

impl std::ops::Deref for Doc {
    type Target = [Elem];
    fn deref(&self) -> &[Elem] {
        &self.0
    }
}

impl IntoIterator for Doc {
    type Item = Elem;
    type IntoIter = std::vec::IntoIter<Elem>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl Extend<Elem> for Doc {
    fn extend<I: IntoIterator<Item = Elem>>(&mut self, iter: I) {
        self.0.extend(iter);
    }
}

impl From<Vec<Elem>> for Doc {
    fn from(v: Vec<Elem>) -> Self {
        Self(v)
    }
}

/// Opaque wrapper for intermediate representation (for debugging)
#[cfg(any(test, feature = "debug-dump"))]
#[derive(Debug)]
pub struct IR(pub(crate) Doc);

pub trait Emit {
    fn emit(&self, doc: &mut Doc);
}

impl Emit for Doc {
    fn emit(&self, doc: &mut Doc) {
        doc.0.extend_from_slice(&self.0);
    }
}

impl<T: Emit + ?Sized> Emit for &T {
    fn emit(&self, doc: &mut Doc) {
        (*self).emit(doc);
    }
}

impl<T: Emit> Emit for Option<T> {
    fn emit(&self, doc: &mut Doc) {
        if let Some(x) = self {
            x.emit(doc);
        }
    }
}

/// Display width of `s`. Haskell `textWidth = Text.length`, i.e. one column
/// per Unicode scalar; we match that so multi-byte UTF-8 (e.g. `«»`) doesn't
/// over-count and force spurious line breaks.
pub fn text_width(s: &str) -> usize {
    s.chars().count()
}

impl Doc {
    /// Force this doc to its compact (single-line) layout: soft spacings become
    /// hard spaces, breaks vanish, groups flatten. Returns `None` if the doc
    /// contains hard line breaks or would exceed `limit` columns.
    pub fn try_compact(&self, mut limit: Option<i32>) -> Option<Self> {
        let mut result = Vec::new();
        let mut stack: Vec<std::slice::Iter<'_, Elem>> = vec![self.iter()];
        while let Some(iter) = stack.last_mut() {
            let Some(elem) = iter.next() else {
                stack.pop();
                continue;
            };
            match elem {
                Elem::Text(_, _, _, t) => {
                    if let Some(n) = limit.as_mut() {
                        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                        {
                            *n -= text_width(t) as i32;
                        }
                    }
                    result.push(elem.clone());
                }
                Elem::Spacing(Spacing::Hardspace | Spacing::Space | Spacing::Softspace) => {
                    if let Some(n) = limit.as_mut() {
                        *n -= 1;
                    }
                    result.push(Elem::Spacing(Spacing::Hardspace));
                }
                Elem::Spacing(Spacing::Break | Spacing::Softbreak) => {}
                Elem::Spacing(_) => return None,
                Elem::Nest(..) => result.push(elem.clone()),
                Elem::Group(_, inner) => stack.push(inner.iter()),
            }
            if matches!(limit, Some(n) if n < 0) {
                return None;
            }
        }
        Some(Self(result))
    }
}

#[cfg(test)]
mod tests {
    use super::Spacing::{
        Break, Emptyline, Hardline, Hardspace, Newlines, Softbreak, Softspace, Space,
    };
    use super::{Doc, Elem, Spacing, TextKind};

    /// `merge` must be commutative; check both orders in one go.
    #[track_caller]
    fn merge_both(a: Spacing, b: Spacing, want: Spacing) {
        assert_eq!(a.merge(b), want, "{a:?}.merge({b:?})");
        assert_eq!(b.merge(a), want, "{b:?}.merge({a:?})");
    }

    #[test]
    fn merge_softbreak_hardspace_is_softspace() {
        merge_both(Softbreak, Hardspace, Softspace);
    }

    #[test]
    fn merge_break_hardspace_is_space() {
        merge_both(Break, Hardspace, Space);
        merge_both(Break, Softspace, Space);
    }

    #[test]
    fn merge_newlines_adds_counts() {
        merge_both(Newlines(2), Newlines(3), Newlines(5));
    }

    #[test]
    fn merge_emptyline_newlines_adds_two() {
        merge_both(Emptyline, Newlines(3), Newlines(5));
    }

    #[test]
    fn merge_hardspace_newlines_keeps_count() {
        // Hardspace contributes no extra line break, unlike the catch-all arm.
        merge_both(Hardspace, Newlines(3), Newlines(3));
    }

    #[test]
    fn merge_other_newlines_adds_one() {
        merge_both(Hardline, Newlines(3), Newlines(4));
        merge_both(Break, Newlines(1), Newlines(2));
    }

    #[test]
    fn merge_fallthrough_takes_stronger() {
        merge_both(Softbreak, Softbreak, Softbreak);
        merge_both(Hardspace, Hardline, Hardline);
    }

    fn text(s: &str) -> Elem {
        Elem::Text(0, 0, TextKind::Regular, s.into())
    }

    #[test]
    fn try_compact_respects_width_limit() {
        let doc = Doc(vec![text("abcde"), Elem::Spacing(Space), text("fgh")]);
        // 5 + 1 + 3 = 9 columns.
        assert!(doc.try_compact(Some(9)).is_some());
        assert!(doc.try_compact(Some(8)).is_none(), "must reject overflow");
        assert!(doc.try_compact(None).is_some());
    }

    #[test]
    fn try_compact_rejects_hard_breaks() {
        let doc = Doc(vec![text("a"), Elem::Spacing(Hardline), text("b")]);
        assert!(doc.try_compact(None).is_none());
    }
}
