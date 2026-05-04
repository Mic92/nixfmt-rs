//! Convert IR (`Doc`) to formatted text.
//!
//! Implementation of the Wadler/Leijen layout algorithm from nixfmt: the
//! `fixup` normalisation pass, the width-probing `fits`/`firstLine*` helpers,
//! and the greedy `Renderer` that drives them.

use super::{Doc, DocE, GroupAnn, Spacing, TextAnn, text_width};

/// Borrowed lookahead: a chain of slices scanned in order. Lets the layout
/// engine pass "rest of current level ++ outer lookahead" without cloning.
type Look<'a> = &'a [&'a [DocE]];

/// Flat iterator over a `Look` chain. Callers may `push_front` a group body to
/// get the `xs ++ ys` traversal Haskell gets for free from lazy lists.
struct LookIter<'a> {
    stack: Vec<(&'a [DocE], usize)>,
}

impl<'a> LookIter<'a> {
    fn new(chain: Look<'a>) -> Self {
        let mut stack: Vec<(&'a [DocE], usize)> = Vec::with_capacity(chain.len());
        for s in chain.iter().rev() {
            if !s.is_empty() {
                stack.push((s, 0));
            }
        }
        LookIter { stack }
    }

    fn push_front(&mut self, s: &'a [DocE]) {
        if !s.is_empty() {
            self.stack.push((s, 0));
        }
    }
}

impl<'a> Iterator for LookIter<'a> {
    type Item = &'a DocE;
    fn next(&mut self) -> Option<&'a DocE> {
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

pub struct RenderConfig {
    /// Maximum line width (default: 100)
    pub width: usize,
    /// Indentation width in spaces (default: 2)
    pub indent_width: usize,
}

pub fn render_with_config(doc: Doc, config: &RenderConfig) -> String {
    layout_greedy(config.width, config.indent_width, doc)
}

/// `simplifyGroup` (Predoc.hs): unwrap `Group ann [Group ann xs]` to `xs`.
fn simplify_group(ann: GroupAnn, mut body: Doc) -> Doc {
    if body.len() == 1
        && matches!(&body[0], DocE::Group(a2, _) if ann == *a2)
        && let Some(DocE::Group(_, inner)) = body.0.pop()
    {
        return inner;
    }
    body
}

/// Span of leading liftable elements (hard spacings + comments) and start of
/// trailing liftable elements (hard spacings) in a fixed-up group body.
fn lift_bounds(body: &[DocE]) -> (usize, usize) {
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

impl DocE {
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
            Self::Text(_, _, TextAnn::Comment | TextAnn::TrailingComment, _) => true,
            Self::Group(_, inner) => inner.iter().all(|x| x.is_comment() || x.is_hard_spacing()),
            _ => false,
        }
    }
}

/// Merge two spacing elements (take maximum in ordering)
fn merge_spacings(a: Spacing, b: Spacing) -> Spacing {
    use Spacing::{Break, Emptyline, Hardspace, Newlines, Softbreak, Softspace, Space};

    let (min_sp, max_sp) = if a <= b { (a, b) } else { (b, a) };

    match (min_sp, max_sp) {
        (Break, Softspace | Hardspace) => Space,
        (Softbreak, Hardspace) => Softspace,
        (Newlines(x), Newlines(y)) => Newlines(x + y),
        (Emptyline, Newlines(x)) => Newlines(x + 2),
        (Hardspace, Newlines(x)) => Newlines(x),
        (_, Newlines(x)) => Newlines(x + 1),
        _ => max_sp,
    }
}

/// Manually force a group to compact layout (does not recurse into inner groups)
fn unexpand_spacing(chain: &[&[DocE]]) -> Doc {
    let mut result = Vec::new();
    for s in chain {
        for elem in *s {
            match elem {
                DocE::Spacing(Spacing::Space | Spacing::Softspace) => {
                    result.push(DocE::Spacing(Spacing::Hardspace));
                }
                DocE::Spacing(Spacing::Break | Spacing::Softbreak) | DocE::Nest(..) => {}
                _ => result.push(elem.clone()),
            }
        }
    }
    Doc(result)
}

/// Cheap pre-check so `render_group` can skip the clone-heavy `priority_groups`
/// machinery for groups that contain no Priority children.
fn has_priority_groups(doc: &[DocE]) -> bool {
    doc.iter().any(|e| match e {
        DocE::Group(GroupAnn::Priority, _) => true,
        DocE::Group(GroupAnn::Transparent, body) => has_priority_groups(body),
        _ => false,
    })
}

/// Fix up a Doc by:
/// - Moving hard spacings and comments out of groups
/// - Merging consecutive spacings
/// - Removing empty groups
pub fn fixup(mut doc: Doc) -> Doc {
    fixup_mut(&mut doc.0, 0, 0);
    doc
}

/// Cheap placeholder used to vacate a slot during the read/write compaction.
const HOLE: DocE = DocE::Spacing(Spacing::Softbreak);

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
fn split_liftable(ann: GroupAnn, mut body: Doc) -> GroupFixup {
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
fn fixup_mut(doc: &mut Vec<DocE>, mut nest_acc: isize, mut offset_acc: isize) {
    let mut read_idx = 0usize;
    let mut write_idx = 0usize;
    while read_idx < doc.len() {
        let elem = std::mem::replace(&mut doc[read_idx], HOLE);
        read_idx += 1;
        match elem {
            DocE::Nest(dn, doff) => {
                nest_acc += dn;
                offset_acc += doff;
            }

            // `Spacing a : Spacing b : ys` — fold into the next slot, or into
            // the previous written slot when a `Nest` marker sat in between.
            DocE::Spacing(a) => {
                if let Some(DocE::Spacing(b)) = doc.get(read_idx) {
                    doc[read_idx] = DocE::Spacing(merge_spacings(a, *b));
                } else if matches!(
                    write_idx.checked_sub(1).map(|i| &doc[i]),
                    Some(DocE::Spacing(_))
                ) {
                    if let DocE::Spacing(b) = &mut doc[write_idx - 1] {
                        *b = merge_spacings(*b, a);
                    }
                } else {
                    doc[write_idx] = DocE::Spacing(a);
                    write_idx += 1;
                }
            }

            // `Text ann a : Text ann b : ys` — concatenate into the previous
            // written slot, keeping the first text's (already baked) indent.
            DocE::Text(nest, offset, ann, txt) => {
                if write_idx > 0
                    && let DocE::Text(_, _, prev_ann, prev_txt) = &mut doc[write_idx - 1]
                    && ann == *prev_ann
                {
                    prev_txt.push_str(&txt);
                    continue;
                }
                let nest = nest.cast_signed() + nest_acc;
                let offset = offset.cast_signed() + offset_acc;
                debug_assert!(nest >= 0 && offset >= 0, "unbalanced Nest deltas");
                doc[write_idx] = DocE::Text(nest.cast_unsigned(), offset.cast_unsigned(), ann, txt);
                write_idx += 1;
            }

            DocE::Group(ann, mut body) => {
                // `Spacing Hardspace : Group ann xs : ys` — pull a just-written
                // hardspace into the group so it can merge with a leading soft
                // spacing during the recursive fixup.
                if write_idx > 0 && matches!(doc[write_idx - 1], DocE::Spacing(Spacing::Hardspace))
                {
                    write_idx -= 1;
                    doc[write_idx] = HOLE;
                    body.0.insert(0, DocE::Spacing(Spacing::Hardspace));
                }
                fixup_mut(&mut body.0, nest_acc, offset_acc);

                match split_liftable(ann, body) {
                    GroupFixup::Keep(body) => {
                        doc[write_idx] = DocE::Group(ann, body);
                        write_idx += 1;
                    }
                    GroupFixup::Dissolve { pre, post } => {
                        // `fixup $ (a : pre) ++ post ++ ys`. Put the lifted
                        // pieces back on the read side. Their `Text` nodes
                        // already carry the baked indent, so wrap with a
                        // `Nest` that cancels the running accumulator for the
                        // reprocess.
                        let mut lifted = Vec::with_capacity(pre.len() + post.len() + 2);
                        lifted.push(DocE::Nest(-nest_acc, -offset_acc));
                        lifted.extend(pre);
                        lifted.extend(post);
                        lifted.push(DocE::Nest(nest_acc, offset_acc));
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
                            && let (DocE::Spacing(prev), Some(DocE::Spacing(first))) =
                                (&doc[write_idx - 1], pre.first())
                        {
                            let merged = merge_spacings(*prev, *first);
                            doc[write_idx - 1] = DocE::Spacing(merged);
                            pre.0.remove(0);
                        }
                        let pre_len = pre.len();
                        // Finalise `pre ++ [Group ann core]` into the write
                        // side and leave `post` on the read side for
                        // `fixup (post ++ ys)`.
                        doc.splice(
                            write_idx..read_idx,
                            pre.into_iter()
                                .chain(std::iter::once(DocE::Group(ann, core)))
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
    chain: &[&[DocE]],
    out: &mut String,
) -> Option<usize> {
    let mark = out.len();
    let mut width = 0usize;
    if budget < 0 {
        return None;
    }

    let mut stack: Vec<std::slice::Iter<'_, DocE>> = Vec::with_capacity(chain.len() + 4);
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
                Some(DocE::Group(_, body)) => stack.push(body.iter()),
                Some(e) => break Some(e),
                None => {
                    stack.pop();
                }
            }
        };

        if let Some(DocE::Spacing(s)) = elem {
            pending = Some(pending.map_or(*s, |p| merge_spacings(p, *s)));
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
            Some(DocE::Text(_, _, TextAnn::RegularT, t)) => {
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
            Some(DocE::Text(_, _, TextAnn::Comment, t)) => {
                if WRITE {
                    out.push_str(t);
                }
                width += text_width(t);
            }
            Some(DocE::Text(_, _, TextAnn::TrailingComment, t)) => {
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
            Some(DocE::Text(_, _, TextAnn::Trailing, _)) => {}
            Some(DocE::Spacing(_) | DocE::Group(_, _) | DocE::Nest(..)) => unreachable!(),
        }
    }
}

/// Try to render `chain` compactly into `out`; on failure `out` is restored.
#[inline]
fn fits(
    next_indent_delta: isize,
    budget: isize,
    chain: &[&[DocE]],
    out: &mut String,
) -> Option<usize> {
    fits_impl::<true>(next_indent_delta, budget, chain, out)
}

/// Width-only variant used by `first_line_fits`.
#[inline]
fn fits_width(budget: isize, doc: &[DocE]) -> Option<usize> {
    let mut sink = String::new();
    fits_impl::<false>(0, budget, &[doc], &mut sink)
}

/// Mirrors `firstLineWidth` in Nixfmt/Predoc.hs.
fn first_line_width(chain: Look<'_>) -> usize {
    let mut width = 0;
    let mut iter = LookIter::new(chain);
    let mut pending: Option<Spacing> = None;
    loop {
        let elem = loop {
            match iter.next() {
                Some(DocE::Group(_, body)) => iter.push_front(body),
                e => break e,
            }
        };
        if let Some(DocE::Spacing(s)) = elem {
            pending = Some(pending.map_or(*s, |p| merge_spacings(p, *s)));
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
            Some(DocE::Text(_, _, TextAnn::Comment | TextAnn::TrailingComment, _)) => {}
            Some(DocE::Text(_, _, _, t)) => width += text_width(t),
            Some(DocE::Spacing(_) | DocE::Group(_, _) | DocE::Nest(..)) => unreachable!(),
        }
    }
}

/// Mirrors `firstLineFits` in Nixfmt/Predoc.hs.
fn first_line_fits(target_width: usize, max_width: usize, chain: Look<'_>) -> bool {
    let max = max_width.cast_signed();
    let target = target_width.cast_signed();
    let mut budget = max;
    let mut iter = LookIter::new(chain);
    let mut pending: Option<Spacing> = None;
    let mut rest: Vec<&[DocE]> = Vec::new();
    loop {
        if budget < 0 {
            return false;
        }
        let elem = iter.next();
        if let Some(DocE::Spacing(s)) = elem {
            pending = Some(pending.map_or(*s, |p| merge_spacings(p, *s)));
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
            Some(DocE::Text(_, _, TextAnn::RegularT, t)) => budget -= text_width(t).cast_signed(),
            Some(DocE::Text(..) | DocE::Nest(..)) => {}
            Some(DocE::Group(_, body)) => {
                rest.clear();
                rest.extend(iter.stack.iter().rev().map(|(s, i)| &s[*i..]));
                let rest_width = first_line_width(&rest);
                match fits_width(budget - rest_width.cast_signed(), body) {
                    Some(w) => budget -= w.cast_signed(),
                    None => iter.push_front(body),
                }
            }
            Some(DocE::Spacing(_)) => unreachable!(),
        }
    }
}

/// Mirrors `nextIndent` in Nixfmt/Predoc.hs.
fn next_indent(chain: Look<'_>) -> (usize, usize) {
    for slice in chain {
        for elem in *slice {
            match elem {
                DocE::Text(nest, offset, _, _) => return (*nest, *offset),
                DocE::Group(_, body) => return next_indent(&[body]),
                DocE::Spacing(_) | DocE::Nest(..) => {}
            }
        }
    }
    (0, 0)
}

type Chain<'a> = Vec<&'a [DocE]>;

/// One `(pre, prio, post)` triple per `Priority` child (in document order),
/// each as a chain of borrowed slices into `doc`. `Transparent` groups are
/// flattened so their `Priority` children associate with this parent.
fn priority_groups(doc: &[DocE]) -> Vec<(Chain<'_>, Chain<'_>, Chain<'_>)> {
    fn segments<'a>(doc: &'a [DocE], out: &mut Vec<(bool, &'a [DocE])>) {
        let mut i = 0;
        while i < doc.len() {
            match &doc[i] {
                DocE::Group(GroupAnn::Priority, body) => {
                    out.push((true, body));
                    i += 1;
                }
                DocE::Group(GroupAnn::Transparent, body) => {
                    segments(body, out);
                    i += 1;
                }
                _ => {
                    let start = i;
                    while i < doc.len()
                        && !matches!(
                            &doc[i],
                            DocE::Group(GroupAnn::Priority | GroupAnn::Transparent, _)
                        )
                    {
                        i += 1;
                    }
                    out.push((false, &doc[start..i]));
                }
            }
        }
    }

    let mut segs = Vec::new();
    segments(doc, &mut segs);

    let mut result = Vec::new();
    for (idx, (is_prio, body)) in segs.iter().enumerate() {
        if !is_prio {
            continue;
        }
        let pre: Chain<'_> = segs[..idx].iter().map(|(_, s)| *s).collect();
        let post: Chain<'_> = segs[idx + 1..].iter().map(|(_, s)| *s).collect();
        result.push((pre, vec![*body], post));
    }
    result
}

/// One frame of the indentation stack: the column to indent to (`indent`) for
/// text at nesting level `nest`.
#[derive(Debug, Clone, Copy)]
struct IndentEntry {
    indent: usize,
    nest: usize,
}

/// Mutable layout state plus configuration. The `render_*` free functions of
/// the original port are methods here so the output buffer, column, indent
/// stack, and width settings no longer have to be threaded through every call.
struct Renderer {
    /// Output buffer.
    out: String,
    /// Current column (0 = at start of line, indentation not yet emitted).
    col: usize,
    /// Indentation stack; never empty.
    indents: Vec<IndentEntry>,
    /// Target line width.
    target_width: usize,
    /// Indent width in spaces.
    indent_width: usize,
}

/// Snapshot of the mutable parts of `Renderer` for trial-and-rollback in
/// `render_group` (mirrors Haskell's `StateT St Maybe`).
struct Checkpoint {
    out_len: usize,
    col: usize,
    indents: Vec<IndentEntry>,
}

fn layout_greedy(target_width: usize, indent_width: usize, doc: Doc) -> String {
    let doc = vec![DocE::Group(GroupAnn::RegularG, fixup(doc))];

    let mut renderer = Renderer {
        out: String::new(),
        col: 0,
        indents: vec![IndentEntry { indent: 0, nest: 0 }],
        target_width,
        indent_width,
    };
    renderer.render_doc(&doc, &[]);

    let mut result = renderer.out;
    let end = result.trim_end().len();
    result.truncate(end);
    let start = result.len() - result.trim_start().len();
    if start > 0 {
        result.drain(..start);
    }
    result.push('\n');
    result
}

impl Renderer {
    fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            out_len: self.out.len(),
            col: self.col,
            indents: self.indents.clone(),
        }
    }

    fn restore(&mut self, checkpoint: &Checkpoint) {
        self.out.truncate(checkpoint.out_len);
        self.col = checkpoint.col;
        self.indents.clone_from(&checkpoint.indents);
    }

    /// Nesting level of the current line (top of the indent stack).
    fn line_nest(&self) -> usize {
        self.indents.last().map_or(0, |e| e.nest)
    }

    fn render_doc(&mut self, doc: &[DocE], lookahead: Look<'_>) {
        let mut chain: Vec<&[DocE]> = Vec::with_capacity(1 + lookahead.len());
        for (i, elem) in doc.iter().enumerate() {
            // Only Group and the soft spacings consult the lookahead; for the
            // common Text/hard-spacing case skip even the small chain rebuild.
            let needs_rest = match elem {
                DocE::Group(_, _) => true,
                DocE::Spacing(Spacing::Softbreak | Spacing::Softspace) => self.col != 0,
                DocE::Text(_, _, TextAnn::TrailingComment, _) => self.col == 2,
                _ => false,
            };
            if needs_rest {
                chain.clear();
                chain.push(&doc[i + 1..]);
                chain.extend_from_slice(lookahead);
                self.render_elem(elem, &chain);
            } else {
                self.render_elem(elem, &[]);
            }
        }
    }

    fn render_elem(&mut self, elem: &DocE, lookahead: Look<'_>) {
        let at_line_start = self.col == 0;

        match elem {
            // `goOne` special case: shift a trailing comment by one column so
            // the re-parser associates it with the same opener token
            // (idempotency).
            DocE::Text(_, _, TextAnn::TrailingComment, t)
                if self.col == 2 && next_indent(lookahead).0 > self.line_nest() =>
            {
                self.col += 1 + text_width(t);
                self.out.push(' ');
                self.out.push_str(t);
            }

            DocE::Text(nest, offset, _ann, t) => self.render_text(*nest, *offset, t),

            // At start of line drop any spacing; the next Text emits indentation.
            DocE::Spacing(_) if at_line_start => {}

            DocE::Spacing(spacing) => match spacing {
                Spacing::Break | Spacing::Space | Spacing::Hardline => {
                    self.col = 0;
                    self.out.push('\n');
                }
                Spacing::Hardspace => {
                    self.col += 1;
                    self.out.push(' ');
                }
                Spacing::Emptyline => {
                    self.col = 0;
                    self.out.push_str("\n\n");
                }
                Spacing::Newlines(n) => {
                    self.col = 0;
                    for _ in 0..*n {
                        self.out.push('\n');
                    }
                }
                Spacing::Softbreak => {
                    if !first_line_fits(self.target_width - self.col, self.target_width, lookahead)
                    {
                        self.col = 0;
                        self.out.push('\n');
                    }
                }
                Spacing::Softspace => {
                    let available = self.target_width.saturating_sub(self.col).saturating_sub(1);
                    if first_line_fits(available, self.target_width, lookahead) {
                        self.col += 1;
                        self.out.push(' ');
                    } else {
                        self.col = 0;
                        self.out.push('\n');
                    }
                }
            },

            DocE::Group(ann, body) => self.render_group(*ann, body, lookahead),

            DocE::Nest(..) => unreachable!("Nest consumed by fixup"),
        }
    }

    /// Compute the indent column `render_text` would use for `text_nest` at
    /// the start of a line, without mutating the indent stack.
    fn indent_for(&self, text_nest: usize) -> usize {
        let mut top = self.indents.len();
        while top > 0 && text_nest < self.indents[top - 1].nest {
            top -= 1;
        }
        match self.indents[..top].last() {
            Some(e) if text_nest > e.nest => e.indent + self.indent_width,
            Some(e) => e.indent,
            None => 0,
        }
    }

    /// Apply the indent-stack mutation `render_text` would perform for
    /// `text_nest` at the start of a line (col == 0).
    fn apply_indent(&mut self, text_nest: usize) {
        while let Some(&top) = self.indents.last() {
            match text_nest.cmp(&top.nest) {
                std::cmp::Ordering::Greater => {
                    self.indents.push(IndentEntry {
                        indent: top.indent + self.indent_width,
                        nest: text_nest,
                    });
                    return;
                }
                std::cmp::Ordering::Less => {
                    self.indents.pop();
                }
                std::cmp::Ordering::Equal => return,
            }
        }
    }

    fn render_text(&mut self, text_nest: usize, text_offset: usize, text: &str) {
        while let Some(&top) = self.indents.last() {
            match text_nest.cmp(&top.nest) {
                std::cmp::Ordering::Greater => {
                    let new_indent = if self.col == 0 {
                        top.indent + self.indent_width
                    } else {
                        top.indent
                    };
                    self.indents.push(IndentEntry {
                        indent: new_indent,
                        nest: text_nest,
                    });
                    break;
                }
                std::cmp::Ordering::Less => {
                    self.indents.pop();
                }
                std::cmp::Ordering::Equal => break,
            }
        }

        let cur_indent = self.indents.last().map_or(0, |e| e.indent);
        let total_indent = cur_indent + text_offset;

        if self.col == 0 {
            for _ in 0..total_indent {
                self.out.push(' ');
            }
        }
        self.col += text_width(text);
        self.out.push_str(text);
    }

    /// Render a chain of slices as one document, threading lookahead so each
    /// slice sees the remaining slices plus the outer lookahead.
    fn render_chain(&mut self, chain: &[&[DocE]], lookahead: Look<'_>) {
        let mut lookahead_buf: Vec<&[DocE]> = Vec::with_capacity(chain.len() + lookahead.len());
        for i in 0..chain.len() {
            lookahead_buf.clear();
            lookahead_buf.extend_from_slice(&chain[i + 1..]);
            lookahead_buf.extend_from_slice(lookahead);
            self.render_doc(chain[i], &lookahead_buf);
        }
    }

    /// Try to render a group compactly. On success, appends to `out` and
    /// updates state in place; on failure leaves both untouched.
    /// Mirrors `goGroup` in Nixfmt/Predoc.hs.
    fn try_render_group(&mut self, grp: &[&[DocE]], lookahead: Look<'_>) -> bool {
        if grp.iter().all(|s| s.is_empty()) {
            return true;
        }

        if self.col == 0 {
            // At start of line a leading spacing is meaningless (the next
            // Text emits indentation), so drop one leading Spacing — looking
            // through a leading nested group — before measuring.
            let mut grp: Vec<&[DocE]> = grp.iter().copied().filter(|s| !s.is_empty()).collect();
            match grp[0].first() {
                Some(DocE::Spacing(_)) => grp[0] = &grp[0][1..],
                Some(DocE::Group(ann, inner))
                    if matches!(inner.first(), Some(DocE::Spacing(_))) =>
                {
                    // Rebuilding the subgroup yields an owned element that
                    // `grp` must borrow, so recurse with it spliced in front.
                    let owned = [DocE::Group(*ann, Doc(inner[1..].to_vec()))];
                    grp[0] = &grp[0][1..];
                    grp.insert(0, &owned);
                    return self.try_render_group(&grp, lookahead);
                }
                _ => {}
            }
            let grp = grp.as_slice();

            let (nest, offset) = next_indent(grp);
            // Haskell `goGroup` (cc == 0): the budget is
            // `tw - firstLineWidth rest`; the pending indentation is *not*
            // subtracted here, so a compact group at the start of a line may
            // overshoot by its indent. This matches the reference layout
            // engine exactly.
            let last_line_nest = self.line_nest();
            let line_nest = last_line_nest
                + if nest > last_line_nest {
                    self.indent_width
                } else {
                    0
                };
            let will_increase = if next_indent(lookahead).0 > line_nest {
                self.indent_width
            } else {
                0
            };

            let budget =
                self.target_width.cast_signed() - first_line_width(lookahead).cast_signed();
            let mark = self.out.len();
            let total_indent = self.indent_for(nest) + offset;
            for _ in 0..total_indent {
                self.out.push(' ');
            }
            if let Some(width) = fits(will_increase.cast_signed(), budget, grp, &mut self.out) {
                self.apply_indent(nest);
                self.col += width;
                true
            } else {
                self.out.truncate(mark);
                false
            }
        } else {
            let line_nest = self.line_nest();
            let will_increase = if next_indent(lookahead).0 > line_nest {
                self.indent_width.cast_signed()
            } else {
                0
            };

            let budget = self.target_width.cast_signed()
                - self.col.cast_signed()
                - first_line_width(lookahead).cast_signed();
            match fits(
                will_increase - self.col.cast_signed(),
                budget,
                grp,
                &mut self.out,
            ) {
                Some(width) => {
                    self.col += width;
                    true
                }
                None => false,
            }
        }
    }

    /// Render a group (try compact first, then expand).
    fn render_group(&mut self, ann: GroupAnn, body: &[DocE], lookahead: Look<'_>) {
        if self.try_render_group(&[body], lookahead) {
            return;
        }

        if ann != GroupAnn::Transparent && has_priority_groups(body) {
            let checkpoint = self.checkpoint();
            for (pre, prio, post) in priority_groups(body).into_iter().rev() {
                let mut pre_lookahead: Vec<&[DocE]> =
                    Vec::with_capacity(prio.len() + post.len() + lookahead.len());
                pre_lookahead.extend_from_slice(&prio);
                pre_lookahead.extend_from_slice(&post);
                pre_lookahead.extend_from_slice(lookahead);
                if self.try_render_group(&pre, &pre_lookahead) {
                    let unexpanded_post = unexpand_spacing(&post);
                    let mut prio_lookahead: Vec<&[DocE]> = Vec::with_capacity(1 + lookahead.len());
                    prio_lookahead.push(&unexpanded_post);
                    prio_lookahead.extend_from_slice(lookahead);
                    self.render_chain(&prio, &prio_lookahead);

                    if self.try_render_group(&post, lookahead) {
                        return;
                    }
                }
                // Attempt failed: discard any mutations from the trial run
                // before trying the next priority group or falling back to
                // full expansion. Haskell threads this via `StateT St Maybe`,
                // which simply drops the state on `Nothing`.
                self.restore(&checkpoint);
            }
        }

        self.render_doc(body, lookahead);
    }
}
