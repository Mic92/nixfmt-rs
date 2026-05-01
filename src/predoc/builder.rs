//! Helpers for constructing a [`Doc`]: text/comment pushers, group/nest
//! combinators and spacing constructors. Kept separate from the renderer so
//! `pretty/` callers only see the building vocabulary.

use super::{Doc, DocE, GroupAnn, Pretty, Spacing, TextAnn};

/// Push a text element with the given annotation, dropping empty strings.
fn push_text_ann(doc: &mut Doc, ann: TextAnn, s: impl Into<String>) {
    let s = s.into();
    if !s.is_empty() {
        doc.push(DocE::Text(0, 0, ann, s));
    }
}

pub fn push_text(doc: &mut Doc, s: impl Into<String>) {
    push_text_ann(doc, TextAnn::RegularT, s);
}

pub fn push_comment(doc: &mut Doc, s: impl Into<String>) {
    push_text_ann(doc, TextAnn::Comment, s);
}

pub fn push_trailing_comment(doc: &mut Doc, s: impl Into<String>) {
    push_text_ann(doc, TextAnn::TrailingComment, s);
}

/// Only rendered in expanded groups.
pub fn push_trailing(doc: &mut Doc, s: impl Into<String>) {
    push_text_ann(doc, TextAnn::Trailing, s);
}

pub fn push_group<F>(doc: &mut Doc, f: F)
where
    F: FnOnce(&mut Doc),
{
    push_group_ann(doc, GroupAnn::RegularG, f);
}

pub fn push_group_ann<F>(doc: &mut Doc, ann: GroupAnn, f: F)
where
    F: FnOnce(&mut Doc),
{
    // Write into the parent's tail and split_off, so the body grows an
    // amortised buffer instead of a fresh zero-cap Vec per group.
    let start = doc.len();
    f(doc);
    let inner = doc.split_off(start);
    doc.push(DocE::Group(ann, inner));
}

/// Surround `f`'s output with a balanced `Nest(dn, doff)` / `Nest(-dn, -doff)`
/// pair. `fixup` later bakes the accumulated deltas into each `Text` so the
/// renderer's indent stack logic is unchanged.
fn push_nest_pair<F>(doc: &mut Doc, dn: isize, doff: isize, f: F)
where
    F: FnOnce(&mut Doc),
{
    doc.push(DocE::Nest(dn, doff));
    f(doc);
    doc.push(DocE::Nest(-dn, -doff));
}

pub fn push_nested<F>(doc: &mut Doc, f: F)
where
    F: FnOnce(&mut Doc),
{
    push_nest_pair(doc, 1, 0, f);
}

/// Line break or nothing (soft)
pub const fn softline_prime() -> DocE {
    DocE::Spacing(Spacing::Softbreak)
}

/// Line break or nothing
pub const fn line_prime() -> DocE {
    DocE::Spacing(Spacing::Break)
}

/// Line break or space (soft)
pub const fn softline() -> DocE {
    DocE::Spacing(Spacing::Softspace)
}

/// Line break or space
pub const fn line() -> DocE {
    DocE::Spacing(Spacing::Space)
}

/// Always space
pub const fn hardspace() -> DocE {
    DocE::Spacing(Spacing::Hardspace)
}

/// Always line break
pub const fn hardline() -> DocE {
    DocE::Spacing(Spacing::Hardline)
}

/// Two line breaks (blank line)
pub const fn emptyline() -> DocE {
    DocE::Spacing(Spacing::Emptyline)
}

/// n line breaks
pub const fn newline() -> DocE {
    DocE::Spacing(Spacing::Newlines(1))
}

pub fn push_sep_by<P: Pretty>(doc: &mut Doc, separator: &Doc, docs: Vec<P>) {
    let mut first = true;
    for item in docs {
        if !first {
            doc.extend(separator.iter().cloned());
        }
        first = false;
        item.pretty(doc);
    }
}

pub fn push_hcat<P: Pretty>(doc: &mut Doc, docs: Vec<P>) {
    for item in docs {
        item.pretty(doc);
    }
}

pub fn push_surrounded<F>(doc: &mut Doc, outside: &Doc, f: F)
where
    F: FnOnce(&mut Doc),
{
    doc.extend(outside.iter().cloned());
    f(doc);
    doc.extend(outside.iter().cloned());
}

/// Manual column offset baked into all enclosed text elements. Used for
/// indented strings where the original indentation must be preserved.
pub fn push_offset<F>(doc: &mut Doc, level: usize, f: F)
where
    F: FnOnce(&mut Doc),
{
    push_nest_pair(doc, 0, level.cast_signed(), f);
}
