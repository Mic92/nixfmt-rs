//! Regression tests for all parameter types

use crate::{parse, Expression, Parameter};

#[test]
fn test_id_parameter() {
    let file = parse("x: x").unwrap();
    match &file.value {
        Expression::Abstraction(Parameter::IDParameter(_), _, _) => {}
        _ => panic!("expected id parameter"),
    }
}

#[test]
fn test_set_parameter_simple() {
    let file = parse("{ x }: x").unwrap();
    match &file.value {
        Expression::Abstraction(Parameter::SetParameter(_, attrs, _), _, _) => {
            assert_eq!(attrs.len(), 1);
        }
        _ => panic!("expected set parameter"),
    }
}

#[test]
fn test_set_parameter_multiple() {
    let file = parse("{ x, y }: x + y").unwrap();
    match &file.value {
        Expression::Abstraction(Parameter::SetParameter(_, attrs, _), _, _) => {
            assert_eq!(attrs.len(), 2);
        }
        _ => panic!("expected set parameter with 2 attrs"),
    }
}

#[test]
fn test_set_parameter_with_default() {
    let file = parse("{ x ? 1 }: x").unwrap();
    match &file.value {
        Expression::Abstraction(Parameter::SetParameter(_, attrs, _), _, _) => {
            assert_eq!(attrs.len(), 1);
        }
        _ => panic!("expected set parameter with default"),
    }
}

#[test]
fn test_set_parameter_with_ellipsis() {
    let file = parse("{ x, ... }: x").unwrap();
    match &file.value {
        Expression::Abstraction(Parameter::SetParameter(_, attrs, _), _, _) => {
            assert_eq!(attrs.len(), 2);
        }
        _ => panic!("expected set parameter with ellipsis"),
    }
}

#[test]
fn test_context_parameter_id_at_set() {
    let file = parse("args @ { x }: x").unwrap();
    match &file.value {
        Expression::Abstraction(Parameter::ContextParameter(_, _, _), _, _) => {}
        _ => panic!("expected context parameter"),
    }
}

#[test]
fn test_context_parameter_set_at_id() {
    let file = parse("{ x } @ args: x").unwrap();
    match &file.value {
        Expression::Abstraction(Parameter::ContextParameter(_, _, _), _, _) => {}
        _ => panic!("expected context parameter"),
    }
}

#[test]
fn test_set_literal_not_parameter() {
    // { a = 1; } should parse as set literal, not parameter
    let file = parse("{ a = 1; }").unwrap();
    match &file.value {
        Expression::Term(_) => {}
        _ => panic!("expected set literal (Term), not abstraction"),
    }
}

#[test]
fn test_empty_set_literal() {
    let file = parse("{}").unwrap();
    match &file.value {
        Expression::Term(_) => {}
        _ => panic!("expected empty set literal"),
    }
}

#[test]
fn test_nested_context_parameters() {
    let file = parse("a @ b @ { x }: x").unwrap();
    match &file.value {
        Expression::Abstraction(Parameter::ContextParameter(_, _, _), _, _) => {}
        _ => panic!("expected nested context parameters"),
    }
}

#[test]
fn test_inherit_simple() {
    let file = parse("{ inherit pkgs; }").unwrap();
    // Should parse successfully as set with inherit
    match &file.value {
        Expression::Term(_) => {}
        _ => panic!("expected set literal"),
    }
}

#[test]
fn test_inherit_multiple() {
    let file = parse("{ inherit pkgs lib stdenv; }").unwrap();
    match &file.value {
        Expression::Term(_) => {}
        _ => panic!("expected set with multiple inherits"),
    }
}

#[test]
fn test_inherit_from() {
    let file = parse("{ inherit (pkgs) gcc; }").unwrap();
    match &file.value {
        Expression::Term(_) => {}
        _ => panic!("expected set with inherit from"),
    }
}
