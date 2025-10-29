//! String processing utilities for Nix strings
//!
//! This module handles the normalization of both simple ("...") and indented (''...'')
//! strings in Nix, including splitting on newlines, merging adjacent text parts, and
//! stripping common indentation for multi-line strings.

use crate::types::StringPart;

/// Process a simple string by splitting on newlines and merging adjacent text
///
/// Simple strings ("...") only need newline splitting and normalization,
/// without the indentation handling required for multi-line strings.
pub(super) fn process_simple(parts: Vec<StringPart>) -> Vec<Vec<StringPart>> {
    split_on_newlines(parts)
        .into_iter()
        .map(merge_adjacent_text)
        .collect()
}

/// Process an indented string by normalizing whitespace and stripping common indentation
///
/// This is the main entry point for processing Nix indented strings (''...''). It:
/// 1. Removes empty first/last lines
/// 2. Strips common indentation from all lines
/// 3. Splits text parts on newlines
/// 4. Normalizes adjacent text parts
pub(super) fn process_indented(lines: Vec<Vec<StringPart>>) -> Vec<Vec<StringPart>> {
    let lines = remove_empty_first_line(lines);
    let lines = remove_empty_last_line(lines);
    let lines = strip_common_indentation(lines);
    let lines: Vec<_> = lines.into_iter().flat_map(split_on_newlines).collect();
    lines.into_iter().map(merge_adjacent_text).collect()
}

/// Split text parts on newlines, creating separate lines
pub(super) fn split_on_newlines(parts: Vec<StringPart>) -> Vec<Vec<StringPart>> {
    let mut result: Vec<Vec<StringPart>> = Vec::new();
    let mut current: Vec<StringPart> = Vec::new();

    for part in parts {
        match part {
            StringPart::TextPart(text) => {
                let mut remaining = text.as_str();
                loop {
                    if let Some(pos) = remaining.find('\n') {
                        let segment = &remaining[..pos];
                        if !segment.is_empty() {
                            current.push(StringPart::TextPart(segment.to_string()));
                        }
                        result.push(current);
                        current = Vec::new();
                        remaining = &remaining[pos + 1..];
                    } else {
                        if !remaining.is_empty() {
                            current.push(StringPart::TextPart(remaining.to_string()));
                        }
                        break;
                    }
                }
            }
            other => current.push(other),
        }
    }

    result.push(current);
    result
}

/// Merge adjacent TextPart elements into a single TextPart
pub(super) fn merge_adjacent_text(line: Vec<StringPart>) -> Vec<StringPart> {
    let mut result: Vec<StringPart> = Vec::new();
    for part in line {
        match part {
            StringPart::TextPart(text) => {
                if text.is_empty() {
                    continue;
                }
                if let Some(StringPart::TextPart(existing)) = result.last_mut() {
                    existing.push_str(&text);
                } else {
                    result.push(StringPart::TextPart(text));
                }
            }
            other => result.push(other),
        }
    }
    result
}

/// Check if a string contains only spaces
fn is_only_spaces(text: &str) -> bool {
    text.bytes().all(|b| b == b' ')
}

/// Check if a line is effectively empty (no parts or only spaces)
fn is_empty_line(line: &[StringPart]) -> bool {
    line.is_empty() || matches!(line, [StringPart::TextPart(text)] if is_only_spaces(text))
}

/// Remove the first line if it's empty or contains only spaces
fn remove_empty_first_line(mut lines: Vec<Vec<StringPart>>) -> Vec<Vec<StringPart>> {
    if let Some(first_line) = lines.first().cloned() {
        let first = merge_adjacent_text(first_line);
        if is_empty_line(&first) && lines.len() > 1 {
            lines.remove(0);
        } else {
            lines[0] = first;
        }
    }
    lines
}

/// Remove the last line if it's empty or contains only spaces
fn remove_empty_last_line(mut lines: Vec<Vec<StringPart>>) -> Vec<Vec<StringPart>> {
    match lines.len() {
        0 => lines,
        1 => {
            let last = merge_adjacent_text(lines[0].clone());
            if is_empty_line(&last) {
                vec![Vec::new()]
            } else {
                vec![last]
            }
        }
        _ => {
            let last_index = lines.len() - 1;
            let last = merge_adjacent_text(lines[last_index].clone());
            lines[last_index] = if is_empty_line(&last) {
                Vec::new()
            } else {
                last
            };
            lines
        }
    }
}

/// Get the text content at the start of a line (for indentation calculation)
fn line_prefix(line: &[StringPart]) -> Option<String> {
    match line.first() {
        None => None,
        Some(StringPart::TextPart(text)) => Some(text.clone()),
        Some(StringPart::Interpolation(_)) => Some(String::new()),
    }
}

/// Find the common leading space prefix across all lines
fn find_common_space_prefix(prefixes: Vec<String>) -> Option<String> {
    if prefixes.is_empty() {
        return None;
    }

    let mut common: String = prefixes[0].chars().take_while(|c| *c == ' ').collect();
    for prefix in prefixes.iter().skip(1) {
        let candidate: String = prefix.chars().take_while(|c| *c == ' ').collect();
        let mut new_common = String::new();
        for (a, b) in common.chars().zip(candidate.chars()) {
            if a == b {
                new_common.push(a);
            } else {
                break;
            }
        }
        common = new_common;
        if common.is_empty() {
            break;
        }
    }
    Some(common)
}

/// Strip a prefix from the first text part of a line
fn strip_prefix_from_line(prefix: &str, mut line: Vec<StringPart>) -> Vec<StringPart> {
    if prefix.is_empty() {
        return line;
    }

    if let Some(StringPart::TextPart(text)) = line.first_mut() {
        if let Some(stripped) = text.strip_prefix(prefix) {
            *text = stripped.to_string();
        }
    }
    line
}

/// Strip common leading indentation from all lines
fn strip_common_indentation(lines: Vec<Vec<StringPart>>) -> Vec<Vec<StringPart>> {
    let prefixes: Vec<String> = lines.iter().filter_map(|line| line_prefix(line)).collect();

    match find_common_space_prefix(prefixes) {
        None => lines.into_iter().map(|_| Vec::new()).collect(),
        Some(prefix) => lines
            .into_iter()
            .map(|line| strip_prefix_from_line(&prefix, line))
            .collect(),
    }
}
