use crate::ast::node::*;
use std::collections::HashMap;

pub struct NativeCodeGen {
    text: Vec<u8>,
    data: Vec<u8>,
    string_literals: Vec<String>,
    string_counter: u32,
    current_class: Option<String>,
    parent_class: Option<String>,
    var_stack: HashMap<String, (i32, Option<String>)>,
    stack_offset: i32,
    label_counter: u32,
    class_fields: HashMap<String, Vec<(String, i32)>>,
    class_sizes: HashMap<String, i32>,
    virtual_methods: HashMap<String, Vec<String>>,
    method_sigs: HashMap<String, Vec<(String, Vec<TypeRef>, String)>>,
    sym_offsets: Vec<(String, u32)>,
    data_sym_offsets: Vec<(String, u32)>,
    relocs: Vec<(u32, String)>,
    debug_mode: bool,
}

impl NativeCodeGen {
    pub fn new() -> Self {
        Self {
            text: Vec::new(),
            data: Vec::new(),
            string_literals: Vec::new(),
            string_counter: 0,
            current_class: None,
            parent_class: None,
            var_stack: HashMap::new(),
            stack_offset: 0,
            label_counter: 0,
            class_fields: HashMap::new(),
            class_sizes: HashMap::new(),
            virtual_methods: HashMap::new(),
            method_sigs: HashMap::new(),
            sym_offsets: Vec::new(),
            data_sym_offsets: Vec::new(),
            relocs: Vec::new(),
            debug_mode: false,
        }
    }

    pub fn with_debug() -> Self {
        let mut gen = Self::new();
        gen.debug_mode = true;
        gen
    }

    fn sym_offset(&self, name: &str) -> Option<u32> {
        self.sym_offsets.iter().find(|(n, _)| n == name).map(|(_, o)| *o)
    }

    fn set_sym_offset(&mut self, name: &str, offset: u32) {
        if let Some(pos) = self.sym_offsets.iter().position(|(n, _)| n == name) {
            self.sym_offsets[pos] = (name.to_string(), offset);
        } else {
            self.sym_offsets.push((name.to_string(), offset));
        }
    }

    fn data_sym_offset(&self, name: &str) -> Option<u32> {
        self.data_sym_offsets.iter().find(|(n, _)| n == name).map(|(_, o)| *o)
    }

    fn set_data_sym_offset(&mut self, name: &str, offset: u32) {
        if let Some(pos) = self.data_sym_offsets.iter().position(|(n, _)| n == name) {
            self.data_sym_offsets[pos] = (name.to_string(), offset);
        } else {
            self.data_sym_offsets.push((name.to_string(), offset));
        }
    }

    pub fn generate(&mut self, ast: &Program) -> Vec<u8> {
        self.collect_info(ast);
        self.collect_vmethods(ast);
        self.collect_sigs(ast);

        for decl in &ast.declarations {
            match decl {
                Declaration::Class(c) => self.gen_class(c),
                Declaration::Function(f) => self.gen_func(f),
                _ => {}
            }
        }

        for decl in &ast.declarations {
            if let Declaration::Class(c) = decl {
                if c.name == "App" {
                    for m in &c.members {
                        if let ClassMember::Method(method) = m {
                            if method.name == "main" {
                                self.gen_main(method);
                            }
                        }
                    }
                }
            }
        }

        self.build_strings();
        self.resolve_relocs();
        self.build_coff()
    }

    fn collect_info(&mut self, ast: &Program) {
        // First pass: collect all classes and their direct fields
        let mut class_map: HashMap<String, ClassDecl> = HashMap::new();
        for decl in &ast.declarations {
            if let Declaration::Class(c) = decl {
                class_map.insert(c.name.clone(), c.clone());
            }
        }

        // Second pass: build field tables with inheritance
        for decl in &ast.declarations {
            if let Declaration::Class(c) = decl {
                let mut fields = vec![("vtable".to_string(), 0)];
                let mut off: i32 = 8;

                // Collect parent fields first
                let mut current = c.extends.as_ref().and_then(|t| {
                    if let TypeRef::Named(n, _) = t { Some(n.clone()) } else { None }
                });
                while let Some(parent_name) = current {
                    if let Some(parent_class) = class_map.get(&parent_name) {
                        for m in &parent_class.members {
                            if let ClassMember::Field(f) = m {
                                fields.push((f.name.clone(), off));
                                off += self.tsize(&f.var_type);
                            }
                        }
                        current = parent_class.extends.as_ref().and_then(|t| {
                            if let TypeRef::Named(n, _) = t { Some(n.clone()) } else { None }
                        });
                    } else {
                        break;
                    }
                }

                // Then add own fields
                for m in &c.members {
                    if let ClassMember::Field(f) = m {
                        fields.push((f.name.clone(), off));
                        off += self.tsize(&f.var_type);
                    }
                }
                self.class_fields.insert(c.name.clone(), fields);
                self.class_sizes.insert(c.name.clone(), off + 8);
            }
        }
    }

    fn collect_vmethods(&mut self, ast: &Program) {
        for decl in &ast.declarations {
            if let Declaration::Class(c) = decl {
                let mut vms = Vec::new();
                for m in &c.members {
                    if let ClassMember::Method(method) = m {
                        if method.modifiers.iter().any(|m2| matches!(m2, MethodModifier::Virtual)) {
                            vms.push(mangle_method_name(&c.name, &method.name, &method.params));
                        }
                    }
                }
                if !vms.is_empty() {
                    self.virtual_methods.insert(c.name.clone(), vms);
                }
            }
        }
    }

    fn collect_sigs(&mut self, ast: &Program) {
        // First pass: collect all classes
        let mut class_map: HashMap<String, ClassDecl> = HashMap::new();
        for decl in &ast.declarations {
            if let Declaration::Class(c) = decl {
                class_map.insert(c.name.clone(), c.clone());
            }
        }

        // Second pass: build method signatures with inheritance
        for decl in &ast.declarations {
            if let Declaration::Class(c) = decl {
                let mut sigs = Vec::new();
                // Key: (method_name, param_count) -> (param_types, defining_class)
                let mut seen: HashMap<(String, usize), (Vec<TypeRef>, String)> = HashMap::new();

                // Collect parent methods first
                let mut current = c.extends.as_ref().and_then(|t| {
                    if let TypeRef::Named(n, _) = t { Some(n.clone()) } else { None }
                });
                while let Some(parent_name) = current {
                    if let Some(parent_class) = class_map.get(&parent_name) {
                        for m in &parent_class.members {
                            if let ClassMember::Method(method) = m {
                                if method.name != parent_class.name {
                                    let pt: Vec<TypeRef> = method.params.iter().map(|p| p.param_type.clone()).collect();
                                    let key = (method.name.clone(), pt.len());
                                    if !seen.contains_key(&key) {
                                        seen.insert(key.clone(), (pt.clone(), parent_name.clone()));
                                        sigs.push((method.name.clone(), pt, parent_name.clone()));
                                    }
                                }
                            }
                        }
                        current = parent_class.extends.as_ref().and_then(|t| {
                            if let TypeRef::Named(n, _) = t { Some(n.clone()) } else { None }
                        });
                    } else {
                        break;
                    }
                }

                // Then add own methods (may override parent with same param count)
                for m in &c.members {
                    if let ClassMember::Method(method) = m {
                        if method.name != c.name {
                            let pt: Vec<TypeRef> = method.params.iter().map(|p| p.param_type.clone()).collect();
                            let key = (method.name.clone(), pt.len());
                            if let Some(pos) = sigs.iter().position(|(n, ppt, _)| n == &method.name && ppt.len() == pt.len()) {
                                sigs[pos] = (method.name.clone(), pt, c.name.clone()); // override same param count
                            } else {
                                sigs.push((method.name.clone(), pt, c.name.clone()));
                            }
                        }
                    }
                }
                self.method_sigs.insert(c.name.clone(), sigs);
            }
        }
    }

    fn has_ctor(&self, c: &ClassDecl) -> bool {
        c.members.iter().any(|m| matches!(m, ClassMember::Constructor(_))
            || matches!(m, ClassMember::Method(method) if method.name == c.name))
    }

    fn gen_main(&mut self, method: &MethodDecl) {
        self.set_sym_offset("main", self.text.len() as u32);
        self.emit(&[0x55]);
        self.emit(&[0x48, 0x89, 0xE5]);
        self.emit_sub_rsp(48);
        self.emit(&[0xE8]);
        let off = self.text.len() as u32;
        self.emit_rel32(0);
        self.relocs.push((off, "__main".to_string()));
        let mangled = mangle_method_name("App", "main", &method.params);
        self.emit(&[0x48, 0xC7, 0xC1, 0, 0, 0, 0]);
        self.emit(&[0xE8]);
        let off2 = self.text.len() as u32;
        self.emit_rel32(0);
        self.relocs.push((off2, mangled));
        self.emit(&[0x48, 0x31, 0xC0]);
        self.emit_add_rsp(48);
        self.emit(&[0x5D]);
        self.emit(&[0xC3]);
    }

    fn gen_class(&mut self, c: &ClassDecl) {
        self.current_class = Some(c.name.clone());
        self.parent_class = c.extends.as_ref().and_then(|t| {
            if let TypeRef::Named(n, _) = t { Some(n.clone()) } else { None }
        });
        if !self.has_ctor(c) {
            self.gen_default_ctor(c);
        }
        for m in &c.members {
            match m {
                ClassMember::Constructor(ctor) => self.gen_ctor(ctor, c),
                ClassMember::Method(method) => {
                    if method.name == c.name { self.gen_ctor_method(method, c); }
                    else { self.gen_method(method, c); }
                }
                ClassMember::Destructor(dtor) => self.gen_dtor(dtor, c),
                _ => {}
            }
        }
        self.current_class = None;
        self.parent_class = None;
    }

    fn gen_default_ctor(&mut self, c: &ClassDecl) {
        let lbl = format!("{}_ctor", c.name);
        self.set_sym_offset(&lbl, self.text.len() as u32);
        self.emit(&[0x55]);
        self.emit(&[0x48, 0x89, 0xE5]);
        self.emit_sub_rsp(32);
        self.emit_mov_rbp_off_rcx(-8);
        self.emit_mov_rax_rbp_off(-8);
        self.emit_mov_qword_ind_rax(0);
        self.emit_mov_rax_rbp_off(-8);
        self.emit_add_rsp(32);
        self.emit(&[0x5D, 0xC3]);
    }

    fn gen_ctor(&mut self, ctor: &ConstructorDecl, c: &ClassDecl) {
        let lbl = format!("{}_ctor", c.name);
        self.set_sym_offset(&lbl, self.text.len() as u32);
        self.var_stack.clear();
        self.stack_offset = 8;
        self.emit(&[0x55]);
        self.emit(&[0x48, 0x89, 0xE5]);
        let ss = align32((ctor.params.len() as i32 + 4) * 8);
        self.emit_sub_rsp(ss as u32);
        self.emit_mov_rbp_off_rcx(-8);
        self.var_stack.insert("self".to_string(), (-8, Some(c.name.clone())));
        let aregs = [Reg::Rdx, Reg::R8, Reg::R9];
        for (i, p) in ctor.params.iter().enumerate() {
            let off = -((i + 2) as i32) * 8;
            if i < aregs.len() { self.emit_mov_rbp_off_reg(off, aregs[i]); }
            self.var_stack.insert(p.name.clone(), (off, None));
        }
        for s in &ctor.body.statements { self.gen_stmt(s); }
        self.emit_mov_rax_rbp_off(-8);
        self.emit_add_rsp(ss as u32);
        self.emit(&[0x5D, 0xC3]);
    }

    fn gen_ctor_method(&mut self, method: &MethodDecl, c: &ClassDecl) {
        let lbl = format!("{}_ctor", c.name);
        self.set_sym_offset(&lbl, self.text.len() as u32);
        self.var_stack.clear();
        self.stack_offset = 8;
        self.emit(&[0x55]);
        self.emit(&[0x48, 0x89, 0xE5]);
        let ss = align32((method.params.len() as i32 + 4) * 8);
        self.emit_sub_rsp(ss as u32);
        self.emit_mov_rbp_off_rcx(-8);
        self.var_stack.insert("self".to_string(), (-8, Some(c.name.clone())));
        let aregs = [Reg::Rdx, Reg::R8, Reg::R9];
        for (i, p) in method.params.iter().enumerate() {
            let off = -((i + 2) as i32) * 8;
            if i < aregs.len() { self.emit_mov_rbp_off_reg(off, aregs[i]); }
            self.var_stack.insert(p.name.clone(), (off, None));
        }
        if let Some(body) = &method.body {
            for s in &body.statements { self.gen_stmt(s); }
        }
        self.emit_mov_rax_rbp_off(-8);
        self.emit_add_rsp(ss as u32);
        self.emit(&[0x5D, 0xC3]);
    }

    fn gen_method(&mut self, method: &MethodDecl, c: &ClassDecl) {
        let mangled = mangle_method_name(&c.name, &method.name, &method.params);
        self.set_sym_offset(&mangled, self.text.len() as u32);
        self.var_stack.clear();
        self.stack_offset = 8;
        self.emit(&[0x55]);
        self.emit(&[0x48, 0x89, 0xE5]);
        let is_static = method.modifiers.iter().any(|m| matches!(m, MethodModifier::Static));
        let extra_slot = if is_static { 0 } else { 1 };
        let ss = align32((method.params.len() as i32 + extra_slot + 4) * 8);
        self.emit_sub_rsp(ss as u32);
        if !is_static {
            self.emit_mov_rbp_off_rcx(-8);
            self.var_stack.insert("self".to_string(), (-8, Some(c.name.clone())));
        }
        let aregs: Vec<Reg> = if is_static {
            vec![Reg::Rcx, Reg::Rdx, Reg::R8, Reg::R9]
        } else {
            vec![Reg::Rdx, Reg::R8, Reg::R9]
        };
        for (i, p) in method.params.iter().enumerate() {
            let off = -((i as i32 + 1 + extra_slot) * 8);
            if i < aregs.len() { self.emit_mov_rbp_off_reg(off, aregs[i]); }
            self.var_stack.insert(p.name.clone(), (off, None));
        }
        if let Some(body) = &method.body {
            for s in &body.statements { self.gen_stmt(s); }
        }
        self.emit_add_rsp(ss as u32);
        self.emit(&[0x5D, 0xC3]);
    }

    fn gen_dtor(&mut self, dtor: &DestructorDecl, c: &ClassDecl) {
        let lbl = format!("{}_dtor", c.name);
        self.set_sym_offset(&lbl, self.text.len() as u32);
        self.var_stack.clear();
        self.stack_offset = 8;
        self.emit(&[0x55]);
        self.emit(&[0x48, 0x89, 0xE5]);
        self.emit_sub_rsp(32);
        self.emit_mov_rbp_off_rcx(-8);
        self.var_stack.insert("self".to_string(), (-8, Some(c.name.clone())));
        for s in &dtor.body.statements { self.gen_stmt(s); }
        self.emit_add_rsp(32);
        self.emit(&[0x5D, 0xC3]);
    }

    fn gen_func(&mut self, f: &FunctionDecl) {
        self.set_sym_offset(&f.name, self.text.len() as u32);
        self.var_stack.clear();
        self.stack_offset = 8;
        self.current_class = None;
        self.emit(&[0x55]);
        self.emit(&[0x48, 0x89, 0xE5]);
        let ss = align32((f.params.len() as i32 + 4) * 8);
        self.emit_sub_rsp(ss as u32);
        let aregs = [Reg::Rcx, Reg::Rdx, Reg::R8, Reg::R9];
        for (i, p) in f.params.iter().enumerate() {
            let off = -((i + 1) as i32) * 8;
            if i < aregs.len() { self.emit_mov_rbp_off_reg(off, aregs[i]); }
            self.var_stack.insert(p.name.clone(), (off, None));
        }
        for s in &f.body.statements { self.gen_stmt(s); }
        self.emit_add_rsp(ss as u32);
        self.emit(&[0x5D, 0xC3]);
    }

    fn gen_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::VarDecl(var) => {
                self.stack_offset += 8;
                let off = -self.stack_offset;
                let var_type = var.initializer.as_ref().and_then(|e| self.infer_class(e));
                self.var_stack.insert(var.name.clone(), (off, var_type));
                if let Some(init) = &var.initializer {
                    self.expr_rax(init);
                    self.emit_mov_rbp_off_rax(off);
                } else {
                    self.emit_mov_qword_rbp_off(off, 0);
                }
            }
            Stmt::Return(expr) => {
                match expr {
                    Some(e) => self.expr_rax(e),
                    None => self.emit(&[0x48, 0x31, 0xC0]),
                }
                // Find the current function's stack size by looking at the max negative offset
                let max_off = self.var_stack.values()
                    .map(|(off, _)| off.abs())
                    .max()
                    .unwrap_or(0);
                let ss = align32(max_off + 32);
                self.emit_add_rsp(ss as u32);
                self.emit(&[0x5D, 0xC3]);
            }
            Stmt::Expr(expr) => { self.expr_rax(expr); }
            Stmt::If(cond, then, else_) => {
                let el = self.new_label();
                let end = self.new_label();
                self.expr_rax(cond);
                self.emit_test_rax();
                self.emit_jz(&el);
                self.gen_stmt(then);
                self.emit_jmp(&end);
                self.set_label(&el);
                if let Some(es) = else_ { self.gen_stmt(es); }
                self.set_label(&end);
            }
            Stmt::While(cond, body) => {
                let start = self.new_label();
                let end = self.new_label();
                self.set_label(&start);
                self.expr_rax(cond);
                self.emit_test_rax();
                self.emit_jz(&end);
                self.gen_stmt(body);
                self.emit_jmp(&start);
                self.set_label(&end);
            }
            Stmt::For(init, cond, update, body) => {
                let start = self.new_label();
                let end = self.new_label();
                if let Some(e) = init { self.expr_rax(e); }
                self.set_label(&start);
                if let Some(e) = cond {
                    self.expr_rax(e);
                    self.emit_test_rax();
                    self.emit_jz(&end);
                }
                self.gen_stmt(body);
                if let Some(e) = update { self.expr_rax(e); }
                self.emit_jmp(&start);
                self.set_label(&end);
            }
            Stmt::Block(block) => {
                for s in &block.statements { self.gen_stmt(s); }
            }
            Stmt::Break | Stmt::Continue => {}
            Stmt::Try(_, _, _) => {}
        }
    }

    fn expr_rax(&mut self, expr: &Expr) {
        match expr {
            Expr::IntegerLiteral(v) => self.emit_mov_rax_imm(*v as u64),
            Expr::FloatLiteral(_) => self.emit(&[0x48, 0x31, 0xC0]),
            Expr::StringLiteral(s) => {
                let idx = self.string_counter;
                self.string_counter += 1;
                self.string_literals.push(s.clone());
                let sym = format!("_str_{}", idx);
                let off = self.text.len() as u32;
                self.emit(&[0x48, 0x8D, 0x05]); // lea rax, [rip+rel32]
                self.emit_rel32(0);
                self.relocs.push((off + 3, sym));
            }
            Expr::CharLiteral(c) => self.emit_mov_rax_imm(*c as u64),
            Expr::BoolLiteral(b) => {
                if *b { self.emit_mov_rax_imm(1); } else { self.emit(&[0x48, 0x31, 0xC0]); }
            }
            Expr::Null => self.emit(&[0x48, 0x31, 0xC0]),
            Expr::This | Expr::Super => {
                if let Some((off, _)) = self.var_stack.get("self") {
                    self.emit_mov_rax_rbp_off(*off);
                }
            }
            Expr::Variable(name) => {
                if let Some((off, _)) = self.var_stack.get(name) {
                    self.emit_mov_rax_rbp_off(*off);
                } else {
                    self.emit(&[0x48, 0x31, 0xC0]);
                }
            }
            Expr::BinaryOp(op, left, right) => {
                self.expr_rax(left);
                self.emit(&[0x50]); // push rax
                self.expr_rax(right);
                self.emit_mov_rcx_rax();
                self.emit(&[0x58]); // pop rax
                match op {
                    BinaryOp::Add => self.emit(&[0x48, 0x01, 0xC8]),
                    BinaryOp::Sub => self.emit(&[0x48, 0x29, 0xC8]),
                    BinaryOp::Mul => self.emit(&[0x48, 0x0F, 0xAF, 0xC1]),
                    BinaryOp::Div => {
                        self.emit(&[0x4C, 0x89, 0xC0]); // mov r8, rax
                        self.emit(&[0x48, 0x89, 0xC8]); // mov rax, rcx
                        self.emit(&[0x48, 0x99]);        // cqo
                        self.emit(&[0x49, 0xF7, 0xF8]); // idiv r8
                    }
                    BinaryOp::Mod => {
                        self.emit(&[0x4C, 0x89, 0xC0]);
                        self.emit(&[0x48, 0x89, 0xC8]);
                        self.emit(&[0x48, 0x99]);
                        self.emit(&[0x49, 0xF7, 0xF8]);
                        self.emit(&[0x48, 0x89, 0xD0]); // mov rax, rdx
                    }
                    BinaryOp::Lt => self.emit_cmp_set(0x9C),
                    BinaryOp::Gt => self.emit_cmp_set(0x9F),
                    BinaryOp::Le => self.emit_cmp_set(0x9E),
                    BinaryOp::Ge => self.emit_cmp_set(0x9D),
                    BinaryOp::Eq => self.emit_cmp_set(0x94),
                    BinaryOp::Ne => self.emit_cmp_set(0x95),
                    BinaryOp::And => self.emit(&[0x48, 0x21, 0xC8]),
                    BinaryOp::Or => self.emit(&[0x48, 0x09, 0xC8]),
                    BinaryOp::BitAnd => self.emit(&[0x48, 0x21, 0xC8]),
                    BinaryOp::BitOr => self.emit(&[0x48, 0x09, 0xC8]),
                    BinaryOp::BitXor => self.emit(&[0x48, 0x31, 0xC8]),
                    BinaryOp::Shl => self.emit(&[0x48, 0xD3, 0xE0]),
                    BinaryOp::Shr => self.emit(&[0x48, 0xD3, 0xF8]),
                }
            }
            Expr::UnaryOp(op, operand) => {
                self.expr_rax(operand);
                match op {
                    UnaryOp::Minus => self.emit(&[0x48, 0xF7, 0xD8]),
                    UnaryOp::Not => {
                        self.emit(&[0x48, 0x85, 0xC0]);
                        self.emit(&[0x0F, 0x94, 0xC0]);
                        self.emit(&[0x48, 0x0F, 0xB6, 0xC0]);
                    }
                    UnaryOp::BitNot => self.emit(&[0x48, 0xF7, 0xD0]),
                    UnaryOp::Deref => self.emit(&[0x48, 0x8B, 0x00]),
                    _ => {}
                }
            }
            Expr::Ternary(cond, then, else_) => {
                let el = self.new_label();
                let end = self.new_label();
                self.expr_rax(cond);
                self.emit_test_rax();
                self.emit_jz(&el);
                self.expr_rax(then);
                self.emit_jmp(&end);
                self.set_label(&el);
                self.expr_rax(else_);
                self.set_label(&end);
            }
            Expr::Assignment(target, value) => {
                self.expr_rax(value);
                match target.as_ref() {
                    Expr::Variable(name) => {
                        if let Some((off, _)) = self.var_stack.get(name) {
                            self.emit_mov_rbp_off_rax(*off);
                        }
                    }
                    Expr::FieldAccess(obj, field) => {
                        self.emit(&[0x50]); // push rax
                        self.expr_rax(obj);
                        if let Some(cn) = self.infer_class(obj) {
                            if let Some(foff) = self.foff(&cn, field) {
                                self.emit(&[0x59]); // pop rcx
                                self.emit_mov_rax_off_rcx(foff);
                            }
                        }
                    }
                    _ => {}
                }
            }
            Expr::Call(callee, args) => {
                match callee.as_ref() {
                    Expr::Variable(name) => {
                        let aregs = [Reg::Rcx, Reg::Rdx, Reg::R8, Reg::R9];
                        for (i, arg) in args.iter().enumerate() {
                            self.expr_rax(arg);
                            self.emit(&[0x50]); // push rax
                        }
                        for i in (0..args.len().min(aregs.len())).rev() {
                            self.emit_pop_reg(aregs[i]);
                        }
                        self.emit_sub_rsp(32);
                        self.emit(&[0xE8]);
                        let off = self.text.len() as u32;
                        self.emit_rel32(0);
                        self.relocs.push((off, name.clone()));
                        self.emit_add_rsp(32);
                    }
                    Expr::FieldAccess(obj, method) => {
                        let cn = self.infer_class(obj);
                        let aregs = [Reg::Rdx, Reg::R8, Reg::R9];
                        self.expr_rax(obj);
                        self.emit(&[0x50]);
                        for (i, arg) in args.iter().enumerate() {
                            self.expr_rax(arg);
                            self.emit(&[0x50]); // push rax
                        }
                        for i in (0..args.len().min(aregs.len())).rev() {
                            self.emit_pop_reg(aregs[i]);
                        }
                        self.emit(&[0x59]); // pop rcx
                        self.emit_sub_rsp(32);
                        if let Some(cn) = &cn {
                            let mangled = self.resolve_overload(cn, method, args.len());
                            self.emit(&[0xE8]);
                            let off = self.text.len() as u32;
                            self.emit_rel32(0);
                            self.relocs.push((off, mangled));
                        }
                        self.emit_add_rsp(32);
                    }
                    Expr::Super => {
                        if let Some((off, _)) = self.var_stack.get("self") {
                            self.emit_mov_rcx_rbp_off(*off);
                        }
                        let aregs = [Reg::Rdx, Reg::R8, Reg::R9];
                        for (i, arg) in args.iter().enumerate() {
                            self.expr_rax(arg);
                            if i < aregs.len() { self.emit_mov_reg_rax(aregs[i]); }
                        }
                        let parent = self.parent_class.clone();
                        if let Some(parent) = parent {
                            self.emit(&[0xE8]);
                            let off = self.text.len() as u32;
                            self.emit_rel32(0);
                            self.relocs.push((off, format!("{}_ctor", parent)));
                        }
                    }
                    _ => {}
                }
            }
            Expr::MethodCall(obj, method, args) => {
                let cn = self.infer_class(obj);
                let aregs = [Reg::Rdx, Reg::R8, Reg::R9];
                self.expr_rax(obj);
                self.emit(&[0x50]);
                for (i, arg) in args.iter().enumerate() {
                    self.expr_rax(arg);
                    self.emit(&[0x50]); // push rax
                }
                for i in (0..args.len().min(aregs.len())).rev() {
                    self.emit_pop_reg(aregs[i]);
                }
                self.emit(&[0x59]); // pop rcx
                self.emit_sub_rsp(32);
                if let Some(cn) = &cn {
                    let mangled = self.resolve_overload(cn, method, args.len());
                    self.emit(&[0xE8]);
                    let off = self.text.len() as u32;
                    self.emit_rel32(0);
                    self.relocs.push((off, mangled));
                }
                self.emit_add_rsp(32);
            }
            Expr::FieldAccess(obj, field) => {
                self.expr_rax(obj);
                if let Some(cn) = self.infer_class(obj) {
                    if let Some(foff) = self.foff(&cn, field) {
                        self.emit_mov_rax_off_rax(foff);
                    }
                }
            }
            Expr::New(class_name, _type_args, args) => {
                let size = self.class_sizes.get(class_name).copied().unwrap_or(16);
                let aregs = [Reg::Rdx, Reg::R8, Reg::R9];
                self.emit_mov_rcx_imm(size as u64);
                self.emit_sub_rsp(32);
                self.emit(&[0xE8]);
                let off = self.text.len() as u32;
                self.emit_rel32(0);
                self.relocs.push((off, "malloc".to_string()));
                self.emit_add_rsp(32);
                self.emit(&[0x50]); // push rax
                for (i, arg) in args.iter().enumerate() {
                    self.expr_rax(arg);
                    self.emit(&[0x50]); // push rax
                }
                for i in (0..args.len().min(aregs.len())).rev() {
                    self.emit_pop_reg(aregs[i]);
                }
                self.emit(&[0x59]); // pop rcx
                self.emit_sub_rsp(32);
                self.emit(&[0xE8]);
                let off2 = self.text.len() as u32;
                self.emit_rel32(0);
                self.relocs.push((off2, format!("{}_ctor", class_name)));
                self.emit_add_rsp(32);
            }
            Expr::Delete(inner) => {
                self.expr_rax(inner);
                self.emit_mov_rcx_rax();
                self.emit_sub_rsp(32);
                self.emit(&[0xE8]);
                let off = self.text.len() as u32;
                self.emit_rel32(0);
                self.relocs.push((off, "free".to_string()));
                self.emit_add_rsp(32);
                self.emit(&[0x48, 0x31, 0xC0]);
            }
            Expr::Cast(_, inner) => self.expr_rax(inner),
            Expr::InstanceOf(_, _) => self.emit_mov_rax_imm(1),
            Expr::Sizeof(_) => self.emit_mov_rax_imm(8),
            Expr::TypeId(_) | Expr::ArrayAccess(_, _) | Expr::Lambda(_, _) | Expr::Throw(_) => {
                self.emit(&[0x48, 0x31, 0xC0]);
            }
        }
    }

    fn infer_class(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::This => self.current_class.clone(),
            Expr::Super => self.parent_class.clone().or(self.current_class.clone()),
            Expr::Variable(name) => {
                if name == "self" {
                    self.current_class.clone()
                } else {
                    self.var_stack.get(name).and_then(|(_, t)| t.clone())
                }
            }
            Expr::New(cn, _, _) => Some(cn.clone()),
            Expr::FieldAccess(obj, _) => self.infer_class(obj),
            _ => None,
        }
    }

    fn foff(&self, cn: &str, field: &str) -> Option<i32> {
        self.class_fields.get(cn).and_then(|fs| fs.iter().find(|(n, _)| n == field).map(|(_, o)| *o))
    }

    fn resolve_overload(&self, cn: &str, method: &str, argc: usize) -> String {
        if let Some(sigs) = self.method_sigs.get(cn) {
            let matching: Vec<&(String, Vec<TypeRef>, String)> = sigs.iter().filter(|(n, _, _)| n == method).collect();
            if matching.len() == 1 {
                let params: Vec<Param> = matching[0].1.iter().map(|pt| Param {
                    param_type: pt.clone(), name: String::new(), default_value: None,
                }).collect();
                return mangle_method_name(&matching[0].2, method, &params);
            }
            if matching.len() > 1 {
                // First try exact match
                for (_, pt, def_class) in &matching {
                    if pt.len() == argc {
                        let params: Vec<Param> = pt.iter().map(|t| Param {
                            param_type: t.clone(), name: String::new(), default_value: None,
                        }).collect();
                        return mangle_method_name(def_class, method, &params);
                    }
                }
                // Fallback to closest match
                let best = matching.iter().min_by_key(|(_, pt, _)| {
                    (pt.len() as i32 - argc as i32).abs() as usize
                });
                if let Some((_, pt, def_class)) = best {
                    let params: Vec<Param> = pt.iter().map(|t| Param {
                        param_type: t.clone(), name: String::new(), default_value: None,
                    }).collect();
                    return mangle_method_name(def_class, method, &params);
                }
            }
        }
        format!("{}_{}", cn, method)
    }

    fn new_label(&mut self) -> String {
        let id = self.label_counter;
        self.label_counter += 1;
        format!("_L{}", id)
    }

    fn set_label(&mut self, label: &str) {
        self.set_sym_offset(label, self.text.len() as u32);
    }

    fn build_strings(&mut self) {
        let strings: Vec<String> = self.string_literals.clone();
        for (i, s) in strings.iter().enumerate() {
            let sym = format!("_str_{}", i);
            self.set_data_sym_offset(&sym, self.data.len() as u32);
            let mut c_str = s.clone();
            c_str.push('\0');
            self.data.extend_from_slice(c_str.as_bytes());
        }
    }

    fn resolve_relocs(&mut self) {
        for (off, sym) in &self.relocs {
            if sym.starts_with("_L") {
                if let Some(target) = self.sym_offset(sym) {
                    let pos = *off as usize;
                    if pos + 4 <= self.text.len() {
                        let rel = target as i32 - (*off as i32 + 4);
                        self.text[pos..pos + 4].copy_from_slice(&rel.to_le_bytes());
                    }
                }
            }
        }
    }

    fn tsize(&self, tr: &TypeRef) -> i32 {
        match tr {
            TypeRef::Primitive(p) => match p {
                PrimitiveType::Void => 0,
                PrimitiveType::Bool | PrimitiveType::Byte | PrimitiveType::Char => 4,
                PrimitiveType::Short => 2,
                PrimitiveType::Int => 4,
                PrimitiveType::Long => 8,
                PrimitiveType::Float => 4,
                PrimitiveType::Double => 8,
            },
            TypeRef::Named(_, _) | TypeRef::Array(_) | TypeRef::FunctionPtr(_, _) => 8,
        }
    }

    fn emit(&mut self, bytes: &[u8]) {
        self.text.extend_from_slice(bytes);
    }

    fn emit_rel32(&mut self, val: i32) {
        self.text.extend_from_slice(&val.to_le_bytes());
    }

    fn emit_sub_rsp(&mut self, imm: u32) {
        if imm < 128 {
            self.emit(&[0x48, 0x83, 0xEC, imm as u8]);
        } else {
            self.emit(&[0x48, 0x81, 0xEC]);
            self.emit_rel32(imm as i32);
        }
    }

    fn emit_add_rsp(&mut self, imm: u32) {
        if imm < 128 {
            self.emit(&[0x48, 0x83, 0xC4, imm as u8]);
        } else {
            self.emit(&[0x48, 0x81, 0xC4]);
            self.emit_rel32(imm as i32);
        }
    }

    fn emit_mov_rax_imm(&mut self, imm: u64) {
        self.emit(&[0x48, 0xB8]);
        self.text.extend_from_slice(&imm.to_le_bytes());
    }

    fn emit_mov_rcx_imm(&mut self, imm: u64) {
        self.emit(&[0x48, 0xB9]);
        self.text.extend_from_slice(&imm.to_le_bytes());
    }

    fn emit_mov_rcx_rax(&mut self) { self.emit(&[0x48, 0x89, 0xC1]); }

    fn emit_mov_rax_rbp_off(&mut self, off: i32) {
        self.emit(&[0x48, 0x8B, 0x85]);
        self.text.extend_from_slice(&off.to_le_bytes());
    }

    fn emit_mov_rbp_off_rax(&mut self, off: i32) {
        self.emit(&[0x48, 0x89, 0x85]);
        self.text.extend_from_slice(&off.to_le_bytes());
    }

    fn emit_mov_rbp_off_rcx(&mut self, off: i32) {
        self.emit(&[0x48, 0x89, 0x8D]);
        self.text.extend_from_slice(&off.to_le_bytes());
    }

    fn emit_mov_rcx_rbp_off(&mut self, off: i32) {
        self.emit(&[0x48, 0x8B, 0x8D]);
        self.text.extend_from_slice(&off.to_le_bytes());
    }

    fn emit_mov_qword_rbp_off(&mut self, off: i32, _val: u32) {
        self.emit(&[0x48, 0xC7, 0x85]);
        self.text.extend_from_slice(&off.to_le_bytes());
        self.text.extend_from_slice(&0u32.to_le_bytes());
    }

    fn emit_mov_qword_ind_rax(&mut self, _val: u32) {
        self.emit(&[0x48, 0xC7, 0x00]);
        self.text.extend_from_slice(&0u32.to_le_bytes());
    }

    fn emit_mov_rax_off_rax(&mut self, off: i32) {
        self.emit(&[0x48, 0x8B, 0x80]);
        self.text.extend_from_slice(&off.to_le_bytes());
    }

    fn emit_mov_rax_off_rcx(&mut self, off: i32) {
        self.emit(&[0x48, 0x89, 0x88]);
        self.text.extend_from_slice(&off.to_le_bytes());
    }

    fn emit_test_rax(&mut self) { self.emit(&[0x48, 0x85, 0xC0]); }

    fn emit_jz(&mut self, label: &str) {
        self.emit(&[0x0F, 0x84]);
        let off = self.text.len() as u32;
        self.emit_rel32(0);
        self.relocs.push((off, label.to_string()));
    }

    fn emit_jmp(&mut self, label: &str) {
        self.emit(&[0xE9]);
        let off = self.text.len() as u32;
        self.emit_rel32(0);
        self.relocs.push((off, label.to_string()));
    }

    fn emit_cmp_set(&mut self, cc: u8) {
        self.emit(&[0x48, 0x39, 0xC8]); // cmp rax, rcx
        self.emit(&[0x0F, cc, 0xC0]);    // setcc al
        self.emit(&[0x48, 0x0F, 0xB6, 0xC0]); // movzx rax, al
    }

    fn emit_push_reg(&mut self, reg: Reg) {
        match reg {
            Reg::Rcx => self.emit(&[0x51]),
            Reg::Rdx => self.emit(&[0x52]),
            Reg::R8 => self.emit(&[0x41, 0x50]),
            Reg::R9 => self.emit(&[0x41, 0x51]),
        }
    }

    fn emit_pop_reg(&mut self, reg: Reg) {
        match reg {
            Reg::Rcx => self.emit(&[0x59]),
            Reg::Rdx => self.emit(&[0x5A]),
            Reg::R8 => self.emit(&[0x41, 0x58]),
            Reg::R9 => self.emit(&[0x41, 0x59]),
        }
    }

    fn emit_mov_rbp_off_reg(&mut self, off: i32, reg: Reg) {
        match reg {
            Reg::Rcx => { self.emit(&[0x48, 0x89, 0x8D]); }
            Reg::Rdx => { self.emit(&[0x48, 0x89, 0x95]); }
            Reg::R8 => { self.emit(&[0x4C, 0x89, 0x85]); }
            Reg::R9 => { self.emit(&[0x4C, 0x89, 0x8D]); }
        }
        self.text.extend_from_slice(&off.to_le_bytes());
    }

    fn emit_mov_reg_rax(&mut self, reg: Reg) {
        match reg {
            Reg::Rcx => self.emit(&[0x48, 0x89, 0xC1]),
            Reg::Rdx => self.emit(&[0x48, 0x89, 0xC2]),
            Reg::R8 => self.emit(&[0x4C, 0x89, 0xC0]),
            Reg::R9 => self.emit(&[0x4C, 0x89, 0xC1]),
        }
    }

    fn build_coff(&self) -> Vec<u8> {
        let mut coff = Vec::new();

        let text_len = self.text.len() as u32;
        let data_len = self.data.len() as u32;
        let text_aligned = align_up(text_len, 16);
        let data_aligned = align_up(data_len, 16);

        let num_sections: u16 = if data_len > 0 { 2 } else { 1 };

        let mut symbols: Vec<SymbolEntry> = Vec::new();
        let mut str_table = Vec::new();
        str_table.extend_from_slice(&4u32.to_le_bytes());
        let mut str_offsets: HashMap<String, u32> = HashMap::new();

        symbols.push(SymbolEntry::section(".text", 1));
        symbols.push(SymbolEntry::section(".data", 2));

        let mut text_relocs: Vec<RelocEntry> = Vec::new();

        let mut add_sym_external = |symbols: &mut Vec<SymbolEntry>, str_table: &mut Vec<u8>, str_offsets: &mut HashMap<String, u32>, name: &str, sec: i16, val: u32| {
            let entry = if name.len() <= 8 {
                let mut n = [0u8; 8];
                for (i, &b) in name.as_bytes().iter().enumerate().take(8) { n[i] = b; }
                SymbolEntry { name: n, value: val, section_number: sec, typ: if sec > 0 { 0x20 } else { 0 }, storage_class: 2, num_aux: 0 }
            } else {
                let off = *str_offsets.entry(name.to_string()).or_insert_with(|| {
                    let offset = str_table.len() as u32;
                    str_table.extend_from_slice(name.as_bytes());
                    str_table.push(0);
                    offset
                });
                let mut n = [0u8; 8];
                (&mut n[0..4]).copy_from_slice(&0u32.to_le_bytes());
                (&mut n[4..8]).copy_from_slice(&off.to_le_bytes());
                SymbolEntry { name: n, value: val, section_number: sec, typ: if sec > 0 { 0x20 } else { 0 }, storage_class: 2, num_aux: 0 }
            };
            symbols.push(entry);
        };

        for (name, offset) in &self.sym_offsets {
            add_sym_external(&mut symbols, &mut str_table, &mut str_offsets, name, 1, *offset);
        }
        for (name, offset) in &self.data_sym_offsets {
            add_sym_external(&mut symbols, &mut str_table, &mut str_offsets, name, 2, *offset);
        }

        add_sym_external(&mut symbols, &mut str_table, &mut str_offsets, "malloc", 0, 0);
        add_sym_external(&mut symbols, &mut str_table, &mut str_offsets, "free", 0, 0);
        add_sym_external(&mut symbols, &mut str_table, &mut str_offsets, "printf", 0, 0);
        add_sym_external(&mut symbols, &mut str_table, &mut str_offsets, "exit", 0, 0);
        add_sym_external(&mut symbols, &mut str_table, &mut str_offsets, "__main", 0, 0);

        let mut sym_index_map: HashMap<String, u32> = HashMap::new();
        let mut idx: u32 = 0;
        for sym in &symbols {
            let name_str = if sym.name[0..4] == [0, 0, 0, 0] {
                let off = u32::from_le_bytes([sym.name[4], sym.name[5], sym.name[6], sym.name[7]]);
                let st = &str_table[off as usize..];
                let end = st.iter().position(|&b| b == 0).unwrap_or(st.len());
                String::from_utf8_lossy(&st[..end]).to_string()
            } else {
                let end = sym.name.iter().position(|&b| b == 0).unwrap_or(8);
                String::from_utf8_lossy(&sym.name[..end]).to_string()
            };
            if !name_str.starts_with('.') {
                sym_index_map.insert(name_str, idx);
            }
            idx += 1 + sym.num_aux as u32;
        }

        if self.debug_mode {
            eprintln!("\n=== COFF Symbol Table ===");
            for (name, &i) in &sym_index_map {
                eprintln!("  [{}] {}", i, name);
            }
            eprintln!("\n=== Relocations ===");
            for (off, sym) in &self.relocs {
                if !sym.starts_with("_L") {
                    if let Some(&idx) = sym_index_map.get(sym) {
                        eprintln!("  off=0x{:04x} sym='{}' idx={}", off, sym, idx);
                    } else {
                        eprintln!("  off=0x{:04x} sym='{}' NOT FOUND", off, sym);
                    }
                }
            }
        }

        let ext_map: HashMap<String, u32> = [
            ("malloc".to_string(), *sym_index_map.get("malloc").unwrap_or(&0)),
            ("free".to_string(), *sym_index_map.get("free").unwrap_or(&0)),
            ("printf".to_string(), *sym_index_map.get("printf").unwrap_or(&0)),
            ("exit".to_string(), *sym_index_map.get("exit").unwrap_or(&0)),
            ("__main".to_string(), *sym_index_map.get("__main").unwrap_or(&0)),
        ].iter().cloned().collect();

        let user_syms: HashMap<String, u32> = self.sym_offsets.iter()
            .chain(self.data_sym_offsets.iter())
            .filter_map(|(name, _)| {
                sym_index_map.get(name).map(|&idx| (name.clone(), idx))
            }).collect();

        for (off, sym) in &self.relocs {
            let sym_idx = if let Some(&idx) = user_syms.get(sym) {
                idx
            } else if let Some(&idx) = ext_map.get(sym) {
                idx
            } else {
                continue;
            };
            text_relocs.push(RelocEntry { virtual_address: *off, symbol_table_index: sym_idx, typ: 4 });
        }

        let num_aux_symbols: u32 = symbols.iter().map(|s| s.num_aux as u32).sum();
        let total_symbols = symbols.len() as u32 + num_aux_symbols;
        let coff_header_size = 20u32;
        let section_header_size = 40u32 * num_sections as u32;
        let ptr_to_sym = coff_header_size + section_header_size + text_aligned + data_aligned
            + (text_relocs.len() as u32 * 10);

        coff.extend_from_slice(&0x8664u16.to_le_bytes());
        coff.extend_from_slice(&num_sections.to_le_bytes());
        coff.extend_from_slice(&0u32.to_le_bytes());
        coff.extend_from_slice(&ptr_to_sym.to_le_bytes());
        coff.extend_from_slice(&total_symbols.to_le_bytes());
        coff.extend_from_slice(&0u16.to_le_bytes());
        coff.extend_from_slice(&0u16.to_le_bytes());

        let text_reloc_offset = coff_header_size + section_header_size + text_aligned + data_aligned;
        coff.extend_from_slice(b".text\0\0\0");
        coff.extend_from_slice(&text_len.to_le_bytes());
        coff.extend_from_slice(&0u32.to_le_bytes());
        coff.extend_from_slice(&text_aligned.to_le_bytes());
        coff.extend_from_slice(&(coff_header_size + section_header_size).to_le_bytes());
        coff.extend_from_slice(&text_reloc_offset.to_le_bytes());
        coff.extend_from_slice(&0u32.to_le_bytes());
        coff.extend_from_slice(&(text_relocs.len() as u16).to_le_bytes());
        coff.extend_from_slice(&0u16.to_le_bytes());
        coff.extend_from_slice(&0x60000020u32.to_le_bytes());

        if data_len > 0 {
            let data_offset = coff_header_size + section_header_size + text_aligned;
            coff.extend_from_slice(b".data\0\0\0");
            coff.extend_from_slice(&data_len.to_le_bytes());
            coff.extend_from_slice(&0u32.to_le_bytes());
            coff.extend_from_slice(&data_aligned.to_le_bytes());
            coff.extend_from_slice(&data_offset.to_le_bytes());
            coff.extend_from_slice(&0u32.to_le_bytes());
            coff.extend_from_slice(&0u32.to_le_bytes());
            coff.extend_from_slice(&0u16.to_le_bytes());
            coff.extend_from_slice(&0u16.to_le_bytes());
            coff.extend_from_slice(&0xC0000040u32.to_le_bytes());
        }

        coff.extend_from_slice(&self.text);
        if text_len < text_aligned {
            coff.extend_from_slice(&vec![0u8; (text_aligned - text_len) as usize]);
        }

        if data_len > 0 {
            coff.extend_from_slice(&self.data);
            if data_len < data_aligned {
                coff.extend_from_slice(&vec![0u8; (data_aligned - data_len) as usize]);
            }
        }

        for r in &text_relocs {
            coff.extend_from_slice(&r.virtual_address.to_le_bytes());
            coff.extend_from_slice(&r.symbol_table_index.to_le_bytes());
            coff.extend_from_slice(&r.typ.to_le_bytes());
        }

        for (si, sym) in symbols.iter().enumerate() {
            coff.extend_from_slice(sym.name_bytes());
            coff.extend_from_slice(&sym.value.to_le_bytes());
            coff.extend_from_slice(&sym.section_number.to_le_bytes());
            coff.extend_from_slice(&sym.typ.to_le_bytes());
            coff.extend_from_slice(&sym.storage_class.to_le_bytes());
            coff.extend_from_slice(&sym.num_aux.to_le_bytes());
            if sym.num_aux > 0 && sym.storage_class == 3 {
                let sec_idx = sym.section_number as usize - 1;
                let scnlen = if sec_idx == 0 { text_len } else { data_len };
                let nreloc = if sec_idx == 0 { text_relocs.len() as u16 } else { 0 };
                coff.extend_from_slice(&scnlen.to_le_bytes());
                coff.extend_from_slice(&nreloc.to_le_bytes());
                coff.extend_from_slice(&0u16.to_le_bytes());
                coff.extend_from_slice(&0u32.to_le_bytes());
                coff.extend_from_slice(&0u16.to_le_bytes());
                coff.extend_from_slice(&0u8.to_le_bytes());
                coff.extend_from_slice(&[0u8; 3]);
            }
        }

        let str_table_size = str_table.len() as u32;
        str_table[0..4].copy_from_slice(&str_table_size.to_le_bytes());
        coff.extend_from_slice(&str_table);

        coff
    }
}

#[derive(Clone, Copy)]
enum Reg { Rcx, Rdx, R8, R9 }

struct SymbolEntry {
    name: [u8; 8],
    value: u32,
    section_number: i16,
    typ: u16,
    storage_class: u8,
    num_aux: u8,
}

impl SymbolEntry {
    fn section(name: &str, sec_num: i16) -> Self {
        let mut n = [0u8; 8];
        let bytes = name.as_bytes();
        for (i, &b) in bytes.iter().enumerate().take(8) { n[i] = b; }
        SymbolEntry { name: n, value: 0, section_number: sec_num, typ: 0, storage_class: 3, num_aux: 1 }
    }

    fn external(name: &str, sec_num: i16, value: u32) -> Self {
        let mut n = [0u8; 8];
        if name.len() <= 8 {
            for (i, &b) in name.as_bytes().iter().enumerate().take(8) { n[i] = b; }
        } else {
            let off = 4u32;
            (&mut n[0..4]).copy_from_slice(&0u32.to_le_bytes());
            (&mut n[4..8]).copy_from_slice(&off.to_le_bytes());
        }
        SymbolEntry { name: n, value, section_number: sec_num, typ: if sec_num > 0 { 0x20 } else { 0 }, storage_class: 2, num_aux: 0 }
    }

    fn name_bytes(&self) -> &[u8; 8] { &self.name }
}

struct RelocEntry {
    virtual_address: u32,
    symbol_table_index: u32,
    typ: u16,
}

fn align_up(val: u32, align: u32) -> u32 {
    (val + align - 1) & !(align - 1)
}

fn align32(val: i32) -> i32 {
    (val + 31) & !31
}
