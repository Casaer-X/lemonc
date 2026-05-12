use super::token::{Span, Token, TokenKind};
use std::iter::Peekable;
use std::str::Chars;

pub struct Lexer<'a> {
    source: &'a str,
    chars: Peekable<Chars<'a>>,
    pos: usize,
    line: usize,
    col: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            source,
            chars: source.chars().peekable(),
            pos: 0,
            line: 1,
            col: 1,
        }
    }

    pub fn next_token(&mut self) -> Token {
        self.skip_whitespace();

        let start_pos = self.pos;
        let start_line = self.line;
        let start_col = self.col;

        match self.peek_char() {
            None => self.make_token(TokenKind::EndOfFile, start_pos, start_pos),

            Some('(') => { self.advance(); self.make_token(TokenKind::LeftParen, start_pos, self.pos) }
            Some(')') => { self.advance(); self.make_token(TokenKind::RightParen, start_pos, self.pos) }
            Some('{') => { self.advance(); self.make_token(TokenKind::LeftBrace, start_pos, self.pos) }
            Some('}') => { self.advance(); self.make_token(TokenKind::RightBrace, start_pos, self.pos) }
            Some('[') => { self.advance(); self.make_token(TokenKind::LeftBracket, start_pos, self.pos) }
            Some(']') => { self.advance(); self.make_token(TokenKind::RightBracket, start_pos, self.pos) }
            Some(',') => { self.advance(); self.make_token(TokenKind::Comma, start_pos, self.pos) }
            Some('.') => { self.advance(); self.make_token(TokenKind::Dot, start_pos, self.pos) }
            Some(';') => { self.advance(); self.make_token(TokenKind::Semicolon, start_pos, self.pos) }
            Some(':') => { self.advance(); self.make_token(TokenKind::Colon, start_pos, self.pos) }
            Some('?') => { self.advance(); self.make_token(TokenKind::Question, start_pos, self.pos) }
            Some('~') => { self.advance(); self.make_token(TokenKind::Tilde, start_pos, self.pos) }
            Some('@') => { self.advance(); self.make_token(TokenKind::At, start_pos, self.pos) }

            Some('+') => {
                self.advance();
                match self.peek_char() {
                    Some('+') => { self.advance(); self.make_token(TokenKind::Inc, start_pos, self.pos) }
                    Some('=') => { self.advance(); self.make_token(TokenKind::PlusAssign, start_pos, self.pos) }
                    _ => self.make_token(TokenKind::Plus, start_pos, self.pos)
                }
            }
            Some('-') => {
                self.advance();
                match self.peek_char() {
                    Some('-') => { self.advance(); self.make_token(TokenKind::Dec, start_pos, self.pos) }
                    Some('=') => { self.advance(); self.make_token(TokenKind::MinusAssign, start_pos, self.pos) }
                    Some('>') => { self.advance(); self.make_token(TokenKind::Arrow, start_pos, self.pos) }
                    _ => self.make_token(TokenKind::Minus, start_pos, self.pos)
                }
            }
            Some('*') => {
                self.advance();
                if self.peek_char() == Some('=') {
                    self.advance();
                    self.make_token(TokenKind::StarAssign, start_pos, self.pos)
                } else {
                    self.make_token(TokenKind::Star, start_pos, self.pos)
                }
            }
            Some('/') => {
                self.advance();
                match self.peek_char() {
                    Some('/') => {
                        self.skip_line_comment();
                        self.next_token()
                    }
                    Some('*') => {
                        self.skip_block_comment();
                        self.next_token()
                    }
                    Some('=') => {
                        self.advance();
                        self.make_token(TokenKind::SlashAssign, start_pos, self.pos)
                    }
                    _ => self.make_token(TokenKind::Slash, start_pos, self.pos)
                }
            }
            Some('%') => {
                self.advance();
                if self.peek_char() == Some('=') {
                    self.advance();
                    self.make_token(TokenKind::PercentAssign, start_pos, self.pos)
                } else {
                    self.make_token(TokenKind::Percent, start_pos, self.pos)
                }
            }
            Some('&') => {
                self.advance();
                match self.peek_char() {
                    Some('&') => { self.advance(); self.make_token(TokenKind::And, start_pos, self.pos) }
                    Some('=') => { self.advance(); self.make_token(TokenKind::AndAssign, start_pos, self.pos) }
                    _ => self.make_token(TokenKind::Amp, start_pos, self.pos)
                }
            }
            Some('|') => {
                self.advance();
                match self.peek_char() {
                    Some('|') => { self.advance(); self.make_token(TokenKind::Or, start_pos, self.pos) }
                    Some('=') => { self.advance(); self.make_token(TokenKind::OrAssign, start_pos, self.pos) }
                    _ => self.make_token(TokenKind::Pipe, start_pos, self.pos)
                }
            }
            Some('^') => {
                self.advance();
                if self.peek_char() == Some('=') {
                    self.advance();
                    self.make_token(TokenKind::XorAssign, start_pos, self.pos)
                } else {
                    self.make_token(TokenKind::Caret, start_pos, self.pos)
                }
            }
            Some('<') => {
                self.advance();
                match self.peek_char() {
                    Some('<') => {
                        self.advance();
                        if self.peek_char() == Some('=') {
                            self.advance();
                            self.make_token(TokenKind::ShlAssign, start_pos, self.pos)
                        } else {
                            self.make_token(TokenKind::Shl, start_pos, self.pos)
                        }
                    }
                    Some('=') => { self.advance(); self.make_token(TokenKind::Le, start_pos, self.pos) }
                    _ => self.make_token(TokenKind::Lt, start_pos, self.pos)
                }
            }
            Some('>') => {
                self.advance();
                match self.peek_char() {
                    Some('>') => {
                        self.advance();
                        if self.peek_char() == Some('=') {
                            self.advance();
                            self.make_token(TokenKind::ShrAssign, start_pos, self.pos)
                        } else {
                            self.make_token(TokenKind::Shr, start_pos, self.pos)
                        }
                    }
                    Some('=') => { self.advance(); self.make_token(TokenKind::Ge, start_pos, self.pos) }
                    _ => self.make_token(TokenKind::Gt, start_pos, self.pos)
                }
            }
            Some('=') => {
                self.advance();
                if self.peek_char() == Some('=') {
                    self.advance();
                    self.make_token(TokenKind::Eq, start_pos, self.pos)
                } else {
                    self.make_token(TokenKind::Assign, start_pos, self.pos)
                }
            }
            Some('!') => {
                self.advance();
                if self.peek_char() == Some('=') {
                    self.advance();
                    self.make_token(TokenKind::Ne, start_pos, self.pos)
                } else {
                    self.make_token(TokenKind::Not, start_pos, self.pos)
                }
            }

            Some('"') => self.read_string(),
            Some('\'') => self.read_char(),

            Some(c) if c.is_ascii_digit() => self.read_number(start_pos, start_line, start_col),
            Some(c) if c.is_alphabetic() || c == '_' => self.read_identifier(start_pos, start_line, start_col),

            _ => {
                let c = self.advance();
                self.make_token(TokenKind::Error(format!("Unexpected character: '{}'", c)), start_pos, self.pos)
            }
        }
    }

    fn peek_char(&mut self) -> Option<char> {
        self.chars.peek().copied()
    }

    fn advance(&mut self) -> char {
        let c = self.chars.next().unwrap_or('\0');
        self.pos += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        c
    }

    fn skip_whitespace(&mut self) {
        while let Some(c) = self.peek_char() {
            if c.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_line_comment(&mut self) {
        while let Some(c) = self.peek_char() {
            if c == '\n' {
                self.advance();
                break;
            }
            self.advance();
        }
    }

    fn skip_block_comment(&mut self) {
        while let Some(c) = self.peek_char() {
            if c == '*' {
                self.advance();
                if self.peek_char() == Some('/') {
                    self.advance();
                    break;
                }
            } else {
                self.advance();
            }
        }
    }

    fn read_string(&mut self) -> Token {
        let start_pos = self.pos;
        let start_line = self.line;
        let start_col = self.col;
        self.advance();

        let mut value = String::new();
        while let Some(c) = self.peek_char() {
            match c {
                '"' => {
                    self.advance();
                    let end_pos = self.pos;
                    return Token {
                        kind: TokenKind::StringLiteral(value),
                        span: Span { start: start_pos, end: end_pos, line: start_line, column: start_col },
                    };
                }
                '\\' => {
                    self.advance();
                    match self.peek_char() {
                        Some('n') => { value.push('\n'); self.advance(); }
                        Some('t') => { value.push('\t'); self.advance(); }
                        Some('r') => { value.push('\r'); self.advance(); }
                        Some('\\') => { value.push('\\'); self.advance(); }
                        Some('"') => { value.push('"'); self.advance(); }
                        Some(c) => { value.push(c); self.advance(); }
                        None => break,
                    }
                }
                _ => {
                    value.push(c);
                    self.advance();
                }
            }
        }
        self.make_token(TokenKind::Error("Unterminated string".to_string()), start_pos, self.pos)
    }

    fn read_char(&mut self) -> Token {
        let start_pos = self.pos;
        let _start_line = self.line;
        let _start_col = self.col;
        self.advance();

        let value = match self.peek_char() {
            Some('\\') => {
                self.advance();
                match self.peek_char() {
                    Some('n') => { self.advance(); '\n' }
                    Some('t') => { self.advance(); '\t' }
                    Some('r') => { self.advance(); '\r' }
                    Some('\\') => { self.advance(); '\\' }
                    Some('\'') => { self.advance(); '\'' }
                    Some(c) => { self.advance(); c }
                    None => '\0',
                }
            }
            Some(c) => { self.advance(); c }
            None => '\0',
        };

        if self.peek_char() == Some('\'') {
            self.advance();
            let end_pos = self.pos;
            self.make_token(TokenKind::CharLiteral(value), start_pos, end_pos)
        } else {
            self.make_token(TokenKind::Error("Unterminated char literal".to_string()), start_pos, self.pos)
        }
    }

    fn read_number(&mut self, start_pos: usize, start_line: usize, start_col: usize) -> Token {
        let mut is_float = false;
        while let Some(c) = self.peek_char() {
            if c.is_ascii_digit() {
                self.advance();
            } else if c == '.' && !is_float {
                let next_after_dot = self.source.as_bytes().get(self.pos + 1);
                if let Some(&b) = next_after_dot {
                    if (b as char).is_ascii_digit() {
                        is_float = true;
                        self.advance();
                    } else {
                        break;
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        let num_str = &self.source[start_pos..self.pos];
        let end_pos = self.pos;

        if is_float {
            match num_str.parse::<f64>() {
                Ok(val) => Token {
                    kind: TokenKind::FloatLiteral(val),
                    span: Span { start: start_pos, end: end_pos, line: start_line, column: start_col },
                },
                Err(_) => self.make_token(TokenKind::Error("Invalid float literal".to_string()), start_pos, end_pos),
            }
        } else {
            match num_str.parse::<i64>() {
                Ok(val) => Token {
                    kind: TokenKind::IntegerLiteral(val),
                    span: Span { start: start_pos, end: end_pos, line: start_line, column: start_col },
                },
                Err(_) => self.make_token(TokenKind::Error("Integer too large".to_string()), start_pos, end_pos),
            }
        }
    }

    fn read_identifier(&mut self, start_pos: usize, start_line: usize, start_col: usize) -> Token {
        while let Some(c) = self.peek_char() {
            if c.is_alphanumeric() || c == '_' {
                self.advance();
            } else {
                break;
            }
        }

        let ident = &self.source[start_pos..self.pos];
        let end_pos = self.pos;

        let kind = match ident {
            "class" => TokenKind::Class,
            "interface" => TokenKind::Interface,
            "extends" => TokenKind::Extends,
            "implements" => TokenKind::Implements,
            "abstract" => TokenKind::Abstract,
            "virtual" => TokenKind::Virtual,
            "override" => TokenKind::Override,
            "final" => TokenKind::Final,
            "static" => TokenKind::Static,
            "public" => TokenKind::Public,
            "private" => TokenKind::Private,
            "reflectable" => TokenKind::Reflectable,
            "new" => TokenKind::New,
            "delete" => TokenKind::Delete,
            "null" => TokenKind::Null,
            "true" => TokenKind::True,
            "false" => TokenKind::False,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "for" => TokenKind::For,
            "while" => TokenKind::While,
            "return" => TokenKind::Return,
            "break" => TokenKind::Break,
            "continue" => TokenKind::Continue,
            "try" => TokenKind::Try,
            "catch" => TokenKind::Catch,
            "throw" => TokenKind::Throw,
            "import" => TokenKind::Import,
            "package" => TokenKind::Package,
            "extern" => TokenKind::Extern,
            "sizeof" => TokenKind::Sizeof,
            "typeid" => TokenKind::TypeId,
            "this" => TokenKind::This,
            "super" => TokenKind::Super,
            "instanceof" => TokenKind::InstanceOf,
            "as" => TokenKind::As,
            "finally" => TokenKind::Finally,
            "void" => TokenKind::Void,
            "bool" => TokenKind::Bool,
            "byte" => TokenKind::Byte,
            "char" => TokenKind::Char,
            "short" => TokenKind::Short,
            "int" => TokenKind::Int,
            "long" => TokenKind::Long,
            "float" => TokenKind::Float,
            "double" => TokenKind::Double,
            _ => TokenKind::Identifier(ident.to_string()),
        };

        Token {
            kind,
            span: Span { start: start_pos, end: end_pos, line: start_line, column: start_col },
        }
    }

    fn make_token(&self, kind: TokenKind, start: usize, end: usize) -> Token {
        Token {
            kind,
            span: Span { start, end, line: self.line, column: self.col },
        }
    }
}

impl<'a> Iterator for Lexer<'a> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        let token = self.next_token();
        if matches!(token.kind, TokenKind::EndOfFile) {
            None
        } else {
            Some(token)
        }
    }
}
