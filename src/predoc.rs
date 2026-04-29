//! Intermediate representation and renderer
//!
//! Implements the Wadler/Leijen-style pretty-printing algorithm
//! from nixfmt's Predoc.hs

/// Spacing types for layout
///
/// Sequential spacings are reduced to a single spacing by taking the maximum.
/// This means that e.g. a Space followed by an Emptyline results in just an Emptyline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Spacing {
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
pub(crate) enum GroupAnn {
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
pub(crate) enum TextAnn {
    /// Regular text
    RegularT,
    /// Comment (doesn't count towards line length limits)
    Comment,
    /// Trailing comment (single-line comment at end of line)
    TrailingComment,
    /// Trailing text (only rendered in expanded groups)
    Trailing,
}

/// Document element
///
/// Documents are represented as lists of these elements
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DocE {
    /// Text element
    /// (nesting_depth, offset, annotation, text)
    Text(usize, usize, TextAnn, String),
    /// Spacing element
    Spacing(Spacing),
    /// Group element
    /// Contains annotation and nested document
    Group(GroupAnn, Doc),
}

/// Document - a list of document elements
pub(crate) type Doc = Vec<DocE>;

/// Borrowed lookahead: a chain of slices scanned in order. Lets the layout
/// engine pass "rest of current level ++ outer lookahead" without cloning.
type Look<'a> = &'a [&'a [DocE]];

/// Flat iterator over a `Look` chain. Callers may `push_front` a group body to
/// get the `xs ++ ys` traversal Haskell gets for free from lazy lists.
struct LookIter<'a> {
    stack: Vec<(&'a [DocE], usize)>,
}

impl<'a> LookIter<'a> {
    fn new(chain: Look<'a>) -> Self {
        let mut stack: Vec<(&'a [DocE], usize)> = Vec::with_capacity(chain.len());
        for s in chain.iter().rev() {
            if !s.is_empty() {
                stack.push((s, 0));
            }
        }
        LookIter { stack }
    }

    fn push_front(&mut self, s: &'a [DocE]) {
        if !s.is_empty() {
            self.stack.push((s, 0));
        }
    }
}

impl<'a> Iterator for LookIter<'a> {
    type Item = &'a DocE;
    fn next(&mut self) -> Option<&'a DocE> {
        while let Some((s, i)) = self.stack.last_mut() {
            if *i < s.len() {
                let e = &s[*i];
                *i += 1;
                return Some(e);
            }
            self.stack.pop();
        }
        None
    }
}

/// Opaque wrapper for intermediate representation (for debugging)
#[derive(Debug)]
pub struct IR(pub(crate) Doc);

/// Pretty-printable trait
pub(crate) trait Pretty {
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

impl<T: Pretty, U: Pretty> Pretty for (T, U) {
    fn pretty(&self, doc: &mut Doc) {
        self.0.pretty(doc);
        self.1.pretty(doc);
    }
}

/// Push a text element
pub(crate) fn push_text(doc: &mut Doc, s: impl Into<String>) {
    let s = s.into();
    if !s.is_empty() {
        doc.push(DocE::Text(0, 0, TextAnn::RegularT, s));
    }
}

/// Push a comment element
pub(crate) fn push_comment(doc: &mut Doc, s: impl Into<String>) {
    let s = s.into();
    if !s.is_empty() {
        doc.push(DocE::Text(0, 0, TextAnn::Comment, s));
    }
}

/// Push a trailing comment element
pub(crate) fn push_trailing_comment(doc: &mut Doc, s: impl Into<String>) {
    let s = s.into();
    if !s.is_empty() {
        doc.push(DocE::Text(0, 0, TextAnn::TrailingComment, s));
    }
}

/// Push a trailing text element (only rendered in expanded groups)
pub(crate) fn push_trailing(doc: &mut Doc, s: impl Into<String>) {
    let s = s.into();
    if !s.is_empty() {
        doc.push(DocE::Text(0, 0, TextAnn::Trailing, s));
    }
}

/// Push a grouped document using a closure
pub(crate) fn push_group<F>(doc: &mut Doc, f: F)
where
    F: FnOnce(&mut Doc),
{
    let mut inner = Vec::new();
    f(&mut inner);
    doc.push(DocE::Group(GroupAnn::RegularG, inner));
}

/// Push a group with specific annotation using a closure
pub(crate) fn push_group_ann<F>(doc: &mut Doc, ann: GroupAnn, f: F)
where
    F: FnOnce(&mut Doc),
{
    let mut inner = Vec::new();
    f(&mut inner);
    doc.push(DocE::Group(ann, inner));
}

/// Push a nested document (increase indentation) using a closure
pub(crate) fn push_nested<F>(doc: &mut Doc, f: F)
where
    F: FnOnce(&mut Doc),
{
    let mut inner = Vec::new();
    f(&mut inner);

    for elem in inner {
        doc.push(match elem {
            DocE::Text(i, o, ann, t) => DocE::Text(i + 1, o, ann, t),
            DocE::Group(ann, inner) => DocE::Group(ann, nest_doc(inner)),
            DocE::Spacing(s) => DocE::Spacing(s),
        });
    }
}

/// Helper for nesting a Doc recursively
fn nest_doc(doc: Doc) -> Doc {
    doc.into_iter()
        .map(|elem| match elem {
            DocE::Text(i, o, ann, t) => DocE::Text(i + 1, o, ann, t),
            DocE::Group(ann, inner) => DocE::Group(ann, nest_doc(inner)),
            DocE::Spacing(s) => DocE::Spacing(s),
        })
        .collect()
}

/// Line break or nothing (soft)
pub(crate) fn softline_prime() -> DocE {
    DocE::Spacing(Spacing::Softbreak)
}

/// Line break or nothing
pub(crate) fn line_prime() -> DocE {
    DocE::Spacing(Spacing::Break)
}

/// Line break or space (soft)
pub(crate) fn softline() -> DocE {
    DocE::Spacing(Spacing::Softspace)
}

/// Line break or space
pub(crate) fn line() -> DocE {
    DocE::Spacing(Spacing::Space)
}

/// Always space
pub(crate) fn hardspace() -> DocE {
    DocE::Spacing(Spacing::Hardspace)
}

/// Always line break
pub(crate) fn hardline() -> DocE {
    DocE::Spacing(Spacing::Hardline)
}

/// Two line breaks (blank line)
pub(crate) fn emptyline() -> DocE {
    DocE::Spacing(Spacing::Emptyline)
}

/// n line breaks
pub(crate) fn newline() -> DocE {
    DocE::Spacing(Spacing::Newlines(1))
}

/// Push documents separated by a separator
pub(crate) fn push_sep_by<P: Pretty>(doc: &mut Doc, separator: &Doc, docs: Vec<P>) {
    let mut first = true;
    for item in docs {
        if !first {
            doc.extend(separator.iter().cloned());
        }
        first = false;
        item.pretty(doc);
    }
}

/// Push multiple documents horizontally without spacing
pub(crate) fn push_hcat<P: Pretty>(doc: &mut Doc, docs: Vec<P>) {
    for item in docs {
        item.pretty(doc);
    }
}

/// Push a document surrounded by the same elements on both sides using a closure
pub(crate) fn push_surrounded<F>(doc: &mut Doc, outside: &Doc, f: F)
where
    F: FnOnce(&mut Doc),
{
    doc.extend(outside.iter().cloned());
    f(doc);
    doc.extend(outside.iter().cloned());
}

/// Push a document with manual offset to all text elements using a closure
/// This is used for indented strings where we need to preserve the original indentation
pub(crate) fn push_offset<F>(doc: &mut Doc, level: usize, f: F)
where
    F: FnOnce(&mut Doc),
{
    let mut inner = Vec::new();
    f(&mut inner);

    for elem in inner {
        doc.push(match elem {
            DocE::Text(i, o, ann, t) => DocE::Text(i, o + level, ann, t),
            DocE::Group(ann, inner) => DocE::Group(ann, offset_doc(level, inner)),
            DocE::Spacing(s) => DocE::Spacing(s),
        });
    }
}

/// Helper for offsetting a Doc recursively
fn offset_doc(level: usize, doc: Doc) -> Doc {
    doc.into_iter()
        .map(|elem| match elem {
            DocE::Text(i, o, ann, t) => DocE::Text(i, o + level, ann, t),
            DocE::Group(ann, inner) => DocE::Group(ann, offset_doc(level, inner)),
            DocE::Spacing(s) => DocE::Spacing(s),
        })
        .collect()
}

// Renderer: Convert IR (Doc) to formatted text
//
// Implementation of the Wadler/Leijen layout algorithm from nixfmt

/// Configuration for rendering
pub(crate) struct RenderConfig {
    /// Maximum line width (default: 100)
    pub width: usize,
    /// Indentation width in spaces (default: 2)
    pub indent_width: usize,
}

impl Default for RenderConfig {
    fn default() -> Self {
        RenderConfig {
            width: 100,
            indent_width: 2,
        }
    }
}

/// Render a document with specific configuration
pub(crate) fn render_with_config(doc: Doc, config: &RenderConfig) -> String {
    layout_greedy(config.width, config.indent_width, doc)
}

/// Calculate text width (for i18n support, this would need patching)
fn text_width(s: &str) -> usize {
    s.len()
}

/// Check if element is hard spacing (always rendered as-is)
fn is_hard_spacing(elem: &DocE) -> bool {
    matches!(
        elem,
        DocE::Spacing(Spacing::Hardspace)
            | DocE::Spacing(Spacing::Hardline)
            | DocE::Spacing(Spacing::Emptyline)
            | DocE::Spacing(Spacing::Newlines(_))
    )
}

/// Check if element is a comment
fn is_comment(elem: &DocE) -> bool {
    match elem {
        DocE::Text(_, _, TextAnn::Comment, _) => true,
        DocE::Text(_, _, TextAnn::TrailingComment, _) => true,
        DocE::Group(_, inner) => inner.iter().all(|x| is_comment(x) || is_hard_spacing(x)),
        _ => false,
    }
}

/// Merge two spacing elements (take maximum in ordering)
fn merge_spacings(a: Spacing, b: Spacing) -> Spacing {
    use Spacing::*;

    let (min_sp, max_sp) = if a <= b { (a, b) } else { (b, a) };

    match (min_sp, max_sp) {
        (Break, Softspace) => Space,
        (Break, Hardspace) => Space,
        (Softbreak, Hardspace) => Softspace,
        (Newlines(x), Newlines(y)) => Newlines(x + y),
        (Emptyline, Newlines(x)) => Newlines(x + 2),
        (Hardspace, Newlines(x)) => Newlines(x),
        (_, Newlines(x)) => Newlines(x + 1),
        _ => max_sp,
    }
}

/// Manually force a doc to its compact layout, replacing all soft whitespace.
/// Recurses into inner groups (flattening them). Returns `None` if the doc
/// contains hard line breaks.
/// Mirrors Haskell `unexpandSpacing' Nothing` (Predoc.hs); the width-limit
/// variant is unused in this port.
pub(crate) fn unexpand_spacing_prime(doc: &[DocE]) -> Option<Doc> {
    let mut result = Vec::new();
    let mut stack: Vec<std::slice::Iter<'_, DocE>> = vec![doc.iter()];
    while let Some(iter) = stack.last_mut() {
        let Some(elem) = iter.next() else {
            stack.pop();
            continue;
        };
        match elem {
            DocE::Text(_, _, _, _) => result.push(elem.clone()),
            DocE::Spacing(Spacing::Hardspace)
            | DocE::Spacing(Spacing::Space)
            | DocE::Spacing(Spacing::Softspace) => {
                result.push(DocE::Spacing(Spacing::Hardspace));
            }
            DocE::Spacing(Spacing::Break) | DocE::Spacing(Spacing::Softbreak) => {}
            DocE::Spacing(_) => return None,
            DocE::Group(_, inner) => stack.push(inner.iter()),
        }
    }
    Some(result)
}

/// Manually force a group to compact layout (does not recurse into inner groups)
fn unexpand_spacing(doc: &Doc) -> Doc {
    let mut result = Vec::new();
    for elem in doc {
        match elem {
            DocE::Spacing(Spacing::Space) => result.push(DocE::Spacing(Spacing::Hardspace)),
            DocE::Spacing(Spacing::Softspace) => result.push(DocE::Spacing(Spacing::Hardspace)),
            DocE::Spacing(Spacing::Break) => {}
            DocE::Spacing(Spacing::Softbreak) => {}
            _ => result.push(elem.clone()),
        }
    }
    result
}

/// Split list into (prefix, trailing_suffix) where trailing_suffix
/// contains all elements at the end that satisfy `pred`.
fn span_end<T, F>(pred: F, mut list: Vec<T>) -> (Vec<T>, Vec<T>)
where
    F: Fn(&T) -> bool,
{
    let split_point = list
        .iter()
        .rposition(|item| !pred(item))
        .map(|i| i + 1)
        .unwrap_or(0);

    let post = list.split_off(split_point);
    (list, post)
}

/// Simplify groups with only one item
fn simplify_group(ann: GroupAnn, doc: Doc) -> Doc {
    if doc.is_empty() {
        return Vec::new();
    }
    if doc.len() == 1 && matches!(&doc[0], DocE::Group(inner_ann, _) if ann == *inner_ann) {
        match doc.into_iter().next() {
            Some(DocE::Group(_, body)) => body,
            _ => unreachable!(),
        }
    } else {
        doc
    }
}

/// Cheap pre-check so render_group can skip the clone-heavy `priority_groups`
/// machinery for groups that contain no Priority children.
fn has_priority_groups(doc: &[DocE]) -> bool {
    doc.iter().any(|e| match e {
        DocE::Group(GroupAnn::Priority, _) => true,
        DocE::Group(GroupAnn::Transparent, ys) => has_priority_groups(ys),
        _ => false,
    })
}

/// Fix up a Doc by:
/// - Moving hard spacings and comments out of groups
/// - Merging consecutive spacings
/// - Removing empty groups
pub(crate) fn fixup(doc: Doc) -> Doc {
    use std::collections::VecDeque;

    if doc.is_empty() {
        return Vec::new();
    }

    let mut doc: VecDeque<DocE> = doc.into();
    let mut result: Doc = Vec::with_capacity(doc.len());

    while let Some(elem) = doc.pop_front() {
        match elem {
            // Move/Merge hard spaces into groups so they can merge with a
            // leading soft spacing inside the group (Haskell `fixup` rule for
            // `Spacing Hardspace : Group ann xs`).
            DocE::Spacing(Spacing::Hardspace) if matches!(doc.front(), Some(DocE::Group(_, _))) => {
                if let Some(DocE::Group(_, xs)) = doc.front_mut() {
                    xs.insert(0, DocE::Spacing(Spacing::Hardspace));
                }
            }
            // Merge consecutive spacings
            DocE::Spacing(a) if !doc.is_empty() => {
                if let DocE::Spacing(b) = &doc[0] {
                    doc[0] = DocE::Spacing(merge_spacings(a, *b));
                } else {
                    result.push(DocE::Spacing(a));
                }
            }

            // Merge consecutive texts with same annotation
            DocE::Text(level, off, ann, a) if !doc.is_empty() => {
                if let DocE::Text(level2, off2, ann2, b) = &mut doc[0] {
                    if ann == *ann2 {
                        let mut merged = a;
                        merged.push_str(b);
                        *level2 = level;
                        *off2 = off;
                        *b = merged;
                    } else {
                        result.push(DocE::Text(level, off, ann, a));
                    }
                } else {
                    result.push(DocE::Text(level, off, ann, a));
                }
            }

            DocE::Group(ann, xs) => {
                let fixed_xs = fixup(xs);

                // Split out LEADING hard spacings and comments (not all of them!)
                let mut pre = Vec::new();
                let mut rest = fixed_xs;
                while let Some(first) = rest.first() {
                    if is_hard_spacing(first) || is_comment(first) {
                        pre.push(rest.remove(0));
                    } else {
                        break;
                    }
                }

                // Haskell `fixup (a@(Spacing _) : Group ann xs : ys)` keeps the
                // preceding spacing together with the lifted `pre` so they can
                // merge (e.g. `Space <> Hardline` -> `Hardline`). Mirror that by
                // pulling a just-emitted spacing back off `result`.
                if matches!(result.last(), Some(DocE::Spacing(_))) {
                    pre.insert(0, result.pop().unwrap());
                }

                if rest.is_empty() {
                    // Dissolve empty group: `fixup $ (a : pre) ++ post ++ ys`
                    for e in pre.into_iter().rev() {
                        doc.push_front(e);
                    }
                } else {
                    let (rest, post) = span_end(is_hard_spacing, rest);
                    let body = simplify_group(ann, rest);

                    if body.is_empty() {
                        for e in pre.into_iter().chain(post).rev() {
                            doc.push_front(e);
                        }
                    } else {
                        // `fixup (a : pre) ++ [Group ann body] ++ fixup (post ++ ys)`
                        result.extend(fixup(pre));
                        result.push(DocE::Group(ann, body));
                        for e in post.into_iter().rev() {
                            doc.push_front(e);
                        }
                    }
                }
            }

            _ => result.push(elem),
        }
    }

    result
}

/// Mirrors `fits` in Nixfmt/Predoc.hs.
///
/// `ni` is the next-line indentation delta used only by the trailing-comment
/// rule; `c` is the remaining width budget. Groups are flattened in place so
/// adjacent spacings across a group boundary merge exactly as in the Haskell
/// `ys ++ xs` recursion, and so comment text inside a group never gets
/// double-counted against `c`.
fn fits(mut ni: isize, mut c: isize, doc: &[DocE], out: &mut String) -> Option<usize> {
    let mark = out.len();
    if c < 0 {
        return None;
    }

    let mut stack: Vec<std::slice::Iter<'_, DocE>> = vec![doc.iter()];
    let mut pending: Option<Spacing> = None;

    loop {
        let elem = loop {
            let Some(it) = stack.last_mut() else {
                break None;
            };
            match it.next() {
                Some(DocE::Group(_, ys)) => stack.push(ys.iter()),
                Some(e) => break Some(e),
                None => {
                    stack.pop();
                }
            }
        };

        if let Some(DocE::Spacing(s)) = elem {
            pending = Some(match pending {
                Some(p) => merge_spacings(p, *s),
                None => *s,
            });
            continue;
        }

        if let Some(sp) = pending.take() {
            match sp {
                Spacing::Softbreak | Spacing::Break => {}
                Spacing::Softspace | Spacing::Space | Spacing::Hardspace => {
                    out.push(' ');
                    c -= 1;
                    ni -= 1;
                    if c < 0 {
                        out.truncate(mark);
                        return None;
                    }
                }
                Spacing::Hardline | Spacing::Emptyline | Spacing::Newlines(_) => {
                    out.truncate(mark);
                    return None;
                }
            }
        }

        match elem {
            None => return Some(out.len() - mark),
            Some(DocE::Text(_, _, TextAnn::RegularT, t)) => {
                let w = text_width(t) as isize;
                out.push_str(t);
                c -= w;
                ni -= w;
                if c < 0 {
                    out.truncate(mark);
                    return None;
                }
            }
            Some(DocE::Text(_, _, TextAnn::Comment, t)) => out.push_str(t),
            Some(DocE::Text(_, _, TextAnn::TrailingComment, t)) => {
                if ni == 0 {
                    out.push(' ');
                }
                out.push_str(t);
            }
            Some(DocE::Text(_, _, TextAnn::Trailing, _)) => {}
            Some(DocE::Spacing(_) | DocE::Group(_, _)) => unreachable!(),
        }
    }
}

/// Like `fits`, but only computes the rendered width instead of allocating the
/// rendered string. Used by `first_line_fits` where only the width matters.
fn fits_width(mut c: isize, doc: &[DocE]) -> Option<usize> {
    if c < 0 {
        return None;
    }
    let mut w = 0usize;
    let mut ni: isize = 0;
    let mut stack: Vec<std::slice::Iter<'_, DocE>> = vec![doc.iter()];
    let mut pending: Option<Spacing> = None;
    loop {
        let elem = loop {
            let Some(it) = stack.last_mut() else {
                break None;
            };
            match it.next() {
                Some(DocE::Group(_, ys)) => stack.push(ys.iter()),
                Some(e) => break Some(e),
                None => {
                    stack.pop();
                }
            }
        };
        if let Some(DocE::Spacing(s)) = elem {
            pending = Some(match pending {
                Some(p) => merge_spacings(p, *s),
                None => *s,
            });
            continue;
        }
        if let Some(sp) = pending.take() {
            match sp {
                Spacing::Softbreak | Spacing::Break => {}
                Spacing::Softspace | Spacing::Space | Spacing::Hardspace => {
                    w += 1;
                    c -= 1;
                    ni -= 1;
                    if c < 0 {
                        return None;
                    }
                }
                Spacing::Hardline | Spacing::Emptyline | Spacing::Newlines(_) => return None,
            }
        }
        match elem {
            None => return Some(w),
            Some(DocE::Text(_, _, TextAnn::RegularT, t)) => {
                let tw = text_width(t);
                w += tw;
                c -= tw as isize;
                ni -= tw as isize;
                if c < 0 {
                    return None;
                }
            }
            Some(DocE::Text(_, _, TextAnn::Comment, t)) => w += text_width(t),
            Some(DocE::Text(_, _, TextAnn::TrailingComment, t)) => {
                if ni == 0 {
                    w += 1;
                }
                w += text_width(t);
            }
            Some(DocE::Text(_, _, TextAnn::Trailing, _)) => {}
            Some(DocE::Spacing(_) | DocE::Group(_, _)) => unreachable!(),
        }
    }
}

/// Find the width of the first line in a document
/// Mirrors `firstLineWidth` in Nixfmt/Predoc.hs.
fn first_line_width(chain: Look<'_>) -> usize {
    let mut width = 0;
    let mut it = LookIter::new(chain);
    let mut pending: Option<Spacing> = None;
    loop {
        let elem = loop {
            match it.next() {
                Some(DocE::Group(_, xs)) => it.push_front(xs),
                e => break e,
            }
        };
        if let Some(DocE::Spacing(s)) = elem {
            pending = Some(match pending {
                Some(p) => merge_spacings(p, *s),
                None => *s,
            });
            continue;
        }
        if let Some(sp) = pending.take() {
            if sp == Spacing::Hardspace {
                width += 1;
            } else {
                return width;
            }
        }
        match elem {
            None => return width,
            Some(DocE::Text(_, _, TextAnn::Comment | TextAnn::TrailingComment, _)) => {}
            Some(DocE::Text(_, _, _, t)) => width += text_width(t),
            Some(DocE::Spacing(_) | DocE::Group(_, _)) => unreachable!(),
        }
    }
}

/// Mirrors `firstLineFits` in Nixfmt/Predoc.hs.
fn first_line_fits(target_width: usize, max_width: usize, chain: Look<'_>) -> bool {
    let max = max_width as isize;
    let target = target_width as isize;
    let mut c = max;
    let mut it = LookIter::new(chain);
    let mut pending: Option<Spacing> = None;
    let mut rest: Vec<&[DocE]> = Vec::new();
    loop {
        if c < 0 {
            return false;
        }
        let elem = it.next();
        if let Some(DocE::Spacing(s)) = elem {
            pending = Some(match pending {
                Some(p) => merge_spacings(p, *s),
                None => *s,
            });
            continue;
        }
        if let Some(sp) = pending.take() {
            if sp == Spacing::Hardspace {
                c -= 1;
                if c < 0 {
                    return false;
                }
            } else {
                return max - c <= target;
            }
        }
        match elem {
            None => return max - c <= target,
            Some(DocE::Text(_, _, TextAnn::RegularT, t)) => c -= text_width(t) as isize,
            Some(DocE::Text(..)) => {}
            Some(DocE::Group(_, ys)) => {
                rest.clear();
                rest.extend(it.stack.iter().rev().map(|(s, i)| &s[*i..]));
                let rest_width = first_line_width(&rest);
                match fits_width(c - rest_width as isize, ys) {
                    Some(w) => c -= w as isize,
                    None => it.push_front(ys),
                }
            }
            Some(DocE::Spacing(_)) => unreachable!(),
        }
    }
}

/// Mirrors `nextIndent` in Nixfmt/Predoc.hs.
fn next_indent(chain: Look<'_>) -> (usize, usize) {
    for elem in LookIter::new(chain) {
        match elem {
            DocE::Text(i, o, _, _) => return (*i, *o),
            DocE::Group(_, xs) => return next_indent(&[xs]),
            DocE::Spacing(_) => {}
        }
    }
    (0, 0)
}

/// Extract and list priority groups
/// Returns (pre, prio, post) triples
fn priority_groups(doc: &[DocE]) -> Vec<(Doc, Doc, Doc)> {
    // Segment the document into (is_priority, content) pairs
    fn segments(doc: &[DocE]) -> Vec<(bool, Doc)> {
        let mut result = Vec::new();
        for elem in doc {
            match elem {
                DocE::Group(GroupAnn::Priority, ys) => {
                    result.push((true, ys.clone()));
                }
                DocE::Group(GroupAnn::Transparent, ys) => {
                    result.extend(segments(ys));
                }
                _ => {
                    result.push((false, vec![elem.clone()]));
                }
            }
        }
        result
    }

    // Merge consecutive non-priority segments
    fn merge_segments(segs: Vec<(bool, Doc)>) -> Vec<(bool, Doc)> {
        let mut result = Vec::new();
        let mut i = 0;
        while i < segs.len() {
            let (is_prio, mut content) = segs[i].clone();
            if !is_prio {
                while i + 1 < segs.len() && !segs[i + 1].0 {
                    i += 1;
                    content.extend(segs[i].1.clone());
                }
            }
            result.push((is_prio, content));
            i += 1;
        }
        result
    }

    // Explode into (pre, prio, post) triples
    fn explode(segs: &[(bool, Doc)]) -> Vec<(Doc, Doc, Doc)> {
        if segs.is_empty() {
            return Vec::new();
        }
        if segs.len() == 1 {
            let (is_prio, content) = &segs[0];
            return if *is_prio {
                vec![(Vec::new(), content.clone(), Vec::new())]
            } else {
                Vec::new()
            };
        }

        let (is_prio, content) = &segs[0];
        let rest = &segs[1..];

        if *is_prio {
            let post: Doc = rest.iter().flat_map(|(_, c)| c.clone()).collect();
            let mut result = vec![(Vec::new(), content.clone(), post.clone())];
            for (pre, prio, post) in explode(rest) {
                let mut new_pre = content.clone();
                new_pre.extend(pre);
                result.push((new_pre, prio, post));
            }
            result
        } else {
            explode(rest)
                .into_iter()
                .map(|(pre, prio, post)| {
                    let mut new_pre = content.clone();
                    new_pre.extend(pre);
                    (new_pre, prio, post)
                })
                .collect()
        }
    }

    let segs = segments(doc);
    let merged = merge_segments(segs);
    explode(&merged)
}

/// State for layout algorithm
/// (current_column, indent_stack)
/// indent_stack: Vec<(current_indent, nesting_level)>
type LayoutState = (usize, Vec<(usize, usize)>);

/// Main layout algorithm
fn layout_greedy(target_width: usize, indent_width: usize, doc: Doc) -> String {
    let doc = vec![DocE::Group(GroupAnn::RegularG, fixup(doc))];

    let mut state: LayoutState = (0, vec![(0, 0)]);
    let mut result = String::new();
    render_doc(
        &mut result,
        &doc,
        &[],
        &mut state,
        target_width,
        indent_width,
    );

    let end = result.trim_end().len();
    result.truncate(end);
    let start = result.len() - result.trim_start().len();
    if start > 0 {
        result.drain(..start);
    }
    result.push('\n');
    result
}

/// Render a document with state tracking
fn render_doc(
    out: &mut String,
    doc: &[DocE],
    lookahead: Look<'_>,
    state: &mut LayoutState,
    tw: usize,
    iw: usize,
) {
    let mut chain: Vec<&[DocE]> = Vec::with_capacity(1 + lookahead.len());
    for (i, elem) in doc.iter().enumerate() {
        // Only Group and the soft spacings consult the lookahead; for the
        // common Text/hard-spacing case skip even the small chain rebuild.
        let needs_rest = match elem {
            DocE::Group(_, _) => true,
            DocE::Spacing(Spacing::Softbreak | Spacing::Softspace) => state.0 != 0,
            DocE::Text(_, _, TextAnn::TrailingComment, _) => state.0 == 2,
            _ => false,
        };
        if needs_rest {
            chain.clear();
            chain.push(&doc[i + 1..]);
            chain.extend_from_slice(lookahead);
            render_elem(out, elem, &chain, state, tw, iw);
        } else {
            render_elem(out, elem, &[], state, tw, iw);
        }
    }
}

/// Render a single element
fn render_elem(
    out: &mut String,
    elem: &DocE,
    lookahead: Look<'_>,
    state: &mut LayoutState,
    tw: usize,
    iw: usize,
) {
    let (cc, _indents) = state;
    let needs_indent = *cc == 0;

    match elem {
        // `goOne` special case: shift a trailing comment by one column so the
        // re-parser associates it with the same opener token (idempotency).
        DocE::Text(_, _, TextAnn::TrailingComment, t)
            if *cc == 2 && {
                let line_nl = state.1.last().map(|(_, l)| *l).unwrap_or(0);
                next_indent(lookahead).0 > line_nl
            } =>
        {
            let (cc, _) = state;
            *cc += 1 + text_width(t);
            out.push(' ');
            out.push_str(t);
        }

        DocE::Text(nl, off, _ann, t) => render_text(out, *nl, *off, t, state, iw),

        // At start of line drop any spacing; the next Text emits indentation.
        DocE::Spacing(_) if needs_indent => {}

        DocE::Spacing(sp) => match sp {
            Spacing::Break | Spacing::Space | Spacing::Hardline => {
                *cc = 0;
                out.push('\n');
            }
            Spacing::Hardspace => {
                *cc += 1;
                out.push(' ');
            }
            Spacing::Emptyline => {
                *cc = 0;
                out.push_str("\n\n");
            }
            Spacing::Newlines(n) => {
                *cc = 0;
                for _ in 0..*n {
                    out.push('\n');
                }
            }
            Spacing::Softbreak => {
                if !first_line_fits(tw - *cc, tw, lookahead) {
                    *cc = 0;
                    out.push('\n');
                }
            }
            Spacing::Softspace => {
                let available = tw.saturating_sub(*cc).saturating_sub(1);
                if first_line_fits(available, tw, lookahead) {
                    *cc += 1;
                    out.push(' ');
                } else {
                    *cc = 0;
                    out.push('\n');
                }
            }
        },

        DocE::Group(ann, ys) => render_group(out, *ann, ys, lookahead, state, tw, iw),
    }
}

/// Compute the current-indent column `render_text` would use for `text_nl` at
/// the start of a line, without mutating the indent stack.
fn indent_for(text_nl: usize, indents: &[(usize, usize)], iw: usize) -> usize {
    let mut top = indents.len();
    while top > 0 && text_nl < indents[top - 1].1 {
        top -= 1;
    }
    match indents[..top].last() {
        Some(&(ci, nl)) if text_nl > nl => ci + iw,
        Some(&(ci, _)) => ci,
        None => 0,
    }
}

/// Apply the indent-stack mutation `render_text` would perform for `text_nl`
/// at the start of a line (cc == 0).
fn apply_indent(text_nl: usize, state: &mut LayoutState, iw: usize) {
    let indents = &mut state.1;
    while let Some(&(_, nl)) = indents.last() {
        if text_nl > nl {
            let ci = indents.last().unwrap().0 + iw;
            indents.push((ci, text_nl));
            return;
        } else if text_nl < nl {
            indents.pop();
        } else {
            return;
        }
    }
}

/// Render text with proper indentation
fn render_text(
    out: &mut String,
    text_nl: usize,
    text_offset: usize,
    text: &str,
    state: &mut LayoutState,
    iw: usize,
) {
    let (cc, indents) = state;

    // Manage indentation stack
    while !indents.is_empty() {
        let (_, nl) = indents.last().unwrap();
        if text_nl > *nl {
            let new_indent = if *cc == 0 {
                indents.last().unwrap().0 + iw
            } else {
                indents.last().unwrap().0
            };
            indents.push((new_indent, text_nl));
            break;
        } else if text_nl < *nl {
            indents.pop();
        } else {
            break;
        }
    }

    let (ci, _) = indents.last().unwrap_or(&(0, 0));
    let total_indent = ci + text_offset;

    let w = text_width(text);
    if *cc == 0 {
        for _ in 0..total_indent {
            out.push(' ');
        }
    }
    *cc += w;
    out.push_str(text);
}

/// Try to render a group compactly. On success, appends to `out` and updates
/// `state` in place; on failure leaves both untouched.
fn try_render_group(
    out: &mut String,
    grp: &[DocE],
    lookahead: Look<'_>,
    state: &mut LayoutState,
    tw: usize,
    iw: usize,
) -> bool {
    // Mirrors `goGroup` in Nixfmt/Predoc.hs.
    if grp.is_empty() {
        return true;
    }

    let (cc, indents) = (state.0, &state.1);

    if cc == 0 {
        // At start of line - drop leading whitespace
        let grp = match grp.first() {
            Some(DocE::Spacing(_)) => &grp[1..],
            Some(DocE::Group(ann, inner)) if matches!(inner.first(), Some(DocE::Spacing(_))) => {
                let mut new_grp = vec![DocE::Group(*ann, inner[1..].to_vec())];
                new_grp.extend_from_slice(&grp[1..]);
                return try_render_group(out, &new_grp, lookahead, state, tw, iw);
            }
            _ => grp,
        };

        let (nl, off) = next_indent(&[grp]);
        // Haskell `goGroup` (cc == 0): the budget is `tw - firstLineWidth rest`;
        // the pending indentation is *not* subtracted here, so a compact group
        // at the start of a line may overshoot by its indent. This matches the
        // reference layout engine exactly.
        let last_line_nl = indents.last().map(|(_, l)| *l).unwrap_or(0);
        let line_nl = last_line_nl + if nl > last_line_nl { iw } else { 0 };
        let will_increase = if next_indent(lookahead).0 > line_nl {
            iw
        } else {
            0
        };

        let budget = tw as isize - first_line_width(lookahead) as isize;
        let mark = out.len();
        let total_indent = indent_for(nl, &state.1, iw) + off;
        for _ in 0..total_indent {
            out.push(' ');
        }
        match fits(will_increase as isize, budget, grp, out) {
            Some(w) => {
                apply_indent(nl, state, iw);
                state.0 += w;
                true
            }
            None => {
                out.truncate(mark);
                false
            }
        }
    } else {
        let line_nl = indents.last().map(|(_, l)| *l).unwrap_or(0);
        let will_increase = if next_indent(lookahead).0 > line_nl {
            iw as isize
        } else {
            0
        };

        let budget = tw as isize - cc as isize - first_line_width(lookahead) as isize;
        match fits(will_increase - cc as isize, budget, grp, out) {
            Some(w) => {
                state.0 += w;
                true
            }
            None => false,
        }
    }
}

/// Render a group (try compact first, then expand)
fn render_group(
    out: &mut String,
    ann: GroupAnn,
    ys: &[DocE],
    lookahead: Look<'_>,
    state: &mut LayoutState,
    tw: usize,
    iw: usize,
) {
    // Try to fit group compactly
    if try_render_group(out, ys, lookahead, state, tw, iw) {
        return;
    }

    // Try priority groups if not transparent
    if ann != GroupAnn::Transparent && has_priority_groups(ys) {
        let state_backup = state.clone();
        let out_len = out.len();
        for (pre, prio, post) in priority_groups(ys).into_iter().rev() {
            let mut pre_lookahead: Vec<&[DocE]> = Vec::with_capacity(2 + lookahead.len());
            pre_lookahead.push(&prio);
            pre_lookahead.push(&post);
            pre_lookahead.extend_from_slice(lookahead);
            if try_render_group(out, &pre, &pre_lookahead, state, tw, iw) {
                // Render prio expanded
                let unexpanded_post = unexpand_spacing(&post);
                let mut prio_lookahead: Vec<&[DocE]> = Vec::with_capacity(1 + lookahead.len());
                prio_lookahead.push(&unexpanded_post);
                prio_lookahead.extend_from_slice(lookahead);
                render_doc(out, &prio, &prio_lookahead, state, tw, iw);

                if try_render_group(out, &post, lookahead, state, tw, iw) {
                    return;
                }
            }
            // Attempt failed: discard any mutations from the trial run before
            // trying the next priority group or falling back to full expansion.
            // Haskell threads this via `StateT St Maybe`, which simply drops
            // the state on `Nothing`.
            state.0 = state_backup.0;
            state.1.clone_from(&state_backup.1);
            out.truncate(out_len);
        }
    }

    // Fully expand group
    render_doc(out, ys, lookahead, state, tw, iw)
}
