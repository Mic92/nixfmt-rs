//! Convert IR ([`Doc`]) to formatted text.
//!
//! Three stages: [`fixup`] normalises a freshly built `Doc` (lifts hard
//! spacings out of groups, merges adjacent spacings, drops empty groups);
//! [`fits`] provides the width-probing primitives the layout engine uses to
//! decide whether a group can stay on one line; [`layout`] drives the greedy
//! Wadler/Leijen renderer that produces the final string.

use super::{Doc, Elem};

mod fits;
mod fixup;
mod layout;

pub use layout::RenderConfig;

/// Borrowed lookahead: a chain of slices scanned in order. Lets the layout
/// engine pass "rest of current level ++ outer lookahead" without cloning.
type Look<'a> = &'a [&'a [Elem]];

/// Flat iterator over a [`Look`] chain. Callers may [`Self::push_front`] a
/// group body to get the `xs ++ ys` traversal Haskell gets for free from lazy
/// lists.
struct LookIter<'a> {
    stack: Vec<(&'a [Elem], usize)>,
}

impl<'a> LookIter<'a> {
    fn new(chain: Look<'a>) -> Self {
        let mut stack: Vec<(&'a [Elem], usize)> = Vec::with_capacity(chain.len());
        for s in chain.iter().rev() {
            if !s.is_empty() {
                stack.push((s, 0));
            }
        }
        LookIter { stack }
    }

    fn push_front(&mut self, s: &'a [Elem]) {
        if !s.is_empty() {
            self.stack.push((s, 0));
        }
    }

    /// Remaining slices in traversal order, for re-seeding a fresh measurement.
    fn remaining(&self) -> impl Iterator<Item = &'a [Elem]> + '_ {
        self.stack.iter().rev().map(|(s, i)| &s[*i..])
    }
}

impl<'a> Iterator for LookIter<'a> {
    type Item = &'a Elem;
    fn next(&mut self) -> Option<&'a Elem> {
        while let Some((s, i)) = self.stack.last_mut() {
            if *i < s.len() {
                let e = &s[*i];
                *i += 1;
                return Some(e);
            }
            self.stack.pop();
        }
        None
    }
}

impl Doc {
    pub fn render(self, config: &RenderConfig) -> String {
        layout::layout_greedy(config.width, config.indent_width, self)
    }

    /// Normalise a freshly built document for rendering: lift hard spacings
    /// and comments out of groups, merge adjacent spacings, drop empty groups.
    pub fn fixup(mut self) -> Self {
        fixup::fixup_mut(&mut self.0, 0, 0);
        self
    }
}
