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
/// builder API; everything outside `predoc` should treat `Doc` as opaque and
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

pub trait Pretty {
    fn pretty(&self, doc: &mut Doc);
}

impl Pretty for Doc {
    fn pretty(&self, doc: &mut Doc) {
        doc.0.extend_from_slice(&self.0);
    }
}

impl<T: Pretty> Pretty for Option<T> {
    fn pretty(&self, doc: &mut Doc) {
        if let Some(x) = self {
            x.pretty(doc);
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
