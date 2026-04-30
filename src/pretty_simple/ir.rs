//! `PrettySimple` implementations for IR (intermediate representation) nodes

use super::{PrettySimple, Writer, format_bracket_list};
use crate::format_constructor;
use crate::predoc::*;

impl PrettySimple for IR {
    fn format<W: Writer>(&self, w: &mut W) {
        // Same layout as Vec<DocE>, but without bumping depth so the top-level
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
        !matches!(self, Spacing::Newlines(_))
    }

    fn renders_inline_parens(&self) -> bool {
        matches!(self, Spacing::Newlines(_))
    }
}

impl PrettySimple for GroupAnn {
    fn format<W: Writer>(&self, w: &mut W) {
        crate::format_enum!(self, w, {
            RegularG => [],
            Priority => [],
            Transparent => [],
        });
    }

    fn is_simple(&self) -> bool {
        true
    }
}

impl PrettySimple for TextAnn {
    fn format<W: Writer>(&self, w: &mut W) {
        crate::format_enum!(self, w, {
            RegularT => [],
            Comment => [],
            TrailingComment => [],
            Trailing => [],
        });
    }

    fn is_simple(&self) -> bool {
        true
    }
}

impl PrettySimple for DocE {
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
            DocE::Spacing(_) | DocE::Text(_, _, _, _) | DocE::Nest(..)
        )
    }
}
