//! Width-probing primitives for the greedy layout engine: render a chain of
//! [`Elem`]s as if compact and report whether/how-far it fits a budget.

use super::{Look, LookIter};
use crate::predoc::{Elem, Spacing, TextKind, text_width};

/// Shared engine for `fits` / `fits_width`. Mirrors `fits` in Nixfmt/Predoc.hs.
///
/// `next_indent_delta` is the next-line indentation delta used only by the
/// trailing-comment rule; `budget` is the remaining width budget. Groups are
/// flattened in place so adjacent spacings across a group boundary merge
/// exactly as in the Haskell `ys ++ xs` recursion, and so comment text inside
/// a group never gets double-counted against `budget`.
///
/// `WRITE` selects whether the compact rendering is appended to `out` (and
/// rolled back on failure). Monomorphised so the width-only path carries no
/// branch or `&mut String` overhead.
#[inline]
fn fits_impl<const WRITE: bool>(
    mut next_indent_delta: isize,
    mut budget: isize,
    chain: &[&[Elem]],
    out: &mut String,
) -> Option<usize> {
    let mark = out.len();
    let mut width = 0usize;
    if budget < 0 {
        return None;
    }

    let mut stack: Vec<std::slice::Iter<'_, Elem>> = Vec::with_capacity(chain.len() + 4);
    for slice in chain.iter().rev() {
        if !slice.is_empty() {
            stack.push(slice.iter());
        }
    }
    let mut pending: Option<Spacing> = None;

    macro_rules! fail {
        () => {{
            if WRITE {
                out.truncate(mark);
            }
            return None;
        }};
    }

    loop {
        let elem = loop {
            let Some(iter) = stack.last_mut() else {
                break None;
            };
            match iter.next() {
                Some(Elem::Group(_, body)) => stack.push(body.iter()),
                Some(e) => break Some(e),
                None => {
                    stack.pop();
                }
            }
        };

        if let Some(Elem::Spacing(s)) = elem {
            pending = Some(pending.map_or(*s, |p| p.merge(*s)));
            continue;
        }

        if let Some(spacing) = pending.take() {
            match spacing {
                Spacing::Softbreak | Spacing::Break => {}
                Spacing::Softspace | Spacing::Space | Spacing::Hardspace => {
                    if WRITE {
                        out.push(' ');
                    }
                    width += 1;
                    budget -= 1;
                    next_indent_delta -= 1;
                    if budget < 0 {
                        fail!();
                    }
                }
                Spacing::Hardline | Spacing::Emptyline | Spacing::Newlines(_) => fail!(),
            }
        }

        match elem {
            None => return Some(width),
            Some(Elem::Text(_, _, TextKind::Regular, t)) => {
                let len = text_width(t);
                if WRITE {
                    out.push_str(t);
                }
                width += len;
                budget -= len.cast_signed();
                next_indent_delta -= len.cast_signed();
                if budget < 0 {
                    fail!();
                }
            }
            Some(Elem::Text(_, _, TextKind::Comment, t)) => {
                if WRITE {
                    out.push_str(t);
                }
                width += text_width(t);
            }
            Some(Elem::Text(_, _, TextKind::TrailingComment, t)) => {
                if next_indent_delta == 0 {
                    if WRITE {
                        out.push(' ');
                    }
                    width += 1;
                }
                if WRITE {
                    out.push_str(t);
                }
                width += text_width(t);
            }
            Some(Elem::Text(_, _, TextKind::Trailing, _)) => {}
            Some(Elem::Spacing(_) | Elem::Group(_, _) | Elem::Nest(..)) => unreachable!(),
        }
    }
}

/// Try to render `chain` compactly into `out`; on failure `out` is restored.
#[inline]
pub(super) fn fits(
    next_indent_delta: isize,
    budget: isize,
    chain: &[&[Elem]],
    out: &mut String,
) -> Option<usize> {
    fits_impl::<true>(next_indent_delta, budget, chain, out)
}

/// Width-only variant used by `first_line_fits`.
#[inline]
fn fits_width(budget: isize, doc: &[Elem]) -> Option<usize> {
    let mut sink = String::new();
    fits_impl::<false>(0, budget, &[doc], &mut sink)
}

/// Mirrors `firstLineWidth` in Nixfmt/Predoc.hs.
pub(super) fn first_line_width(chain: Look<'_>) -> usize {
    let mut width = 0;
    let mut iter = LookIter::new(chain);
    let mut pending: Option<Spacing> = None;
    loop {
        let elem = loop {
            match iter.next() {
                Some(Elem::Group(_, body)) => iter.push_front(body),
                e => break e,
            }
        };
        if let Some(Elem::Spacing(s)) = elem {
            pending = Some(pending.map_or(*s, |p| p.merge(*s)));
            continue;
        }
        if let Some(spacing) = pending.take() {
            if spacing == Spacing::Hardspace {
                width += 1;
            } else {
                return width;
            }
        }
        match elem {
            None => return width,
            Some(Elem::Text(_, _, TextKind::Comment | TextKind::TrailingComment, _)) => {}
            Some(Elem::Text(_, _, _, t)) => width += text_width(t),
            Some(Elem::Spacing(_) | Elem::Group(_, _) | Elem::Nest(..)) => unreachable!(),
        }
    }
}

/// Mirrors `firstLineFits` in Nixfmt/Predoc.hs.
pub(super) fn first_line_fits(target_width: usize, max_width: usize, chain: Look<'_>) -> bool {
    let max = max_width.cast_signed();
    let target = target_width.cast_signed();
    let mut budget = max;
    let mut iter = LookIter::new(chain);
    let mut pending: Option<Spacing> = None;
    let mut rest: Vec<&[Elem]> = Vec::new();
    loop {
        if budget < 0 {
            return false;
        }
        let elem = iter.next();
        if let Some(Elem::Spacing(s)) = elem {
            pending = Some(pending.map_or(*s, |p| p.merge(*s)));
            continue;
        }
        if let Some(spacing) = pending.take() {
            if spacing == Spacing::Hardspace {
                budget -= 1;
                if budget < 0 {
                    return false;
                }
            } else {
                return max - budget <= target;
            }
        }
        match elem {
            None => return max - budget <= target,
            Some(Elem::Text(_, _, TextKind::Regular, t)) => budget -= text_width(t).cast_signed(),
            Some(Elem::Text(..) | Elem::Nest(..)) => {}
            Some(Elem::Group(_, body)) => {
                rest.clear();
                rest.extend(iter.remaining());
                let rest_width = first_line_width(&rest);
                match fits_width(budget - rest_width.cast_signed(), body) {
                    Some(w) => budget -= w.cast_signed(),
                    None => iter.push_front(body),
                }
            }
            Some(Elem::Spacing(_)) => unreachable!(),
        }
    }
}

/// Mirrors `nextIndent` in Nixfmt/Predoc.hs.
pub(super) fn next_indent(chain: Look<'_>) -> (usize, usize) {
    for slice in chain {
        for elem in *slice {
            match elem {
                Elem::Text(nest, offset, _, _) => return (*nest, *offset),
                Elem::Group(_, body) => return next_indent(&[body]),
                Elem::Spacing(_) | Elem::Nest(..) => {}
            }
        }
    }
    (0, 0)
}
