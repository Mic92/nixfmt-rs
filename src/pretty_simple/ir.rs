//! `PrettySimple` implementations for IR (intermediate representation) nodes

use super::{PrettySimple, Writer, format_bracket_list};
use crate::format_constructor;
use crate::predoc::{Doc, Elem, GroupKind, IR, Spacing, TextKind};

impl PrettySimple for Doc {
    fn format<W: Writer>(&self, w: &mut W) {
        self.0.format(w);
    }
    fn is_simple(&self) -> bool {
        self.0.is_simple()
    }
    fn has_delimiters(&self) -> bool {
        self.0.has_delimiters()
    }
    fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl PrettySimple for IR {
    fn format<W: Writer>(&self, w: &mut W) {
        // Same layout as Vec<Elem>, but without bumping depth so the top-level
        // dump stays at column 0 like the Haskell reference output.
        format_bracket_list(w, &self.0, false);
        if !self.0.is_empty() {
            // Final newline to match nixfmt output.
            w.newline();
        }
    }
}

impl PrettySimple for Spacing {
    fn format<W: Writer>(&self, w: &mut W) {
        crate::format_enum!(self, w, {
            Newlines(n) => [n],
            Softbreak => [],
            Break => [],
            Hardspace => [],
            Softspace => [],
            Space => [],
            Hardline => [],
            Emptyline => [],
        });
    }

    fn is_simple(&self) -> bool {
        !matches!(self, Self::Newlines(_))
    }

    fn renders_inline_parens(&self) -> bool {
        matches!(self, Self::Newlines(_))
    }
}

impl PrettySimple for GroupKind {
    fn format<W: Writer>(&self, w: &mut W) {
        // Reference `nixfmt --ir` uses Haskell constructor names; preserve
        // them so the snapshot diffing against the reference stays exact.
        w.write_plain(match self {
            Self::Regular => "RegularG",
            Self::Priority => "Priority",
            Self::Transparent => "Transparent",
        });
    }

    fn is_simple(&self) -> bool {
        true
    }
}

impl PrettySimple for TextKind {
    fn format<W: Writer>(&self, w: &mut W) {
        w.write_plain(match self {
            Self::Regular => "RegularT",
            Self::Comment => "Comment",
            Self::TrailingComment => "TrailingComment",
            Self::Trailing => "Trailing",
        });
    }

    fn is_simple(&self) -> bool {
        true
    }
}

impl PrettySimple for Elem {
    fn format<W: Writer>(&self, w: &mut W) {
        crate::format_enum!(self, w, {
            Text(nest, off, ann, text) => [nest, off, ann, text],
            Spacing(sp) => [sp],
            Group(ann, doc) => [ann, doc],
            Nest(n, o) => [n, o],
        });
    }

    fn is_simple(&self) -> bool {
        matches!(
            self,
            Self::Spacing(_) | Self::Text(_, _, _, _) | Self::Nest(..)
        )
    }
}
