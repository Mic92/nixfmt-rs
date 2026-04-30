use crate::predoc::{Doc, Pretty, hardline, hardspace};
use crate::types::*;

/// Whether a set/absorbed term should prefer its expanded (multi-line)
/// layout. Replaces the unlabelled `Bool` argument of Haskell `prettySet`
/// and `absorbExpr`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Width {
    Regular,
    Wide,
}

// ---------------------------------------------------------------------------
// Classification predicates
//
// These mirror the small predicates scattered through `Nixfmt/Types.hs` and
// `Nixfmt/Pretty.hs` in the reference implementation. Each is documented with
// its Haskell counterpart so behavioural drift is easy to audit.
// ---------------------------------------------------------------------------

/// Haskell `hasTrivia` (Types.hs): annotation carries leading or trailing trivia.
pub(super) fn has_trivia<T>(ann: &Ann<T>) -> bool {
    !ann.pre_trivia.is_empty() || ann.trail_comment.is_some()
}

/// Haskell `LoneAnn` pattern (Types.hs): annotation with no surrounding trivia.
pub(super) fn is_lone_ann<T>(ann: &Ann<T>) -> bool {
    !has_trivia(ann)
}

/// Haskell `hasPreTrivia` (Types.hs).
pub(super) fn has_pre_trivia<T>(ann: &Ann<T>) -> bool {
    !ann.pre_trivia.is_empty()
}

/// Haskell `matchFirstToken hasPreTrivia` (Types.hs), specialised to `Term`.
pub(super) fn term_first_token_has_pre_trivia(term: &Term) -> bool {
    match term {
        Term::Token(l) => has_pre_trivia(l),
        Term::SimpleString(s) | Term::IndentedString(s) => has_pre_trivia(s),
        Term::Path(p) => has_pre_trivia(p),
        Term::List(open, _, _) => has_pre_trivia(open),
        Term::Set(Some(rec), _, _, _) => has_pre_trivia(rec),
        Term::Set(None, open, _, _) => has_pre_trivia(open),
        Term::Selection(inner, _, _) => term_first_token_has_pre_trivia(inner),
        Term::Parenthesized(open, _, _) => has_pre_trivia(open),
    }
}

/// Haskell `hasOnlyComments` (Pretty.hs): non-empty `Items` containing only comment items.
pub(super) fn items_has_only_comments<T>(items: &Items<T>) -> bool {
    !items.0.is_empty() && items.0.iter().all(|i| matches!(i, Item::Comments(_)))
}

pub(super) fn is_spaces(s: &str) -> bool {
    s.chars().all(|c| c.is_whitespace())
}

/// Haskell `isSimpleSelector` (Pretty.hs).
pub(super) fn is_simple_selector(selector: &Selector) -> bool {
    matches!(selector.selector, SimpleSelector::ID(_))
}

/// Haskell `isSimple` (Pretty.hs), `Term` arm; split out so list items can be
/// classified without wrapping them in an `Expression`.
pub(super) fn is_simple_term(term: &Term) -> bool {
    match term {
        Term::SimpleString(s) | Term::IndentedString(s) => is_lone_ann(s),
        Term::Path(p) => is_lone_ann(p),
        Term::Token(leaf)
            if is_lone_ann(leaf)
                && matches!(
                    leaf.value,
                    Token::Identifier(_) | Token::Integer(_) | Token::Float(_) | Token::EnvPath(_)
                ) =>
        {
            true
        }
        Term::Selection(term, selectors, def) => {
            is_simple_term(term) && selectors.iter().all(is_simple_selector) && def.is_none()
        }
        Term::Parenthesized(open, expr, close) => {
            is_lone_ann(open) && is_lone_ann(close) && is_simple_expression(expr)
        }
        _ => false,
    }
}

/// Haskell `isSimple` (Pretty.hs).
pub(super) fn is_simple_expression(expr: &Expression) -> bool {
    match expr {
        Expression::Term(term) => is_simple_term(term),
        Expression::Application(f, a) => {
            // No more than two arguments.
            if let Expression::Application(f2, _) = &**f {
                if matches!(**f2, Expression::Application(_, _)) {
                    return false;
                }
            }
            is_simple_expression(f) && is_simple_expression(a)
        }
        _ => false,
    }
}

// ---------------------------------------------------------------------------

/// Render an empty bracketed container (`[]`, `{}`), preserving a user-inserted
/// line break between the delimiters. Shared by empty list / set / param-set.
pub(super) fn push_empty_brackets(doc: &mut Doc, open: &Leaf, close: &Leaf) {
    open.pretty(doc);
    if open.span.start_line != close.span.start_line {
        doc.push(hardline());
    } else {
        doc.push(hardspace());
    }
    close.pretty(doc);
}

pub(super) fn pretty_ann_with<T>(doc: &mut Doc, ann: &Ann<T>, f: impl FnOnce(&mut Doc, &T)) {
    ann.pre_trivia.pretty(doc);
    f(doc, &ann.value);
    ann.trail_comment.pretty(doc);
}

/// Move a trailing comment on a token into its leading trivia.
/// Mirrors Haskell `moveTrailingCommentUp` (Pretty.hs).
pub(super) fn move_trailing_comment_up<T: Clone>(ann: &Ann<T>) -> Ann<T> {
    let mut out = ann.clone();
    if let Some(tc) = out.trail_comment.take() {
        out.pre_trivia.push(Trivium::from(&tc));
    }
    out
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
