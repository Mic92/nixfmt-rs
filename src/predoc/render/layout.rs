//! The greedy Wadler/Leijen layout engine: walk a fixed-up [`Doc`], try each
//! group compact first (via [`super::fits`]), expand on failure, and resolve
//! `Priority` children before falling back to full expansion.

use super::Look;
use super::fits::{first_line_fits, first_line_width, fits, next_indent};
use crate::predoc::{Doc, Elem, GroupKind, Spacing, TextKind, text_width};

pub struct RenderConfig {
    /// Maximum line width (default: 100)
    pub width: usize,
    /// Indentation width in spaces (default: 2)
    pub indent_width: usize,
}

/// Manually force a group to compact layout (does not recurse into inner groups)
fn unexpand_spacing(chain: &[&[Elem]]) -> Doc {
    let mut result = Vec::new();
    for s in chain {
        for elem in *s {
            match elem {
                Elem::Spacing(Spacing::Space | Spacing::Softspace) => {
                    result.push(Elem::Spacing(Spacing::Hardspace));
                }
                Elem::Spacing(Spacing::Break | Spacing::Softbreak) | Elem::Nest(..) => {}
                _ => result.push(elem.clone()),
            }
        }
    }
    Doc(result)
}

/// Cheap pre-check so `render_group` can skip the clone-heavy `priority_groups`
/// machinery for groups that contain no Priority children.
fn has_priority_groups(doc: &[Elem]) -> bool {
    doc.iter().any(|e| match e {
        Elem::Group(GroupKind::Priority, _) => true,
        Elem::Group(GroupKind::Transparent, body) => has_priority_groups(body),
        _ => false,
    })
}

type Chain<'a> = Vec<&'a [Elem]>;

/// One `(pre, prio, post)` triple per `Priority` child (in document order),
/// each as a chain of borrowed slices into `doc`. `Transparent` groups are
/// flattened so their `Priority` children associate with this parent.
fn priority_groups(doc: &[Elem]) -> Vec<(Chain<'_>, Chain<'_>, Chain<'_>)> {
    fn segments<'a>(doc: &'a [Elem], out: &mut Vec<(bool, &'a [Elem])>) {
        let mut i = 0;
        while i < doc.len() {
            match &doc[i] {
                Elem::Group(GroupKind::Priority, body) => {
                    out.push((true, body));
                    i += 1;
                }
                Elem::Group(GroupKind::Transparent, body) => {
                    segments(body, out);
                    i += 1;
                }
                _ => {
                    let start = i;
                    while i < doc.len()
                        && !matches!(
                            &doc[i],
                            Elem::Group(GroupKind::Priority | GroupKind::Transparent, _)
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

pub(super) fn layout_greedy(target_width: usize, indent_width: usize, doc: Doc) -> String {
    let doc = vec![Elem::Group(GroupKind::Regular, doc.fixup())];

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

    fn render_doc(&mut self, doc: &[Elem], lookahead: Look<'_>) {
        let mut chain: Vec<&[Elem]> = Vec::with_capacity(1 + lookahead.len());
        for (i, elem) in doc.iter().enumerate() {
            // Only Group and the soft spacings consult the lookahead; for the
            // common Text/hard-spacing case skip even the small chain rebuild.
            let needs_rest = match elem {
                Elem::Group(_, _) => true,
                Elem::Spacing(Spacing::Softbreak | Spacing::Softspace) => self.col != 0,
                Elem::Text(_, _, TextKind::TrailingComment, _) => self.col == 2,
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

    fn render_elem(&mut self, elem: &Elem, lookahead: Look<'_>) {
        let at_line_start = self.col == 0;

        match elem {
            // `goOne` special case: shift a trailing comment by one column so
            // the re-parser associates it with the same opener token
            // (idempotency).
            Elem::Text(_, _, TextKind::TrailingComment, t)
                if self.col == 2 && next_indent(lookahead).0 > self.line_nest() =>
            {
                self.col += 1 + text_width(t);
                self.out.push(' ');
                self.out.push_str(t);
            }

            Elem::Text(nest, offset, _ann, t) => self.render_text(*nest, *offset, t),

            // At start of line drop any spacing; the next Text emits indentation.
            Elem::Spacing(_) if at_line_start => {}

            Elem::Spacing(spacing) => match spacing {
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

            Elem::Group(ann, body) => self.render_group(*ann, body, lookahead),

            Elem::Nest(..) => unreachable!("Nest consumed by fixup"),
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
    fn render_chain(&mut self, chain: &[&[Elem]], lookahead: Look<'_>) {
        let mut lookahead_buf: Vec<&[Elem]> = Vec::with_capacity(chain.len() + lookahead.len());
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
    fn try_render_group(&mut self, grp: &[&[Elem]], lookahead: Look<'_>) -> bool {
        if grp.iter().all(|s| s.is_empty()) {
            return true;
        }

        if self.col == 0 {
            // At start of line a leading spacing is meaningless (the next
            // Text emits indentation), so drop one leading Spacing — looking
            // through a leading nested group — before measuring.
            let mut grp: Vec<&[Elem]> = grp.iter().copied().filter(|s| !s.is_empty()).collect();
            match grp[0].first() {
                Some(Elem::Spacing(_)) => grp[0] = &grp[0][1..],
                Some(Elem::Group(ann, inner))
                    if matches!(inner.first(), Some(Elem::Spacing(_))) =>
                {
                    // Rebuilding the subgroup yields an owned element that
                    // `grp` must borrow, so recurse with it spliced in front.
                    let owned = [Elem::Group(*ann, Doc(inner[1..].to_vec()))];
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
    fn render_group(&mut self, ann: GroupKind, body: &[Elem], lookahead: Look<'_>) {
        if self.try_render_group(&[body], lookahead) {
            return;
        }

        if ann != GroupKind::Transparent && has_priority_groups(body) {
            let checkpoint = self.checkpoint();
            for (pre, prio, post) in priority_groups(body).into_iter().rev() {
                let mut pre_lookahead: Vec<&[Elem]> =
                    Vec::with_capacity(prio.len() + post.len() + lookahead.len());
                pre_lookahead.extend_from_slice(&prio);
                pre_lookahead.extend_from_slice(&post);
                pre_lookahead.extend_from_slice(lookahead);
                if self.try_render_group(&pre, &pre_lookahead) {
                    let unexpanded_post = unexpand_spacing(&post);
                    let mut prio_lookahead: Vec<&[Elem]> = Vec::with_capacity(1 + lookahead.len());
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
