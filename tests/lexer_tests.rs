use lemonc::lexer::lexer::Lexer;
use lemonc::lexer::token::TokenKind;

#[test]
fn test_keywords() {
    let source = "class interface extends implements";
    let mut lexer = Lexer::new(source);
    let tokens: Vec<_> = lexer.collect();
    assert!(tokens.len() >= 4);
    assert!(matches!(tokens[0].kind, TokenKind::Class));
    assert!(matches!(tokens[1].kind, TokenKind::Interface));
    assert!(matches!(tokens[2].kind, TokenKind::Extends));
    assert!(matches!(tokens[3].kind, TokenKind::Implements));
}

#[test]
fn test_string_literal() {
    let source = "\"Hello, Lemon!\"";
    let mut lexer = Lexer::new(source);
    let token = lexer.next().unwrap();
    match token.kind {
        TokenKind::StringLiteral(ref s) => assert_eq!(s, "Hello, Lemon!"),
        _ => panic!("Expected string literal"),
    }
}

#[test]
fn test_number_literal() {
    let source = "42 3.14";
    let mut lexer = Lexer::new(source);
    let t1 = lexer.next().unwrap();
    let t2 = lexer.next().unwrap();
    match t1.kind {
        TokenKind::IntegerLiteral(42) => (),
        _ => panic!("Expected 42"),
    }
    match t2.kind {
        TokenKind::FloatLiteral(v) => assert!((v - 3.14).abs() < 0.001),
        _ => panic!("Expected 3.14"),
    }
}

#[test]
fn test_operators() {
    let source = "+ - * / % ++ -- == != < > <= >= && || = += -= *= /=";
    let mut lexer = Lexer::new(source);
    let expected = vec![
        TokenKind::Plus, TokenKind::Minus, TokenKind::Star, TokenKind::Slash,
        TokenKind::Percent, TokenKind::Inc, TokenKind::Dec, TokenKind::Eq,
        TokenKind::Ne, TokenKind::Lt, TokenKind::Gt, TokenKind::Le,
        TokenKind::Ge, TokenKind::And, TokenKind::Or, TokenKind::Assign,
        TokenKind::PlusAssign, TokenKind::MinusAssign, TokenKind::StarAssign,
        TokenKind::SlashAssign,
    ];
    for expected_kind in expected {
        let token = lexer.next().unwrap();
        assert_eq!(token.kind, expected_kind);
    }
}

#[test]
fn test_comments() {
    let source = r#"
        // This is a line comment
        class Dog { /* block comment */ }
    "#;
    let mut lexer = Lexer::new(source);
    let tokens: Vec<_> = lexer.collect();
    assert!(tokens.len() >= 4);
    assert!(matches!(tokens[0].kind, TokenKind::Class));
}

#[test]
fn test_lemon_class() {
    let source = r#"
        @reflectable
        public class Animal {
            private String name;
            public virtual void speak() {
                printf("...");
            }
        }
    "#;
    let mut lexer = Lexer::new(source);
    let tokens: Vec<_> = lexer.collect();
    assert!(tokens.len() > 10);
    assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Class)));
    assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Reflectable)));
    assert!(tokens.iter().any(|t| matches!(t.kind, TokenKind::Virtual)));
}
