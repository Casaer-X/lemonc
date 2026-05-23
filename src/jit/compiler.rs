use crate::ast::node::*;
use crate::jit::bytecode::*;
use std::collections::HashMap;

/// Bytecode compiler - converts AST to BytecodeModule
pub struct BytecodeCompiler {
    module: BytecodeModule,
    class_map: HashMap<String, ClassDecl>,
    func_map: HashMap<String, u32>,
    class_indices: HashMap<String, u32>,
    field_indices: HashMap<String, HashMap<String, u32>>,
    local_vars: Vec<HashMap<String, u32>>,
    local_types: Vec<HashMap<String, String>>,
    global_vars: HashMap<String, u32>,
    current_class: Option<String>,
    break_labels: Vec<u32>,
    continue_labels: Vec<u32>,
    label_counter: u32,
    label_positions: HashMap<u32, u32>, // label_id -> code_position
}

impl BytecodeCompiler {
    pub fn new() -> Self {
        Self {
            module: BytecodeModule::new(),
            class_map: HashMap::new(),
            func_map: HashMap::new(),
            class_indices: HashMap::new(),
            field_indices: HashMap::new(),
            local_vars: Vec::new(),
            local_types: Vec::new(),
            global_vars: HashMap::new(),
            current_class: None,
            break_labels: Vec::new(),
            continue_labels: Vec::new(),
            label_counter: 0,
            label_positions: HashMap::new(),
        }
    }

    pub fn compile(&mut self, ast: &Program) -> BytecodeModule {
        // First pass: collect classes and functions
        for decl in &ast.declarations {
            match decl {
                Declaration::Class(c) => {
                    self.class_map.insert(c.name.clone(), c.clone());
                }
                Declaration::Function(f) => {
                    let idx = self.module.functions.len() as u32;
                    self.func_map.insert(f.name.clone(), idx);
                }
                Declaration::Enum(e) => {
                    // Compile enum as a class with variant constructors
                    self.compile_enum(e);
                }
                _ => {}
            }
        }

        // Compile classes
        for decl in &ast.declarations {
            if let Declaration::Class(c) = decl {
                self.compile_class(c);
            }
        }

        // Compile global functions
        for decl in &ast.declarations {
            if let Declaration::Function(f) = decl {
                self.compile_function(f);
            }
        }

        // Find entry point (any class with static main)
        for (i, func) in self.module.functions.iter().enumerate() {
            if func.name.ends_with("_main") || func.name == "main" {
                self.module.entry_point = i as u32;
                break;
            }
        }

        self.module.clone()
    }

    fn compile_enum(&mut self, e: &EnumDecl) {
        // Compile enum as a class with:
        // - An int field for the ordinal value
        // - A string field for the variant name
        // - Static factory methods for each variant
        let class_idx = self.module.classes.len() as u32;
        self.class_indices.insert(e.name.clone(), class_idx);

        let mut fields = vec!["_ordinal".to_string(), "_name".to_string()];
        // Add variant data fields
        for variant in &e.variants {
            for (fi, field) in variant.fields.iter().enumerate() {
                let field_name = field.name.clone()
                    .unwrap_or_else(|| format!("_{}", fi));
                fields.push(format!("_{}_{}", variant.name, field_name));
            }
        }

        let mut field_map = HashMap::new();
        for (i, field) in fields.iter().enumerate() {
            field_map.insert(field.clone(), i as u32);
        }
        self.field_indices.insert(e.name.clone(), field_map);

        let mut methods = Vec::new();
        // Compile constructor
        let ctor_idx = self.compile_enum_ctor(&e.name, &fields);
        methods.push(ctor_idx);

        // Compile variant factory methods
        for variant in &e.variants {
            let factory_idx = self.compile_enum_variant_factory(&e.name, variant, &e.variants);
            methods.push(factory_idx);
        }

        self.module.classes.push(BytecodeClass {
            name: e.name.clone(),
            parent: None,
            fields,
            methods,
            vtable: Vec::new(),
        });
    }

    fn compile_enum_ctor(&mut self, enum_name: &str, fields: &[String]) -> u32 {
        let mangled = format!("{}_ctor", enum_name);
        let mut func = BytecodeFunction {
            name: mangled.clone(),
            params: vec!["self".to_string(), "_ordinal".to_string(), "_name".to_string()],
            locals: 3,
            code: Vec::new(),
            is_static: false,
            class_name: Some(enum_name.to_string()),
        };

        self.local_vars.push(HashMap::new());
        self.local_types.push(HashMap::new());
        self.label_positions.clear();
        self.current_class = Some(enum_name.to_string());

        self.local_vars.last_mut().unwrap().insert("self".to_string(), 0);
        self.local_vars.last_mut().unwrap().insert("_ordinal".to_string(), 1);
        self.local_vars.last_mut().unwrap().insert("_name".to_string(), 2);

        // self._ordinal = ordinal
        func.code.push(Bytecode::LoadLocal(0)); // self
        func.code.push(Bytecode::LoadLocal(1)); // ordinal
        func.code.push(Bytecode::StoreField(0));

        // self._name = name
        func.code.push(Bytecode::LoadLocal(0)); // self
        func.code.push(Bytecode::LoadLocal(2)); // name
        func.code.push(Bytecode::StoreField(1));

        func.code.push(Bytecode::PushNull);
        func.code.push(Bytecode::Return);

        self.patch_all_labels(&mut func);

        let idx = self.module.add_function(func);
        self.func_map.insert(mangled, idx);

        self.local_vars.pop();
        self.local_types.pop();
        self.current_class = None;
        idx
    }

    fn compile_enum_variant_factory(
        &mut self,
        enum_name: &str,
        variant: &EnumVariant,
        all_variants: &[EnumVariant],
    ) -> u32 {
        let mangled = format!("{}_{}", enum_name, variant.name);
        let param_count = variant.fields.len() as u32 + 2; // ordinal + name + data fields

        let mut func = BytecodeFunction {
            name: mangled.clone(),
            params: vec![],
            locals: 0,
            code: Vec::new(),
            is_static: true,
            class_name: Some(enum_name.to_string()),
        };

        self.local_vars.push(HashMap::new());
        self.local_types.push(HashMap::new());
        self.label_positions.clear();
        self.current_class = Some(enum_name.to_string());

        // Find ordinal for this variant
        let ordinal = all_variants.iter().position(|v| v.name == variant.name).unwrap_or(0) as i64;

        // Create new enum instance
        if let Some(&class_idx) = self.class_indices.get(enum_name) {
            func.code.push(Bytecode::New(class_idx));
        }

        // Push ordinal
        func.code.push(Bytecode::PushConst(ordinal));
        // Push variant name
        let name_idx = self.module.add_string(&variant.name);
        func.code.push(Bytecode::PushString(name_idx));

        // Push variant data fields
        for (fi, field) in variant.fields.iter().enumerate() {
            let local_idx = func.locals;
            func.locals += 1;
            let field_name = field.name.clone()
                .unwrap_or_else(|| format!("_{}", fi));
            self.local_vars.last_mut().unwrap().insert(field_name.clone(), local_idx);
            func.params.push(field_name);
            func.code.push(Bytecode::LoadLocal(local_idx));
        }

        // Call constructor
        let ctor_mangled = format!("{}_ctor", enum_name);
        if let Some(&idx) = self.func_map.get(&ctor_mangled) {
            func.code.push(Bytecode::CallMethod(idx, param_count));
            func.code.push(Bytecode::Pop); // pop ctor return
        }

        func.code.push(Bytecode::Return);

        self.patch_all_labels(&mut func);

        let idx = self.module.add_function(func);
        self.func_map.insert(mangled, idx);

        self.local_vars.pop();
        self.local_types.pop();
        self.current_class = None;
        idx
    }

    fn compile_class(&mut self, c: &ClassDecl) {
        let class_idx = self.module.classes.len() as u32;
        self.class_indices.insert(c.name.clone(), class_idx);

        let mut fields = Vec::new();
        let mut field_map = HashMap::new();
        let mut field_idx = 0u32;

        // Parent fields first
        if let Some(TypeRef::Named(parent_name, _)) = &c.extends {
            if let Some(parent_class) = self.class_map.get(parent_name) {
                for m in &parent_class.members {
                    if let ClassMember::Field(f) = m {
                        fields.push(f.name.clone());
                        field_map.insert(f.name.clone(), field_idx);
                        field_idx += 1;
                    }
                }
            }
        }

        // Own fields
        for m in &c.members {
            if let ClassMember::Field(f) = m {
                fields.push(f.name.clone());
                field_map.insert(f.name.clone(), field_idx);
                field_idx += 1;
            }
        }

        self.field_indices.insert(c.name.clone(), field_map);

        let mut methods = Vec::new();
        let mut vtable = Vec::new();

        for m in &c.members {
            if let ClassMember::Method(method) = m {
                let func_idx = self.compile_method(method, &c.name);
                methods.push(func_idx);
                if method.modifiers.iter().any(|m| matches!(m, MethodModifier::Virtual)) {
                    vtable.push(func_idx);
                }
            }
        }

        self.module.classes.push(BytecodeClass {
            name: c.name.clone(),
            parent: c.extends.as_ref().and_then(|t| {
                if let TypeRef::Named(n, _) = t { Some(n.clone()) } else { None }
            }),
            fields,
            methods,
            vtable,
        });
    }

    fn compile_method(&mut self, method: &MethodDecl, class_name: &str) -> u32 {
        let is_static = method.modifiers.iter().any(|m| matches!(m, MethodModifier::Static));
        let mangled = if is_static && method.name == "main" {
            "main".to_string()
        } else {
            mangle_method_name(class_name, &method.name, &method.params)
        };

        let mut func = BytecodeFunction {
            name: mangled,
            params: method.params.iter().map(|p| p.name.clone()).collect(),
            locals: 0,
            code: Vec::new(),
            is_static,
            class_name: Some(class_name.to_string()),
        };

        self.local_vars.push(HashMap::new());
        self.local_types.push(HashMap::new());
        self.label_positions.clear();
        self.current_class = Some(class_name.to_string());

        // Map parameters to local indices
        let mut local_idx = 0u32;
        if !is_static {
            // 'self' is parameter 0
            self.local_vars.last_mut().unwrap().insert("self".to_string(), local_idx);
            self.local_types.last_mut().unwrap().insert("self".to_string(), class_name.to_string());
            local_idx += 1;
        }
        for p in &method.params {
            self.local_vars.last_mut().unwrap().insert(p.name.clone(), local_idx);
            if let TypeRef::Named(tn, _) = &p.param_type {
                self.local_types.last_mut().unwrap().insert(p.name.clone(), tn.clone());
            }
            local_idx += 1;
        }

        if let Some(body) = &method.body {
            self.compile_block(&mut func, body);
        }

        // Ensure function ends with return
        if func.code.is_empty() || !matches!(func.code.last(), Some(Bytecode::Return)) {
            func.code.push(Bytecode::PushNull);
            func.code.push(Bytecode::Return);
        }

        // Patch all labels in the function
        self.patch_all_labels(&mut func);

        // locals should be at least as large as the highest local index used
        func.locals = func.locals.max(local_idx);
        let idx = self.module.add_function(func);
        self.func_map.insert(
            mangle_method_name(class_name, &method.name, &method.params),
            idx,
        );

        self.local_vars.pop();
        self.local_types.pop();
        self.current_class = None;
        idx
    }

    fn compile_function(&mut self, f: &FunctionDecl) {
        let mut func = BytecodeFunction {
            name: f.name.clone(),
            params: f.params.iter().map(|p| p.name.clone()).collect(),
            locals: 0,
            code: Vec::new(),
            is_static: true,
            class_name: None,
        };

        self.local_vars.push(HashMap::new());
        self.local_types.push(HashMap::new());
        self.label_positions.clear();

        let mut local_idx = 0u32;
        for p in &f.params {
            self.local_vars.last_mut().unwrap().insert(p.name.clone(), local_idx);
            local_idx += 1;
        }

        self.compile_block(&mut func, &f.body);

        if func.code.is_empty() || !matches!(func.code.last(), Some(Bytecode::Return)) {
            func.code.push(Bytecode::PushNull);
            func.code.push(Bytecode::Return);
        }

        // Patch all labels in the function
        self.patch_all_labels(&mut func);

        // locals should be at least as large as the highest local index used
        func.locals = func.locals.max(local_idx);
        let idx = self.module.add_function(func);
        self.func_map.insert(f.name.clone(), idx);

        self.local_vars.pop();
        self.local_types.pop();
    }

    fn compile_block(&mut self, func: &mut BytecodeFunction, block: &Block) {
        for stmt in &block.statements {
            self.compile_stmt(func, stmt);
        }
    }

    fn compile_stmt(&mut self, func: &mut BytecodeFunction, stmt: &Stmt) {
        match stmt {
            Stmt::VarDecl(var) => {
                let idx = func.locals;
                func.locals += 1;
                self.local_vars.last_mut().unwrap().insert(var.name.clone(), idx);
                let type_name = match &var.var_type {
                    TypeRef::Named(tn, _) => Some(tn.clone()),
                    TypeRef::Primitive(PrimitiveType::Int) => Some("int".to_string()),
                    TypeRef::Primitive(PrimitiveType::Void) => Some("void".to_string()),
                    TypeRef::Primitive(PrimitiveType::Bool) => Some("bool".to_string()),
                    TypeRef::Primitive(PrimitiveType::Float) => Some("float".to_string()),
                    TypeRef::Primitive(PrimitiveType::Double) => Some("double".to_string()),
                    _ => None,
                };
                if let Some(tn) = type_name {
                    self.local_types.last_mut().unwrap().insert(var.name.clone(), tn);
                }

                if let Some(init) = &var.initializer {
                    self.compile_expr(func, init);
                } else {
                    func.code.push(Bytecode::PushNull);
                }
                func.code.push(Bytecode::StoreLocal(idx));
            }
            Stmt::Return(expr) => {
                if let Some(e) = expr {
                    self.compile_expr(func, e);
                } else {
                    func.code.push(Bytecode::PushNull);
                }
                func.code.push(Bytecode::Return);
            }
            Stmt::Expr(expr) => {
                self.compile_expr(func, expr);
                func.code.push(Bytecode::Pop);
            }
            Stmt::If(cond, then_branch, else_branch) => {
                self.compile_expr(func, cond);
                let else_label = self.new_label();
                let end_label = self.new_label();

                func.code.push(Bytecode::JumpIfNot(else_label));
                self.compile_stmt(func, then_branch);
                func.code.push(Bytecode::Jump(end_label));
                self.emit_label(func, else_label);
                if let Some(else_stmt) = else_branch {
                    self.compile_stmt(func, else_stmt);
                }
                self.emit_label(func, end_label);
            }
            Stmt::While(cond, body) => {
                let start_label = self.new_label();
                let end_label = self.new_label();

                self.break_labels.push(end_label);
                self.continue_labels.push(start_label);

                self.emit_label(func, start_label);
                self.compile_expr(func, cond);
                func.code.push(Bytecode::JumpIfNot(end_label));
                self.compile_stmt(func, body);
                func.code.push(Bytecode::Jump(start_label));
                self.emit_label(func, end_label);

                self.break_labels.pop();
                self.continue_labels.pop();
            }
            Stmt::For(init, cond, update, body) => {
                let start_label = self.new_label();
                let end_label = self.new_label();

                self.break_labels.push(end_label);
                self.continue_labels.push(start_label);

                if let Some(stmt) = init {
                    self.compile_stmt(func, stmt.as_ref());
                }
                self.emit_label(func, start_label);
                if let Some(e) = cond {
                    self.compile_expr(func, e);
                    func.code.push(Bytecode::JumpIfNot(end_label));
                }
                self.compile_stmt(func, body);
                if let Some(e) = update {
                    self.compile_expr(func, e);
                    func.code.push(Bytecode::Pop);
                }
                func.code.push(Bytecode::Jump(start_label));
                self.emit_label(func, end_label);

                self.break_labels.pop();
                self.continue_labels.pop();
            }
            Stmt::Block(block) => {
                self.compile_block(func, block);
            }
            Stmt::Break => {
                if let Some(&label) = self.break_labels.last() {
                    func.code.push(Bytecode::Jump(label));
                }
            }
            Stmt::Continue => {
                if let Some(&label) = self.continue_labels.last() {
                    func.code.push(Bytecode::Jump(label));
                }
            }
            Stmt::ForEach(elem_type, elem_name, iterable, body) => {
                // ForEach: for (Type name : iterable) { body }
                // Compile as: get iterator, loop with hasNext/next
                // Simplified: iterate using ArrayLen + ArrayGet
                let iter_local = func.locals;
                func.locals += 1; // array reference
                let idx_local = func.locals;
                func.locals += 1; // loop index
                let elem_local = func.locals;
                func.locals += 1; // element variable

                // Store iterable
                self.compile_expr(func, iterable);
                func.code.push(Bytecode::StoreLocal(iter_local));
                // Initialize index = 0
                func.code.push(Bytecode::PushConst(0));
                func.code.push(Bytecode::StoreLocal(idx_local));

                let start_label = self.new_label();
                let end_label = self.new_label();

                self.break_labels.push(end_label);
                self.continue_labels.push(start_label);

                self.emit_label(func, start_label);
                // Check: idx < array.length
                func.code.push(Bytecode::LoadLocal(iter_local));
                func.code.push(Bytecode::ArrayLen);
                func.code.push(Bytecode::LoadLocal(idx_local));
                func.code.push(Bytecode::Lt);
                func.code.push(Bytecode::JumpIfNot(end_label));

                // Get element: elem = array[idx]
                func.code.push(Bytecode::LoadLocal(iter_local));
                func.code.push(Bytecode::LoadLocal(idx_local));
                func.code.push(Bytecode::ArrayGet);
                func.code.push(Bytecode::StoreLocal(elem_local));

                // Register element variable name
                self.local_vars.last_mut().unwrap().insert(elem_name.clone(), elem_local);
                if let TypeRef::Named(tn, _) = elem_type {
                    self.local_types.last_mut().unwrap().insert(elem_name.clone(), tn.clone());
                }

                // Compile body
                self.compile_stmt(func, body);

                // Increment index
                func.code.push(Bytecode::LoadLocal(idx_local));
                func.code.push(Bytecode::PushConst(1));
                func.code.push(Bytecode::Add);
                func.code.push(Bytecode::StoreLocal(idx_local));
                func.code.push(Bytecode::Jump(start_label));

                self.emit_label(func, end_label);

                self.break_labels.pop();
                self.continue_labels.pop();
            }
            Stmt::Switch(scrutinee, cases, default) => {
                // Switch: compile scrutinee, then compare each case
                self.compile_expr(func, scrutinee);
                let end_label = self.new_label();
                let mut case_labels = Vec::new();
                let break_labels_backup = self.break_labels.clone();
                self.break_labels.push(end_label);

                for _ in cases {
                    case_labels.push(self.new_label());
                }

                // Generate comparison chain
                for (i, case) in cases.iter().enumerate() {
                    for pattern in &case.patterns {
                        func.code.push(Bytecode::Dup); // duplicate scrutinee
                        self.compile_expr(func, pattern);
                        func.code.push(Bytecode::Eq);
                        func.code.push(Bytecode::JumpIf(case_labels[i]));
                    }
                }

                // Default case
                if let Some(default_block) = default {
                    self.compile_block(func, default_block);
                }
                func.code.push(Bytecode::Jump(end_label));

                // Case bodies
                for (i, case) in cases.iter().enumerate() {
                    self.emit_label(func, case_labels[i]);
                    self.compile_block(func, &case.body);
                    // Fall through to next case (no automatic break)
                }

                self.emit_label(func, end_label);
                func.code.push(Bytecode::Pop); // pop scrutinee

                self.break_labels = break_labels_backup;
            }
            Stmt::Try(_, _, _) => {
                // TODO: Exception handling - requires exception table support
                // For now, skip try/catch/finally
            }
        }
    }

    fn compile_expr(&mut self, func: &mut BytecodeFunction, expr: &Expr) {
        match expr {
            Expr::IntegerLiteral(v) => {
                func.code.push(Bytecode::PushConst(*v));
            }
            Expr::FloatLiteral(v) => {
                func.code.push(Bytecode::PushFloat(*v));
            }
            Expr::StringLiteral(s) => {
                let idx = self.module.add_string(s);
                func.code.push(Bytecode::PushString(idx));
            }
            Expr::CharLiteral(c) => {
                func.code.push(Bytecode::PushConst(*c as i64));
            }
            Expr::BoolLiteral(b) => {
                func.code.push(Bytecode::PushBool(*b));
            }
            Expr::Null => {
                func.code.push(Bytecode::PushNull);
            }
            Expr::This => {
                if let Some(idx) = self.local_vars.last().unwrap().get("self") {
                    func.code.push(Bytecode::LoadLocal(*idx));
                }
            }
            Expr::Super => {
                if let Some(idx) = self.local_vars.last().unwrap().get("self") {
                    func.code.push(Bytecode::LoadLocal(*idx));
                }
            }
            Expr::Variable(name) => {
                if let Some(idx) = self.local_vars.last().unwrap().get(name) {
                    func.code.push(Bytecode::LoadLocal(*idx));
                } else if let Some(idx) = self.global_vars.get(name) {
                    func.code.push(Bytecode::LoadGlobal(*idx));
                } else {
                    // Try as field access on 'this'
                    if let Some(class_name) = &self.current_class {
                        if let Some(field_map) = self.field_indices.get(class_name) {
                            if let Some(&idx) = field_map.get(name) {
                                if let Some(self_idx) = self.local_vars.last().unwrap().get("self") {
                                    func.code.push(Bytecode::LoadLocal(*self_idx));
                                    func.code.push(Bytecode::LoadField(idx));
                                }
                            }
                        }
                    }
                }
            }
            Expr::BinaryOp(op, left, right) => {
                if matches!(op, BinaryOp::NullCoalesce) {
                    // a ?? b => if a != null then a else b
                    self.compile_expr(func, left);
                    func.code.push(Bytecode::Dup); // duplicate a
                    let not_null_label = self.new_label();
                    let end_label = self.new_label();
                    func.code.push(Bytecode::JumpIf(not_null_label)); // if a is truthy, skip b
                    func.code.push(Bytecode::Pop); // pop the duplicated a (it was null)
                    self.compile_expr(func, right);
                    func.code.push(Bytecode::Jump(end_label));
                    self.emit_label(func, not_null_label);
                    // a is not null, keep the duplicated value, pop the extra
                    // Actually we have [a, a] on stack, we want just [a]
                    func.code.push(Bytecode::Swap);
                    func.code.push(Bytecode::Pop); // pop the second a, keep first
                    self.emit_label(func, end_label);
                } else {
                    self.compile_expr(func, left);
                    self.compile_expr(func, right);
                    func.code.push(binop_to_bytecode(op));
                }
            }
            Expr::UnaryOp(op, operand) => {
                match op {
                    UnaryOp::Minus => {
                        self.compile_expr(func, operand);
                        func.code.push(Bytecode::Neg);
                    }
                    UnaryOp::Plus => {
                        self.compile_expr(func, operand);
                        // Plus is a no-op
                    }
                    UnaryOp::Not => {
                        self.compile_expr(func, operand);
                        func.code.push(Bytecode::Not);
                    }
                    UnaryOp::BitNot => {
                        self.compile_expr(func, operand);
                        func.code.push(Bytecode::BitNot);
                    }
                    UnaryOp::Deref => {
                        self.compile_expr(func, operand);
                        // Deref: treat pointer as the value itself (no-op in bytecode)
                    }
                    UnaryOp::AddressOf => {
                        self.compile_expr(func, operand);
                        // AddressOf: in bytecode, values are already references
                    }
                    UnaryOp::PreInc => {
                        // ++x => x = x + 1; evaluate to new value
                        if let Expr::Variable(name) = operand.as_ref() {
                            if let Some(&idx) = self.local_vars.last().unwrap().get(name) {
                                func.code.push(Bytecode::LoadLocal(idx));
                                func.code.push(Bytecode::PushConst(1));
                                func.code.push(Bytecode::Add);
                                func.code.push(Bytecode::Dup);
                                func.code.push(Bytecode::StoreLocal(idx));
                            }
                        } else if let Expr::FieldAccess(obj, field) = operand.as_ref() {
                            self.compile_expr(func, obj);
                            if let Some(class_name) = self.infer_class(obj) {
                                if let Some(field_map) = self.field_indices.get(&class_name) {
                                    if let Some(&idx) = field_map.get(field) {
                                        func.code.push(Bytecode::Dup);
                                        func.code.push(Bytecode::LoadField(idx));
                                        func.code.push(Bytecode::PushConst(1));
                                        func.code.push(Bytecode::Add);
                                        func.code.push(Bytecode::StoreField(idx));
                                    }
                                }
                            }
                        }
                    }
                    UnaryOp::PreDec => {
                        // --x => x = x - 1; evaluate to new value
                        if let Expr::Variable(name) = operand.as_ref() {
                            if let Some(&idx) = self.local_vars.last().unwrap().get(name) {
                                func.code.push(Bytecode::LoadLocal(idx));
                                func.code.push(Bytecode::PushConst(1));
                                func.code.push(Bytecode::Sub);
                                func.code.push(Bytecode::Dup);
                                func.code.push(Bytecode::StoreLocal(idx));
                            }
                        } else if let Expr::FieldAccess(obj, field) = operand.as_ref() {
                            self.compile_expr(func, obj);
                            if let Some(class_name) = self.infer_class(obj) {
                                if let Some(field_map) = self.field_indices.get(&class_name) {
                                    if let Some(&idx) = field_map.get(field) {
                                        func.code.push(Bytecode::Dup);
                                        func.code.push(Bytecode::LoadField(idx));
                                        func.code.push(Bytecode::PushConst(1));
                                        func.code.push(Bytecode::Sub);
                                        func.code.push(Bytecode::StoreField(idx));
                                    }
                                }
                            }
                        }
                    }
                    UnaryOp::PostInc => {
                        // x++ => temp = x; x = x + 1; evaluate to temp
                        if let Expr::Variable(name) = operand.as_ref() {
                            if let Some(&idx) = self.local_vars.last().unwrap().get(name) {
                                func.code.push(Bytecode::LoadLocal(idx));
                                func.code.push(Bytecode::Dup);
                                func.code.push(Bytecode::PushConst(1));
                                func.code.push(Bytecode::Add);
                                func.code.push(Bytecode::StoreLocal(idx));
                            }
                        }
                    }
                    UnaryOp::PostDec => {
                        // x-- => temp = x; x = x - 1; evaluate to temp
                        if let Expr::Variable(name) = operand.as_ref() {
                            if let Some(&idx) = self.local_vars.last().unwrap().get(name) {
                                func.code.push(Bytecode::LoadLocal(idx));
                                func.code.push(Bytecode::Dup);
                                func.code.push(Bytecode::PushConst(1));
                                func.code.push(Bytecode::Sub);
                                func.code.push(Bytecode::StoreLocal(idx));
                            }
                        }
                    }
                }
            }
            Expr::Assignment(target, value) => {
                match target.as_ref() {
                    Expr::ArrayAccess(arr, idx_expr) => {
                        // ArraySet expects stack: [arr, idx, value] (pops value, idx, arr)
                        self.compile_expr(func, arr);
                        self.compile_expr(func, idx_expr);
                        self.compile_expr(func, value);
                        func.code.push(Bytecode::ArraySet);
                    }
                    _ => {
                        self.compile_expr(func, value);
                        match target.as_ref() {
                            Expr::Variable(name) => {
                                if let Some(idx) = self.local_vars.last().unwrap().get(name) {
                                    func.code.push(Bytecode::StoreLocal(*idx));
                                } else if let Some(idx) = self.global_vars.get(name) {
                                    func.code.push(Bytecode::StoreGlobal(*idx));
                                }
                            }
                            Expr::FieldAccess(obj, field) => {
                                self.compile_expr(func, obj);
                                func.code.push(Bytecode::Swap);
                                if let Some(class_name) = self.infer_class(obj) {
                                    if let Some(field_map) = self.field_indices.get(&class_name) {
                                        if let Some(&idx) = field_map.get(field) {
                                            func.code.push(Bytecode::StoreField(idx));
                                        }
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Expr::Call(callee, args) => {
                match callee.as_ref() {
                    Expr::Variable(name) => {
                        // Handle builtin functions
                        if name == "printf" {
                            for arg in args {
                                self.compile_expr(func, arg);
                            }
                            func.code.push(Bytecode::Printf(args.len() as u32));
                        } else if name == "println" || name == "print" {
                            for arg in args {
                                self.compile_expr(func, arg);
                            }
                            if name == "println" || args.len() == 1 {
                                func.code.push(Bytecode::Println);
                            } else {
                                func.code.push(Bytecode::Print);
                            }
                        } else {
                            for arg in args {
                                self.compile_expr(func, arg);
                            }
                            if let Some(&idx) = self.func_map.get(name) {
                                func.code.push(Bytecode::Call(idx, args.len() as u32));
                            }
                        }
                    }
                    Expr::FieldAccess(obj, method) => {
                        self.compile_expr(func, obj);
                        for arg in args {
                            self.compile_expr(func, arg);
                        }
                        if let Some(class_name) = self.infer_class(obj) {
                            // Try all possible mangled names
                            let mangled = format!("{}_{}", class_name, method);
                            // Find method with matching param count
                            let mut found = false;
                            for (key, &idx) in &self.func_map {
                                if key.starts_with(&mangled) {
                                    // Count params from mangled name
                                    let param_count = key.matches('_').count().saturating_sub(1);
                                    if param_count == args.len() {
                                        func.code.push(Bytecode::CallMethod(idx, args.len() as u32));
                                        found = true;
                                        break;
                                    }
                                }
                            }
                            if !found {
                                // Fallback: try exact match
                                if let Some(&idx) = self.func_map.get(&mangled) {
                                    func.code.push(Bytecode::CallMethod(idx, args.len() as u32));
                                }
                            }
                        }
                    }
                    _ => {
                        // Indirect call: compile callee as expression, push args
                        // For now, compile callee and args but we can't do an indirect
                        // call with current bytecode - push null as placeholder
                        self.compile_expr(func, callee);
                        for arg in args {
                            self.compile_expr(func, arg);
                        }
                        func.code.push(Bytecode::PushNull);
                    }
                }
            }
            Expr::MethodCall(obj, method, args) => {
                self.compile_expr(func, obj);
                for arg in args {
                    self.compile_expr(func, arg);
                }
                if let Some(class_name) = self.infer_class(obj) {
                    let mangled = format!("{}_{}", class_name, method);
                    let mut found = false;
                    for (key, &idx) in &self.func_map {
                        if key.starts_with(&mangled) {
                            let param_count = key.matches('_').count().saturating_sub(1);
                            if param_count == args.len() {
                                func.code.push(Bytecode::CallMethod(idx, args.len() as u32));
                                found = true;
                                break;
                            }
                        }
                    }
                    if !found {
                        if let Some(&idx) = self.func_map.get(&mangled) {
                            func.code.push(Bytecode::CallMethod(idx, args.len() as u32));
                        }
                    }
                }
            }
            Expr::FieldAccess(obj, field) => {
                self.compile_expr(func, obj);
                if let Some(class_name) = self.infer_class(obj) {
                    if let Some(field_map) = self.field_indices.get(&class_name) {
                        if let Some(&idx) = field_map.get(field) {
                            func.code.push(Bytecode::LoadField(idx));
                        }
                    }
                }
            }
            Expr::New(class_name, _type_args, args) => {
                // Create object
                if let Some(&idx) = self.class_indices.get(class_name) {
                    func.code.push(Bytecode::New(idx));
                    func.code.push(Bytecode::Dup); // Duplicate for constructor call
                }
                // Push args for constructor
                for arg in args {
                    self.compile_expr(func, arg);
                }
                // Call constructor
                let ctor_mangled = format!("{}_ctor", class_name);
                if let Some(&idx) = self.func_map.get(&ctor_mangled) {
                    func.code.push(Bytecode::CallMethod(idx, args.len() as u32));
                    func.code.push(Bytecode::Pop); // Pop constructor return value
                }
            }
            Expr::Delete(obj) => {
                self.compile_expr(func, obj);
                func.code.push(Bytecode::Delete);
            }
            Expr::Cast(target_type, inner) => {
                self.compile_expr(func, inner);
                // Use the Cast instruction with a type index from string pool
                let type_idx = self.module.add_string(&format!("{:?}", target_type));
                func.code.push(Bytecode::Cast(type_idx));
            }
            Expr::InstanceOf(obj, target_type) => {
                self.compile_expr(func, obj);
                let type_idx = self.module.add_string(&format!("{:?}", target_type));
                func.code.push(Bytecode::InstanceOf(type_idx));
            }
            Expr::Sizeof(ty) => {
                let size = match ty {
                    TypeRef::Primitive(PrimitiveType::Int) => 8,
                    TypeRef::Primitive(PrimitiveType::Float) => 8,
                    TypeRef::Primitive(PrimitiveType::Double) => 8,
                    TypeRef::Primitive(PrimitiveType::Bool) => 1,
                    TypeRef::Primitive(PrimitiveType::Void) => 0,
                    TypeRef::Named(_, _) => 8, // pointer size
                    TypeRef::Array(_) => 16, // pointer + length
                    _ => 8,
                };
                func.code.push(Bytecode::PushConst(size));
            }
            Expr::TypeId(obj) => {
                self.compile_expr(func, obj);
                func.code.push(Bytecode::TypeId);
            }
            Expr::Ternary(cond, then_expr, else_expr) => {
                self.compile_expr(func, cond);
                let else_label = self.new_label();
                let end_label = self.new_label();

                func.code.push(Bytecode::JumpIfNot(else_label));
                self.compile_expr(func, then_expr);
                func.code.push(Bytecode::Jump(end_label));
                self.emit_label(func, else_label);
                self.compile_expr(func, else_expr);
                self.emit_label(func, end_label);
            }
            Expr::ArrayAccess(arr, idx) => {
                self.compile_expr(func, arr);
                self.compile_expr(func, idx);
                func.code.push(Bytecode::ArrayGet);
            }
            Expr::Lambda(_, _) => {
                // TODO: Lambda support
                func.code.push(Bytecode::PushNull);
            }
            Expr::Throw(e) => {
                self.compile_expr(func, e);
                // TODO: Exception handling
            }
            Expr::Match(scrutinee, arms) => {
                // Match expression: compile scrutinee, then check each arm
                self.compile_expr(func, scrutinee);
                let end_label = self.new_label();
                let mut arm_labels = Vec::new();
                for _ in arms {
                    arm_labels.push(self.new_label());
                }

                for (i, arm) in arms.iter().enumerate() {
                    match &arm.pattern {
                        MatchPattern::Wildcard => {
                            // Always matches - jump to this arm's body
                            func.code.push(Bytecode::JumpIf(arm_labels[i]));
                        }
                        MatchPattern::Literal(lit) => {
                            func.code.push(Bytecode::Dup);
                            self.compile_expr(func, lit);
                            func.code.push(Bytecode::Eq);
                            func.code.push(Bytecode::JumpIf(arm_labels[i]));
                        }
                        MatchPattern::Variant(name, _bindings) => {
                            // For enum variant matching, check TypeId
                            func.code.push(Bytecode::Dup);
                            let type_idx = self.module.add_string(name);
                            func.code.push(Bytecode::InstanceOf(type_idx));
                            func.code.push(Bytecode::JumpIf(arm_labels[i]));
                        }
                        MatchPattern::Or(patterns) => {
                            for sub_pattern in patterns {
                                match sub_pattern {
                                    MatchPattern::Literal(lit) => {
                                        func.code.push(Bytecode::Dup);
                                        self.compile_expr(func, lit);
                                        func.code.push(Bytecode::Eq);
                                        func.code.push(Bytecode::JumpIf(arm_labels[i]));
                                    }
                                    MatchPattern::Variant(name, _) => {
                                        func.code.push(Bytecode::Dup);
                                        let type_idx = self.module.add_string(name);
                                        func.code.push(Bytecode::InstanceOf(type_idx));
                                        func.code.push(Bytecode::JumpIf(arm_labels[i]));
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                // No match - push null
                func.code.push(Bytecode::Pop);
                func.code.push(Bytecode::PushNull);
                func.code.push(Bytecode::Jump(end_label));

                // Compile each arm's body
                for (i, arm) in arms.iter().enumerate() {
                    self.emit_label(func, arm_labels[i]);
                    // Pop the scrutinee (only if not already popped by wildcard)
                    if !matches!(&arm.pattern, MatchPattern::Wildcard) {
                        func.code.push(Bytecode::Pop);
                    }
                    match &arm.body {
                        MatchBody::Expr(e) => self.compile_expr(func, e),
                        MatchBody::Block(b) => self.compile_block(func, b),
                    }
                    func.code.push(Bytecode::Jump(end_label));
                }

                self.emit_label(func, end_label);
            }
        }
    }

    fn infer_class(&self, expr: &Expr) -> Option<String> {
        match expr {
            Expr::This => self.current_class.clone(),
            Expr::Variable(name) => {
                if name == "self" {
                    self.current_class.clone()
                } else {
                    // Look up local variable type
                    for scope in self.local_types.iter().rev() {
                        if let Some(t) = scope.get(name) {
                            return Some(t.clone());
                        }
                    }
                    None
                }
            }
            Expr::New(cn, _, _) => Some(cn.clone()),
            Expr::FieldAccess(obj, _) => self.infer_class(obj),
            Expr::Match(_, _) => None,
            _ => None,
        }
    }

    fn new_label(&mut self) -> u32 {
        let id = self.label_counter;
        self.label_counter += 1;
        id
    }

    fn emit_label(&mut self, func: &mut BytecodeFunction, label: u32) {
        // Store the current code position for this label
        let pos = func.code.len() as u32;
        self.label_positions.insert(label, pos);
    }

    fn patch_all_labels(&mut self, func: &mut BytecodeFunction) {
        // Second pass: patch all jump instructions with actual label positions
        for instr in func.code.iter_mut() {
            match instr {
                Bytecode::Jump(ref mut target) |
                Bytecode::JumpIf(ref mut target) |
                Bytecode::JumpIfNot(ref mut target) => {
                    if let Some(&pos) = self.label_positions.get(target) {
                        *target = pos;
                    }
                }
                _ => {}
            }
        }
    }
}

impl Default for BytecodeCompiler {
    fn default() -> Self {
        Self::new()
    }
}
