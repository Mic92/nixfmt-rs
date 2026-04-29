//! PrettySimple implementations for IR (intermediate representation) nodes

use super::{PrettySimple, Writer};
use crate::format_constructor;
use crate::predoc::*;

impl PrettySimple for IR {
    fn format<W: Writer>(&self, w: &mut W) {
        // Format like Vec<DocE> but WITHOUT incrementing depth
        // The top-level IR should be at depth 0, not depth 1

        if self.0.is_empty() {
            w.with_color(|w_color| {
                let bracket_color = w_color.current_color();
                w_color.write_colored("[", bracket_color);
                w_color.write_colored("]", bracket_color);
            });
            return;
        }

        // Same as Vec::format but skip the with_depth call
        w.with_color(|w_color| {
            let bracket_color = w_color.current_color();
            // Vec::format calls w_color.with_depth here, but we don't (keeps IR at depth 0)

            if self.0.len() == 1 && self.0[0].is_simple() {
                w_color.write_colored("[", bracket_color);
                w_color.write_plain(" ");
                self.0[0].format(w_color);
                w_color.write_plain(" ");
                w_color.write_colored("]", bracket_color);
            } else {
                w_color.write_colored("[", bracket_color);
                for (i, item) in self.0.iter().enumerate() {
                    if i > 0 {
                        w_color.newline();
                        w_color.write_colored(",", bracket_color);
                    }
                    // Inline format_delimited_value logic
                    if item.has_delimiters() && !item.is_empty() && !item.is_simple() {
                        w_color.newline();
                        item.format(w_color);
                    } else {
                        w_color.write_plain(" ");
                        item.format(w_color);
                    }
                }
                w_color.newline();
                w_color.write_colored("]", bracket_color);
            }
        });
        // Add final newline to match nixfmt output
        w.newline();
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
