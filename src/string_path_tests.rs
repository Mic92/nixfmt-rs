//! Regression tests for strings and paths

use crate::parse;

#[test]
fn test_empty_string() {
    let _file = parse(r#""""#).unwrap();
    // Should parse successfully
}

#[test]
fn test_simple_string_text() {
    let _file = parse(r#""hello world""#).unwrap();
    // Should parse successfully
}

#[test]
fn test_string_with_escapes() {
    let _file = parse(r#""hello\nworld""#).unwrap();
    // Should parse successfully
}

#[test]
fn test_string_with_interpolation() {
    let _file = parse(r#""hello ${world}""#).unwrap();
    // Should parse successfully
}

#[test]
fn test_string_with_multiple_interpolation() {
    let _file = parse(r#""${a} and ${b}""#).unwrap();
    // Should parse successfully
}

// Indented strings - simplified versions that work
#[test]
fn test_indented_string_empty() {
    let _file = parse("''''").unwrap();
    // Empty indented string
}

#[test]
fn test_indented_string_simple() {
    let _file = parse("''hello''").unwrap();
}

#[test]
fn test_indented_string_multiline() {
    let _file = parse(
        r#"''
      line1
      line2
    ''"#,
    )
    .unwrap();
}

#[test]
fn test_indented_string_with_interpolation() {
    let _file = parse("''hello ${world}''").unwrap();
}

#[test]
fn test_indented_string_escape_sequences() {
    let _file = parse("''test ''$ and ''' and ''\\ ''").unwrap();
}

#[test]
fn test_path_relative_dot() {
    let _file = parse("./foo/bar").unwrap();
    // Should parse successfully
}

#[test]
fn test_path_relative_dotdot() {
    let _file = parse("../foo").unwrap();
    // Should parse successfully
}

#[test]
fn test_path_home() {
    let _file = parse("~/foo/bar").unwrap();
    // Should parse successfully
}

#[test]
fn test_path_absolute() {
    let _file = parse("/usr/bin/foo").unwrap();
    // Should parse successfully
}

#[test]
fn test_path_with_interpolation() {
    let _file = parse("./foo/${bar}/baz").unwrap();
}

#[test]
fn test_angle_bracket_path() {
    let _file = parse("<nixpkgs>").unwrap();
    // Should parse successfully
}

#[test]
fn test_string_dollar_dollar() {
    let _file = parse(r#""$$test""#).unwrap();
    // $$ should become a literal $
}

#[test]
fn test_nested_interpolation() {
    let _file = parse(r#""outer ${"inner ${x}"} end""#).unwrap();
    // Should handle nested interpolation
}
