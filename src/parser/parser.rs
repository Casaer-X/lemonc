use crate::ast::node::*;
use crate::diagnostics;
use crate::lexer::token::{Token, TokenKind};

#[derive(Debug)]
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    errors: Vec<String>,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            errors: Vec::new(),
        }
    }

    pub fn parse(&mut self) -> Program {
        let decls = self.parse_program();
        Program { declarations: decls }
    }

    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    pub fn errors(&self) -> &[String] {
        &self.errors
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    fn peek_kind(&self) -> Option<TokenKind> {
        self.peek().map(|t| t.kind.clone())
    }

    fn current(&self) -> Option<&Token> {
        self.peek()
    }

    fn advance(&mut self) -> Option<Token> {
        let token = self.tokens.get(self.pos).cloned();
        if token.is_some() {
            self.pos += 1;
        }
        token
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token, String> {
        if let Some(token) = self.advance() {
            if token.kind == kind {
                Ok(token)
            } else {
                let msg = format!(
                    "Expected {:?}, found {:?} at line {} col {}",
                    kind, token.kind, token.span.line, token.span.column
                );
                self.errors.push(msg.clone());
                diagnostics::report_error(token.span.line, token.span.column, &msg);
                Err(msg)
            }
        } else {
            let msg = format!("Unexpected end of input, expected {:?}", kind);
            self.errors.push(msg.clone());
            Err(msg)
        }
    }

    fn expect_identifier(&mut self) -> Result<Token, String> {
        if let Some(token) = self.advance() {
            if matches!(token.kind, TokenKind::Identifier(_)) {
                Ok(token)
            } else {
                let msg = format!(
                    "Expected identifier, found {:?} at line {} col {}",
                    token.kind, token.span.line, token.span.column
                );
                self.errors.push(msg.clone());
                diagnostics::report_error(token.span.line, token.span.column, &msg);
                Err(msg)
            }
        } else {
            let msg = "Unexpected end of input, expected identifier".to_string();
            self.errors.push(msg.clone());
            Err(msg)
        }
    }

    fn match_token(&mut self, kind: TokenKind) -> bool {
        if self.peek_kind() == Some(kind.clone()) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn is_at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn check_keyword(&self, keyword: TokenKind) -> bool {
        self.peek_kind() == Some(keyword)
    }

    fn parse_program(&mut self) -> Vec<Declaration> {
        let mut decls = Vec::new();

        if self.check_keyword(TokenKind::Package) {
            let pkg = self.parse_package_decl();
            decls.push(Declaration::Package(pkg));
        }

        while self.check_keyword(TokenKind::Import) {
            let import = self.parse_import_decl();
            decls.push(Declaration::Import(import));
        }

        while !self.is_at_end() {
            if let Some(decl) = self.parse_declaration() {
                decls.push(decl);
            } else {
                self.advance();
            }
        }

        decls
    }

    fn parse_package_decl(&mut self) -> PackageDecl {
        self.expect(TokenKind::Package).unwrap();
        let name = self.expect_identifier().unwrap();
        self.expect(TokenKind::Semicolon).ok();
        let lexeme = match name.kind {
            TokenKind::Identifier(s) => s,
            _ => String::new(),
        };
        PackageDecl { name: lexeme }
    }

    fn parse_import_decl(&mut self) -> ImportDecl {
        self.expect(TokenKind::Import).unwrap();
        let path = self.parse_import_path();
        let alias = if self.match_token(TokenKind::As) {
            let name = self.expect_identifier().unwrap();
            match name.kind {
                TokenKind::Identifier(s) => Some(s),
                _ => Some(String::new()),
            }
        } else {
            None
        };
        self.expect(TokenKind::Semicolon).ok();
        ImportDecl { path, alias }
    }

    fn parse_import_path(&mut self) -> String {
        let mut parts = Vec::new();

        while !self.is_at_end() {
            match self.peek_kind() {
                Some(TokenKind::Identifier(_)) => {
                    let token = self.advance().unwrap();
                    if let TokenKind::Identifier(s) = token.kind {
                        parts.push(s);
                    }
                }
                Some(TokenKind::Dot) => {
                    self.advance();
                }
                Some(TokenKind::Star) => {
                    self.advance();
                    parts.push("*".to_string());
                    break;
                }
                _ => break,
            }
        }

        parts.join(".")
    }

    fn parse_declaration(&mut self) -> Option<Declaration> {
        let mut lookahead = self.pos;
        while lookahead < self.tokens.len() {
            match &self.tokens[lookahead].kind {
                TokenKind::At => { lookahead += 2; }
                TokenKind::Public | TokenKind::Private | TokenKind::Abstract
                | TokenKind::Final | TokenKind::Static | TokenKind::Virtual
                | TokenKind::Override | TokenKind::Reflectable => { lookahead += 1; }
                _ => break,
            }
        }

        if lookahead < self.tokens.len() {
            match self.tokens[lookahead].kind {
                TokenKind::Class => {
                    let class = self.parse_class_decl();
                    return Some(Declaration::Class(class));
                }
                TokenKind::Interface => {
                    let interface = self.parse_interface_decl();
                    return Some(Declaration::Interface(interface));
                }
                _ => {}
            }
        }

        if let Some(func) = self.parse_function_decl() {
            Some(Declaration::Function(func))
        } else {
            None
        }
    }

    fn parse_class_decl(&mut self) -> ClassDecl {
        let modifiers = self.parse_class_modifiers();
        self.expect(TokenKind::Class).unwrap();

        let name_token = self.expect_identifier().unwrap();
        let name = match name_token.kind {
            TokenKind::Identifier(s) => s,
            _ => String::new(),
        };

        let type_params = if self.peek_kind() == Some(TokenKind::Lt) {
            self.parse_type_params()
        } else {
            Vec::new()
        };

        let extends = if self.match_token(TokenKind::Extends) {
            Some(self.parse_type_ref())
        } else {
            None
        };

        let implements = if self.match_token(TokenKind::Implements) {
            let mut types = vec![self.parse_type_ref()];
            while self.match_token(TokenKind::Comma) {
                types.push(self.parse_type_ref());
            }
            types
        } else {
            Vec::new()
        };

        let members = if self.match_token(TokenKind::LeftBrace) {
            let mut members = Vec::new();
            while !self.check_keyword(TokenKind::RightBrace) && !self.is_at_end() {
                if let Some(member) = self.parse_class_member() {
                    members.push(member);
                } else {
                    self.advance();
                }
            }
            self.expect(TokenKind::RightBrace).ok();
            members
        } else {
            Vec::new()
        };

        ClassDecl {
            name,
            modifiers,
            type_params,
            extends,
            implements,
            members,
        }
    }

    fn parse_class_modifiers(&mut self) -> Vec<ClassModifier> {
        let mut modifiers = Vec::new();

        while let Some(kind) = self.peek_kind() {
            let modifier = match kind {
                TokenKind::Public => Some(ClassModifier::Public),
                TokenKind::Private => Some(ClassModifier::Private),
                TokenKind::Abstract => Some(ClassModifier::Abstract),
                TokenKind::Final => Some(ClassModifier::Final),
                TokenKind::Reflectable => Some(ClassModifier::Reflectable),
                TokenKind::At => {
                    self.parse_annotation();
                    continue;
                }
                _ => None,
            };
            if let Some(m) = modifier {
                modifiers.push(m);
                self.advance();
            } else {
                break;
            }
        }

        modifiers
    }

    fn parse_interface_decl(&mut self) -> InterfaceDecl {
        self.expect(TokenKind::Interface).unwrap();

        let name_token = self.expect_identifier().unwrap();
        let name = match name_token.kind {
            TokenKind::Identifier(s) => s,
            _ => String::new(),
        };

        let type_params = if self.peek_kind() == Some(TokenKind::Lt) {
            self.parse_type_params()
        } else {
            Vec::new()
        };

        let extends = if self.match_token(TokenKind::Extends) {
            let mut types = vec![self.parse_type_ref()];
            while self.match_token(TokenKind::Comma) {
                types.push(self.parse_type_ref());
            }
            types
        } else {
            Vec::new()
        };

        let members = if self.match_token(TokenKind::LeftBrace) {
            let mut members = Vec::new();
            while !self.check_keyword(TokenKind::RightBrace) && !self.is_at_end() {
                if let Some(member) = self.parse_interface_member() {
                    members.push(member);
                } else {
                    self.advance();
                }
            }
            self.expect(TokenKind::RightBrace).ok();
            members
        } else {
            Vec::new()
        };

        InterfaceDecl {
            name,
            type_params,
            extends,
            members,
        }
    }

    fn parse_interface_member(&mut self) -> Option<InterfaceMember> {
        let return_type = self.parse_type_ref();
        let name_token = self.expect_identifier().ok()?;
        let name = match name_token.kind {
            TokenKind::Identifier(s) => s,
            _ => String::new(),
        };

        self.expect(TokenKind::LeftParen).ok();
        let params = self.parse_params();
        self.expect(TokenKind::RightParen).ok();

        self.expect(TokenKind::Semicolon).ok();

        Some(InterfaceMember::MethodSignature(MethodSignature {
            return_type,
            name,
            params,
        }))
    }

    fn parse_function_decl(&mut self) -> Option<FunctionDecl> {
        let modifiers = self.parse_method_modifiers();

        let return_type = self.parse_type_ref();

        if !matches!(self.peek_kind(), Some(TokenKind::Identifier(_))) {
            return None;
        }

        let name_token = self.advance().unwrap();
        let name = match name_token.kind {
            TokenKind::Identifier(s) => s,
            _ => String::new(),
        };

        self.expect(TokenKind::LeftParen).ok();
        let params = self.parse_params();
        self.expect(TokenKind::RightParen).ok();

        let body = self.parse_block();

        Some(FunctionDecl {
            return_type,
            name,
            params,
            body,
            modifiers,
        })
    }

    fn parse_method_modifiers(&mut self) -> Vec<MethodModifier> {
        let mut modifiers = Vec::new();

        while let Some(kind) = self.peek_kind() {
            let modifier = match kind {
                TokenKind::Public => Some(MethodModifier::Public),
                TokenKind::Private => Some(MethodModifier::Private),
                TokenKind::Static => Some(MethodModifier::Static),
                TokenKind::Virtual => Some(MethodModifier::Virtual),
                TokenKind::Override => Some(MethodModifier::Override),
                TokenKind::Final => Some(MethodModifier::Final),
                TokenKind::Abstract => Some(MethodModifier::Abstract),
                _ => None,
            };
            if let Some(m) = modifier {
                modifiers.push(m);
                self.advance();
            } else {
                break;
            }
        }

        modifiers
    }

    fn is_type_token(&self) -> bool {
        if self.is_at_end() {
            return false;
        }
        matches!(
            self.peek_kind().unwrap(),
            TokenKind::Identifier(_)
                | TokenKind::Void
                | TokenKind::Bool
                | TokenKind::Byte
                | TokenKind::Char
                | TokenKind::Short
                | TokenKind::Int
                | TokenKind::Long
                | TokenKind::Float
                | TokenKind::Double
        )
    }

    fn parse_class_member(&mut self) -> Option<ClassMember> {
        while self.check_keyword(TokenKind::At) {
            self.parse_annotation();
        }

        let modifiers = self.parse_method_modifiers();

        while self.check_keyword(TokenKind::At) {
            self.parse_annotation();
        }

        if self.check_keyword(TokenKind::Tilde) {
            return Some(ClassMember::Destructor(self.parse_destructor_decl()));
        }

        if !self.is_type_token() {
            return None;
        }

        let saved_pos = self.pos;
        let return_type = self.parse_type_ref();

        if matches!(self.peek_kind(), Some(TokenKind::LeftParen)) {
            let name = match &return_type {
                TypeRef::Named(n, _) => n.clone(),
                TypeRef::Primitive(p) => format!("{:?}", p).to_lowercase(),
                _ => String::new(),
            };
            self.advance();
            let params = self.parse_params();
            self.expect(TokenKind::RightParen).ok();
            let body = if self.peek_kind() == Some(TokenKind::LeftBrace) {
                Some(self.parse_block())
            } else {
                self.expect(TokenKind::Semicolon).ok();
                None
            };
            return Some(ClassMember::Method(MethodDecl {
                return_type,
                name,
                params,
                body,
                modifiers,
            }));
        }

        if !matches!(self.peek_kind(), Some(TokenKind::Identifier(_))) {
            self.pos = saved_pos;
            return None;
        }

        let name_token = self.advance().unwrap();
        let name = match name_token.kind {
            TokenKind::Identifier(s) => s,
            _ => { self.pos = saved_pos; return None; }
        };

        if name == "this" {
            self.expect(TokenKind::LeftParen).ok();
            let params = self.parse_params();
            self.expect(TokenKind::RightParen).ok();
            let body = self.parse_block();
            return Some(ClassMember::Constructor(ConstructorDecl {
                name: "this".to_string(),
                params,
                body,
            }));
        }

        if self.peek_kind() == Some(TokenKind::LeftParen) {
            self.advance();
            let params = self.parse_params();
            self.expect(TokenKind::RightParen).ok();
            let body = if self.peek_kind() == Some(TokenKind::LeftBrace) {
                Some(self.parse_block())
            } else {
                self.expect(TokenKind::Semicolon).ok();
                None
            };
            Some(ClassMember::Method(MethodDecl {
                return_type,
                name,
                params,
                body,
                modifiers,
            }))
        } else {
            let initializer = if self.match_token(TokenKind::Assign) {
                Some(self.parse_expression())
            } else {
                None
            };
            self.expect(TokenKind::Semicolon).ok();
            Some(ClassMember::Field(VarDecl {
                var_type: return_type,
                name,
                initializer,
                modifiers: Vec::new(),
            }))
        }
    }

    fn parse_constructor_decl(&mut self) -> ConstructorDecl {
        self.expect(TokenKind::This).unwrap();
        self.expect(TokenKind::LeftParen).ok();
        let params = self.parse_params();
        self.expect(TokenKind::RightParen).ok();
        let body = self.parse_block();
        ConstructorDecl {
            name: "this".to_string(),
            params,
            body,
        }
    }

    fn parse_destructor_decl(&mut self) -> DestructorDecl {
        self.expect(TokenKind::Tilde).unwrap();
        let name_token = self.expect_identifier().unwrap();
        let name = match name_token.kind {
            TokenKind::Identifier(s) => s,
            _ => String::new(),
        };
        self.expect(TokenKind::LeftParen).ok();
        self.expect(TokenKind::RightParen).ok();
        let body = self.parse_block();
        DestructorDecl { name, body }
    }

    fn parse_params(&mut self) -> Vec<Param> {
        if self.peek_kind() == Some(TokenKind::RightParen) {
            return Vec::new();
        }

        let mut params = Vec::new();
        loop {
            let param_type = self.parse_type_ref();
            let name_token = self.expect_identifier().ok();
            if name_token.is_none() {
                break;
            }
            let name = match name_token.unwrap().kind {
                TokenKind::Identifier(s) => s,
                _ => String::new(),
            };

            let default_value = if self.match_token(TokenKind::Assign) {
                Some(self.parse_expression())
            } else {
                None
            };

            params.push(Param {
                param_type,
                name,
                default_value,
            });

            if !self.match_token(TokenKind::Comma) {
                break;
            }
        }

        params
    }

    fn parse_type_ref(&mut self) -> TypeRef {
        if let Some(primitive) = self.parse_primitive_type() {
            return primitive;
        }

        if matches!(self.peek_kind(), Some(TokenKind::Identifier(_))) {
            let name_token = self.advance().unwrap();
            let name = match name_token.kind {
                TokenKind::Identifier(s) => s,
                _ => String::new(),
            };

            let type_args = if self.peek_kind() == Some(TokenKind::Lt) {
                self.parse_type_args()
            } else {
                Vec::new()
            };

            let mut base = TypeRef::Named(name, type_args);

            while self.match_token(TokenKind::Star) {
                base = TypeRef::Array(Box::new(base));
            }

            while self.match_token(TokenKind::LeftBracket) {
                self.expect(TokenKind::RightBracket).ok();
                base = TypeRef::Array(Box::new(base));
            }

            if self.peek_kind() == Some(TokenKind::Arrow) {
                self.expect(TokenKind::Arrow).ok();
                self.expect(TokenKind::LeftParen).ok();
                let param_types = self.parse_type_list();
                self.expect(TokenKind::RightParen).ok();
                let return_type = self.parse_type_ref();
                return TypeRef::FunctionPtr(Box::new(return_type), param_types);
            }

            return base;
        }

        TypeRef::Primitive(PrimitiveType::Void)
    }

    fn parse_primitive_type(&mut self) -> Option<TypeRef> {
        let primitive = match self.peek_kind() {
            Some(TokenKind::Void) => PrimitiveType::Void,
            Some(TokenKind::Bool) => PrimitiveType::Bool,
            Some(TokenKind::Byte) => PrimitiveType::Byte,
            Some(TokenKind::Char) => PrimitiveType::Char,
            Some(TokenKind::Short) => PrimitiveType::Short,
            Some(TokenKind::Int) => PrimitiveType::Int,
            Some(TokenKind::Long) => PrimitiveType::Long,
            Some(TokenKind::Float) => PrimitiveType::Float,
            Some(TokenKind::Double) => PrimitiveType::Double,
            _ => return None,
        };
        self.advance();

        let mut base = TypeRef::Primitive(primitive);
        while self.match_token(TokenKind::LeftBracket) {
            self.expect(TokenKind::RightBracket).ok();
            base = TypeRef::Array(Box::new(base));
        }

        Some(base)
    }

    fn parse_type_params(&mut self) -> Vec<TypeParam> {
        self.expect(TokenKind::Lt).ok();
        let mut params = Vec::new();

        loop {
            if self.peek_kind() == Some(TokenKind::Gt) {
                break;
            }

            let name_token = self.expect_identifier().unwrap();
            let name = match name_token.kind {
                TokenKind::Identifier(s) => s,
                _ => String::new(),
            };

            let bound = if self.match_token(TokenKind::Extends) {
                Some(self.parse_type_ref())
            } else {
                None
            };

            params.push(TypeParam { name, bound });

            if !self.match_token(TokenKind::Comma) {
                break;
            }
        }

        self.expect(TokenKind::Gt).ok();
        params
    }

    fn parse_type_args(&mut self) -> Vec<TypeRef> {
        self.expect(TokenKind::Lt).ok();
        let mut args = Vec::new();

        loop {
            if self.peek_kind() == Some(TokenKind::Gt) {
                break;
            }

            args.push(self.parse_type_ref());

            if !self.match_token(TokenKind::Comma) {
                break;
            }
        }

        self.expect(TokenKind::Gt).ok();
        args
    }

    fn parse_type_list(&mut self) -> Vec<TypeRef> {
        if self.peek_kind() == Some(TokenKind::RightParen) {
            return Vec::new();
        }

        let mut types = Vec::new();
        loop {
            types.push(self.parse_type_ref());
            if !self.match_token(TokenKind::Comma) {
                break;
            }
        }
        types
    }

    fn parse_annotation(&mut self) {
        self.expect(TokenKind::At).ok();
        if self.peek_kind().is_some() && !self.check_keyword(TokenKind::LeftBrace)
            && !self.check_keyword(TokenKind::RightBrace) && !self.is_at_end() {
            self.advance();
        }
    }

    fn parse_block(&mut self) -> Block {
        self.expect(TokenKind::LeftBrace).ok();
        let mut statements = Vec::new();

        while !self.check_keyword(TokenKind::RightBrace) && !self.is_at_end() {
            if let Some(stmt) = self.parse_statement() {
                statements.push(stmt);
            } else {
                self.advance();
            }
        }

        self.expect(TokenKind::RightBrace).ok();
        Block { statements }
    }

    fn parse_statement(&mut self) -> Option<Stmt> {
        if self.peek_kind().is_none() {
            return None;
        }

        let stmt = match self.peek_kind().unwrap() {
            TokenKind::LeftBrace => Stmt::Block(self.parse_block()),
            TokenKind::If => self.parse_if_stmt(),
            TokenKind::For => self.parse_for_stmt(),
            TokenKind::While => self.parse_while_stmt(),
            TokenKind::Return => {
                self.advance();
                let expr = if self.peek_kind() != Some(TokenKind::Semicolon) {
                    Some(self.parse_expression())
                } else {
                    None
                };
                self.expect(TokenKind::Semicolon).ok();
                Stmt::Return(expr)
            }
            TokenKind::Break => {
                self.advance();
                self.expect(TokenKind::Semicolon).ok();
                Stmt::Break
            }
            TokenKind::Continue => {
                self.advance();
                self.expect(TokenKind::Semicolon).ok();
                Stmt::Continue
            }
            TokenKind::Try => self.parse_try_stmt(),
            TokenKind::Throw => {
                self.advance();
                let expr = self.parse_expression();
                self.expect(TokenKind::Semicolon).ok();
                Stmt::Expr(Expr::Throw(Box::new(expr)))
            }
            TokenKind::At
            | TokenKind::Public
            | TokenKind::Private
            | TokenKind::Static
            | TokenKind::Final
            | TokenKind::Virtual
            | TokenKind::Override
            | TokenKind::Abstract => self.parse_var_decl_stmt(),
            _ => {
                if let Some(kind) = self.peek_kind() {
                    if matches!(
                        kind,
                        TokenKind::Void
                            | TokenKind::Int
                            | TokenKind::Long
                            | TokenKind::Float
                            | TokenKind::Double
                            | TokenKind::Bool
                            | TokenKind::Byte
                            | TokenKind::Char
                            | TokenKind::Short
                    ) {
                        self.parse_var_decl_stmt()
                    } else if matches!(kind, TokenKind::Identifier(_)) {
                        if let Some(next_kind) = self.tokens.get(self.pos + 1).map(|t| t.kind.clone())
                        {
                            if matches!(
                                next_kind,
                                TokenKind::LeftBracket
                                    | TokenKind::Identifier(_)
                                    | TokenKind::Lt
                                    | TokenKind::Star
                            ) {
                                self.parse_var_decl_stmt()
                            } else {
                                self.parse_expr_stmt()
                            }
                        } else {
                            self.parse_expr_stmt()
                        }
                    } else {
                        self.parse_expr_stmt()
                    }
                } else {
                    self.parse_expr_stmt()
                }
            }
        };

        Some(stmt)
    }

    fn parse_if_stmt(&mut self) -> Stmt {
        self.advance();
        self.expect(TokenKind::LeftParen).ok();
        let condition = self.parse_expression();
        self.expect(TokenKind::RightParen).ok();

        let then_branch = Box::new(self.parse_statement().unwrap());
        let else_branch = if self.match_token(TokenKind::Else) {
            Some(Box::new(self.parse_statement().unwrap()))
        } else {
            None
        };

        Stmt::If(condition, then_branch, else_branch)
    }

    fn parse_for_stmt(&mut self) -> Stmt {
        self.advance();
        self.expect(TokenKind::LeftParen).ok();

        let init = if self.peek_kind() != Some(TokenKind::Semicolon) {
            Some(self.parse_expression())
        } else {
            None
        };
        self.expect(TokenKind::Semicolon).ok();

        let condition = if self.peek_kind() != Some(TokenKind::Semicolon) {
            Some(self.parse_expression())
        } else {
            None
        };
        self.expect(TokenKind::Semicolon).ok();

        let update = if self.peek_kind() != Some(TokenKind::RightParen) {
            Some(self.parse_expression())
        } else {
            None
        };
        self.expect(TokenKind::RightParen).ok();

        let body = Box::new(self.parse_statement().unwrap());

        Stmt::For(init, condition, update, body)
    }

    fn parse_while_stmt(&mut self) -> Stmt {
        self.advance();
        self.expect(TokenKind::LeftParen).ok();
        let condition = self.parse_expression();
        self.expect(TokenKind::RightParen).ok();
        let body = Box::new(self.parse_statement().unwrap());
        Stmt::While(condition, body)
    }

    fn parse_try_stmt(&mut self) -> Stmt {
        self.advance();
        let try_block = self.parse_block();

        let mut catch_clauses = Vec::new();
        while self.match_token(TokenKind::Catch) {
            self.expect(TokenKind::LeftParen).ok();
            let var_type = self.parse_type_ref();
            let name_token = match self.expect_identifier().ok() {
                Some(t) => t,
                None => break,
            };
            let name = match name_token.kind {
                TokenKind::Identifier(s) => s,
                _ => break,
            };
            self.expect(TokenKind::RightParen).ok();
            let body = self.parse_block();
            catch_clauses.push(CatchClause {
                var_type,
                name,
                body,
            });
        }

        let finally_block = if self.match_token(TokenKind::Finally) {
            Some(self.parse_block())
        } else {
            None
        };

        Stmt::Try(try_block, catch_clauses, finally_block)
    }

    fn parse_var_decl_stmt(&mut self) -> Stmt {
        let mut modifiers = Vec::new();

        while let Some(kind) = self.peek_kind() {
            let modifier = match kind {
                TokenKind::Public => Some(VarModifier::Public),
                TokenKind::Private => Some(VarModifier::Private),
                TokenKind::Static => Some(VarModifier::Static),
                TokenKind::Final => Some(VarModifier::Final),
                TokenKind::Virtual => Some(VarModifier::Virtual),
                TokenKind::Override => Some(VarModifier::Override),
                TokenKind::Abstract => Some(VarModifier::Abstract),
                TokenKind::At => {
                    self.parse_annotation();
                    continue;
                }
                _ => break,
            };
            if let Some(m) = modifier {
                modifiers.push(m);
                self.advance();
            } else {
                break;
            }
        }

        let var_type = self.parse_type_ref();
        let name_token = match self.expect_identifier().ok() {
            Some(t) => t,
            None => return Stmt::Expr(Expr::Variable(String::new())),
        };
        let name = match name_token.kind {
            TokenKind::Identifier(s) => s,
            _ => String::new(),
        };

        let initializer = if self.match_token(TokenKind::Assign) {
            Some(self.parse_expression())
        } else {
            None
        };

        self.expect(TokenKind::Semicolon).ok();

        Stmt::VarDecl(VarDecl {
            var_type,
            name,
            initializer,
            modifiers,
        })
    }

    fn parse_expr_stmt(&mut self) -> Stmt {
        let expr = self.parse_expression();
        self.expect(TokenKind::Semicolon).ok();
        Stmt::Expr(expr)
    }

    fn parse_expression(&mut self) -> Expr {
        self.parse_expression_with_min_precedence(0)
    }

    fn get_precedence(kind: &TokenKind) -> u8 {
        match kind {
            TokenKind::Question => 1,
            TokenKind::Or => 2,
            TokenKind::And => 3,
            TokenKind::Pipe => 4,
            TokenKind::Caret => 5,
            TokenKind::Amp => 6,
            TokenKind::Eq | TokenKind::Ne => 7,
            TokenKind::Lt | TokenKind::Gt | TokenKind::Le | TokenKind::Ge => 8,
            TokenKind::Shl | TokenKind::Shr => 9,
            TokenKind::Plus | TokenKind::Minus => 10,
            TokenKind::Star | TokenKind::Slash | TokenKind::Percent => 11,
            _ => 0,
        }
    }

    fn parse_expression_with_min_precedence(&mut self, min_prec: u8) -> Expr {
        let mut expr = self.parse_unary_expression();

        loop {
            if self.is_at_end() {
                break;
            }

            let precedence = match self.peek_kind() {
                Some(kind) => Self::get_precedence(&kind),
                None => break,
            };

            if precedence < min_prec {
                break;
            }

            if let Some(op_kind) = self.peek_kind() {
                if op_kind == TokenKind::Question {
                    self.advance();
                    let then_expr = self.parse_expression();
                    self.expect(TokenKind::Colon).ok();
                    let else_expr = self.parse_expression();
                    expr = Expr::Ternary(Box::new(expr), Box::new(then_expr), Box::new(else_expr));
                    continue;
                }

                if op_kind == TokenKind::Assign
                    || op_kind == TokenKind::PlusAssign
                    || op_kind == TokenKind::MinusAssign
                    || op_kind == TokenKind::StarAssign
                    || op_kind == TokenKind::SlashAssign
                    || op_kind == TokenKind::PercentAssign
                    || op_kind == TokenKind::AndAssign
                    || op_kind == TokenKind::OrAssign
                    || op_kind == TokenKind::XorAssign
                    || op_kind == TokenKind::ShlAssign
                    || op_kind == TokenKind::ShrAssign
                {
                    self.advance();
                    let right = self.parse_expression();
                    expr = Expr::Assignment(Box::new(expr), Box::new(right));
                    continue;
                }

                if Self::get_precedence(&op_kind) > 0 {
                    self.advance();
                    let right = self.parse_expression_with_min_precedence(precedence + 1);
                    if let Ok(op) = BinaryOp::try_from(op_kind) {
                        expr = Expr::BinaryOp(op, Box::new(expr), Box::new(right));
                    }
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        expr
    }

    fn parse_unary_expression(&mut self) -> Expr {
        if let Some(kind) = self.peek_kind() {
            let unary_op = match kind {
                TokenKind::Plus => Some(UnaryOp::Plus),
                TokenKind::Minus => Some(UnaryOp::Minus),
                TokenKind::Not => Some(UnaryOp::Not),
                TokenKind::Tilde => Some(UnaryOp::BitNot),
                TokenKind::Star => Some(UnaryOp::Deref),
                TokenKind::Amp => Some(UnaryOp::AddressOf),
                TokenKind::Inc => Some(UnaryOp::PreInc),
                TokenKind::Dec => Some(UnaryOp::PreDec),
                _ => None,
            };

            if let Some(op) = unary_op {
                self.advance();
                let operand = self.parse_unary_expression();
                return Expr::UnaryOp(op, Box::new(operand));
            }
        }

        self.parse_postfix_expression()
    }

    fn parse_postfix_expression(&mut self) -> Expr {
        let mut expr = self.parse_primary_expression();

        loop {
            if self.is_at_end() {
                break;
            }

            expr = match self.peek_kind() {
                Some(TokenKind::Inc) => {
                    self.advance();
                    Expr::UnaryOp(UnaryOp::PostInc, Box::new(expr))
                }
                Some(TokenKind::Dec) => {
                    self.advance();
                    Expr::UnaryOp(UnaryOp::PostDec, Box::new(expr))
                }
                Some(TokenKind::LeftParen) => {
                    self.advance();
                    let args = self.parse_arguments();
                    self.expect(TokenKind::RightParen).ok();
                    Expr::Call(Box::new(expr), args)
                }
                Some(TokenKind::Dot) => {
                    self.advance();
                    if let Some(name_token) = self.advance() {
                        let name = match name_token.kind {
                            TokenKind::Identifier(s) => s,
                            _ => String::new(),
                        };
                        Expr::FieldAccess(Box::new(expr), name)
                    } else {
                        break;
                    }
                }
                Some(TokenKind::Arrow) => {
                    self.advance();
                    if let Some(name_token) = self.advance() {
                        let name = match name_token.kind {
                            TokenKind::Identifier(s) => s,
                            _ => String::new(),
                        };
                        if self.peek_kind() == Some(TokenKind::LeftParen) {
                            self.advance();
                            let args = self.parse_arguments();
                            self.expect(TokenKind::RightParen).ok();
                            Expr::MethodCall(Box::new(expr), name, args)
                        } else {
                            Expr::FieldAccess(Box::new(expr), name)
                        }
                    } else {
                        break;
                    }
                }
                Some(TokenKind::LeftBracket) => {
                    self.advance();
                    let index = self.parse_expression();
                    self.expect(TokenKind::RightBracket).ok();
                    Expr::ArrayAccess(Box::new(expr), Box::new(index))
                }
                Some(TokenKind::InstanceOf) => {
                    self.advance();
                    let type_ref = self.parse_type_ref();
                    Expr::InstanceOf(Box::new(expr), type_ref)
                }
                _ => break,
            };
        }

        expr
    }

    fn parse_primary_expression(&mut self) -> Expr {
        if self.is_at_end() {
            return Expr::Variable(String::new());
        }

        let expr = match self.peek_kind().unwrap() {
            TokenKind::IntegerLiteral(v) => {
                self.advance();
                Expr::IntegerLiteral(v)
            }
            TokenKind::FloatLiteral(v) => {
                self.advance();
                Expr::FloatLiteral(v)
            }
            TokenKind::StringLiteral(ref s) => {
                let s = s.clone();
                self.advance();
                Expr::StringLiteral(s)
            }
            TokenKind::CharLiteral(c) => {
                let c = c;
                self.advance();
                Expr::CharLiteral(c)
            }
            TokenKind::True => {
                self.advance();
                Expr::BoolLiteral(true)
            }
            TokenKind::False => {
                self.advance();
                Expr::BoolLiteral(false)
            }
            TokenKind::Null => {
                self.advance();
                Expr::Null
            }
            TokenKind::This => {
                self.advance();
                Expr::This
            }
            TokenKind::Super => {
                self.advance();
                Expr::Super
            }
            TokenKind::Identifier(_) => {
                let name_token = self.advance().unwrap();
                let name = match name_token.kind {
                    TokenKind::Identifier(s) => s,
                    _ => String::new(),
                };
                Expr::Variable(name)
            }
            TokenKind::New => self.parse_new_expression(),
            TokenKind::Delete => self.parse_delete_expression(),
            TokenKind::Sizeof => self.parse_sizeof_expression(),
            TokenKind::TypeId => self.parse_typeid_expression(),
            TokenKind::LeftParen => {
                self.advance();
                if self.is_type_start() {
                    let type_ref = self.parse_type_ref();
                    self.expect(TokenKind::RightParen).ok();
                    let expr = self.parse_unary_expression();
                    Expr::Cast(type_ref, Box::new(expr))
                } else {
                    let expr = self.parse_expression();
                    self.expect(TokenKind::RightParen).ok();
                    expr
                }
            }
            _ => {
                if let Some(token) = self.advance() {
                    diagnostics::report_error(
                        token.span.line,
                        token.span.column,
                        &format!("Unexpected token: {:?}", token.kind),
                    );
                }
                Expr::Variable(String::new())
            }
        };

        expr
    }

    fn is_type_start(&self) -> bool {
        if self.is_at_end() {
            return false;
        }
        matches!(
            self.peek_kind().unwrap(),
            TokenKind::Void
                | TokenKind::Bool
                | TokenKind::Byte
                | TokenKind::Char
                | TokenKind::Short
                | TokenKind::Int
                | TokenKind::Long
                | TokenKind::Float
                | TokenKind::Double
                | TokenKind::Identifier(_)
        )
    }

    fn parse_new_expression(&mut self) -> Expr {
        self.advance();
        let name_token = self.expect_identifier().unwrap();
        let name = match name_token.kind {
            TokenKind::Identifier(s) => s,
            _ => String::new(),
        };

        let type_args = if self.peek_kind() == Some(TokenKind::Lt) {
            self.parse_type_args()
        } else {
            Vec::new()
        };

        let args = if self.peek_kind() == Some(TokenKind::LeftParen) {
            self.advance();
            self.parse_arguments()
        } else {
            Vec::new()
        };

        if self.peek_kind() == Some(TokenKind::RightParen) {
            self.expect(TokenKind::RightParen).ok();
        }

        Expr::New(name, type_args, args)
    }

    fn parse_delete_expression(&mut self) -> Expr {
        self.advance();
        let expr = self.parse_unary_expression();
        Expr::Delete(Box::new(expr))
    }

    fn parse_sizeof_expression(&mut self) -> Expr {
        self.advance();
        self.expect(TokenKind::LeftParen).ok();
        let type_ref = self.parse_type_ref();
        self.expect(TokenKind::RightParen).ok();
        Expr::Sizeof(type_ref)
    }

    fn parse_typeid_expression(&mut self) -> Expr {
        self.advance();
        self.expect(TokenKind::LeftParen).ok();
        let expr = self.parse_expression();
        self.expect(TokenKind::RightParen).ok();
        Expr::TypeId(Box::new(expr))
    }

    fn parse_arguments(&mut self) -> Vec<Expr> {
        if self.peek_kind() == Some(TokenKind::RightParen) {
            return Vec::new();
        }

        let mut args = Vec::new();
        loop {
            args.push(self.parse_expression());
            if !self.match_token(TokenKind::Comma) {
                break;
            }
        }
        args
    }
}
