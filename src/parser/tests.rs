use super::Parser;
use crate::types::{Expression, StringPart, Term, Token};

#[test]
fn test_parse_simple_int() {
    let mut parser = Parser::new("42").unwrap();
    let file = parser.parse_file().unwrap();

    // Check it's a Term(Token(Integer))
    match &file.value {
        Expression::Term(Term::Token(ann)) => {
            assert!(matches!(&ann.value, Token::Integer(s) if s == "42"));
        }
        _ => panic!("expected Term(Token(Integer))"),
    }
}

#[test]
fn test_parse_identifier() {
    let mut parser = Parser::new("foo").unwrap();
    let file = parser.parse_file().unwrap();

    match &file.value {
        Expression::Term(Term::Token(ann)) => {
            assert!(matches!(&ann.value, Token::Identifier(s) if s == "foo"));
        }
        _ => panic!("expected Term(Token(Identifier))"),
    }
}

#[test]
fn test_parse_empty_set() {
    let mut parser = Parser::new("{}").unwrap();
    let file = parser.parse_file().unwrap();

    match &file.value {
        Expression::Term(Term::Set(None, _, items, _)) => {
            assert_eq!(items.0.len(), 0);
        }
        _ => panic!("expected empty set"),
    }
}

#[test]
fn test_parse_empty_list() {
    let mut parser = Parser::new("[]").unwrap();
    let file = parser.parse_file().unwrap();

    match &file.value {
        Expression::Term(Term::List(_, items, _)) => {
            assert_eq!(items.0.len(), 0);
        }
        _ => panic!("expected empty list"),
    }
}

#[test]
fn test_parse_parenthesized() {
    let mut parser = Parser::new("(42)").unwrap();
    let file = parser.parse_file().unwrap();

    match &file.value {
        Expression::Term(Term::Parenthesized(_, expr, _)) => match expr.as_ref() {
            Expression::Term(Term::Token(ann)) => {
                assert!(matches!(&ann.value, Token::Integer(s) if s == "42"));
            }
            _ => panic!("expected integer inside parens"),
        },
        _ => panic!("expected parenthesized expression"),
    }
}

#[test]
fn test_simple_string_trailing_space_preserved() {
    let file = crate::parse("\"outer ${\"inner ${x}\"} end\"").unwrap();
    match file.value {
        Expression::Term(Term::SimpleString(ann)) => {
            let line = &ann.value[0];
            match &line[2] {
                StringPart::TextPart(text) => assert_eq!(text, " end"),
                _ => panic!("expected trailing text part"),
            }
        }
        _ => panic!("unexpected parse result"),
    }
}

#[test]
fn test_parse_set_with_binding() {
    let mut parser = Parser::new("{ a = 1; }").unwrap();
    let file = parser.parse_file().unwrap();

    match &file.value {
        Expression::Term(Term::Set(None, _, bindings, _)) => {
            // Should have one binding
            assert!(bindings.0.len() > 0);
        }
        _ => panic!("expected set with bindings"),
    }
}

#[test]
fn test_parse_binary_op() {
    let mut parser = Parser::new("1 + 2").unwrap();
    let file = parser.parse_file().unwrap();

    match &file.value {
        Expression::Operation(_, op, _) => {
            assert!(matches!(op.value, Token::TPlus));
        }
        _ => panic!("expected operation"),
    }
}

#[test]
fn test_parse_let_in() {
    let mut parser = Parser::new("let a = 1; in a").unwrap();
    let file = parser.parse_file().unwrap();

    match &file.value {
        Expression::Let(_, _, _, _) => {
            // Success
        }
        _ => panic!("expected let expression"),
    }
}

#[test]
fn test_parse_if_then_else() {
    let mut parser = Parser::new("if true then 1 else 2").unwrap();
    let file = parser.parse_file().unwrap();

    match &file.value {
        Expression::If(_, _, _, _, _, _) => {
            // Success
        }
        _ => panic!("expected if expression"),
    }
}

#[test]
fn test_parse_lambda() {
    let mut parser = Parser::new("x: x").unwrap();
    let file = parser.parse_file().unwrap();

    match &file.value {
        Expression::Abstraction(_, _, _) => {
            // Success
        }
        _ => panic!("expected lambda expression"),
    }
}

#[test]
fn test_parse_application() {
    let mut parser = Parser::new("f x").unwrap();
    let file = parser.parse_file().unwrap();

    match &file.value {
        Expression::Application(_, _) => {
            // Success
        }
        _ => panic!("expected application"),
    }
}

#[test]
fn test_parse_list_with_items() {
    let mut parser = Parser::new("[1 2 3]").unwrap();
    let file = parser.parse_file().unwrap();

    match &file.value {
        Expression::Term(Term::List(_, items, _)) => {
            // Should have 3 items
            assert!(items.0.len() >= 3);
        }
        _ => panic!("expected list with items"),
    }
}

#[test]
fn test_parse_empty_string() {
    let mut parser = Parser::new(r#""""#).unwrap();
    let file = parser.parse_file().unwrap();

    match &file.value {
        Expression::Term(Term::SimpleString(_)) => {
            // Success
        }
        _ => panic!("expected simple string"),
    }
}

#[test]
fn test_parse_rec_set() {
    let mut parser = Parser::new("rec { a = 1; }").unwrap();
    let file = parser.parse_file().unwrap();

    match &file.value {
        Expression::Term(Term::Set(Some(_), _, _, _)) => {
            // Success - has rec token
        }
        _ => panic!("expected rec set"),
    }
}

#[test]
fn test_parse_negation() {
    let mut parser = Parser::new("-5").unwrap();
    let file = parser.parse_file().unwrap();

    match &file.value {
        Expression::Negation(_, _) => {
            // Success
        }
        _ => panic!("expected negation"),
    }
}

#[test]
fn test_parse_double_negation() {
    let mut parser = Parser::new("- -5").unwrap();
    let file = parser.parse_file().unwrap();

    match &file.value {
        Expression::Negation(_, inner) => {
            match inner.as_ref() {
                Expression::Negation(_, _) => {
                    // Success - double negation
                }
                _ => panic!("expected nested negation"),
            }
        }
        _ => panic!("expected negation"),
    }
}

#[test]
fn test_parse_env_path() {
    let mut parser = Parser::new("<nixpkgs>").unwrap();
    let file = parser.parse_file().unwrap();

    match &file.value {
        Expression::Term(Term::Token(ann)) => {
            assert!(matches!(&ann.value, Token::EnvPath(s) if s == "nixpkgs"));
        }
        _ => panic!("expected env path"),
    }
}

#[test]
fn test_parse_subtraction_not_application() {
    // f -5 should parse as (f - 5), NOT f(-5)
    let mut parser = Parser::new("f -5").unwrap();
    let file = parser.parse_file().unwrap();

    match &file.value {
        Expression::Operation(_, op, _) => {
            assert!(matches!(op.value, Token::TMinus));
        }
        _ => panic!("expected operation (subtraction), not application"),
    }
}

#[test]
fn test_parse_application_with_parens() {
    // f (-5) should parse as Application(f, Negation(5))
    let mut parser = Parser::new("f (-5)").unwrap();
    let file = parser.parse_file().unwrap();

    match &file.value {
        Expression::Application(_, arg) => {
            match arg.as_ref() {
                Expression::Term(Term::Parenthesized(_, inner, _)) => {
                    match inner.as_ref() {
                        Expression::Negation(_, _) => {
                            // Success
                        }
                        _ => panic!("expected negation inside parens"),
                    }
                }
                _ => panic!("expected parenthesized negation as argument"),
            }
        }
        _ => panic!("expected application"),
    }
}

#[test]
fn test_parse_selection() {
    let mut parser = Parser::new("pkgs.gcc").unwrap();
    let file = parser.parse_file().unwrap();

    match &file.value {
        Expression::Term(Term::Selection(_base, sels, None)) => {
            // Should have one selector
            assert!(sels.len() == 1);
        }
        _ => panic!("expected selection"),
    }
}

#[test]
fn test_parse_selection_chain() {
    let mut parser = Parser::new("a.b.c").unwrap();
    let file = parser.parse_file().unwrap();

    match &file.value {
        Expression::Term(Term::Selection(_, sels, None)) => {
            // Should have two selectors (b and c)
            assert!(sels.len() == 2);
        }
        _ => panic!("expected selection chain"),
    }
}

#[test]
fn test_parse_selection_with_default() {
    let mut parser = Parser::new("x.y or z").unwrap();
    let file = parser.parse_file().unwrap();

    match &file.value {
        Expression::Term(Term::Selection(_, sels, Some(_))) => {
            assert!(sels.len() == 1);
        }
        _ => panic!("expected selection with or-default"),
    }
}

#[test]
fn test_parse_member_check() {
    let mut parser = Parser::new("x ? y").unwrap();
    let file = parser.parse_file().unwrap();

    match &file.value {
        Expression::MemberCheck(_, _, sels) => {
            assert!(sels.len() == 1);
        }
        _ => panic!("expected member check"),
    }
}

#[test]
fn test_parse_member_check_chain() {
    let mut parser = Parser::new("x ? y.z").unwrap();
    let file = parser.parse_file().unwrap();

    match &file.value {
        Expression::MemberCheck(_, _, sels) => {
            assert!(sels.len() == 2);
        }
        _ => panic!("expected member check with selector chain"),
    }
}
