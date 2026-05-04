//! Trivia conversion utilities
//!
//! This module handles conversion of intermediate `RawTrivia` tokens into
//! final Trivia and `TrailingComment` structures. It implements the logic for
//! splitting trivia into trailing comments (inline comments on the same line)
//! and leading trivia (comments and empty lines before the next token).

use super::RawTrivia;
use crate::ast::{TrailingComment, Trivia, TriviaPiece};

/// Check if a `RawTrivia` should be classified as trailing
const fn is_trailing(pt: &RawTrivia) -> bool {
    match pt {
        RawTrivia::LineComment { .. } => true,
        RawTrivia::BlockComment(false, lines) => lines.len() <= 1,
        _ => false,
    }
}

/// Convert trailing trivia to `TrailingComment`
fn convert_trailing(pts: &[RawTrivia]) -> Option<TrailingComment> {
    let texts: Vec<String> = pts
        .iter()
        .filter_map(|pt| match pt {
            RawTrivia::LineComment { text, .. } => Some(text.trim().to_string()),
            RawTrivia::BlockComment(false, lines) if lines.len() == 1 => {
                Some(lines[0].trim().to_string())
            }
            _ => None,
        })
        .filter(|s| !s.is_empty())
        .collect();

    let joined = texts.join(" ");
    if joined.is_empty() {
        None
    } else {
        Some(TrailingComment(joined.into()))
    }
}

/// Convert leading trivia to Trivia
/// Merges consecutive Newlines (matching Haskell's `some (preLexeme eol)` behavior)
/// and converts to final [`TriviaPiece`] entries in a single pass to avoid intermediate allocations.
pub(super) fn convert_leading(pts: &[RawTrivia]) -> Trivia {
    // State: (result_vec, accumulated_newline_count)
    let (mut result, pending_newlines) =
        pts.iter()
            .fold((Vec::new(), 0), |(mut acc, newline_count), pt| match pt {
                RawTrivia::Newlines(count) => (acc, newline_count + count),
                other => {
                    // Flush pending newlines first (single newlines are discarded)
                    if newline_count > 1 {
                        acc.push(TriviaPiece::EmptyLine);
                    }

                    match other {
                        RawTrivia::LineComment { text, .. } => {
                            acc.push(TriviaPiece::LineComment(text.clone().into_boxed_str()));
                        }
                        RawTrivia::BlockComment(_, lines) if lines.is_empty() => {}
                        RawTrivia::BlockComment(false, lines) if lines.len() == 1 => {
                            acc.push(TriviaPiece::LineComment(
                                format!(" {}", lines[0].trim()).into_boxed_str(),
                            ));
                        }
                        RawTrivia::BlockComment(is_doc, lines) => {
                            acc.push(TriviaPiece::BlockComment(
                                *is_doc,
                                lines.iter().cloned().map(String::into_boxed_str).collect(),
                            ));
                        }
                        RawTrivia::LanguageAnnotation(text) => {
                            acc.push(TriviaPiece::LanguageAnnotation(
                                text.clone().into_boxed_str(),
                            ));
                        }
                        RawTrivia::Newlines(_) => unreachable!(),
                    }

                    (acc, 0)
                }
            });

    if pending_newlines > 1 {
        result.push(TriviaPiece::EmptyLine);
    }

    result.into()
}

/// Convert `RawTrivia` list to (`trailing_comment`, `leading_trivia`)
///
/// This is the main conversion function that splits trivia into:
/// - Trailing comments: inline comments on the same line as the previous token
/// - Leading trivia: comments and empty lines before the next token
///
/// Special handling for comment blocks:
/// - If a trailing comment visually forms a block with the following line,
///   treat it as leading instead to preserve formatting intent
pub fn convert_trivia(pts: &[RawTrivia], next_col: usize) -> (Option<TrailingComment>, Trivia) {
    // Fast path: the overwhelmingly common case between two tokens is a single
    // run of newlines (or nothing at all) with no comments.
    match pts {
        [] => return (None, Trivia::new()),
        [RawTrivia::Newlines(n)] => {
            return (
                None,
                if *n > 1 {
                    Trivia::one(TriviaPiece::EmptyLine)
                } else {
                    Trivia::new()
                },
            );
        }
        _ => {}
    }

    let split_pos = pts
        .iter()
        .position(|pt| !is_trailing(pt))
        .unwrap_or(pts.len());
    let (trailing_pts, leading_pts) = pts.split_at(split_pos);

    // Special case: if trailing comment visually forms a block with following line,
    // treat it as leading instead
    match (trailing_pts, leading_pts) {
        // Case 1: [ # comment ] followed by single newline and another # at same column
        (
            [RawTrivia::LineComment { col: col1, .. }],
            [
                RawTrivia::Newlines(1),
                RawTrivia::LineComment { col: col2, .. },
                ..,
            ],
        ) if col1 == col2 => (None, convert_leading(pts)),

        // Case 2: [ # comment ] followed by single newline, and next token is at same column
        ([RawTrivia::LineComment { col, .. }], [RawTrivia::Newlines(1)]) if *col == next_col => {
            (None, convert_leading(pts))
        }

        _ => (convert_trailing(trailing_pts), convert_leading(leading_pts)),
    }
}
