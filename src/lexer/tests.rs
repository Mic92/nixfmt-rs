//! Lexer unit tests

use super::*;
use crate::ast::TriviaPiece;

#[test]
fn test_split_lines() {
    let input = "line1\nline2\r\nline3";
    let lines = comments::split_lines(input);
    assert_eq!(lines, vec!["line1", "line2", "line3"]);
}

#[test]
fn test_remove_stars() {
    // Stars at column 0: " *" gets replaced with ""
    let lines = vec![
        "first line".to_string(),
        " * second".to_string(),
        " * third".to_string(),
    ];
    let result = comments::remove_stars(0, lines);
    // After stripping " *", we get " second" (space from after the star)
    assert_eq!(
        result,
        vec![
            "first line".to_string(),
            " second".to_string(),
            " third".to_string()
        ]
    );

    // Stars at column 1: "  *" gets replaced with " "
    let lines2 = vec![
        "first".to_string(),
        "  * line2".to_string(),
        "  * line3".to_string(),
    ];
    let result2 = comments::remove_stars(1, lines2);
    assert_eq!(
        result2,
        vec![
            "first".to_string(),
            "  line2".to_string(),
            "  line3".to_string()
        ]
    );

    // No stars - should return unchanged
    let lines3 = vec!["a".to_string(), "b".to_string()];
    let result3 = comments::remove_stars(0, lines3.clone());
    assert_eq!(result3, lines3);
}

#[test]
fn test_parse_line_comment() {
    let mut lexer = Lexer::new("# hello world\n");
    lexer.parse_trivia();
    assert_eq!(lexer.trivia_scratch.len(), 2); // comment + newline
}

#[test]
fn test_parse_block_comment() {
    let mut lexer = Lexer::new("/* hello */");
    lexer.parse_trivia();
    assert_eq!(lexer.trivia_scratch.len(), 1);
}

#[test]
fn test_tokenize_keywords() {
    let mut lexer = Lexer::new("let in if then else");
    assert!(matches!(lexer.next_token(), Ok(Token::Let)));
    assert!(matches!(lexer.next_token(), Ok(Token::In)));
    assert!(matches!(lexer.next_token(), Ok(Token::If)));
    assert!(matches!(lexer.next_token(), Ok(Token::Then)));
    assert!(matches!(lexer.next_token(), Ok(Token::Else)));
}

#[test]
fn test_tokenize_operators() {
    let mut lexer = Lexer::new("+ - * / ++ // == != < > <= >= && || ->");
    assert!(matches!(lexer.next_token(), Ok(Token::Plus)));
    assert!(matches!(lexer.next_token(), Ok(Token::Minus)));
    assert!(matches!(lexer.next_token(), Ok(Token::Mul)));
    assert!(matches!(lexer.next_token(), Ok(Token::Div)));
    assert!(matches!(lexer.next_token(), Ok(Token::Concat)));
    assert!(matches!(lexer.next_token(), Ok(Token::Update)));
    assert!(matches!(lexer.next_token(), Ok(Token::Equal)));
    assert!(matches!(lexer.next_token(), Ok(Token::Unequal)));
    assert!(matches!(lexer.next_token(), Ok(Token::Less)));
    assert!(matches!(lexer.next_token(), Ok(Token::Greater)));
    assert!(matches!(lexer.next_token(), Ok(Token::LessEqual)));
    assert!(matches!(lexer.next_token(), Ok(Token::GreaterEqual)));
    assert!(matches!(lexer.next_token(), Ok(Token::And)));
    assert!(matches!(lexer.next_token(), Ok(Token::Or)));
    assert!(matches!(lexer.next_token(), Ok(Token::Implies)));
}

#[test]
fn test_tokenize_numbers() {
    let mut lexer = Lexer::new("42 3.14 1.5e10 2e-5");

    assert!(matches!(lexer.next_token(), Ok(Token::Integer(s)) if s == "42"));
    assert!(matches!(lexer.next_token(), Ok(Token::Float(s)) if s == "3.14"));
}

#[test]
fn test_tokenize_delimiters() {
    let mut lexer = Lexer::new("{ } [ ] ( ) , ; : @ . ...");
    assert!(matches!(lexer.next_token(), Ok(Token::BraceOpen)));
    assert!(matches!(lexer.next_token(), Ok(Token::BraceClose)));
    assert!(matches!(lexer.next_token(), Ok(Token::BrackOpen)));
    assert!(matches!(lexer.next_token(), Ok(Token::BrackClose)));
    assert!(matches!(lexer.next_token(), Ok(Token::ParenOpen)));
    assert!(matches!(lexer.next_token(), Ok(Token::ParenClose)));
    assert!(matches!(lexer.next_token(), Ok(Token::Comma)));
    assert!(matches!(lexer.next_token(), Ok(Token::Semicolon)));
    assert!(matches!(lexer.next_token(), Ok(Token::Colon)));
    assert!(matches!(lexer.next_token(), Ok(Token::At)));
    assert!(matches!(lexer.next_token(), Ok(Token::Dot)));
    assert!(matches!(lexer.next_token(), Ok(Token::Ellipsis)));
}

#[test]
fn test_tokenize_identifiers() {
    let mut lexer = Lexer::new("foo bar_baz hello-world");
    assert!(matches!(lexer.next_token(), Ok(Token::Identifier(s)) if s == "foo"));
    assert!(matches!(lexer.next_token(), Ok(Token::Identifier(s)) if s == "bar_baz"));
    assert!(matches!(lexer.next_token(), Ok(Token::Identifier(s)) if s == "hello-world"));
}

#[test]
fn test_tokenize_simple_expression() {
    let mut lexer = Lexer::new("{ a = 1; }");
    assert!(matches!(lexer.next_token(), Ok(Token::BraceOpen)));
    assert!(matches!(lexer.next_token(), Ok(Token::Identifier(s)) if s == "a"));
    assert!(matches!(lexer.next_token(), Ok(Token::Assign)));
    assert!(matches!(lexer.next_token(), Ok(Token::Integer(s)) if s == "1"));
    assert!(matches!(lexer.next_token(), Ok(Token::Semicolon)));
    assert!(matches!(lexer.next_token(), Ok(Token::BraceClose)));
}

#[test]
fn test_lexeme_with_comments() {
    let mut lexer = Lexer::new("# leading comment\n{ a = 1; # trailing\n}");
    lexer.start_parse();

    // First token: { with leading comment
    let brace = lexer.lexeme().unwrap();
    assert!(matches!(brace.value, Token::BraceOpen));
    assert_eq!(brace.pre_trivia.len(), 1); // Should have the leading comment
    assert!(
        matches!(&brace.pre_trivia[0], TriviaPiece::LineComment(s) if &**s == " leading comment")
    );

    // Second token: a
    let ident = lexer.lexeme().unwrap();
    assert!(matches!(&ident.value, Token::Identifier(s) if s == "a"));

    // Third token: =
    let eq = lexer.lexeme().unwrap();
    assert!(matches!(eq.value, Token::Assign));

    // Fourth token: 1
    let num = lexer.lexeme().unwrap();
    assert!(matches!(&num.value, Token::Integer(s) if s == "1"));

    // Fifth token: ; with trailing comment
    let semi = lexer.lexeme().unwrap();
    assert!(matches!(semi.value, Token::Semicolon));
    assert!(semi.trail_comment.is_some());
    if let Some(ref tc) = semi.trail_comment {
        assert_eq!(&*tc.0, "trailing");
    }

    // Sixth token: }
    let close = lexer.lexeme().unwrap();
    assert!(matches!(close.value, Token::BraceClose));
}

#[test]
fn test_lexeme_preserves_trivia() {
    let mut lexer = Lexer::new("let\n\n  # comment\n  a = 1; in a");
    lexer.start_parse();

    let let_tok = lexer.lexeme().unwrap();
    assert!(matches!(let_tok.value, Token::Let));

    // 'a' should have EmptyLine and comment
    let a_tok = lexer.lexeme().unwrap();
    assert!(matches!(&a_tok.value, Token::Identifier(s) if s == "a"));
    assert!(!a_tok.pre_trivia.is_empty());
    // Should have EmptyLine and LineComment
    assert!(
        a_tok
            .pre_trivia
            .iter()
            .any(|t| matches!(t, TriviaPiece::EmptyLine))
    );
}
