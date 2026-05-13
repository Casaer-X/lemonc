use super::function::IRFunction;
use super::instruction::*;
use super::module::IRModule;
use super::type_system::IRType;
use crate::ast::node::*;
use std::collections::HashMap;

pub struct IRGenerator {
    module: IRModule,
    value_counter: u32,
    variables: HashMap<String, IRValue>,
    current_class: Option<String>,
    string_literals: Vec<String>,
}

impl IRGenerator {
    pub fn new() -> Self {
        Self {
            module: IRModule::new(),
            value_counter: 0,
            variables: HashMap::new(),
            current_class: None,
            string_literals: Vec::new(),
        }
    }

    pub fn generate(&mut self, ast: &Program) -> &IRModule {
        for decl in &ast.declarations {
            match decl {
                Declaration::Package(pkg) => {
                    self.module.name = pkg.name.clone();
                }
                Declaration::Class(class) => self.generate_class(class),
                Declaration::Function(func) => self.generate_function(func),
                Declaration::Interface(iface) => self.generate_interface(iface),
                _ => {}
            }
        }
        &self.module
    }

    pub fn string_literals(&self) -> &[String] {
        &self.string_literals
    }

    fn new_value(&mut self, typ: IRType, prefix: &str) -> IRValue {
        let id = self.value_counter;
        self.value_counter += 1;
        IRValue {
            name: format!("{}_{}", prefix, id),
            typ,
            id,
        }
    }

    fn emit(&mut self, func: &mut IRFunction, inst: IRInstruction) {
        func.push_instruction(inst);
    }

    fn generate_class(&mut self, class: &ClassDecl) {
        self.current_class = Some(class.name.clone());

        let mut ir_class = super::module::IRClass {
            name: class.name.clone(),
            instance_size: 16,
            vtable_methods: Vec::new(),
            itable_interfaces: Vec::new(),
            fields: Vec::new(),
            constructor: format!("{}_ctor", class.name),
        };

        let mut offset = 16u32;
        for member in &class.members {
            if let ClassMember::Field(field) = member {
                let field_type = self.resolve_type(&field.var_type);
                let size = field_type.size_bytes();
                ir_class.fields.push(super::module::IRField {
                    name: field.name.clone(),
                    typ: field_type,
                    offset,
                });
                offset += size;
                offset = (offset + 7) & !7;
            } else if let ClassMember::Method(method) = member {
                ir_class.vtable_methods.push(method.name.clone());
            }
        }
        ir_class.instance_size = offset;

        for iface in &class.implements {
            if let TypeRef::Named(name, _) = iface {
                ir_class.itable_interfaces.push(name.clone());
            }
        }

        let ctor_name = ir_class.constructor.clone();
        self.module.classes.push(ir_class);

        let mut ctor_func = IRFunction::new(&ctor_name, IRType::Void);
        ctor_func.add_param("this".to_string(), IRType::Object);
        self.module.functions.push(ctor_func);

        for member in &class.members {
            if let ClassMember::Method(method) = member {
                self.generate_method(method, &class.name);
            } else if let ClassMember::Constructor(ctor) = member {
                self.generate_constructor(ctor, &class.name);
            } else if let ClassMember::Destructor(dtor) = member {
                self.generate_destructor(dtor, &class.name);
            }
        }

        self.current_class = None;
    }

    fn generate_method(&mut self, method: &MethodDecl, class_name: &str) {
        let return_type = self.resolve_type(&method.return_type);
        let ir_name = format!("{}_{}", class_name, method.name);
        let mut func = IRFunction::new(&ir_name, return_type.clone());

        func.add_param("this".to_string(), IRType::Object);
        for param in &method.params {
            let param_type = self.resolve_type(&param.param_type);
            func.add_param(param.name.clone(), param_type);
        }

        self.variables.clear();
        self.variables.insert("this".to_string(), IRValue {
            name: "this".to_string(),
            typ: IRType::Object,
            id: 0,
        });
        for param in &method.params {
            let param_type = self.resolve_type(&param.param_type);
            self.variables.insert(param.name.clone(), IRValue {
                name: param.name.clone(),
                typ: param_type,
                id: 0,
            });
        }

        if let Some(body) = &method.body {
            self.generate_block(&mut func, body);
        }

        if return_type == IRType::Void && !func.current_block().terminated {
            self.emit(&mut func, IRInstruction::Ret(None));
        }

        self.module.functions.push(func);
    }

    fn generate_constructor(&mut self, ctor: &ConstructorDecl, class_name: &str) {
        let ir_name = format!("{}_ctor", class_name);
        let mut func = IRFunction::new(&ir_name, IRType::Void);

        func.add_param("this".to_string(), IRType::Object);
        for param in &ctor.params {
            let param_type = self.resolve_type(&param.param_type);
            func.add_param(param.name.clone(), param_type);
        }

        self.variables.clear();
        self.variables.insert("this".to_string(), IRValue {
            name: "this".to_string(),
            typ: IRType::Object,
            id: 0,
        });
        for param in &ctor.params {
            let param_type = self.resolve_type(&param.param_type);
            self.variables.insert(param.name.clone(), IRValue {
                name: param.name.clone(),
                typ: param_type,
                id: 0,
            });
        }

        self.generate_block(&mut func, &ctor.body);
        if !func.current_block().terminated {
            self.emit(&mut func, IRInstruction::Ret(None));
        }

        self.module.functions.push(func);
    }

    fn generate_destructor(&mut self, dtor: &DestructorDecl, class_name: &str) {
        let ir_name = format!("{}_dtor", class_name);
        let mut func = IRFunction::new(&ir_name, IRType::Void);

        func.add_param("this".to_string(), IRType::Object);

        self.variables.clear();
        self.variables.insert("this".to_string(), IRValue {
            name: "this".to_string(),
            typ: IRType::Object,
            id: 0,
        });

        self.generate_block(&mut func, &dtor.body);
        if !func.current_block().terminated {
            self.emit(&mut func, IRInstruction::Ret(None));
        }

        self.module.functions.push(func);
    }

    fn generate_function(&mut self, func: &FunctionDecl) {
        let return_type = self.resolve_type(&func.return_type);
        let mut ir_func = IRFunction::new(&func.name, return_type.clone());

        for param in &func.params {
            let param_type = self.resolve_type(&param.param_type);
            ir_func.add_param(param.name.clone(), param_type);
        }

        self.variables.clear();
        for param in &func.params {
            let param_type = self.resolve_type(&param.param_type);
            self.variables.insert(param.name.clone(), IRValue {
                name: param.name.clone(),
                typ: param_type,
                id: 0,
            });
        }

        self.generate_block(&mut ir_func, &func.body);

        if return_type == IRType::Void && !ir_func.current_block().terminated {
            self.emit(&mut ir_func, IRInstruction::Ret(None));
        } else if !ir_func.current_block().terminated {
            let zero = self.new_value(return_type.clone(), "zero");
            self.emit(&mut ir_func, IRInstruction::Ret(Some(zero)));
        }

        self.module.functions.push(ir_func);
    }

    fn generate_interface(&mut self, _iface: &InterfaceDecl) {}

    fn generate_block(&mut self, func: &mut IRFunction, block: &Block) {
        for stmt in &block.statements {
            self.generate_stmt(func, stmt);
        }
    }

    fn generate_stmt(&mut self, func: &mut IRFunction, stmt: &Stmt) {
        match stmt {
            Stmt::VarDecl(var) => {
                let var_type = self.resolve_type(&var.var_type);
                let alloca_val = self.new_value(var_type.clone(), &var.name);
                self.emit(func, IRInstruction::Alloca(var_type));
                self.variables.insert(var.name.clone(), alloca_val.clone());

                if let Some(init) = &var.initializer {
                    let init_val = self.generate_expr(func, init);
                    self.emit(func, IRInstruction::Store(init_val, alloca_val));
                }
            }
            Stmt::Return(expr) => {
                if let Some(expr) = expr {
                    let val = self.generate_expr(func, expr);
                    self.emit(func, IRInstruction::Ret(Some(val)));
                } else {
                    self.emit(func, IRInstruction::Ret(None));
                }
            }
            Stmt::Expr(expr) => {
                self.generate_expr(func, expr);
            }
            Stmt::If(cond, then, else_) => {
                let cond_val = self.generate_expr(func, cond);
                let then_label = format!("then_{}", self.value_counter);
                let else_label = format!("else_{}", self.value_counter);
                let end_label = format!("endif_{}", self.value_counter);
                self.value_counter += 1;

                if else_.is_some() {
                    self.emit(func, IRInstruction::CondBr(cond_val, then_label.clone(), else_label.clone()));
                } else {
                    self.emit(func, IRInstruction::CondBr(cond_val, then_label.clone(), end_label.clone()));
                }

                func.new_block(&then_label);
                self.generate_stmt(func, then);
                if !func.current_block().terminated {
                    self.emit(func, IRInstruction::Br(end_label.clone()));
                }

                if let Some(else_stmt) = else_ {
                    func.new_block(&else_label);
                    self.generate_stmt(func, else_stmt);
                    if !func.current_block().terminated {
                        self.emit(func, IRInstruction::Br(end_label.clone()));
                    }
                }

                func.new_block(&end_label);
            }
            Stmt::While(cond, body) => {
                let loop_label = format!("while_{}", self.value_counter);
                let body_label = format!("while_body_{}", self.value_counter);
                let end_label = format!("while_end_{}", self.value_counter);
                self.value_counter += 1;

                func.new_block(&loop_label);
                let cond_val = self.generate_expr(func, cond);
                self.emit(func, IRInstruction::CondBr(cond_val, body_label.clone(), end_label.clone()));

                func.new_block(&body_label);
                self.generate_stmt(func, body);
                if !func.current_block().terminated {
                    self.emit(func, IRInstruction::Br(loop_label));
                }

                func.new_block(&end_label);
            }
            Stmt::For(init, cond, update, body) => {
                let loop_label = format!("for_{}", self.value_counter);
                let body_label = format!("for_body_{}", self.value_counter);
                let update_label = format!("for_update_{}", self.value_counter);
                let end_label = format!("for_end_{}", self.value_counter);
                self.value_counter += 1;

                if let Some(stmt) = init {
                    self.generate_stmt(func, stmt.as_ref());
                }

                func.new_block(&loop_label);
                if let Some(cond_expr) = cond {
                    let cond_val = self.generate_expr(func, cond_expr);
                    self.emit(func, IRInstruction::CondBr(cond_val, body_label.clone(), end_label.clone()));
                } else {
                    self.emit(func, IRInstruction::Br(body_label.clone()));
                }

                func.new_block(&body_label);
                self.generate_stmt(func, body);
                if !func.current_block().terminated {
                    self.emit(func, IRInstruction::Br(update_label.clone()));
                }

                func.new_block(&update_label);
                if let Some(update_expr) = update {
                    self.generate_expr(func, update_expr);
                }
                if !func.current_block().terminated {
                    self.emit(func, IRInstruction::Br(loop_label));
                }

                func.new_block(&end_label);
            }
            Stmt::Block(block) => {
                self.generate_block(func, block);
            }
            Stmt::Break => {
                self.emit(func, IRInstruction::Br(format!("break_{}", self.value_counter)));
            }
            Stmt::Continue => {
                self.emit(func, IRInstruction::Br(format!("continue_{}", self.value_counter)));
            }
            Stmt::Try(_try_block, _catches, _finally) => {
                self.emit(func, IRInstruction::Debug("try-catch not fully supported".to_string()));
            }
        }
    }

    fn generate_expr(&mut self, func: &mut IRFunction, expr: &Expr) -> IRValue {
        match expr {
            Expr::IntegerLiteral(v) => {
                let val = self.new_value(IRType::i32(), "int");
                self.emit(func, IRInstruction::Debug(format!("int {}", v)));
                val
            }
            Expr::FloatLiteral(v) => {
                let val = self.new_value(IRType::f64(), "float");
                self.emit(func, IRInstruction::Debug(format!("float {}", v)));
                val
            }
            Expr::StringLiteral(s) => {
                let idx = self.string_literals.len();
                self.string_literals.push(s.clone());
                let val = self.new_value(IRType::ptr(IRType::u8()), "str");
                self.emit(func, IRInstruction::Debug(format!("str @{}", idx)));
                val
            }
            Expr::CharLiteral(c) => {
                let val = self.new_value(IRType::u8(), "char");
                self.emit(func, IRInstruction::Debug(format!("char {}", c)));
                val
            }
            Expr::BoolLiteral(b) => {
                let val = self.new_value(IRType::u8(), "bool");
                self.emit(func, IRInstruction::Debug(format!("bool {}", b)));
                val
            }
            Expr::Null => {
                self.new_value(IRType::Object, "null")
            }
            Expr::This => {
                self.variables.get("this").cloned().unwrap_or_else(|| {
                    self.new_value(IRType::Object, "this")
                })
            }
            Expr::Super => {
                self.variables.get("this").cloned().unwrap_or_else(|| {
                    self.new_value(IRType::Object, "super")
                })
            }
            Expr::Variable(name) => {
                self.variables.get(name).cloned().unwrap_or_else(|| {
                    self.new_value(IRType::i32(), name)
                })
            }
            Expr::BinaryOp(op, left, right) => {
                let l = self.generate_expr(func, left);
                let r = self.generate_expr(func, right);
                let result = self.new_value(IRType::i32(), "binop");
                match op {
                    BinaryOp::Add => self.emit(func, IRInstruction::Add(l, r)),
                    BinaryOp::Sub => self.emit(func, IRInstruction::Sub(l, r)),
                    BinaryOp::Mul => self.emit(func, IRInstruction::Mul(l, r)),
                    BinaryOp::Div => self.emit(func, IRInstruction::Div(l, r)),
                    BinaryOp::Mod => self.emit(func, IRInstruction::Mod(l, r)),
                    BinaryOp::Lt => self.emit(func, IRInstruction::ICmp(CmpOp::Lt, l, r)),
                    BinaryOp::Gt => self.emit(func, IRInstruction::ICmp(CmpOp::Gt, l, r)),
                    BinaryOp::Le => self.emit(func, IRInstruction::ICmp(CmpOp::Le, l, r)),
                    BinaryOp::Ge => self.emit(func, IRInstruction::ICmp(CmpOp::Ge, l, r)),
                    BinaryOp::Eq => self.emit(func, IRInstruction::ICmp(CmpOp::Eq, l, r)),
                    BinaryOp::Ne => self.emit(func, IRInstruction::ICmp(CmpOp::Ne, l, r)),
                    BinaryOp::And => self.emit(func, IRInstruction::ICmp(CmpOp::Ne, l, r)),
                    BinaryOp::Or => self.emit(func, IRInstruction::ICmp(CmpOp::Ne, l, r)),
                    _ => self.emit(func, IRInstruction::Debug(format!("binop {:?}", op))),
                }
                result
            }
            Expr::UnaryOp(op, operand) => {
                let _val = self.generate_expr(func, operand);
                let result = self.new_value(IRType::i32(), "unary");
                self.emit(func, IRInstruction::Debug(format!("unary {:?}", op)));
                result
            }
            Expr::Ternary(cond, then, else_) => {
                let cond_val = self.generate_expr(func, cond);
                let result = self.new_value(IRType::i32(), "ternary");
                let then_label = format!("tern_then_{}", self.value_counter);
                let else_label = format!("tern_else_{}", self.value_counter);
                let end_label = format!("tern_end_{}", self.value_counter);
                self.value_counter += 1;

                self.emit(func, IRInstruction::CondBr(cond_val, then_label.clone(), else_label.clone()));

                func.new_block(&then_label);
                let then_val = self.generate_expr(func, then);
                self.emit(func, IRInstruction::Br(end_label.clone()));

                func.new_block(&else_label);
                let else_val = self.generate_expr(func, else_);
                self.emit(func, IRInstruction::Br(end_label.clone()));

                func.new_block(&end_label);
                self.emit(func, IRInstruction::Phi(vec![
                    (then_val, then_label),
                    (else_val, else_label),
                ]));

                result
            }
            Expr::Assignment(target, value) => {
                let val = self.generate_expr(func, value);
                match target.as_ref() {
                    Expr::Variable(name) => {
                        if let Some(var) = self.variables.get(name) {
                            self.emit(func, IRInstruction::Store(val.clone(), var.clone()));
                        }
                    }
                    Expr::FieldAccess(obj, field) => {
                        let _obj_val = self.generate_expr(func, obj);
                        self.emit(func, IRInstruction::Debug(format!("store field {}", field)));
                    }
                    _ => {}
                }
                val
            }
            Expr::Call(callee, args) => {
                let arg_vals: Vec<IRValue> = args.iter()
                    .map(|a| self.generate_expr(func, a))
                    .collect();

                let func_name = match callee.as_ref() {
                    Expr::Variable(name) => name.clone(),
                    Expr::FieldAccess(obj, method) => {
                        let _obj_val = self.generate_expr(func, obj);
                        format!("call_{}", method)
                    }
                    _ => "unknown_call".to_string(),
                };

                let result = self.new_value(IRType::i32(), "call");
                self.emit(func, IRInstruction::Call(func_name, arg_vals));
                result
            }
            Expr::MethodCall(obj, _method, args) => {
                let obj_val = self.generate_expr(func, obj);
                let arg_vals: Vec<IRValue> = args.iter()
                    .map(|a| self.generate_expr(func, a))
                    .collect();
                let result = self.new_value(IRType::i32(), "mcall");
                self.emit(func, IRInstruction::VCall(obj_val, 0, arg_vals));
                result
            }
            Expr::FieldAccess(obj, _field) => {
                let obj_val = self.generate_expr(func, obj);
                let result = self.new_value(IRType::i32(), "field");
                self.emit(func, IRInstruction::GEP(obj_val, vec![]));
                result
            }
            Expr::New(class_name, _type_args, args) => {
                let arg_vals: Vec<IRValue> = args.iter()
                    .map(|a| self.generate_expr(func, a))
                    .collect();
                let result = self.new_value(IRType::Object, "new");
                self.emit(func, IRInstruction::GcAlloc(IRType::Struct(class_name.clone(), Vec::new())));
                self.emit(func, IRInstruction::Call(format!("{}_ctor", class_name), arg_vals));
                result
            }
            Expr::ArrayAccess(arr, idx) => {
                let arr_val = self.generate_expr(func, arr);
                let idx_val = self.generate_expr(func, idx);
                let result = self.new_value(IRType::i32(), "arr");
                self.emit(func, IRInstruction::GEP(arr_val, vec![idx_val]));
                result
            }
            Expr::Cast(target_type, expr) => {
                let val = self.generate_expr(func, expr);
                let ir_type = self.resolve_type(target_type);
                let result = self.new_value(ir_type.clone(), "cast");
                self.emit(func, IRInstruction::Cast(val, ir_type));
                result
            }
            Expr::InstanceOf(expr, _type_ref) => {
                let _val = self.generate_expr(func, expr);
                let result = self.new_value(IRType::u8(), "instanceof");
                self.emit(func, IRInstruction::Debug("instanceof".to_string()));
                result
            }
            Expr::Delete(expr) => {
                let val = self.generate_expr(func, expr);
                self.emit(func, IRInstruction::Release(val));
                self.new_value(IRType::Void, "delete")
            }
            Expr::Sizeof(type_ref) => {
                let _ir_type = self.resolve_type(type_ref);
                self.new_value(IRType::i32(), "sizeof")
            }
            Expr::TypeId(expr) => {
                let _val = self.generate_expr(func, expr);
                self.new_value(IRType::ptr(IRType::u8()), "typeid")
            }
            Expr::Lambda(_, _) => {
                self.new_value(IRType::ptr(IRType::u8()), "lambda")
            }
            Expr::Throw(expr) => {
                let _val = self.generate_expr(func, expr);
                self.emit(func, IRInstruction::Debug("throw".to_string()));
                self.new_value(IRType::Void, "throw")
            }
        }
    }

    fn resolve_type(&self, type_ref: &TypeRef) -> IRType {
        match type_ref {
            TypeRef::Primitive(p) => match p {
                PrimitiveType::Void => IRType::Void,
                PrimitiveType::Bool => IRType::u8(),
                PrimitiveType::Byte => IRType::i8(),
                PrimitiveType::Char => IRType::u8(),
                PrimitiveType::Short => IRType::i16(),
                PrimitiveType::Int => IRType::i32(),
                PrimitiveType::Long => IRType::i64(),
                PrimitiveType::Float => IRType::f32(),
                PrimitiveType::Double => IRType::f64(),
            },
            TypeRef::Named(name, _) => match name.as_str() {
                "int" => IRType::i32(),
                "long" => IRType::i64(),
                "float" => IRType::f32(),
                "double" => IRType::f64(),
                "bool" => IRType::u8(),
                "void" => IRType::Void,
                "String" => IRType::ptr(IRType::u8()),
                _ => IRType::Pointer(Box::new(IRType::Struct(name.clone(), Vec::new()))),
            },
            TypeRef::Array(inner) => IRType::Pointer(Box::new(self.resolve_type(inner))),
            TypeRef::FunctionPtr(ret, params) => {
                let ret_ir = self.resolve_type(ret);
                let params_ir = params.iter().map(|p| self.resolve_type(p)).collect();
                IRType::Function(params_ir, Box::new(ret_ir))
            }
        }
    }
}
