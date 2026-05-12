use lemonc::lexer::lexer::Lexer;
use lemonc::parser::parser::Parser;

#[test]
fn test_parse_simple_program() {
    let source = r#"
package test;
import std.io;

class MyClass {
    public field: int;

    this() {
        field = 0;
    }

    public void doSomething() {
        int x = 42;
        x = x + 1;
    }
}

int main() {
    MyClass obj = new MyClass();
    obj.doSomething();
    return 0;
}
"#;

    let lexer = Lexer::new(source);
    let tokens: Vec<_> = lexer.collect();
    let mut parser = Parser::new(tokens);
    let program = parser.parse();

    assert!(!parser.has_errors(), "Parser had errors: {:?}", parser.errors());
    assert_eq!(program.declarations.len(), 3);
}

#[test]
fn test_parse_interface() {
    let source = r#"
interface MyInterface {
    void doWork(int x);
    int getValue();
}
"#;

    let lexer = Lexer::new(source);
    let tokens: Vec<_> = lexer.collect();
    let mut parser = Parser::new(tokens);
    let program = parser.parse();

    assert!(!parser.has_errors(), "Parser had errors: {:?}", parser.errors());
    assert_eq!(program.declarations.len(), 1);
}

#[test]
fn test_parse_control_flow() {
    let source = r#"
int main() {
    int x = 10;
    if (x > 5) {
        x = x - 1;
    } else {
        x = x + 1;
    }

    for (int i = 0; i < 10; i = i + 1) {
        x = x * 2;
    }

    while (x > 0) {
        x = x - 1;
    }

    return x;
}
"#;

    let lexer = Lexer::new(source);
    let tokens: Vec<_> = lexer.collect();
    let mut parser = Parser::new(tokens);
    let program = parser.parse();

    assert!(!parser.has_errors(), "Parser had errors: {:?}", parser.errors());
}

#[test]
fn test_parse_try_catch() {
    let source = r#"
int main() {
    try {
        int x = 10 / 0;
    } catch (Exception e) {
        x = 0;
    } finally {
        x = -1;
    }
    return x;
}
"#;

    let lexer = Lexer::new(source);
    let tokens: Vec<_> = lexer.collect();
    let mut parser = Parser::new(tokens);
    let program = parser.parse();

    assert!(!parser.has_errors(), "Parser had errors: {:?}", parser.errors());
}

#[test]
fn test_parse_expressions() {
    let source = r#"
int main() {
    int a = 1 + 2 * 3;
    int b = (1 + 2) * 3;
    bool c = a > b && a < 10;
    int d = c ? 1 : 0;
    return d;
}
"#;

    let lexer = Lexer::new(source);
    let tokens: Vec<_> = lexer.collect();
    let mut parser = Parser::new(tokens);
    let program = parser.parse();

    assert!(!parser.has_errors(), "Parser had errors: {:?}", parser.errors());
}
