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
pub(crate) fn render_with_config(doc: &Doc, config: &RenderConfig) -> String {
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
/// contains hard line breaks or exceeds the optional width limit.
/// Mirrors Haskell `unexpandSpacing'` (Predoc.hs).
pub(crate) fn unexpand_spacing_prime(mut limit: Option<i32>, doc: &[DocE]) -> Option<Doc> {
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
                    *n -= text_width(t) as i32;
                    if *n < 0 {
                        return None;
                    }
                }
                result.push(elem.clone());
            }
            DocE::Spacing(Spacing::Hardspace)
            | DocE::Spacing(Spacing::Space)
            | DocE::Spacing(Spacing::Softspace) => {
                if let Some(n) = limit.as_mut() {
                    *n -= 1;
                    if *n < 0 {
                        return None;
                    }
                }
                result.push(DocE::Spacing(Spacing::Hardspace));
            }
            DocE::Spacing(Spacing::Break) | DocE::Spacing(Spacing::Softbreak) => {}
            DocE::Spacing(_) => return None,
            DocE::Group(_, inner) => {
                stack.push(inner.iter());
            }
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
            DocE::Text(_, _, TextAnn::Trailing, _) => {}
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
    if doc.len() == 1 {
        if let DocE::Group(inner_ann, body) = &doc[0] {
            if ann == *inner_ann {
                return body.clone();
            }
        }
    }
    doc
}

/// Check if an element is trailing spacing (hard spacing or groups containing only hard spacing)
fn is_trailing_spacing_elem(elem: &DocE) -> bool {
    if is_hard_spacing(elem) {
        return true;
    }
    match elem {
        DocE::Group(_, inner) => inner.iter().all(|e| match e {
            DocE::Spacing(s) => matches!(
                s,
                Spacing::Hardspace | Spacing::Hardline | Spacing::Emptyline | Spacing::Newlines(_)
            ),
            _ => false,
        }),
        _ => false,
    }
}

/// Recursively peel trailing spacing out of nested groups
fn split_trailing(doc: Doc) -> (Doc, Doc) {
    // First, peel any trailing spacing at this level
    let (mut body, post) = span_end(is_trailing_spacing_elem, doc);
    if !post.is_empty() {
        return (body, post);
    }

    // If the last element is a group, try peeling from inside it
    if let Some(DocE::Group(last_ann, last_inner)) = body.last().cloned() {
        let (new_inner, inner_post) = split_trailing(last_inner);
        if !inner_post.is_empty() {
            body.pop();
            let new_inner = simplify_group(last_ann, new_inner);
            if !new_inner.is_empty() {
                body.push(DocE::Group(last_ann, new_inner));
            }
            return (body, inner_post);
        }
    }

    (body, post)
}

/// Fix up a Doc by:
/// - Moving hard spacings and comments out of groups
/// - Merging consecutive spacings
/// - Removing empty groups
pub(crate) fn fixup(doc: &Doc) -> Doc {
    if doc.is_empty() {
        return Vec::new();
    }

    let mut doc = doc.to_vec();
    let mut result = Vec::new();

    while !doc.is_empty() {
        let elem = doc.remove(0);

        match elem {
            // Move/Merge hard spaces into groups so they can merge with a
            // leading soft spacing inside the group (Haskell `fixup` rule for
            // `Spacing Hardspace : Group ann xs`).
            DocE::Spacing(Spacing::Hardspace) if matches!(doc.first(), Some(DocE::Group(_, _))) => {
                if let Some(DocE::Group(_, xs)) = doc.first_mut() {
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
                if let DocE::Text(_, _, ann2, b) = &doc[0] {
                    if ann == *ann2 {
                        doc[0] = DocE::Text(level, off, ann, format!("{}{}", a, b));
                    } else {
                        result.push(DocE::Text(level, off, ann, a));
                    }
                } else {
                    result.push(DocE::Text(level, off, ann, a));
                }
            }

            DocE::Group(ann, xs) => {
                let fixed_xs = fixup(&xs);

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

                if rest.is_empty() {
                    // Dissolve empty group
                    result.extend(pre);
                } else {
                    // Split out trailing hard spacings, and also peel from nested groups
                    let (body, post) = split_trailing(rest);

                    let body = simplify_group(ann, body);

                    if body.is_empty() {
                        result.extend(pre);
                        result.extend(post);
                    } else {
                        result.extend(pre);
                        result.push(DocE::Group(ann, body));
                        result.extend(fixup(&post));
                    }
                }
            }

            _ => result.push(elem),
        }
    }

    result
}

/// Attempt to fit a document in a single line with specific width
/// ni: next indentation (for trailing comment calculations)
/// c: allowed width
/// Returns rendered text if it fits, None otherwise
fn fits(ni: isize, c: isize, doc: &[DocE]) -> Option<String> {
    if c < 0 {
        return None;
    }
    if doc.is_empty() {
        return Some(String::new());
    }

    let mut result = String::new();
    let mut remaining = c;
    let mut next_indent = ni;
    let mut i = 0;

    while i < doc.len() {
        let elem = &doc[i];
        i += 1;

        match elem {
            DocE::Text(_, _, TextAnn::RegularT, t) => {
                let w = text_width(t) as isize;
                result.push_str(t);
                remaining -= w;
                next_indent -= w;
                if remaining < 0 {
                    return None;
                }
            }
            DocE::Text(_, _, TextAnn::Comment, t) => {
                // Comments don't count towards width
                result.push_str(t);
            }
            DocE::Text(_, _, TextAnn::TrailingComment, t) => {
                if next_indent < 0 {
                    return None;
                }
                result.push_str(t);
            }
            DocE::Text(_, _, TextAnn::Trailing, _) => {}
            DocE::Spacing(Spacing::Softbreak) => {}
            DocE::Spacing(Spacing::Break) => {}
            DocE::Spacing(Spacing::Softspace) => {
                result.push(' ');
                remaining -= 1;
                next_indent -= 1;
                if remaining < 0 {
                    return None;
                }
            }
            DocE::Spacing(Spacing::Space) => {
                result.push(' ');
                remaining -= 1;
                next_indent -= 1;
                if remaining < 0 {
                    return None;
                }
            }
            DocE::Spacing(Spacing::Hardspace) => {
                result.push(' ');
                remaining -= 1;
                next_indent -= 1;
                if remaining < 0 {
                    return None;
                }
            }
            DocE::Spacing(Spacing::Hardline) => return None,
            DocE::Spacing(Spacing::Emptyline) => return None,
            DocE::Spacing(Spacing::Newlines(_)) => return None,
            DocE::Group(_, ys) => match fits(next_indent, remaining, ys) {
                Some(s) => {
                    let w = text_width(&s) as isize;
                    result.push_str(&s);
                    remaining -= w;
                    next_indent -= w;
                    if remaining < 0 {
                        return None;
                    }
                }
                None => {
                    return None;
                }
            },
        }
    }

    Some(result)
}

/// Find the width of the first line in a document
fn first_line_width(doc: &[DocE]) -> usize {
    let mut width = 0;

    for elem in doc {
        match elem {
            DocE::Text(_, _, TextAnn::Comment, _) => {}
            DocE::Text(_, _, TextAnn::TrailingComment, _) => {}
            DocE::Text(_, _, _, t) => width += text_width(t),
            DocE::Spacing(Spacing::Hardspace) => width += 1,
            DocE::Spacing(_) => break,
            DocE::Group(_, xs) => {
                width += first_line_width(xs);
            }
        }
    }

    width
}

/// Check if the first line fits within target width given a maximum width
fn first_line_fits(target_width: usize, max_width: usize, doc: &[DocE]) -> bool {
    fn go(remaining: isize, target: usize, max: usize, doc: &[DocE]) -> bool {
        if remaining < 0 {
            return false;
        }
        if doc.is_empty() {
            return (max as isize - remaining) as usize <= target;
        }

        let mut c = remaining;
        for (idx, elem) in doc.iter().enumerate() {
            match elem {
                DocE::Text(_, _, TextAnn::RegularT, t) => {
                    c -= text_width(t) as isize;
                    if c < 0 {
                        return false;
                    }
                }
                DocE::Text(_, _, TextAnn::Trailing, t) => {
                    c -= text_width(t) as isize;
                    if c < 0 {
                        return false;
                    }
                }
                DocE::Text(_, _, _, _) => {} // Comments don't count
                DocE::Spacing(Spacing::Hardspace) => {
                    c -= 1;
                    if c < 0 {
                        return false;
                    }
                }
                DocE::Spacing(_) => {
                    return (max as isize - c) as usize <= target;
                }
                DocE::Group(_, ys) => {
                    let rest = &doc[idx + 1..];
                    let rest_width = first_line_width(rest);

                    match fits(0, c - rest_width as isize, ys) {
                        Some(t) => {
                            c -= text_width(&t) as isize;
                            if c < 0 {
                                return false;
                            }
                        }
                        None => {
                            return go(c, target, max, ys);
                        }
                    }
                }
            }
        }

        (max as isize - c) as usize <= target
    }

    go(max_width as isize, target_width, max_width, doc)
}

/// Calculate next indentation level
fn next_indent(doc: &[DocE]) -> (usize, usize) {
    for elem in doc {
        match elem {
            DocE::Text(i, o, _, _) => return (*i, *o),
            DocE::Group(_, xs) => return next_indent(xs),
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
fn layout_greedy(target_width: usize, indent_width: usize, doc: &Doc) -> String {
    let inner = fixup(doc);
    // Wrap in a top-level group to mirror nixfmt's structure
    let wrapped = vec![DocE::Group(GroupAnn::RegularG, inner)];
    // Run fixup again so trailing spacing moves outside the top-level group
    let doc = fixup(&wrapped);

    let mut state: LayoutState = (0, vec![(0, 0)]);
    let result = render_doc(&doc, &[], &mut state, target_width, indent_width);

    format!("{}\n", result.trim())
}

/// Render a document with state tracking
fn render_doc(
    doc: &[DocE],
    lookahead: &[DocE],
    state: &mut LayoutState,
    tw: usize,
    iw: usize,
) -> String {
    let mut result = String::new();
    for (i, elem) in doc.iter().enumerate() {
        let rest: Doc = doc[i + 1..].iter().chain(lookahead).cloned().collect();
        result.push_str(&render_elem(elem, &rest, state, tw, iw));
    }
    result
}

/// Render a single element
fn render_elem(
    elem: &DocE,
    lookahead: &[DocE],
    state: &mut LayoutState,
    tw: usize,
    iw: usize,
) -> String {
    let (cc, _indents) = state;
    let needs_indent = *cc == 0;

    match elem {
        DocE::Text(nl, off, _ann, t) => render_text(*nl, *off, t, state, iw),

        DocE::Spacing(sp) if needs_indent => {
            // When cc == 0, drop all spacings except hardspace (matches nixfmt)
            match sp {
                Spacing::Hardspace => {
                    *cc += 1;
                    " ".to_string()
                }
                _ => String::new(),
            }
        }

        DocE::Spacing(sp) => match sp {
            Spacing::Break | Spacing::Space | Spacing::Hardline => {
                *cc = 0;
                "\n".to_string()
            }
            Spacing::Hardspace => {
                *cc += 1;
                " ".to_string()
            }
            Spacing::Emptyline => {
                *cc = 0;
                "\n\n".to_string()
            }
            Spacing::Newlines(n) => {
                *cc = 0;
                "\n".repeat(*n)
            }
            Spacing::Softbreak => {
                if first_line_fits(tw - *cc, tw, lookahead) {
                    String::new()
                } else {
                    *cc = 0;
                    "\n".to_string()
                }
            }
            Spacing::Softspace => {
                let available = tw.saturating_sub(*cc).saturating_sub(1);
                if first_line_fits(available, tw, lookahead) {
                    *cc += 1;
                    " ".to_string()
                } else {
                    *cc = 0;
                    "\n".to_string()
                }
            }
        },

        DocE::Group(ann, ys) => render_group(*ann, ys, lookahead, state, tw, iw),
    }
}

/// Render text with proper indentation
fn render_text(
    text_nl: usize,
    text_offset: usize,
    text: &str,
    state: &mut LayoutState,
    iw: usize,
) -> String {
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

    *cc += text_width(text);

    if *cc == text_width(text) {
        // First token on line - add indentation
        format!("{}{}", " ".repeat(total_indent), text)
    } else {
        text.to_string()
    }
}

/// Try to render a group compactly
/// Returns (rendered_text, updated_state) if successful
fn try_render_group(
    grp: &[DocE],
    lookahead: &[DocE],
    state: &LayoutState,
    tw: usize,
    iw: usize,
) -> Option<(String, LayoutState)> {
    let (cc, indents) = state;

    if *cc == 0 {
        // At start of line - drop leading whitespace
        let grp = match grp.first() {
            Some(DocE::Spacing(_)) => &grp[1..],
            Some(DocE::Group(ann, inner)) if matches!(inner.first(), Some(DocE::Spacing(_))) => {
                let mut new_grp = vec![DocE::Group(*ann, inner[1..].to_vec())];
                new_grp.extend_from_slice(&grp[1..]);
                return try_render_group(&new_grp, lookahead, state, tw, iw);
            }
            _ => grp,
        };

        let (nl, off) = next_indent(grp);
        let line_nl = indents.last().map(|(_, l)| *l).unwrap_or(0);
        let will_increase = if next_indent(lookahead).0 > line_nl {
            iw
        } else {
            0
        };

        // Calculate the indentation that will be added when rendering this group at cc==0
        let group_indent = if nl > line_nl {
            // Will push a new indentation level
            indents.last().map(|(i, _)| i + iw).unwrap_or(iw)
        } else {
            // Will use current indentation
            indents.last().map(|(i, _)| *i).unwrap_or(0)
        };

        let target_width = tw
            .saturating_sub(first_line_width(lookahead))
            .saturating_sub(group_indent);
        fits(will_increase as isize, target_width as isize, grp).map(|t| {
            let mut new_state = state.clone();
            let rendered = render_text(nl, off, &t, &mut new_state, iw);
            (rendered, new_state)
        })
    } else {
        let line_nl = indents.last().map(|(_, l)| *l).unwrap_or(0);
        let will_increase = if next_indent(lookahead).0 > line_nl {
            iw as isize
        } else {
            0
        };

        let target_width = tw
            .saturating_sub(*cc)
            .saturating_sub(first_line_width(lookahead));
        let new_cc = *cc;
        fits(will_increase - new_cc as isize, target_width as isize, grp).map(|t| {
            let mut new_state = state.clone();
            new_state.0 += text_width(&t);
            (t, new_state)
        })
    }
}

/// Render a group (try compact first, then expand)
fn render_group(
    ann: GroupAnn,
    ys: &[DocE],
    lookahead: &[DocE],
    state: &mut LayoutState,
    tw: usize,
    iw: usize,
) -> String {
    // Try to fit group compactly
    if let Some((compact, new_state)) = try_render_group(ys, lookahead, state, tw, iw) {
        *state = new_state;
        return compact;
    }

    // Try priority groups if not transparent
    if ann != GroupAnn::Transparent {
        for (pre, prio, post) in priority_groups(ys).into_iter().rev() {
            let state_backup = state.clone();

            let pre_lookahead = [prio.clone(), post.clone(), lookahead.to_vec()].concat();
            if let Some((pre_text, state_after_pre)) =
                try_render_group(&pre, &pre_lookahead, &state_backup, tw, iw)
            {
                *state = state_after_pre;

                // Render prio expanded
                let unexpanded_post = unexpand_spacing(&post);
                let combined_lookahead: Vec<_> = unexpanded_post
                    .into_iter()
                    .chain(lookahead.iter().cloned())
                    .collect();
                let prio_text = render_doc(&prio, &combined_lookahead, state, tw, iw);

                if let Some((post_text, state_after_post)) =
                    try_render_group(&post, lookahead, state, tw, iw)
                {
                    *state = state_after_post;
                    return format!("{}{}{}", pre_text, prio_text, post_text);
                }
            }
        }
    }

    // Fully expand group
    render_doc(ys, lookahead, state, tw, iw)
}
