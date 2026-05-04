//! [`Annotated`]: wraps a payload with leading trivia, source span and an
//! optional trailing comment. Every leaf in the AST is an `Annotated<_>`.

use super::span::Span;
use super::trivia::{TrailingComment, Trivia, TriviaPiece};

/// Annotated wrapper - every AST node has:
/// - `pre_trivia`: Comments/whitespace before the token
/// - span: Byte range in source
/// - value: The actual value
/// - `trail_comment`: Optional trailing comment on same line
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Annotated<T> {
    pub pre_trivia: Trivia,
    pub span: Span,
    pub value: T,
    pub trail_comment: Option<TrailingComment>,
}

impl<T> Annotated<T> {
    /// Annotation carries leading or trailing trivia (Haskell `hasTrivia` /
    /// negation of the `LoneAnn` pattern in Types.hs).
    pub fn has_trivia(&self) -> bool {
        !self.pre_trivia.is_empty() || self.trail_comment.is_some()
    }
}

impl<T: Clone> Annotated<T> {
    /// Move a trailing comment on a token into its leading trivia.
    /// Mirrors Haskell `moveTrailingCommentUp` (Pretty.hs).
    pub fn move_trailing_comment_up(&self) -> Self {
        let mut out = self.clone();
        if let Some(tc) = out.trail_comment.take() {
            out.pre_trivia.push(TriviaPiece::from(&tc));
        }
        out
    }
}

/// Type-erased shared view of an `Annotated<_>`'s trivia fields.
///
/// Lets `FirstToken` return a uniform borrow regardless of the underlying
/// `Annotated<T>` payload type (`Token`, `NixString`, `Path`, ...).
pub struct TriviaSlot<'a> {
    pub pre_trivia: &'a Trivia,
    pub trail_comment: &'a Option<TrailingComment>,
}

/// Mutable counterpart of [`TriviaSlot`].
pub struct TriviaSlotMut<'a> {
    pub pre_trivia: &'a mut Trivia,
    pub trail_comment: &'a mut Option<TrailingComment>,
}

impl<'a, T> From<&'a Annotated<T>> for TriviaSlot<'a> {
    fn from(a: &'a Annotated<T>) -> Self {
        TriviaSlot {
            pre_trivia: &a.pre_trivia,
            trail_comment: &a.trail_comment,
        }
    }
}

impl<'a, T> From<&'a mut Annotated<T>> for TriviaSlotMut<'a> {
    fn from(a: &'a mut Annotated<T>) -> Self {
        TriviaSlotMut {
            pre_trivia: &mut a.pre_trivia,
            trail_comment: &mut a.trail_comment,
        }
    }
}

/// Walk to the leftmost leaf `Annotated<_>` of an AST node.
///
/// Haskell analogue: `mapFirstToken'` / `matchFirstToken` (Types.hs).
pub trait FirstToken {
    fn first_token(&self) -> TriviaSlot<'_>;
    fn first_token_mut(&mut self) -> TriviaSlotMut<'_>;
}

/// Expand one match arm for `first_token_impl!`: `leaf` arms hit an `Annotated<_>`
/// directly via `.into()`, `recurse` arms call `$rec` (`first_token` or
/// `first_token_mut`) on a child node.
macro_rules! first_token_arm {
    (leaf, $rec:ident, $e:expr) => {
        $e.into()
    };
    (recurse, $rec:ident, $e:expr) => {
        $e.$rec()
    };
}

/// Generate both `first_token` and `first_token_mut` from one set of match
/// arms, avoiding the otherwise-identical `&`/`&mut` duplication.
macro_rules! first_token_impl {
    ($ty:ty; $($pat:pat => $kind:ident $e:expr),+ $(,)?) => {
        impl $crate::ast::FirstToken for $ty {
            fn first_token(&self) -> $crate::ast::TriviaSlot<'_> {
                match self { $($pat => first_token_arm!($kind, first_token, $e),)+ }
            }
            fn first_token_mut(&mut self) -> $crate::ast::TriviaSlotMut<'_> {
                match self { $($pat => first_token_arm!($kind, first_token_mut, $e),)+ }
            }
        }
    };
}

/// A value followed by trailing trivia (comments/whitespace) up to the next
/// closing delimiter or EOF. Used for whole files and interpolation bodies.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Trailed<T> {
    pub value: T,
    pub trailing_trivia: Trivia,
}
