//! `Emit` impls for the wrapper / leaf types every other renderer builds on:
//! trivia, [`Annotated`], [`Item`], [`Trailed`] and bare [`Token`].

use crate::ast::{Annotated, Item, Token, Trailed, TrailingComment, Trivia, TriviaPiece};
use crate::doc::{Doc, Emit};

impl Emit for TrailingComment {
    fn emit(&self, doc: &mut Doc) {
        doc.hardspace()
            .trailing_comment(format!("# {}", self.0))
            .hardline();
    }
}

impl Emit for TriviaPiece {
    fn emit(&self, doc: &mut Doc) {
        match self {
            Self::EmptyLine => {
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

impl Emit for Trivia {
    fn emit(&self, doc: &mut Doc) {
        if self.is_empty() {
            return;
        }

        // Special case: single language annotation renders inline
        if self.len() == 1
            && let TriviaPiece::LanguageAnnotation(_) = &self[0]
        {
            self[0].emit(doc);
            return;
        }

        doc.hardline();
        for trivium in self {
            trivium.emit(doc);
        }
    }
}

impl<T: Emit> Emit for Annotated<T> {
    fn emit(&self, doc: &mut Doc) {
        self.pre_trivia.emit(doc);
        self.value.emit(doc);
        self.trail_comment.emit(doc);
    }
}

impl<T: Emit> Annotated<T> {
    /// Emit `pre_trivia` and value, leaving `trail_comment` for the caller to
    /// place elsewhere (typically inside a following nested group).
    pub(super) fn emit_head(&self, doc: &mut Doc) {
        self.pre_trivia.emit(doc);
        self.value.emit(doc);
    }

    /// Emit value and `trail_comment`, leaving `pre_trivia` for the caller to
    /// place elsewhere (typically inside a preceding nested group).
    pub(super) fn emit_tail(&self, doc: &mut Doc) {
        self.value.emit(doc);
        self.trail_comment.emit(doc);
    }
}

impl<T> Annotated<T> {
    /// Emit `pre_trivia`, then the value via `f`, then `trail_comment`.
    /// Used for `Annotated<T>` payloads that have no blanket `Emit` impl.
    pub(super) fn emit_with(&self, doc: &mut Doc, f: impl FnOnce(&mut Doc, &T)) {
        self.pre_trivia.emit(doc);
        f(doc, &self.value);
        self.trail_comment.emit(doc);
    }
}

impl<T: Emit> Emit for Item<T> {
    fn emit(&self, doc: &mut Doc) {
        match self {
            Self::Comments(trivia) => trivia.emit(doc),
            Self::Item(x) => {
                doc.group(|d| x.emit(d));
            }
        }
    }
}

impl Emit for Token {
    fn emit(&self, doc: &mut Doc) {
        if let Self::EnvPath(s) = self {
            doc.text(format!("<{s}>"));
            return;
        }
        doc.text(self.text());
    }
}

impl<T: Emit> Emit for Trailed<T> {
    fn emit(&self, doc: &mut Doc) {
        doc.group(|doc| {
            self.value.emit(doc);
            self.trailing_trivia.emit(doc);
        });
        // No trailing Hardline: reference nixfmt's `--ir` output does not emit one.
    }
}
