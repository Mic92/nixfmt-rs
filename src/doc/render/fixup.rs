//! Normalise a freshly built [`Doc`] before layout: lift hard spacings and
//! comments out of groups (so the renderer's group-fits test sees only the
//! soft content), merge adjacent spacings, concatenate adjacent text, drop
//! empty groups, and bake `Nest` deltas into each `Text` so the renderer
//! never sees `Nest`.

use crate::doc::{Doc, Elem, GroupKind, Spacing, TextKind};

impl Elem {
    /// Check if element is hard spacing (always rendered as-is)
    const fn is_hard_spacing(&self) -> bool {
        matches!(
            self,
            Self::Spacing(
                Spacing::Hardspace | Spacing::Hardline | Spacing::Emptyline | Spacing::Newlines(_)
            )
        )
    }

    fn is_comment(&self) -> bool {
        match self {
            Self::Text(_, _, TextKind::Comment | TextKind::TrailingComment, _) => true,
            Self::Group(_, inner) => inner.iter().all(|x| x.is_comment() || x.is_hard_spacing()),
            _ => false,
        }
    }
}

/// `simplifyGroup` (Predoc.hs): unwrap `Group ann [Group ann xs]` to `xs`.
fn simplify_group(ann: GroupKind, mut body: Doc) -> Doc {
    if body.len() == 1
        && matches!(&body[0], Elem::Group(a2, _) if ann == *a2)
        && let Some(Elem::Group(_, inner)) = body.0.pop()
    {
        return inner;
    }
    body
}

/// Span of leading liftable elements (hard spacings + comments) and start of
/// trailing liftable elements (hard spacings) in a fixed-up group body.
fn lift_bounds(body: &[Elem]) -> (usize, usize) {
    let pre_end = body
        .iter()
        .position(|e| !e.is_hard_spacing() && !e.is_comment())
        .unwrap_or(body.len());
    let post_start = body
        .iter()
        .rposition(|e| !e.is_hard_spacing())
        .map_or(0, |p| p + 1)
        .max(pre_end);
    (pre_end, post_start)
}

/// Cheap placeholder used to vacate a slot during the read/write compaction.
const HOLE: Elem = Elem::Spacing(Spacing::Softbreak);

/// Outcome of lifting hard spacings/comments out of a fixed-up group body.
enum GroupFixup {
    /// Nothing to lift; body becomes the (simplified) group contents.
    Keep(Doc),
    /// Core is empty: the group dissolves into its lifted surroundings.
    Dissolve { pre: Doc, post: Doc },
    /// `pre` and `post` are lifted to the parent; `core` stays grouped.
    Lift { pre: Doc, core: Doc, post: Doc },
}

/// Classify a recursively fixed-up group body. Pure in `body`; the caller
/// handles splicing the result back into the parent's read/write window.
fn split_liftable(ann: GroupKind, mut body: Doc) -> GroupFixup {
    let (pre_end, post_start) = lift_bounds(&body);
    if pre_end == 0 && post_start == body.len() && !body.is_empty() {
        return GroupFixup::Keep(simplify_group(ann, body));
    }
    let post = Doc(body.0.split_off(post_start));
    let core = Doc(body.0.split_off(pre_end));
    let pre = body;
    if core.is_empty() {
        GroupFixup::Dissolve { pre, post }
    } else {
        GroupFixup::Lift {
            pre,
            core: simplify_group(ann, core),
            post,
        }
    }
}

/// In-place `fixup`. Walks `doc` with a read index and write index
/// (`write_idx <= read_idx`), recursing into group bodies via `&mut` so the
/// existing `Vec` allocations are reused. Mirrors Haskell `fixup`
/// clause-by-clause; see the per-arm comments for the corresponding rule.
pub(super) fn fixup_mut(doc: &mut Vec<Elem>, mut nest_acc: isize, mut offset_acc: isize) {
    let mut read_idx = 0usize;
    let mut write_idx = 0usize;
    while read_idx < doc.len() {
        let elem = std::mem::replace(&mut doc[read_idx], HOLE);
        read_idx += 1;
        match elem {
            Elem::Nest(dn, doff) => {
                nest_acc += dn;
                offset_acc += doff;
            }

            // `Spacing a : Spacing b : ys` — fold into the next slot, or into
            // the previous written slot when a `Nest` marker sat in between.
            Elem::Spacing(a) => {
                if let Some(Elem::Spacing(b)) = doc.get(read_idx) {
                    doc[read_idx] = Elem::Spacing(a.merge(*b));
                } else if matches!(
                    write_idx.checked_sub(1).map(|i| &doc[i]),
                    Some(Elem::Spacing(_))
                ) {
                    if let Elem::Spacing(b) = &mut doc[write_idx - 1] {
                        *b = b.merge(a);
                    }
                } else {
                    doc[write_idx] = Elem::Spacing(a);
                    write_idx += 1;
                }
            }

            // `Text ann a : Text ann b : ys` — concatenate into the previous
            // written slot, keeping the first text's (already baked) indent.
            Elem::Text(nest, offset, ann, txt) => {
                if write_idx > 0
                    && let Elem::Text(_, _, prev_ann, prev_txt) = &mut doc[write_idx - 1]
                    && ann == *prev_ann
                {
                    prev_txt.push_str(&txt);
                    continue;
                }
                let nest = nest.cast_signed() + nest_acc;
                let offset = offset.cast_signed() + offset_acc;
                debug_assert!(nest >= 0 && offset >= 0, "unbalanced Nest deltas");
                doc[write_idx] = Elem::Text(nest.cast_unsigned(), offset.cast_unsigned(), ann, txt);
                write_idx += 1;
            }

            Elem::Group(ann, mut body) => {
                // `Spacing Hardspace : Group ann xs : ys` — pull a just-written
                // hardspace into the group so it can merge with a leading soft
                // spacing during the recursive fixup.
                if write_idx > 0 && matches!(doc[write_idx - 1], Elem::Spacing(Spacing::Hardspace))
                {
                    write_idx -= 1;
                    doc[write_idx] = HOLE;
                    body.0.insert(0, Elem::Spacing(Spacing::Hardspace));
                }
                fixup_mut(&mut body.0, nest_acc, offset_acc);

                match split_liftable(ann, body) {
                    GroupFixup::Keep(body) => {
                        doc[write_idx] = Elem::Group(ann, body);
                        write_idx += 1;
                    }
                    GroupFixup::Dissolve { pre, post } => {
                        // `fixup $ (a : pre) ++ post ++ ys`. Put the lifted
                        // pieces back on the read side. Their `Text` nodes
                        // already carry the baked indent, so wrap with a
                        // `Nest` that cancels the running accumulator for the
                        // reprocess.
                        let mut lifted = Vec::with_capacity(pre.len() + post.len() + 2);
                        lifted.push(Elem::Nest(-nest_acc, -offset_acc));
                        lifted.extend(pre);
                        lifted.extend(post);
                        lifted.push(Elem::Nest(nest_acc, offset_acc));
                        doc.splice(write_idx..read_idx, lifted);
                        read_idx = write_idx;
                    }
                    GroupFixup::Lift {
                        mut pre,
                        core,
                        post,
                    } => {
                        // The lifted prefix is already fixed internally, so
                        // the only remaining rewrite is a possible spacing
                        // merge across the boundary with `doc[write_idx-1]`.
                        if write_idx > 0
                            && let (Elem::Spacing(prev), Some(Elem::Spacing(first))) =
                                (&doc[write_idx - 1], pre.first())
                        {
                            let merged = prev.merge(*first);
                            doc[write_idx - 1] = Elem::Spacing(merged);
                            pre.0.remove(0);
                        }
                        let pre_len = pre.len();
                        // Finalise `pre ++ [Group ann core]` into the write
                        // side and leave `post` on the read side for
                        // `fixup (post ++ ys)`.
                        doc.splice(
                            write_idx..read_idx,
                            pre.into_iter()
                                .chain(std::iter::once(Elem::Group(ann, core)))
                                .chain(post),
                        );
                        write_idx += pre_len + 1;
                        read_idx = write_idx;
                    }
                }
            }
        }
    }
    doc.truncate(write_idx);
}
