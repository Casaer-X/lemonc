use crate::ast::node::*;
use crate::codegen::bytecode::*;
use std::collections::HashMap;

pub struct BytecodeGen {
    program: BytecodeProgram,
    current_function: Option<String>,
    local_map: HashMap<String, u16>,
    label_map: HashMap<String, usize>,
    pending_jumps: Vec<(usize, String)>,
    current_class: Option<String>,
    field_map: HashMap<String, Vec<(String, u16)>>,
    class_methods: HashMap<String, HashMap<String, u16>>,
}

impl BytecodeGen {
    pub fn new() -> Self {
        Self {
            program: BytecodeProgram::new(),
            current_function: None,
            local_map: HashMap::new(),
            label_map: HashMap::new(),
            pending_jumps: Vec::new(),
            current_class: None,
            field_map: HashMap::new(),
            class_methods: HashMap::new(),
        }
    }

    pub fn generate(&mut self, ast: &Program) -> BytecodeProgram {
        self.collect_classes(ast);
        self.generate_classes(ast);
        self.generate_functions(ast);
        self.resolve_jumps();
        self.program.clone()
    }

    fn collect_classes(&mut self, ast: &Program) {
        for decl in &ast.declarations {
            if let Declaration::Class(c) = decl {
                let mut fields = Vec::new();
                let mut field_idx = 0u16;
                for m in &c.members {
                    if let ClassMember::Field(f) = m {
                        fields.push((f.name.clone(), field_idx));
                        field_idx += 1;
                    }
                }
                self.field_map.insert(c.name.clone(), fields);

                let mut methods = HashMap::new();
                let mut method_idx = 0u16;
                for m in &c.members {
                    if let ClassMember::Method(method) = m {
                        methods.insert(method.name.clone(), method_idx);
                        method_idx += 1;
                    }
                }
                self.class_methods.insert(c.name.clone(), methods);
            }
        }
    }

    fn generate_classes(&mut self, ast: &Program) {
        for decl in &ast.declarations {
            if let Declaration::Class(c) = decl {
                let mut fields = Vec::new();
                let mut methods = Vec::new();
                let mut vtable = Vec::new();

                for m in &c.members {
                    match m {
                        ClassMember::Field(f) => {
                            fields.push((f.name.clone(), f.var_type.clone()));
                        }
                        ClassMember::Method(method) => {
                            let mangled = format!("{}_{}", c.name, method.name);
                            methods.push(mangled.clone());
                            if method.modifiers.iter().any(|m| matches!(m, MethodModifier::Virtual)) {
                                vtable.push(mangled);
                            }
                        }
                        _ => {}
                    }
                }

                let bc_class = BytecodeClass {
                    name: c.name.clone(),
                    parent: c.extends.as_ref().and_then(|t| {
                        if let TypeRef::Named(n, _) = t { Some(n.clone()) } else { None }
                    }),
                    fields,
                    methods,
                    vtable,
                };
                self.program.add_class(bc_class);
            }
        }
    }

    fn generate_functions(&mut self, ast: &Program) {
        for decl in &ast.declarations {
            match decl {
                Declaration::Function(f) => {
                    self.generate_function(f, None);
                }
                Declaration::Class(c) => {
                    self.current_class = Some(c.name.clone());
                    for m in &c.members {
                        match m {
                            ClassMember::Constructor(ctor) => {
                                self.generate_constructor(ctor, c);
                            }
                            ClassMember::Method(method) => {
                                self.generate_method(method, c);
                            }
                            _ => {}
                        }
                    }
                    self.current_class = None;
                }
                _ => {}
            }
        }
    }

    fn generate_function(&mut self, f: &FunctionDecl, class_name: Option<&str>) {
        let name = if let Some(cn) = class_name {
            format!("{}_{}", cn, f.name)
        } else {
            f.name.clone()
        };

        let mut bc_func = BytecodeFunction {
            name: name.clone(),
            params: f.params.iter().map(|p| p.name.clone()).collect(),
            locals: Vec::new(),
            code: Vec::new(),
            max_stack: 16,
        };

        self.current_function = Some(name.clone());
        self.local_map.clear();

        for (i, p) in f.params.iter().enumerate() {
            self.local_map.insert(p.name.clone(), i as u16);
        }

        for s in &f.body.statements {
            self.gen_stmt(&mut bc_func, s);
        }

        if bc_func.code.is_empty() || !matches!(bc_func.code.last(), Some(BytecodeOp::Ret) | Some(BytecodeOp::RetVal)) {
            bc_func.code.push(BytecodeOp::LoadNull);
            bc_func.code.push(BytecodeOp::RetVal);
        }

        self.program.add_function(bc_func);
        self.current_function = None;
    }

    fn generate_constructor(&mut self, ctor: &ConstructorDecl, c: &ClassDecl) {
        let name = format!("{}_ctor", c.name);
        let mut bc_func = BytecodeFunction {
            name: name.clone(),
            params: ctor.params.iter().map(|p| p.name.clone()).collect(),
            locals: Vec::new(),
            code: Vec::new(),
            max_stack: 16,
        };

        self.current_function = Some(name.clone());
        self.local_map.clear();
        self.local_map.insert("self".to_string(), 0);

        for (i, p) in ctor.params.iter().enumerate() {
            self.local_map.insert(p.name.clone(), (i + 1) as u16);
        }

        for s in &ctor.body.statements {
            self.gen_stmt(&mut bc_func, s);
        }

        bc_func.code.push(BytecodeOp::PushLocal(0));
        bc_func.code.push(BytecodeOp::RetVal);

        self.program.add_function(bc_func);
        self.current_function = None;
    }

    fn generate_method(&mut self, method: &MethodDecl, c: &ClassDecl) {
        let name = format!("{}_{}", c.name, method.name);
        let mut bc_func = BytecodeFunction {
            name: name.clone(),
            params: method.params.iter().map(|p| p.name.clone()).collect(),
            locals: Vec::new(),
            code: Vec::new(),
            max_stack: 16,
        };

        self.current_function = Some(name.clone());
        self.local_map.clear();

        let is_static = method.modifiers.iter().any(|m| matches!(m, MethodModifier::Static));
        if !is_static {
            self.local_map.insert("self".to_string(), 0);
        }

        let offset = if is_static { 0 } else { 1 };
        for (i, p) in method.params.iter().enumerate() {
            self.local_map.insert(p.name.clone(), (i + offset) as u16);
        }

        if let Some(body) = &method.body {
            for s in &body.statements {
                self.gen_stmt(&mut bc_func, s);
            }
        }

        if bc_func.code.is_empty() || !matches!(bc_func.code.last(), Some(BytecodeOp::Ret) | Some(BytecodeOp::RetVal)) {
            bc_func.code.push(BytecodeOp::LoadNull);
            bc_func.code.push(BytecodeOp::RetVal);
        }

        self.program.add_function(bc_func);
        self.current_function = None;
    }

    fn gen_stmt(&mut self, func: &mut BytecodeFunction, stmt: &Stmt) {
        match stmt {
            Stmt::Expression(expr) => {
                self.gen_expr(func, expr);
                func.code.push(BytecodeOp::Pop);
            }
            Stmt::VarDecl(var) => {
                let idx = func.locals.len() as u16;
                func.locals.push(var.name.clone());
                self.local_map.insert(var.name.clone(), idx);
                if let Some(init) = &var.initializer {
                    self.gen_expr(func, init);
                    func.code.push(BytecodeOp::PopLocal(idx));
                }
            }
            Stmt::Return(expr) => {
                if let Some(e) = expr {
                    self.gen_expr(func, e);
                    func.code.push(BytecodeOp::RetVal);
                } else {
                    func.code.push(BytecodeOp::Ret);
                }
            }
            Stmt::Block(block) => {
                for s in &block.statements {
                    self.gen_stmt(func, s);
                }
            }
            Stmt::If(cond, then_branch, else_branch) => {
                self.gen_expr(func, cond);
                let else_label = format!("_L_else_{}", self.label_map.len());
                let end_label = format!("_L_end_{}", self.label_map.len());
                self.label_map.insert(format!("_L_else_{}", self.label_map.len()), 0);
                self.label_map.insert(format!("_L_end_{}", self.label_map.len()), 0);

                func.code.push(BytecodeOp::Jz(0));
                let jz_pos = func.code.len() - 1;
                self.pending_jumps.push((jz_pos, else_label.clone()));

                self.gen_stmt(func, then_branch);
                func.code.push(BytecodeOp::Jmp(0));
                let jmp_pos = func.code.len() - 1;
                self.pending_jumps.push((jmp_pos, end_label.clone()));

                let else_pos = func.code.len();
                self.label_map.insert(else_label.clone(), else_pos);

                if let Some(else_stmt) = else_branch {
                    self.gen_stmt(func, else_stmt);
                }

                let end_pos = func.code.len();
                self.label_map.insert(end_label.clone(), end_pos);
            }
            Stmt::While(cond, body) => {
                let start_label = format!("_L_while_start_{}", self.label_map.len());
                let end_label = format!("_L_while_end_{}", self.label_map.len());
                let start_pos = func.code.len();
                self.label_map.insert(start_label.clone(), start_pos);

                self.gen_expr(func, cond);
                func.code.push(BytecodeOp::Jz(0));
                let jz_pos = func.code.len() - 1;
                self.pending_jumps.push((jz_pos, end_label.clone()));

                self.gen_stmt(func, body);
                func.code.push(BytecodeOp::Jmp(0));
                let jmp_pos = func.code.len() - 1;
                self.pending_jumps.push((jmp_pos, start_label.clone()));

                let end_pos = func.code.len();
                self.label_map.insert(end_label.clone(), end_pos);
            }
            Stmt::For(init, cond, update, body) => {
                if let Some(stmt) = init {
                    self.gen_stmt(func, stmt.as_ref());
                }

                let start_label = format!("_L_for_start_{}", self.label_map.len());
                let end_label = format!("_L_for_end_{}", self.label_map.len());
                let start_pos = func.code.len();
                self.label_map.insert(start_label.clone(), start_pos);

                if let Some(c) = cond {
                    self.gen_expr(func, c);
                    func.code.push(BytecodeOp::Jz(0));
                    let jz_pos = func.code.len() - 1;
                    self.pending_jumps.push((jz_pos, end_label.clone()));
                }

                self.gen_stmt(func, body);

                if let Some(u) = update {
                    self.gen_expr(func, u);
                    func.code.push(BytecodeOp::Pop);
                }

                func.code.push(BytecodeOp::Jmp(0));
                let jmp_pos = func.code.len() - 1;
                self.pending_jumps.push((jmp_pos, start_label.clone()));

                let end_pos = func.code.len();
                self.label_map.insert(end_label.clone(), end_pos);
            }
            _ => {}
        }
    }

    fn gen_expr(&mut self, func: &mut BytecodeFunction, expr: &Expr) {
        match expr {
            Expr::IntegerLiteral(v) => {
                let idx = self.program.add_constant(Constant::Int(*v));
                func.code.push(BytecodeOp::PushConst(idx));
            }
            Expr::FloatLiteral(v) => {
                let idx = self.program.add_constant(Constant::Float(*v));
                func.code.push(BytecodeOp::PushConst(idx));
            }
            Expr::StringLiteral(v) => {
                let idx = self.program.add_constant(Constant::String(v.clone()));
                func.code.push(BytecodeOp::PushConst(idx));
            }
            Expr::BoolLiteral(v) => {
                let idx = self.program.add_constant(Constant::Bool(*v));
                func.code.push(BytecodeOp::PushConst(idx));
            }
            Expr::Null => {
                func.code.push(BytecodeOp::LoadNull);
            }
            Expr::Variable(name) => {
                if name == "self" {
                    func.code.push(BytecodeOp::LoadThis);
                } else if let Some(&idx) = self.local_map.get(name) {
                    func.code.push(BytecodeOp::PushLocal(idx));
                }
            }
            Expr::This => {
                func.code.push(BytecodeOp::LoadThis);
            }
            Expr::Super => {
                func.code.push(BytecodeOp::LoadThis);
            }
            Expr::BinaryOp(op, left, right) => {
                self.gen_expr(func, left);
                self.gen_expr(func, right);
                let bc_op = match op {
                    BinaryOp::Add => BytecodeOp::Add,
                    BinaryOp::Sub => BytecodeOp::Sub,
                    BinaryOp::Mul => BytecodeOp::Mul,
                    BinaryOp::Div => BytecodeOp::Div,
                    BinaryOp::Mod => BytecodeOp::Mod,
                    BinaryOp::Eq => BytecodeOp::Eq,
                    BinaryOp::Ne => BytecodeOp::Ne,
                    BinaryOp::Lt => BytecodeOp::Lt,
                    BinaryOp::Le => BytecodeOp::Le,
                    BinaryOp::Gt => BytecodeOp::Gt,
                    BinaryOp::Ge => BytecodeOp::Ge,
                    BinaryOp::And => BytecodeOp::LAnd,
                    BinaryOp::Or => BytecodeOp::LOr,
                    BinaryOp::BitAnd => BytecodeOp::BitAnd,
                    BinaryOp::BitOr => BytecodeOp::BitOr,
                    BinaryOp::BitXor => BytecodeOp::BitXor,
                    BinaryOp::Shl => BytecodeOp::Shl,
                    BinaryOp::Shr => BytecodeOp::Shr,
                };
                func.code.push(bc_op);
            }
            Expr::UnaryOp(op, operand) => {
                self.gen_expr(func, operand);
                let bc_op = match op {
                    UnaryOp::Minus => BytecodeOp::Neg,
                    UnaryOp::Not => BytecodeOp::LNot,
                    UnaryOp::BitNot => BytecodeOp::BitNot,
                    _ => BytecodeOp::Nop,
                };
                func.code.push(bc_op);
            }
            Expr::Assignment(target, value) => {
                self.gen_expr(func, value);
                match target.as_ref() {
                    Expr::Variable(name) => {
                        if let Some(&idx) = self.local_map.get(name) {
                            func.code.push(BytecodeOp::PopLocal(idx));
                        }
                    }
                    Expr::FieldAccess(obj, field) => {
                        self.gen_expr(func, obj);
                        func.code.push(BytecodeOp::Swap);
                        if let Some(class_name) = self.infer_class(obj) {
                            if let Some(fields) = self.field_map.get(&class_name) {
                                if let Some((_, idx)) = fields.iter().find(|(n, _)| n == field) {
                                    func.code.push(BytecodeOp::SetField(*idx));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Expr::FieldAccess(obj, field) => {
                self.gen_expr(func, obj);
                if let Some(class_name) = self.infer_class(obj) {
                    if let Some(fields) = self.field_map.get(&class_name) {
                        if let Some((_, idx)) = fields.iter().find(|(n, _)| n == field) {
                            func.code.push(BytecodeOp::GetField(*idx));
                        }
                    }
                }
            }
            Expr::MethodCall(obj, method, args) => {
                self.gen_expr(func, obj);
                for arg in args {
                    self.gen_expr(func, arg);
                }
                if let Some(class_name) = self.infer_class(obj) {
                    let mangled = format!("{}_{}", class_name, method);
                    if let Some(idx) = self.program.get_function_idx(&mangled) {
                        func.code.push(BytecodeOp::Call(idx));
                    }
                }
            }
            Expr::Call(callee, args) => {
                match callee.as_ref() {
                    Expr::Variable(name) => {
                        for arg in args {
                            self.gen_expr(func, arg);
                        }
                        if let Some(idx) = self.program.get_function_idx(name) {
                            func.code.push(BytecodeOp::Call(idx));
                        } else if name == "printf" || name == "malloc" || name == "free" || name == "exit" || name == "clock" || name == "strlen" || name == "strcmp" || name == "srand" || name == "rand" {
                            let idx = self.program.add_constant(Constant::String(name.clone()));
                            func.code.push(BytecodeOp::ExternCall(idx));
                        }
                    }
                    Expr::FieldAccess(obj, method) => {
                        if let Expr::Variable(class_name) = obj.as_ref() {
                            let mangled = format!("{}_{}", class_name, method);
                            for arg in args {
                                self.gen_expr(func, arg);
                            }
                            if let Some(idx) = self.program.get_function_idx(&mangled) {
                                func.code.push(BytecodeOp::Call(idx));
                            }
                        } else {
                            self.gen_expr(func, obj);
                            for arg in args {
                                self.gen_expr(func, arg);
                            }
                        }
                    }
                    _ => {
                        self.gen_expr(func, callee);
                        for arg in args {
                            self.gen_expr(func, arg);
                        }
                    }
                }
            }
            Expr::New(class_name, _, args) => {
                for arg in args {
                    self.gen_expr(func, arg);
                }
                if let Some(idx) = self.program.get_class_idx(class_name) {
                    func.code.push(BytecodeOp::New(idx));
                }
            }
            Expr::Cast(target_type, expr) => {
                self.gen_expr(func, expr);
                if let TypeRef::Named(name, _) = target_type {
                    if let Some(idx) = self.program.get_class_idx(name) {
                        func.code.push(BytecodeOp::Cast(idx));
                    }
                }
            }
            Expr::ArrayAccess(arr, idx) => {
                self.gen_expr(func, arr);
                self.gen_expr(func, idx);
                func.code.push(BytecodeOp::ArrayGet);
            }
            Expr::Ternary(cond, then_expr, else_expr) => {
                self.gen_expr(func, cond);
                let else_label = format!("_L_tern_else_{}", self.label_map.len());
                let end_label = format!("_L_tern_end_{}", self.label_map.len());

                func.code.push(BytecodeOp::Jz(0));
                let jz_pos = func.code.len() - 1;
                self.pending_jumps.push((jz_pos, else_label.clone()));

                self.gen_expr(func, then_expr);
                func.code.push(BytecodeOp::Jmp(0));
                let jmp_pos = func.code.len() - 1;
                self.pending_jumps.push((jmp_pos, end_label.clone()));

                let else_pos = func.code.len();
                self.label_map.insert(else_label.clone(), else_pos);

                self.gen_expr(func, else_expr);

                let end_pos = func.code.len();
                self.label_map.insert(end_label.clone(), end_pos);
            }
            _ => {}
        }
    }

    fn infer_class(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::This | Expr::Super => self.current_class.clone(),
            Expr::Variable(name) => {
                if name == "self" { self.current_class.clone() } else { None }
            }
            Expr::New(class_name, _, _) => Some(class_name.clone()),
            _ => None,
        }
    }

    fn resolve_jumps(&mut self) {
        for (pos, label) in &self.pending_jumps {
            if let Some(&target) = self.label_map.get(label) {
                let offset = target as i32 - *pos as i32;
                if let BytecodeOp::Jmp(_) = &self.program.functions.last_mut().unwrap().code[*pos] {
                    self.program.functions.last_mut().unwrap().code[*pos] = BytecodeOp::Jmp(offset);
                } else if let BytecodeOp::Jz(_) = &self.program.functions.last_mut().unwrap().code[*pos] {
                    self.program.functions.last_mut().unwrap().code[*pos] = BytecodeOp::Jz(offset);
                } else if let BytecodeOp::Jnz(_) = &self.program.functions.last_mut().unwrap().code[*pos] {
                    self.program.functions.last_mut().unwrap().code[*pos] = BytecodeOp::Jnz(offset);
                }
            }
        }
        self.pending_jumps.clear();
    }
}
