//! Simple diff implementation with colored output for test diagnostics
//!
//! # Example Output
//!
//! When comparing two strings, the output uses colors to indicate changes:
//!
//! ```text
//! - removed line     (in red)
//!   unchanged line   (in dim gray)
//! + added line       (in green)
//! ```
//!
//! For example, comparing:
//! ```text
//! Left:  "foo\nbar\nbaz"
//! Right: "foo\nqux\nbaz"
//! ```
//!
//! Produces:
//! ```text
//!   foo              (dim - unchanged)
//! - bar              (red - removed from left)
//! + qux              (green - added in right)
//!   baz              (dim - unchanged)
//! ```

/// ANSI color codes
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffResult<T> {
    /// Item only in left (removed)
    Left(T),
    /// Item in both (unchanged)
    Both(T, T),
    /// Item only in right (added)
    Right(T),
}

/// Compute the longest common subsequence table using dynamic programming
fn lcs_table<T: PartialEq>(left: &[T], right: &[T]) -> Vec<Vec<usize>> {
    let m = left.len();
    let n = right.len();
    let mut table = vec![vec![0; n + 1]; m + 1];

    for i in 1..=m {
        for j in 1..=n {
            if left[i - 1] == right[j - 1] {
                table[i][j] = table[i - 1][j - 1] + 1;
            } else {
                table[i][j] = table[i - 1][j].max(table[i][j - 1]);
            }
        }
    }

    table
}

/// Backtrack through the LCS table to produce diff results
fn backtrack<T: Clone + PartialEq>(
    left: &[T],
    right: &[T],
    table: &[Vec<usize>],
    i: usize,
    j: usize,
    result: &mut Vec<DiffResult<T>>,
) {
    if i == 0 && j == 0 {
        return;
    }

    if i > 0 && j > 0 && left[i - 1] == right[j - 1] {
        backtrack(left, right, table, i - 1, j - 1, result);
        result.push(DiffResult::Both(left[i - 1].clone(), right[j - 1].clone()));
    } else if j > 0 && (i == 0 || table[i][j - 1] >= table[i - 1][j]) {
        backtrack(left, right, table, i, j - 1, result);
        result.push(DiffResult::Right(right[j - 1].clone()));
    } else if i > 0 {
        backtrack(left, right, table, i - 1, j, result);
        result.push(DiffResult::Left(left[i - 1].clone()));
    }
}

/// Compare two slices and return a vector of diff results
pub fn slice<T: Clone + PartialEq>(left: &[T], right: &[T]) -> Vec<DiffResult<T>> {
    let table = lcs_table(left, right);
    let mut result = Vec::new();
    backtrack(left, right, &table, left.len(), right.len(), &mut result);
    result
}

/// Compare two strings line-by-line
pub fn lines<'a>(left: &'a str, right: &'a str) -> Vec<DiffResult<&'a str>> {
    let left_lines: Vec<&str> = left.lines().collect();
    let right_lines: Vec<&str> = right.lines().collect();
    slice(&left_lines, &right_lines)
}

/// Print a colored diff to stderr
///
/// # Example
///
/// ```rust
/// let expected = "let x = 1;\nlet y = 2;";
/// let actual = "let x = 1;\nlet z = 3;";
/// print_colored_diff(expected, actual);
/// // Output (with colors):
/// //   let x = 1;
/// // - let y = 2;
/// // + let z = 3;
/// ```
pub fn print_colored_diff(expected: &str, actual: &str) {
    for diff_line in lines(expected, actual) {
        match diff_line {
            DiffResult::Left(l) => {
                eprintln!("{RED}- {l}{RESET}");
            }
            DiffResult::Both(l, _) => {
                eprintln!("{DIM}  {l}{RESET}");
            }
            DiffResult::Right(r) => {
                eprintln!("{GREEN}+ {r}{RESET}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identical_strings() {
        let left = "foo\nbar\nbaz";
        let right = "foo\nbar\nbaz";
        let result = lines(left, right);

        assert_eq!(result.len(), 3);
        assert!(matches!(result[0], DiffResult::Both("foo", "foo")));
        assert!(matches!(result[1], DiffResult::Both("bar", "bar")));
        assert!(matches!(result[2], DiffResult::Both("baz", "baz")));
    }

    #[test]
    fn test_addition() {
        let left = "foo\nbaz";
        let right = "foo\nbar\nbaz";
        let result = lines(left, right);

        assert_eq!(result.len(), 3);
        assert!(matches!(result[0], DiffResult::Both("foo", "foo")));
        assert!(matches!(result[1], DiffResult::Right("bar")));
        assert!(matches!(result[2], DiffResult::Both("baz", "baz")));
    }

    #[test]
    fn test_removal() {
        let left = "foo\nbar\nbaz";
        let right = "foo\nbaz";
        let result = lines(left, right);

        assert_eq!(result.len(), 3);
        assert!(matches!(result[0], DiffResult::Both("foo", "foo")));
        assert!(matches!(result[1], DiffResult::Left("bar")));
        assert!(matches!(result[2], DiffResult::Both("baz", "baz")));
    }

    #[test]
    fn test_replacement() {
        let left = "foo\nbar\nbaz";
        let right = "foo\nqux\nbaz";
        let result = lines(left, right);

        assert_eq!(result.len(), 4);
        assert!(matches!(result[0], DiffResult::Both("foo", "foo")));
        assert!(matches!(result[1], DiffResult::Left("bar")));
        assert!(matches!(result[2], DiffResult::Right("qux")));
        assert!(matches!(result[3], DiffResult::Both("baz", "baz")));
    }

    #[test]
    fn test_empty_strings() {
        let left = "";
        let right = "";
        let result = lines(left, right);
        assert_eq!(result.len(), 0);
    }

    #[test]
    fn test_empty_left() {
        let left = "";
        let right = "foo\nbar";
        let result = lines(left, right);

        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], DiffResult::Right("foo")));
        assert!(matches!(result[1], DiffResult::Right("bar")));
    }

    #[test]
    fn test_empty_right() {
        let left = "foo\nbar";
        let right = "";
        let result = lines(left, right);

        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], DiffResult::Left("foo")));
        assert!(matches!(result[1], DiffResult::Left("bar")));
    }

    #[test]
    fn test_slice_with_integers() {
        let left = vec![1, 2, 3, 4];
        let right = vec![1, 2, 5, 4];
        let result = slice(&left, &right);

        assert_eq!(result.len(), 5);
        assert!(matches!(result[0], DiffResult::Both(1, 1)));
        assert!(matches!(result[1], DiffResult::Both(2, 2)));
        assert!(matches!(result[2], DiffResult::Left(3)));
        assert!(matches!(result[3], DiffResult::Right(5)));
        assert!(matches!(result[4], DiffResult::Both(4, 4)));
    }

    #[test]
    fn test_complex_diff() {
        let left = "a\nb\nc\nd\ne";
        let right = "a\nx\ny\nd\ne\nf";
        let result = lines(left, right);

        assert_eq!(result.len(), 8);
        assert!(matches!(result[0], DiffResult::Both("a", "a")));
        assert!(matches!(result[1], DiffResult::Left("b")));
        assert!(matches!(result[2], DiffResult::Left("c")));
        assert!(matches!(result[3], DiffResult::Right("x")));
        assert!(matches!(result[4], DiffResult::Right("y")));
        assert!(matches!(result[5], DiffResult::Both("d", "d")));
        assert!(matches!(result[6], DiffResult::Both("e", "e")));
        assert!(matches!(result[7], DiffResult::Right("f")));
    }
}
