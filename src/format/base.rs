//! `Pretty` impls for the wrapper / leaf types every other renderer builds on:
//! trivia, [`Annotated`], [`Item`], [`Trailed`] and bare [`Token`].

use crate::ast::{Annotated, Item, Token, Trailed, TrailingComment, Trivia, TriviaPiece};
use crate::doc::{Doc, Pretty};

impl Pretty for TrailingComment {
    fn pretty(&self, doc: &mut Doc) {
        doc.hardspace()
            .trailing_comment(format!("# {}", self.0))
            .hardline();
    }
}

impl Pretty for TriviaPiece {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Self::EmptyLine() => {
                doc.emptyline();
            }
            Self::LineComment(c) => {
                doc.comment(format!("#{c}")).hardline();
            }
            Self::BlockComment(is_doc, lines) => {
                doc.comment(if *is_doc { "/**" } else { "/*" }).hardline();
                // Indent the comment using offset instead of nest
                doc.offset(2, |offset_doc| {
                    for line in lines {
                        if line.is_empty() {
                            offset_doc.emptyline();
                        } else {
                            offset_doc.comment(&**line).hardline();
                        }
                    }
                });
                doc.comment("*/").hardline();
            }
            Self::LanguageAnnotation(lang) => {
                doc.comment(format!("/* {lang} */")).hardspace();
            }
        }
    }
}

impl Pretty for Trivia {
    fn pretty(&self, doc: &mut Doc) {
        if self.is_empty() {
            return;
        }

        // Special case: single language annotation renders inline
        if self.len() == 1
            && let TriviaPiece::LanguageAnnotation(_) = &self[0]
        {
            self[0].pretty(doc);
            return;
        }

        doc.hardline();
        for trivium in self {
            trivium.pretty(doc);
        }
    }
}

impl<T: Pretty> Pretty for Annotated<T> {
    fn pretty(&self, doc: &mut Doc) {
        self.pre_trivia.pretty(doc);
        self.value.pretty(doc);
        self.trail_comment.pretty(doc);
    }
}

impl<T: Pretty> Annotated<T> {
    /// Emit `pre_trivia` and value, leaving `trail_comment` for the caller to
    /// place elsewhere (typically inside a following nested group).
    pub(super) fn pretty_head(&self, doc: &mut Doc) {
        self.pre_trivia.pretty(doc);
        self.value.pretty(doc);
    }

    /// Emit value and `trail_comment`, leaving `pre_trivia` for the caller to
    /// place elsewhere (typically inside a preceding nested group).
    pub(super) fn pretty_tail(&self, doc: &mut Doc) {
        self.value.pretty(doc);
        self.trail_comment.pretty(doc);
    }
}

impl<T> Annotated<T> {
    /// Emit `pre_trivia`, then the value via `f`, then `trail_comment`.
    /// Used for `Annotated<T>` payloads that have no blanket `Pretty` impl.
    pub(super) fn pretty_with(&self, doc: &mut Doc, f: impl FnOnce(&mut Doc, &T)) {
        self.pre_trivia.pretty(doc);
        f(doc, &self.value);
        self.trail_comment.pretty(doc);
    }
}

impl<T: Pretty> Pretty for Item<T> {
    fn pretty(&self, doc: &mut Doc) {
        match self {
            Self::Comments(trivia) => trivia.pretty(doc),
            Self::Item(x) => {
                doc.group(|d| x.pretty(d));
            }
        }
    }
}

impl Pretty for Token {
    fn pretty(&self, doc: &mut Doc) {
        if let Self::EnvPath(s) = self {
            doc.text(format!("<{s}>"));
            return;
        }
        doc.text(self.text());
    }
}

impl<T: Pretty> Pretty for Trailed<T> {
    fn pretty(&self, doc: &mut Doc) {
        doc.group(|doc| {
            self.value.pretty(doc);
            self.trailing_trivia.pretty(doc);
        });
        // No trailing Hardline: reference nixfmt's `--ir` output does not emit one.
    }
}
