//! Lexer unit tests

use super::*;
use crate::types::Trivium;

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
    assert!(matches!(lexer.next_token(), Ok(Token::KLet)));
    assert!(matches!(lexer.next_token(), Ok(Token::KIn)));
    assert!(matches!(lexer.next_token(), Ok(Token::KIf)));
    assert!(matches!(lexer.next_token(), Ok(Token::KThen)));
    assert!(matches!(lexer.next_token(), Ok(Token::KElse)));
}

#[test]
fn test_tokenize_operators() {
    let mut lexer = Lexer::new("+ - * / ++ // == != < > <= >= && || ->");
    assert!(matches!(lexer.next_token(), Ok(Token::TPlus)));
    assert!(matches!(lexer.next_token(), Ok(Token::TMinus)));
    assert!(matches!(lexer.next_token(), Ok(Token::TMul)));
    assert!(matches!(lexer.next_token(), Ok(Token::TDiv)));
    assert!(matches!(lexer.next_token(), Ok(Token::TConcat)));
    assert!(matches!(lexer.next_token(), Ok(Token::TUpdate)));
    assert!(matches!(lexer.next_token(), Ok(Token::TEqual)));
    assert!(matches!(lexer.next_token(), Ok(Token::TUnequal)));
    assert!(matches!(lexer.next_token(), Ok(Token::TLess)));
    assert!(matches!(lexer.next_token(), Ok(Token::TGreater)));
    assert!(matches!(lexer.next_token(), Ok(Token::TLessEqual)));
    assert!(matches!(lexer.next_token(), Ok(Token::TGreaterEqual)));
    assert!(matches!(lexer.next_token(), Ok(Token::TAnd)));
    assert!(matches!(lexer.next_token(), Ok(Token::TOr)));
    assert!(matches!(lexer.next_token(), Ok(Token::TImplies)));
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
    assert!(matches!(lexer.next_token(), Ok(Token::TBraceOpen)));
    assert!(matches!(lexer.next_token(), Ok(Token::TBraceClose)));
    assert!(matches!(lexer.next_token(), Ok(Token::TBrackOpen)));
    assert!(matches!(lexer.next_token(), Ok(Token::TBrackClose)));
    assert!(matches!(lexer.next_token(), Ok(Token::TParenOpen)));
    assert!(matches!(lexer.next_token(), Ok(Token::TParenClose)));
    assert!(matches!(lexer.next_token(), Ok(Token::TComma)));
    assert!(matches!(lexer.next_token(), Ok(Token::TSemicolon)));
    assert!(matches!(lexer.next_token(), Ok(Token::TColon)));
    assert!(matches!(lexer.next_token(), Ok(Token::TAt)));
    assert!(matches!(lexer.next_token(), Ok(Token::TDot)));
    assert!(matches!(lexer.next_token(), Ok(Token::TEllipsis)));
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
    assert!(matches!(lexer.next_token(), Ok(Token::TBraceOpen)));
    assert!(matches!(lexer.next_token(), Ok(Token::Identifier(s)) if s == "a"));
    assert!(matches!(lexer.next_token(), Ok(Token::TAssign)));
    assert!(matches!(lexer.next_token(), Ok(Token::Integer(s)) if s == "1"));
    assert!(matches!(lexer.next_token(), Ok(Token::TSemicolon)));
    assert!(matches!(lexer.next_token(), Ok(Token::TBraceClose)));
}

#[test]
fn test_lexeme_with_comments() {
    let mut lexer = Lexer::new("# leading comment\n{ a = 1; # trailing\n}");
    lexer.start_parse();

    // First token: { with leading comment
    let brace = lexer.lexeme().unwrap();
    assert!(matches!(brace.value, Token::TBraceOpen));
    assert_eq!(brace.pre_trivia.len(), 1); // Should have the leading comment
    assert!(matches!(&brace.pre_trivia[0], Trivium::LineComment(s) if &**s == " leading comment"));

    // Second token: a
    let ident = lexer.lexeme().unwrap();
    assert!(matches!(&ident.value, Token::Identifier(s) if s == "a"));

    // Third token: =
    let eq = lexer.lexeme().unwrap();
    assert!(matches!(eq.value, Token::TAssign));

    // Fourth token: 1
    let num = lexer.lexeme().unwrap();
    assert!(matches!(&num.value, Token::Integer(s) if s == "1"));

    // Fifth token: ; with trailing comment
    let semi = lexer.lexeme().unwrap();
    assert!(matches!(semi.value, Token::TSemicolon));
    assert!(semi.trail_comment.is_some());
    if let Some(ref tc) = semi.trail_comment {
        assert_eq!(&*tc.0, "trailing");
    }

    // Sixth token: }
    let close = lexer.lexeme().unwrap();
    assert!(matches!(close.value, Token::TBraceClose));
}

#[test]
fn test_lexeme_preserves_trivia() {
    let mut lexer = Lexer::new("let\n\n  # comment\n  a = 1; in a");
    lexer.start_parse();

    let let_tok = lexer.lexeme().unwrap();
    assert!(matches!(let_tok.value, Token::KLet));

    // 'a' should have EmptyLine and comment
    let a_tok = lexer.lexeme().unwrap();
    assert!(matches!(&a_tok.value, Token::Identifier(s) if s == "a"));
    assert!(!a_tok.pre_trivia.is_empty());
    // Should have EmptyLine and LineComment
    assert!(
        a_tok
            .pre_trivia
            .iter()
            .any(|t| matches!(t, Trivium::EmptyLine()))
    );
}
