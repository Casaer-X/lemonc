use crate::ast::node::*;
use std::collections::HashMap;
use std::io::Write;

pub struct NasmCodeGen {
    output: Vec<u8>,
    data_section: Vec<u8>,
    indent: u32,
    string_literals: Vec<String>,
    string_counter: u32,
    current_class: Option<String>,
    parent_class: Option<String>,
    var_stack: HashMap<String, i32>,
    stack_offset: i32,
    label_counter: u32,
    class_fields: HashMap<String, Vec<(String, String, i32)>>,
    class_has_vtable: HashMap<String, bool>,
    virtual_methods: HashMap<String, Vec<NasmVirtualMethodEntry>>,
    method_signatures: HashMap<String, Vec<(String, Vec<TypeRef>)>>,
}

#[derive(Clone)]
struct NasmVirtualMethodEntry {
    name: String,
    params: Vec<Param>,
}

impl NasmCodeGen {
    pub fn new() -> Self {
        Self {
            output: Vec::new(),
            data_section: Vec::new(),
            indent: 0,
            string_literals: Vec::new(),
            string_counter: 0,
            current_class: None,
            parent_class: None,
            var_stack: HashMap::new(),
            stack_offset: 0,
            label_counter: 0,
            class_fields: HashMap::new(),
            class_has_vtable: HashMap::new(),
            virtual_methods: HashMap::new(),
            method_signatures: HashMap::new(),
        }
    }

    pub fn generate(&mut self, ast: &Program) -> String {
        self.collect_class_info(ast);
        self.collect_virtual_methods(ast);
        self.collect_method_signatures(ast);

        self.emit_data(&format!("extern printf"));
        self.emit_data(&format!("extern malloc"));
        self.emit_data(&format!("extern free"));
        self.emit_data(&format!("extern exit"));
        self.emit_data(&format!("extern strcmp"));
        self.emit_data(&format!("extern strlen"));
        self.emit_data(&format!("extern strdup"));
        self.emit_data(&format!("extern memcpy"));
        self.emit_data(&format!("extern snprintf"));
        self.emit_data(&format!("extern atoi"));
        self.emit_data("");

        self.emit_line("default rel");
        self.emit_line("");

        self.emit_line("section .text");
        self.emit_line("");

        for decl in &ast.declarations {
            match decl {
                Declaration::Class(class) => self.generate_class(class),
                Declaration::Function(func) => self.generate_function(func),
                _ => {}
            }
        }

        for decl in &ast.declarations {
            if let Declaration::Class(class) = decl {
                if class.name == "App" {
                    for member in &class.members {
                        if let ClassMember::Method(method) = member {
                            if method.name == "main" {
                                let mangled = mangle_method_name(&class.name, &method.name, &method.params);
                                self.emit_line("global main");
                                self.emit_line("");
                                self.emit_line("main:");
                                self.emit_prologue(32);
                                self.emit_line("    mov rcx, 0");
                                self.emit_line(&format!("    call {}", mangled));
                                self.emit_line("    xor rax, rax");
                                self.emit_epilogue();
                            }
                        }
                    }
                }
            }
        }

        self.emit_line("");
        self.emit_line("section .data");

        let string_data: Vec<(usize, String)> = self.string_literals.iter().enumerate()
            .map(|(i, s)| (i, self.escape_string_for_nasm(s)))
            .collect();
        for (i, escaped) in &string_data {
            self.emit_line(&format!("_str_{} db {}, 0", i, escaped));
        }

        let vtable_data: Vec<(String, Vec<NasmVirtualMethodEntry>)> = self.virtual_methods.iter()
            .filter(|(_, methods)| !methods.is_empty())
            .map(|(cn, methods)| (cn.clone(), (*methods).clone()))
            .collect();
        for (class_name, methods) in &vtable_data {
            self.emit_line(&format!("_{}_vtable:", class_name));
            for vm in methods {
                let mangled = mangle_method_name(class_name, &vm.name, &vm.params);
                self.emit_line(&format!("    dq {}", mangled));
            }
        }

        let mut result = String::from_utf8_lossy(&self.data_section).to_string();
        result.push_str(&String::from_utf8_lossy(&self.output));
        result
    }

    fn collect_class_info(&mut self, ast: &Program) {
        for decl in &ast.declarations {
            if let Declaration::Class(class) = decl {
                let mut fields = Vec::new();
                let mut offset: i32;
                fields.push(("void**".to_string(), "vtable".to_string(), 0));
                offset = 8;

                for member in &class.members {
                    if let ClassMember::Field(field) = member {
                        let size = self.type_size(&field.var_type);
                        fields.push((self.asm_type(&field.var_type), field.name.clone(), offset));
                        offset += size;
                    }
                }

                self.class_fields.insert(class.name.clone(), fields);
            }
        }
    }

    fn collect_virtual_methods(&mut self, ast: &Program) {
        for decl in &ast.declarations {
            if let Declaration::Class(class) = decl {
                let mut vmethods = Vec::new();
                for member in &class.members {
                    if let ClassMember::Method(m) = member {
                        let is_virtual = m.modifiers.iter().any(|m2| matches!(m2, MethodModifier::Virtual));
                        if is_virtual {
                            vmethods.push(NasmVirtualMethodEntry {
                                name: m.name.clone(),
                                params: m.params.clone(),
                            });
                        }
                    }
                }
                if !vmethods.is_empty() {
                    self.class_has_vtable.insert(class.name.clone(), true);
                    self.virtual_methods.insert(class.name.clone(), vmethods);
                } else {
                    self.class_has_vtable.insert(class.name.clone(), false);
                }
            }
        }
    }

    fn class_has_explicit_ctor(&self, class: &ClassDecl) -> bool {
        for member in &class.members {
            match member {
                ClassMember::Constructor(_) => return true,
                ClassMember::Method(method) if method.name == class.name => return true,
                _ => {}
            }
        }
        false
    }

    fn class_size(&self, class_name: &str) -> i32 {
        self.class_fields.get(class_name).map_or(16, |fields| {
            if fields.is_empty() {
                16
            } else {
                let last = fields.last().unwrap();
                let last_size = 8;
                last.2 + last_size
            }
        })
    }

    fn field_offset(&self, class_name: &str, field_name: &str) -> Option<i32> {
        self.class_fields.get(class_name).and_then(|fields| {
            fields.iter().find(|(_, name, _)| name == field_name).map(|(_, _, offset)| *offset)
        })
    }

    fn resolve_class_for_var(&self, var_name: &str) -> Option<String> {
        if var_name == "self" {
            return self.current_class.clone();
        }
        None
    }

    fn infer_class_from_expr(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::This => self.current_class.clone(),
            Expr::Super => self.parent_class.clone().or(self.current_class.clone()),
            Expr::Variable(name) => self.resolve_class_for_var(name),
            Expr::New(class_name, _, _) => Some(class_name.clone()),
            Expr::FieldAccess(obj, _) => self.infer_class_from_expr(obj),
            _ => None,
        }
    }

    fn new_label(&mut self) -> String {
        let id = self.label_counter;
        self.label_counter += 1;
        format!("_L{}", id)
    }

    fn emit_data(&mut self, line: &str) {
        self.data_section.write_all(line.as_bytes()).unwrap();
        self.data_section.write_all(b"\n").unwrap();
    }

    fn emit_line(&mut self, line: &str) {
        for _ in 0..self.indent {
            self.output.write_all(b"    ").unwrap();
        }
        self.output.write_all(line.as_bytes()).unwrap();
        self.output.write_all(b"\n").unwrap();
    }

    fn emit_prologue(&mut self, stack_space: i32) {
        self.emit_line("    push rbp");
        self.emit_line("    mov rbp, rsp");
        let aligned = ((stack_space + 31) / 32) * 32;
        if aligned > 0 {
            self.emit_line(&format!("    sub rsp, {}", aligned));
        }
    }

    fn emit_epilogue(&mut self) {
        self.emit_line("    add rsp, 0");
        self.emit_line("    mov rsp, rbp");
        self.emit_line("    pop rbp");
        self.emit_line("    ret");
    }

    fn generate_class(&mut self, class: &ClassDecl) {
        self.current_class = Some(class.name.clone());
        self.parent_class = class.extends.as_ref().and_then(|t| {
            if let TypeRef::Named(name, _) = t {
                Some(name.clone())
            } else {
                None
            }
        });

        let has_ctor = self.class_has_explicit_ctor(class);
        if !has_ctor {
            self.generate_default_ctor(class);
        }

        for member in &class.members {
            match member {
                ClassMember::Constructor(ctor) => {
                    self.generate_constructor(ctor, class);
                }
                ClassMember::Method(method) => {
                    if method.name == class.name {
                        self.generate_ctor_method(method, class);
                    } else {
                        self.generate_method(method, class);
                    }
                }
                ClassMember::Destructor(dtor) => {
                    self.generate_destructor(dtor, class);
                }
                _ => {}
            }
        }

        self.current_class = None;
        self.parent_class = None;
    }

    fn generate_default_ctor(&mut self, class: &ClassDecl) {
        self.emit_line(&format!("{}_ctor:", class.name));
        self.emit_prologue(32);
        self.emit_line("    mov [rbp-8], rcx");

        self.emit_line("    mov rax, [rbp-8]");
        if self.class_has_vtable.get(&class.name).copied().unwrap_or(false) {
            self.emit_line(&format!("    lea rcx, [_{}_vtable]", class.name));
            self.emit_line("    mov [rax], rcx");
        } else {
            self.emit_line("    mov qword [rax], 0");
        }

        self.emit_line("    mov rax, [rbp-8]");
        self.emit_epilogue();
    }

    fn generate_constructor(&mut self, ctor: &ConstructorDecl, class: &ClassDecl) {
        self.var_stack.clear();
        self.stack_offset = 8;

        self.emit_line(&format!("{}_ctor:", class.name));
        let param_count = ctor.params.len() + 1;
        let stack_space = (param_count as i32 + 4) * 8;
        self.emit_prologue(stack_space);

        self.emit_line("    mov [rbp-8], rcx");
        self.var_stack.insert("self".to_string(), -8);

        let arg_regs = ["rcx", "rdx", "r8", "r9"];
        for (i, p) in ctor.params.iter().enumerate() {
            let offset = -((i + 2) as i32) * 8;
            if i + 1 < arg_regs.len() {
                self.emit_line(&format!("    mov [rbp{}], {}", offset, arg_regs[i + 1]));
            } else {
                let stack_pos = ((i + 1 - (arg_regs.len() - 1)) as i32) * 8 + 32;
                self.emit_line(&format!("    mov rax, [rbp+{}]", stack_pos));
                self.emit_line(&format!("    mov [rbp{}], rax", offset));
            }
            self.var_stack.insert(p.name.clone(), offset);
        }

        for stmt in &ctor.body.statements {
            self.generate_stmt(stmt);
        }

        self.emit_line("    mov rax, [rbp-8]");
        if self.class_has_vtable.get(&class.name).copied().unwrap_or(false) {
            self.emit_line(&format!("    lea rcx, [_{}_vtable]", class.name));
            self.emit_line("    mov [rax], rcx");
        } else {
            self.emit_line("    mov qword [rax], 0");
        }

        self.emit_line("    mov rax, [rbp-8]");
        self.emit_epilogue();
    }

    fn generate_ctor_method(&mut self, method: &MethodDecl, class: &ClassDecl) {
        self.var_stack.clear();
        self.stack_offset = 8;

        self.emit_line(&format!("{}_ctor:", class.name));
        let param_count = method.params.len() + 1;
        let stack_space = (param_count as i32 + 4) * 8;
        self.emit_prologue(stack_space);

        self.emit_line("    mov [rbp-8], rcx");
        self.var_stack.insert("self".to_string(), -8);

        let arg_regs = ["rcx", "rdx", "r8", "r9"];
        for (i, p) in method.params.iter().enumerate() {
            let offset = -((i + 2) as i32) * 8;
            if i + 1 < arg_regs.len() {
                self.emit_line(&format!("    mov [rbp{}], {}", offset, arg_regs[i + 1]));
            } else {
                let stack_pos = ((i + 1 - (arg_regs.len() - 1)) as i32) * 8 + 32;
                self.emit_line(&format!("    mov rax, [rbp+{}]", stack_pos));
                self.emit_line(&format!("    mov [rbp{}], rax", offset));
            }
            self.var_stack.insert(p.name.clone(), offset);
        }

        if let Some(body) = &method.body {
            for stmt in &body.statements {
                self.generate_stmt(stmt);
            }
        }

        if self.class_has_vtable.get(&class.name).copied().unwrap_or(false) {
            self.emit_line(&format!("    lea rcx, [_{}_vtable]", class.name));
            self.emit_line("    mov rax, [rbp-8]");
            self.emit_line("    mov [rax], rcx");
        } else {
            self.emit_line("    mov rax, [rbp-8]");
            self.emit_line("    mov qword [rax], 0");
        }

        self.emit_line("    mov rax, [rbp-8]");
        self.emit_epilogue();
    }

    fn generate_method(&mut self, method: &MethodDecl, class: &ClassDecl) {
        self.var_stack.clear();
        self.stack_offset = 8;

        let mangled = mangle_method_name(&class.name, &method.name, &method.params);
        self.emit_line(&format!("{}:", mangled));
        let param_count = method.params.len() + 1;
        let stack_space = (param_count as i32 + 4) * 8;
        self.emit_prologue(stack_space);

        self.emit_line("    mov [rbp-8], rcx");
        self.var_stack.insert("self".to_string(), -8);

        let arg_regs = ["rcx", "rdx", "r8", "r9"];
        for (i, p) in method.params.iter().enumerate() {
            let offset = -((i + 2) as i32) * 8;
            if i + 1 < arg_regs.len() {
                self.emit_line(&format!("    mov [rbp{}], {}", offset, arg_regs[i + 1]));
            } else {
                let stack_pos = ((i + 1 - (arg_regs.len() - 1)) as i32) * 8 + 32;
                self.emit_line(&format!("    mov rax, [rbp+{}]", stack_pos));
                self.emit_line(&format!("    mov [rbp{}], rax", offset));
            }
            self.var_stack.insert(p.name.clone(), offset);
        }

        if let Some(body) = &method.body {
            for stmt in &body.statements {
                self.generate_stmt(stmt);
            }
        }

        self.emit_epilogue();
    }

    fn generate_destructor(&mut self, dtor: &DestructorDecl, class: &ClassDecl) {
        self.var_stack.clear();
        self.stack_offset = 8;

        self.emit_line(&format!("{}_dtor:", class.name));
        self.emit_prologue(32);

        self.emit_line("    mov [rbp-8], rcx");
        self.var_stack.insert("self".to_string(), -8);

        for stmt in &dtor.body.statements {
            self.generate_stmt(stmt);
        }

        self.emit_epilogue();
    }

    fn generate_function(&mut self, func: &FunctionDecl) {
        self.var_stack.clear();
        self.stack_offset = 8;
        self.current_class = None;

        self.emit_line(&format!("{}:", func.name));
        let param_count = func.params.len();
        let stack_space = (param_count as i32 + 4) * 8;
        self.emit_prologue(stack_space);

        let arg_regs = ["rcx", "rdx", "r8", "r9"];
        for (i, p) in func.params.iter().enumerate() {
            let offset = -((i + 1) as i32) * 8;
            if i < arg_regs.len() {
                self.emit_line(&format!("    mov [rbp{}], {}", offset, arg_regs[i]));
            } else {
                let stack_pos = ((i - (arg_regs.len() - 1)) as i32) * 8 + 32;
                self.emit_line(&format!("    mov rax, [rbp+{}]", stack_pos));
                self.emit_line(&format!("    mov [rbp{}], rax", offset));
            }
            self.var_stack.insert(p.name.clone(), offset);
        }

        for stmt in &func.body.statements {
            self.generate_stmt(stmt);
        }

        self.emit_epilogue();
    }

    fn generate_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::VarDecl(var) => {
                self.stack_offset += 8;
                let offset = -self.stack_offset;
                self.var_stack.insert(var.name.clone(), offset);
                self.emit_line(&format!("    ; var {} at [rbp{}]", var.name, offset));

                if let Some(init) = &var.initializer {
                    self.gen_expr_to_rax(init);
                    self.emit_line(&format!("    mov [rbp{}], rax", offset));
                } else {
                    self.emit_line(&format!("    mov qword [rbp{}], 0", offset));
                }
            }
            Stmt::Return(expr) => {
                match expr {
                    Some(e) => {
                        self.gen_expr_to_rax(e);
                    }
                    None => {
                        self.emit_line("    xor rax, rax");
                    }
                }
                self.emit_line("    mov rsp, rbp");
                self.emit_line("    pop rbp");
                self.emit_line("    ret");
            }
            Stmt::Expr(expr) => {
                self.gen_expr_to_rax(expr);
            }
            Stmt::If(cond, then, else_) => {
                let else_label = self.new_label();
                let end_label = self.new_label();

                self.gen_expr_to_rax(cond);
                self.emit_line("    test rax, rax");
                self.emit_line(&format!("    jz {}", else_label));

                self.generate_stmt(then);
                self.emit_line(&format!("    jmp {}", end_label));

                self.emit_line(&format!("{}:", else_label));
                if let Some(else_stmt) = else_ {
                    self.generate_stmt(else_stmt);
                }

                self.emit_line(&format!("{}:", end_label));
            }
            Stmt::While(cond, body) => {
                let start_label = self.new_label();
                let end_label = self.new_label();

                self.emit_line(&format!("{}:", start_label));
                self.gen_expr_to_rax(cond);
                self.emit_line("    test rax, rax");
                self.emit_line(&format!("    jz {}", end_label));

                self.generate_stmt(body);
                self.emit_line(&format!("    jmp {}", start_label));

                self.emit_line(&format!("{}:", end_label));
            }
            Stmt::For(init, cond, update, body) => {
                let start_label = self.new_label();
                let end_label = self.new_label();

                if let Some(init_expr) = init {
                    self.gen_expr_to_rax(init_expr);
                }

                self.emit_line(&format!("{}:", start_label));
                if let Some(cond_expr) = cond {
                    self.gen_expr_to_rax(cond_expr);
                    self.emit_line("    test rax, rax");
                    self.emit_line(&format!("    jz {}", end_label));
                }

                self.generate_stmt(body);

                if let Some(update_expr) = update {
                    self.gen_expr_to_rax(update_expr);
                }
                self.emit_line(&format!("    jmp {}", start_label));

                self.emit_line(&format!("{}:", end_label));
            }
            Stmt::Block(block) => {
                for s in &block.statements {
                    self.generate_stmt(s);
                }
            }
            Stmt::Break => {
                self.emit_line("    ; break - not fully supported in NASM gen");
            }
            Stmt::Continue => {
                self.emit_line("    ; continue - not fully supported in NASM gen");
            }
            Stmt::Try(_try_block, _catches, _finally) => {
                self.emit_line("    ; try/catch - not fully supported in NASM gen");
            }
        }
    }

    fn gen_expr_to_rax(&mut self, expr: &Expr) {
        match expr {
            Expr::IntegerLiteral(v) => {
                self.emit_line(&format!("    mov rax, {}", v));
            }
            Expr::FloatLiteral(_v) => {
                self.emit_line("    ; float literal - not fully supported");
                self.emit_line("    xor rax, rax");
            }
            Expr::StringLiteral(s) => {
                let idx = self.string_counter;
                self.string_counter += 1;
                self.string_literals.push(s.clone());
                self.emit_line(&format!("    lea rax, [_str_{}]", idx));
            }
            Expr::CharLiteral(c) => {
                self.emit_line(&format!("    mov rax, {}", *c as i64));
            }
            Expr::BoolLiteral(b) => {
                if *b {
                    self.emit_line("    mov rax, 1");
                } else {
                    self.emit_line("    xor rax, rax");
                }
            }
            Expr::Null => {
                self.emit_line("    xor rax, rax");
            }
            Expr::This => {
                if let Some(offset) = self.var_stack.get("self") {
                    self.emit_line(&format!("    mov rax, [rbp{}]", offset));
                }
            }
            Expr::Super => {
                if let Some(offset) = self.var_stack.get("self") {
                    self.emit_line(&format!("    mov rax, [rbp{}]", offset));
                }
            }
            Expr::Variable(name) => {
                if let Some(offset) = self.var_stack.get(name) {
                    self.emit_line(&format!("    mov rax, [rbp{}]", offset));
                } else {
                    self.emit_line(&format!("    ; unknown var: {}", name));
                    self.emit_line("    xor rax, rax");
                }
            }
            Expr::BinaryOp(op, left, right) => {
                self.gen_expr_to_rax(left);
                self.emit_line("    push rax");
                self.gen_expr_to_rax(right);
                self.emit_line("    mov rcx, rax");
                self.emit_line("    pop rax");

                match op {
                    BinaryOp::Add => self.emit_line("    add rax, rcx"),
                    BinaryOp::Sub => self.emit_line("    sub rax, rcx"),
                    BinaryOp::Mul => self.emit_line("    imul rax, rcx"),
                    BinaryOp::Div => {
                        self.emit_line("    mov r8, rax");
                        self.emit_line("    mov rax, rcx");
                        self.emit_line("    cqo");
                        self.emit_line("    idiv r8");
                    }
                    BinaryOp::Mod => {
                        self.emit_line("    mov r8, rax");
                        self.emit_line("    mov rax, rcx");
                        self.emit_line("    cqo");
                        self.emit_line("    idiv r8");
                        self.emit_line("    mov rax, rdx");
                    }
                    BinaryOp::Lt => {
                        self.emit_line("    cmp rax, rcx");
                        self.emit_line("    setl al");
                        self.emit_line("    movzx rax, al");
                    }
                    BinaryOp::Gt => {
                        self.emit_line("    cmp rax, rcx");
                        self.emit_line("    setg al");
                        self.emit_line("    movzx rax, al");
                    }
                    BinaryOp::Le => {
                        self.emit_line("    cmp rax, rcx");
                        self.emit_line("    setle al");
                        self.emit_line("    movzx rax, al");
                    }
                    BinaryOp::Ge => {
                        self.emit_line("    cmp rax, rcx");
                        self.emit_line("    setge al");
                        self.emit_line("    movzx rax, al");
                    }
                    BinaryOp::Eq => {
                        self.emit_line("    cmp rax, rcx");
                        self.emit_line("    sete al");
                        self.emit_line("    movzx rax, al");
                    }
                    BinaryOp::Ne => {
                        self.emit_line("    cmp rax, rcx");
                        self.emit_line("    setne al");
                        self.emit_line("    movzx rax, al");
                    }
                    BinaryOp::And => {
                        self.emit_line("    test rax, rax");
                        self.emit_line("    setne al");
                        self.emit_line("    test rcx, rcx");
                        self.emit_line("    setne cl");
                        self.emit_line("    and al, cl");
                        self.emit_line("    movzx rax, al");
                    }
                    BinaryOp::Or => {
                        self.emit_line("    or rax, rcx");
                        self.emit_line("    setne al");
                        self.emit_line("    movzx rax, al");
                    }
                    BinaryOp::BitAnd => self.emit_line("    and rax, rcx"),
                    BinaryOp::BitOr => self.emit_line("    or rax, rcx"),
                    BinaryOp::BitXor => self.emit_line("    xor rax, rcx"),
                    BinaryOp::Shl => self.emit_line("    shl rax, cl"),
                    BinaryOp::Shr => self.emit_line("    sar rax, cl"),
                }
            }
            Expr::UnaryOp(op, operand) => {
                self.gen_expr_to_rax(operand);
                match op {
                    UnaryOp::Minus => {
                        self.emit_line("    neg rax");
                    }
                    UnaryOp::Not => {
                        self.emit_line("    test rax, rax");
                        self.emit_line("    sete al");
                        self.emit_line("    movzx rax, al");
                    }
                    UnaryOp::BitNot => {
                        self.emit_line("    not rax");
                    }
                    UnaryOp::Plus => {}
                    UnaryOp::Deref => {
                        self.emit_line("    mov rax, [rax]");
                    }
                    UnaryOp::AddressOf => {
                        self.emit_line("    ; addressof - limited support");
                    }
                    UnaryOp::PreInc => {
                        self.emit_line("    add rax, 1");
                    }
                    UnaryOp::PreDec => {
                        self.emit_line("    sub rax, 1");
                    }
                    UnaryOp::PostInc | UnaryOp::PostDec => {
                        self.emit_line("    ; post inc/dec - limited support");
                    }
                }
            }
            Expr::Ternary(cond, then, else_) => {
                let else_label = self.new_label();
                let end_label = self.new_label();

                self.gen_expr_to_rax(cond);
                self.emit_line("    test rax, rax");
                self.emit_line(&format!("    jz {}", else_label));

                self.gen_expr_to_rax(then);
                self.emit_line(&format!("    jmp {}", end_label));

                self.emit_line(&format!("{}:", else_label));
                self.gen_expr_to_rax(else_);

                self.emit_line(&format!("{}:", end_label));
            }
            Expr::Assignment(target, value) => {
                self.gen_expr_to_rax(value);

                match target.as_ref() {
                    Expr::Variable(name) => {
                        if let Some(offset) = self.var_stack.get(name) {
                            self.emit_line(&format!("    mov [rbp{}], rax", offset));
                        }
                    }
                    Expr::FieldAccess(obj, field) => {
                        self.emit_line("    push rax");
                        self.gen_expr_to_rax(obj);
                        let class_name = self.infer_class_from_expr(obj);
                        if let Some(cn) = &class_name {
                            if let Some(foff) = self.field_offset(cn, field) {
                                self.emit_line("    pop rcx");
                                self.emit_line(&format!("    mov [rax+{}], rcx", foff));
                            }
                        } else {
                            self.emit_line("    pop rcx");
                            self.emit_line(&format!("    ; field assign: {} - offset unknown", field));
                        }
                    }
                    _ => {
                        self.emit_line("    ; assignment to complex target - not fully supported");
                    }
                }
            }
            Expr::Call(callee, args) => {
                match callee.as_ref() {
                    Expr::Variable(name) => {
                        let arg_regs = ["rcx", "rdx", "r8", "r9"];
                        let _saved: Vec<String> = args.iter().enumerate().map(|(i, _)| {
                            if i < arg_regs.len() {
                                arg_regs[i].to_string()
                            } else {
                                format!("stack_{}", i)
                            }
                        }).collect();

                        for (i, arg) in args.iter().enumerate() {
                            self.gen_expr_to_rax(arg);
                            if i < arg_regs.len() {
                                self.emit_line(&format!("    push rax"));
                            }
                        }

                        for i in (0..args.len().min(arg_regs.len())).rev() {
                            self.emit_line(&format!("    pop {}", arg_regs[i]));
                        }

                        for i in arg_regs.len()..args.len() {
                            self.emit_line(&format!("    ; stack arg {} - not fully supported", i));
                        }

                        self.emit_line(&format!("    call {}", name));
                    }
                    Expr::FieldAccess(obj, method) => {
                        let class_name = self.infer_class_from_expr(obj);
                        let arg_regs = ["rcx", "rdx", "r8", "r9"];

                        self.gen_expr_to_rax(obj);
                        self.emit_line("    push rax");

                        for (i, arg) in args.iter().enumerate() {
                            self.gen_expr_to_rax(arg);
                            if i + 1 < arg_regs.len() {
                                self.emit_line("    push rax");
                            }
                        }

                        self.emit_line("    pop rax");
                        if args.len() > 0 && args.len() <= arg_regs.len() - 1 {
                            for i in (0..args.len()).rev() {
                                self.emit_line(&format!("    pop {}", arg_regs[i + 1]));
                            }
                        }

                        self.emit_line("    pop rcx");

                        if let Some(cn) = &class_name {
                            if self.virtual_methods.get(cn).map_or(false, |v| v.iter().any(|vm| vm.name == *method && vm.params.len() == args.len())) {
                                let vtable_idx = self.vtable_offset(cn, method, args.len());
                                self.emit_line("    mov rax, [rcx]");
                                self.emit_line(&format!("    mov rax, [rax+{}]", vtable_idx * 8));
                                self.emit_line("    call rax");
                            } else {
                                let mangled = self.resolve_method_overload(cn, method, args.len());
                                self.emit_line(&format!("    call {}", mangled));
                            }
                        } else {
                            self.emit_line(&format!("    ; unknown class for method call: {}", method));
                            self.emit_line(&format!("    call {}_unknown", method));
                        }
                    }
                    Expr::Super => {
                        let arg_regs = ["rcx", "rdx", "r8", "r9"];

                        if let Some(offset) = self.var_stack.get("self") {
                            self.emit_line(&format!("    mov rcx, [rbp{}]", offset));
                        }

                        for (i, arg) in args.iter().enumerate() {
                            self.gen_expr_to_rax(arg);
                            if i + 1 < arg_regs.len() {
                                self.emit_line(&format!("    mov {}, rax", arg_regs[i + 1]));
                            }
                        }

                        if let Some(parent) = &self.parent_class {
                            self.emit_line(&format!("    call {}_ctor", parent));
                        }
                    }
                    _ => {
                        self.emit_line("    ; unknown call pattern");
                    }
                }
            }
            Expr::MethodCall(obj, method, args) => {
                let class_name = self.infer_class_from_expr(obj);
                let arg_regs = ["rcx", "rdx", "r8", "r9"];

                self.gen_expr_to_rax(obj);
                self.emit_line("    push rax");

                for (i, arg) in args.iter().enumerate() {
                    self.gen_expr_to_rax(arg);
                    if i + 1 < arg_regs.len() {
                        self.emit_line("    push rax");
                    }
                }

                self.emit_line("    pop rax");
                if args.len() > 0 && args.len() <= arg_regs.len() - 1 {
                    for i in (0..args.len()).rev() {
                        self.emit_line(&format!("    pop {}", arg_regs[i + 1]));
                    }
                }

                self.emit_line("    pop rcx");

                if let Some(cn) = &class_name {
                    if self.virtual_methods.get(cn).map_or(false, |v| v.iter().any(|vm| vm.name == *method && vm.params.len() == args.len())) {
                        let vtable_idx = self.vtable_offset(cn, method, args.len());
                        self.emit_line("    mov rax, [rcx]");
                        self.emit_line(&format!("    mov rax, [rax+{}]", vtable_idx * 8));
                        self.emit_line("    call rax");
                    } else {
                        let mangled = self.resolve_method_overload(cn, method, args.len());
                        self.emit_line(&format!("    call {}", mangled));
                    }
                } else {
                    self.emit_line(&format!("    ; unknown class for method: {}", method));
                }
            }
            Expr::FieldAccess(obj, field) => {
                self.gen_expr_to_rax(obj);
                let class_name = self.infer_class_from_expr(obj);
                if let Some(cn) = &class_name {
                    if let Some(foff) = self.field_offset(cn, field) {
                        self.emit_line(&format!("    mov rax, [rax+{}]", foff));
                    } else {
                        self.emit_line(&format!("    ; unknown field offset: {}.{}", cn, field));
                    }
                } else {
                    self.emit_line(&format!("    ; field access: {} - class unknown", field));
                }
            }
            Expr::New(class_name, _type_args, args) => {
                let size = self.class_size(class_name);
                let arg_regs = ["rcx", "rdx", "r8", "r9"];

                self.emit_line(&format!("    mov rcx, {}", size));
                self.emit_line("    call malloc");
                self.emit_line("    push rax");

                for (i, arg) in args.iter().enumerate() {
                    self.gen_expr_to_rax(arg);
                    if i + 1 < arg_regs.len() {
                        self.emit_line("    push rax");
                    }
                }

                self.emit_line("    pop rcx");

                if args.len() > 0 && args.len() <= arg_regs.len() - 1 {
                    for i in (0..args.len()).rev() {
                        self.emit_line(&format!("    pop {}", arg_regs[i + 1]));
                    }
                }

                self.emit_line(&format!("    call {}_ctor", class_name));
            }
            Expr::ArrayAccess(_arr, _idx) => {
                self.emit_line("    ; array access - not fully supported");
                self.emit_line("    xor rax, rax");
            }
            Expr::Cast(_target_type, inner) => {
                self.gen_expr_to_rax(inner);
            }
            Expr::InstanceOf(_inner_expr, _type_ref) => {
                self.emit_line("    ; instanceof - not fully supported");
                self.emit_line("    mov rax, 1");
            }
            Expr::Delete(inner) => {
                self.gen_expr_to_rax(inner);
                self.emit_line("    mov rcx, rax");
                self.emit_line("    call free");
                self.emit_line("    xor rax, rax");
            }
            Expr::Sizeof(_type_ref) => {
                self.emit_line("    mov rax, 8");
            }
            Expr::TypeId(_inner) => {
                self.emit_line("    ; typeid - not fully supported");
                self.emit_line("    xor rax, rax");
            }
            Expr::Lambda(_, _) => {
                self.emit_line("    ; lambda - not supported in NASM gen");
                self.emit_line("    xor rax, rax");
            }
            Expr::Throw(inner) => {
                self.gen_expr_to_rax(inner);
                self.emit_line("    ; throw - not fully supported in NASM gen");
            }
        }
    }

    fn vtable_offset(&self, class_name: &str, method_name: &str, param_count: usize) -> usize {
        if let Some(vmethods) = self.virtual_methods.get(class_name) {
            vmethods.iter().position(|vm| vm.name == method_name && vm.params.len() == param_count).unwrap_or(0)
        } else {
            0
        }
    }

    fn type_size(&self, type_ref: &TypeRef) -> i32 {
        match type_ref {
            TypeRef::Primitive(p) => match p {
                PrimitiveType::Void => 0,
                PrimitiveType::Bool => 4,
                PrimitiveType::Byte => 1,
                PrimitiveType::Char => 1,
                PrimitiveType::Short => 2,
                PrimitiveType::Int => 4,
                PrimitiveType::Long => 8,
                PrimitiveType::Float => 4,
                PrimitiveType::Double => 8,
            },
            TypeRef::Named(name, _) => match name.as_str() {
                "int" => 4,
                "long" => 8,
                "float" => 4,
                "double" => 8,
                "bool" => 4,
                "String" => 8,
                "Array" => 8,
                "Map" => 8,
                _ => 8,
            },
            TypeRef::Array(_) => 8,
            TypeRef::FunctionPtr(_, _) => 8,
        }
    }

    fn asm_type(&self, type_ref: &TypeRef) -> String {
        match type_ref {
            TypeRef::Primitive(p) => match p {
                PrimitiveType::Void => "void".to_string(),
                PrimitiveType::Bool => "dword".to_string(),
                PrimitiveType::Byte => "byte".to_string(),
                PrimitiveType::Char => "byte".to_string(),
                PrimitiveType::Short => "word".to_string(),
                PrimitiveType::Int => "dword".to_string(),
                PrimitiveType::Long => "qword".to_string(),
                PrimitiveType::Float => "dword".to_string(),
                PrimitiveType::Double => "qword".to_string(),
            },
            TypeRef::Named(name, _) => match name.as_str() {
                "int" => "dword".to_string(),
                "long" => "qword".to_string(),
                "float" => "dword".to_string(),
                "double" => "qword".to_string(),
                "bool" => "dword".to_string(),
                "String" => "qword".to_string(),
                _ => "qword".to_string(),
            },
            TypeRef::Array(_) => "qword".to_string(),
            TypeRef::FunctionPtr(_, _) => "qword".to_string(),
        }
    }

    fn escape_string_for_nasm(&self, s: &str) -> String {
        let mut result = String::new();
        for c in s.chars() {
            match c {
                '\n' => result.push_str("10"),
                '\r' => result.push_str("13"),
                '\t' => result.push_str("9"),
                '\0' => result.push_str("0"),
                '"' => result.push_str("34"),
                '\\' => result.push_str("92"),
                c if (c as u32) < 32 || (c as u32) > 126 => {
                    result.push_str(&format!("{}", c as u32));
                }
                c => {
                    if !result.is_empty() && !result.ends_with(',') {
                        result.push(',');
                    }
                    result.push_str(&format!("'{}'", c));
                }
            }
        }
        if result.is_empty() {
            return "0".to_string();
        }
        result
    }

    pub fn string_literals(&self) -> &[String] {
        &self.string_literals
    }

    fn collect_method_signatures(&mut self, ast: &Program) {
        for decl in &ast.declarations {
            if let Declaration::Class(class) = decl {
                let mut sigs = Vec::new();
                for member in &class.members {
                    if let ClassMember::Method(m) = member {
                        if m.name != class.name {
                            let param_types: Vec<TypeRef> = m.params.iter().map(|p| p.param_type.clone()).collect();
                            sigs.push((m.name.clone(), param_types));
                        }
                    }
                }
                self.method_signatures.insert(class.name.clone(), sigs);
            }
        }
    }

    fn resolve_method_overload(&self, class_name: &str, method_name: &str, arg_count: usize) -> String {
        if let Some(sigs) = self.method_signatures.get(class_name) {
            let matching: Vec<&(String, Vec<TypeRef>)> = sigs.iter()
                .filter(|(name, _)| name == method_name)
                .collect();
            if matching.len() == 1 {
                let params: Vec<Param> = matching[0].1.iter().map(|pt| Param {
                    param_type: pt.clone(),
                    name: String::new(),
                    default_value: None,
                }).collect();
                return mangle_method_name(class_name, method_name, &params);
            }
            if matching.len() > 1 {
                let best = matching.iter().min_by_key(|(_, param_types)| {
                    if param_types.len() == arg_count { 0 } else { (param_types.len() as i32 - arg_count as i32).abs() as usize }
                });
                if let Some((_, param_types)) = best {
                    let params: Vec<Param> = param_types.iter().map(|pt| Param {
                        param_type: pt.clone(),
                        name: String::new(),
                        default_value: None,
                    }).collect();
                    return mangle_method_name(class_name, method_name, &params);
                }
            }
        }
        format!("{}_{}", class_name, method_name)
    }
}
