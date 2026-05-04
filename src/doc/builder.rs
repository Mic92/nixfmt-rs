//! Builder methods for constructing a [`Doc`]: text/comment pushers,
//! group/nest combinators and spacing constructors. Kept separate from the
//! renderer so `format/` callers only see the building vocabulary.
//!
//! The original Haskell implementation models this as a `Writer Doc` monad and
//! the first Rust port mirrored that with free `push_*(doc, …)` functions. The
//! idiomatic Rust shape is a builder struct with chainable inherent methods,
//! which is what this module now provides.

use super::{Doc, Elem, Emit, GroupKind, Spacing, TextKind};

impl Doc {
    /// Push a text element with the given annotation, dropping empty strings.
    fn text_with(&mut self, ann: TextKind, s: impl Into<String>) -> &mut Self {
        let s = s.into();
        if !s.is_empty() {
            self.0.push(Elem::Text(0, 0, ann, s));
        }
        self
    }

    pub fn text(&mut self, s: impl Into<String>) -> &mut Self {
        self.text_with(TextKind::Regular, s)
    }

    pub fn comment(&mut self, s: impl Into<String>) -> &mut Self {
        self.text_with(TextKind::Comment, s)
    }

    pub fn trailing_comment(&mut self, s: impl Into<String>) -> &mut Self {
        self.text_with(TextKind::TrailingComment, s)
    }

    /// Only rendered in expanded groups.
    pub fn trailing(&mut self, s: impl Into<String>) -> &mut Self {
        self.text_with(TextKind::Trailing, s)
    }

    pub fn group(&mut self, f: impl FnOnce(&mut Self)) -> &mut Self {
        self.group_with(GroupKind::Regular, f)
    }

    pub fn priority_group(&mut self, f: impl FnOnce(&mut Self)) -> &mut Self {
        self.group_with(GroupKind::Priority, f)
    }

    pub fn transparent_group(&mut self, f: impl FnOnce(&mut Self)) -> &mut Self {
        self.group_with(GroupKind::Transparent, f)
    }

    pub fn group_with(&mut self, kind: GroupKind, f: impl FnOnce(&mut Self)) -> &mut Self {
        // Write into the parent's tail and split_off, so the body grows an
        // amortised buffer instead of a fresh zero-cap Vec per group.
        let start = self.0.len();
        f(self);
        let inner = Self(self.0.split_off(start));
        self.0.push(Elem::Group(kind, inner));
        self
    }

    /// Surround `f`'s output with a balanced `Nest(dn, doff)` / `Nest(-dn, -doff)`
    /// pair. `fixup` later bakes the accumulated deltas into each `Text` so the
    /// renderer's indent stack logic is unchanged.
    fn nest_pair(&mut self, dn: isize, doff: isize, f: impl FnOnce(&mut Self)) -> &mut Self {
        self.0.push(Elem::Nest(dn, doff));
        f(self);
        self.0.push(Elem::Nest(-dn, -doff));
        self
    }

    pub fn nested(&mut self, f: impl FnOnce(&mut Self)) -> &mut Self {
        self.nest_pair(1, 0, f)
    }

    /// Manual column offset baked into all enclosed text elements. Used for
    /// indented strings where the original indentation must be preserved.
    pub fn offset(&mut self, level: usize, f: impl FnOnce(&mut Self)) -> &mut Self {
        self.nest_pair(0, level.cast_signed(), f)
    }

    pub fn sep_by<P: Emit>(
        &mut self,
        separator: &[Elem],
        items: impl IntoIterator<Item = P>,
    ) -> &mut Self {
        let mut first = true;
        for item in items {
            if !first {
                self.0.extend_from_slice(separator);
            }
            first = false;
            item.emit(self);
        }
        self
    }

    pub fn hcat<P: Emit>(&mut self, items: impl IntoIterator<Item = P>) -> &mut Self {
        for item in items {
            item.emit(self);
        }
        self
    }

    pub fn surrounded(&mut self, outside: &[Elem], f: impl FnOnce(&mut Self)) -> &mut Self {
        self.0.extend_from_slice(outside);
        f(self);
        self.0.extend_from_slice(outside);
        self
    }

    // -- Spacing pushers ----------------------------------------------------
    //
    // Thin wrappers over the free spacing constructors below. Having both lets
    // call sites write `doc.hardline()` for the common "emit a spacing" case
    // while still being able to pass `hardline()` as a `Elem` value (e.g. as a
    // separator argument).

    /// Line break or nothing (soft)
    pub fn softbreak(&mut self) -> &mut Self {
        self.push_raw(Elem::Spacing(Spacing::Softbreak))
    }
    /// Line break or nothing
    pub fn linebreak(&mut self) -> &mut Self {
        self.push_raw(linebreak())
    }
    /// Line break or space (soft)
    pub fn softline(&mut self) -> &mut Self {
        self.push_raw(Elem::Spacing(Spacing::Softspace))
    }
    /// Line break or space
    pub fn line(&mut self) -> &mut Self {
        self.push_raw(line())
    }
    /// Always space
    pub fn hardspace(&mut self) -> &mut Self {
        self.push_raw(hardspace())
    }
    /// Always line break
    pub fn hardline(&mut self) -> &mut Self {
        self.push_raw(hardline())
    }
    /// Two line breaks (blank line)
    pub fn emptyline(&mut self) -> &mut Self {
        self.push_raw(Elem::Spacing(Spacing::Emptyline))
    }
}

// -- Free spacing constructors ---------------------------------------------
//
// Kept as free functions because spacings are also used as first-class `Elem`
// values (separator arguments, `push_raw`, pattern matches), not just emitted
// into a `Doc`.

/// Line break or nothing
pub const fn linebreak() -> Elem {
    Elem::Spacing(Spacing::Break)
}

/// Line break or space
pub const fn line() -> Elem {
    Elem::Spacing(Spacing::Space)
}

/// Always space
pub const fn hardspace() -> Elem {
    Elem::Spacing(Spacing::Hardspace)
}

/// Always line break
pub const fn hardline() -> Elem {
    Elem::Spacing(Spacing::Hardline)
}

/// n line breaks
pub const fn newline() -> Elem {
    Elem::Spacing(Spacing::Newlines(1))
}
