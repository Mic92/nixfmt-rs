//! PrettySimple implementations for IR (intermediate representation) nodes

use super::{PrettySimple, Writer};
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
            // NOTE: Vec calls w_color.with_depth here, but we DON'T
            // This keeps IR at depth 0 instead of incrementing to depth 1

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
        match self {
            Spacing::Softbreak => w.write_plain("Softbreak"),
            Spacing::Break => w.write_plain("Break"),
            Spacing::Hardspace => w.write_plain("Hardspace"),
            Spacing::Softspace => w.write_plain("Softspace"),
            Spacing::Space => w.write_plain("Space"),
            Spacing::Hardline => w.write_plain("Hardline"),
            Spacing::Emptyline => w.write_plain("Emptyline"),
            Spacing::Newlines(n) => {
                w.write_plain("Newlines ");
                w.write_plain(&n.to_string());
            }
        }
    }

    fn is_simple(&self) -> bool {
        true
    }
}

impl PrettySimple for GroupAnn {
    fn format<W: Writer>(&self, w: &mut W) {
        match self {
            GroupAnn::RegularG => w.write_plain("RegularG"),
            GroupAnn::Priority => w.write_plain("Priority"),
            GroupAnn::Transparent => w.write_plain("Transparent"),
        }
    }

    fn is_simple(&self) -> bool {
        true
    }
}

impl PrettySimple for TextAnn {
    fn format<W: Writer>(&self, w: &mut W) {
        match self {
            TextAnn::RegularT => w.write_plain("RegularT"),
            TextAnn::Comment => w.write_plain("Comment"),
            TextAnn::TrailingComment => w.write_plain("TrailingComment"),
            TextAnn::Trailing => w.write_plain("Trailing"),
        }
    }

    fn is_simple(&self) -> bool {
        true
    }
}

impl PrettySimple for DocE {
    fn format<W: Writer>(&self, w: &mut W) {
        match self {
            DocE::Text(nest, off, ann, text) => {
                w.write_plain("Text ");
                // Color the numbers to match nixfmt output
                w.write_colored(&nest.to_string(), "\x1b[0;92;1m"); // Green bold
                w.write_plain(" ");
                w.write_colored(&off.to_string(), "\x1b[0;92;1m"); // Green bold
                w.write_plain(" ");
                ann.format(w);
                w.write_plain(" ");
                text.format(w);
            }
            DocE::Spacing(sp) => {
                w.write_plain("Spacing ");
                sp.format(w);
            }
            DocE::Group(ann, doc) => {
                w.write_plain("Group ");
                ann.format(w);
                w.newline();
                doc.format(w);
            }
        }
    }

    fn is_simple(&self) -> bool {
        matches!(self, DocE::Spacing(_))
    }
}
