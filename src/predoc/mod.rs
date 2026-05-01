//! Intermediate representation and renderer
//!
//! Implements the Wadler/Leijen-style pretty-printing algorithm
//! from nixfmt's Predoc.hs

mod builder;
mod render;

pub use builder::{
    emptyline, hardline, hardspace, line, line_prime, newline, push_comment, push_group,
    push_group_ann, push_hcat, push_nested, push_offset, push_sep_by, push_surrounded, push_text,
    push_trailing, push_trailing_comment, softline, softline_prime,
};
pub use render::{RenderConfig, fixup, render_with_config};

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
pub enum GroupAnn {
    /// Regular group - expand if doesn't fit
    RegularG,
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
pub enum TextAnn {
    /// Regular text
    RegularT,
    /// Comment (doesn't count towards line length limits)
    Comment,
    /// Trailing comment (single-line comment at end of line)
    TrailingComment,
    /// Trailing text (only rendered in expanded groups)
    Trailing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocE {
    /// (`nesting_depth`, offset, annotation, text)
    Text(usize, usize, TextAnn, String),
    Spacing(Spacing),
    Group(GroupAnn, Doc),
    /// Indentation delta marker (nest, offset). Emitted in begin/end pairs by
    /// `push_nested`/`push_offset` and folded into `Text` during `fixup`, so
    /// the renderer never sees it.
    Nest(isize, isize),
}

pub type Doc = Vec<DocE>;

/// Opaque wrapper for intermediate representation (for debugging)
#[derive(Debug)]
pub struct IR(pub(crate) Doc);

pub trait Pretty {
    fn pretty(&self, doc: &mut Doc);
}

impl Pretty for Doc {
    fn pretty(&self, doc: &mut Doc) {
        doc.extend(self.iter().cloned());
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

/// Manually force a doc to its compact layout, replacing all soft whitespace.
/// Recurses into inner groups (flattening them). Returns `None` if the doc
/// contains hard line breaks or exceeds the optional width limit.
/// Mirrors Haskell `unexpandSpacing'` (Predoc.hs).
pub fn unexpand_spacing_prime(mut limit: Option<i32>, doc: &[DocE]) -> Option<Doc> {
    let mut result = Vec::new();
    let mut stack: Vec<std::slice::Iter<'_, DocE>> = vec![doc.iter()];
    while let Some(iter) = stack.last_mut() {
        let Some(elem) = iter.next() else {
            stack.pop();
            continue;
        };
        match elem {
            DocE::Text(_, _, _, t) => {
                if let Some(n) = limit.as_mut() {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
                    {
                        *n -= text_width(t) as i32;
                    }
                }
                result.push(elem.clone());
            }
            DocE::Spacing(Spacing::Hardspace | Spacing::Space | Spacing::Softspace) => {
                if let Some(n) = limit.as_mut() {
                    *n -= 1;
                }
                result.push(DocE::Spacing(Spacing::Hardspace));
            }
            DocE::Spacing(Spacing::Break | Spacing::Softbreak) => {}
            DocE::Spacing(_) => return None,
            DocE::Nest(..) => result.push(elem.clone()),
            DocE::Group(_, inner) => stack.push(inner.iter()),
        }
        if matches!(limit, Some(n) if n < 0) {
            return None;
        }
    }
    Some(result)
}
