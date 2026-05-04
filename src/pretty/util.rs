use crate::predoc::{Doc, Pretty};
use crate::types::{Ann, Leaf, Token, Trivia, Trivium};

/// Whether a set/absorbed term should prefer its expanded (multi-line)
/// layout. Replaces the unlabelled `Bool` argument of Haskell `prettySet`
/// and `absorbExpr`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Width {
    Regular,
    Wide,
}

pub(super) fn is_spaces(s: &str) -> bool {
    s.chars().all(char::is_whitespace)
}

/// Render an empty bracketed container (`[]`, `{}`), preserving a user-inserted
/// line break between the delimiters. Shared by empty list / set / param-set.
pub(super) fn push_empty_brackets(doc: &mut Doc, open: &Leaf, close: &Leaf) {
    open.pretty(doc);
    if open.span.start_line() == close.span.start_line() {
        doc.hardspace();
    } else {
        doc.hardline();
    }
    close.pretty(doc);
}

pub(super) fn pretty_ann_with<T>(doc: &mut Doc, ann: &Ann<T>, f: impl FnOnce(&mut Doc, &T)) {
    ann.pre_trivia.pretty(doc);
    f(doc, &ann.value);
    ann.trail_comment.pretty(doc);
}

/// Shared trivia juggling for parenthesized rendering: strips the opening
/// token's trailing comment (returned as `Trivia`) and the closing token's
/// leading trivia so callers can re-emit them inside the nested body.
pub(super) fn split_paren_trivia(
    open: &Ann<Token>,
    close: &Ann<Token>,
) -> (Ann<Token>, Trivia, Trivia, Ann<Token>) {
    let mut open = open.clone();
    let trail: Trivia = open
        .trail_comment
        .take()
        .map(|tc| vec![Trivium::from(&tc)])
        .unwrap_or_default()
        .into();
    let mut close = close.clone();
    let close_pre = std::mem::replace(&mut close.pre_trivia, Trivia::new());
    (open, trail, close_pre, close)
}
