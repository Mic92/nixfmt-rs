//! Nix abstract syntax tree.
//!
//! Split by concern: source spans, trivia (comments/whitespace), the
//! [`Annotated`] wrapper that attaches trivia + span to a payload, lexical
//! tokens, and the expression grammar itself. Everything is re-exported flat
//! so callers `use crate::ast::Expression` without naming submodules.

mod span;
mod trivia;
#[macro_use]
mod annotated;
mod expr;
mod items;
mod token;

pub use annotated::{Annotated, FirstToken, Trailed, TriviaSlot, TriviaSlotMut};
pub use expr::{
    Binder, Expression, File, NixString, ParamAttr, ParamDefault, Parameter, Selector, SetDefault,
    SimpleSelector, StringPart, Term,
};
pub use items::{Item, Items};
pub use span::Span;
pub use token::{Leaf, Token};
pub use trivia::{TrailingComment, Trivia, TriviaPiece};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trivia_is_two_words() {
        // Guard against accidentally regressing to a fatter representation;
        // every Annotated<T> in the AST embeds one of these.
        assert_eq!(std::mem::size_of::<Trivia>(), 16);
    }
}
