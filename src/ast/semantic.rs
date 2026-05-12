use crate::ast::node::*;
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone)]
pub enum SemanticError {
    UndefinedType {
        name: String,
    },
    UndefinedVariable {
        name: String,
    },
    UndefinedMethod {
        class_name: String,
        method_name: String,
    },
    UndefinedClass {
        name: String,
    },
    DuplicateDefinition {
        name: String,
    },
    TypeMismatch {
        expected: String,
        found: String,
    },
    NotAClass {
        name: String,
    },
    CircularInheritance {
        class_name: String,
    },
    MissingOverride {
        class_name: String,
        method_name: String,
    },
    InvalidOverride {
        class_name: String,
        method_name: String,
        reason: String,
    },
    AbstractMethodNotImplemented {
        class_name: String,
        method_name: String,
    },
    FieldShadowing {
        class_name: String,
        field_name: String,
    },
    InvalidAccess {
        member: String,
        class_name: String,
    },
    InterfaceMethodNotImplemented {
        class_name: String,
        interface_name: String,
        method_name: String,
    },
    InterfaceMethodSignatureMismatch {
        class_name: String,
        interface_name: String,
        method_name: String,
        reason: String,
    },
}

impl std::fmt::Display for SemanticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SemanticError::UndefinedType { name } => write!(f, "Undefined type '{}'", name),
            SemanticError::UndefinedVariable { name } => write!(f, "Undefined variable '{}'", name),
            SemanticError::UndefinedMethod { class_name, method_name } => {
                write!(f, "Undefined method '{}' in class '{}'", method_name, class_name)
            }
            SemanticError::UndefinedClass { name } => write!(f, "Undefined class '{}'", name),
            SemanticError::DuplicateDefinition { name } => write!(f, "Duplicate definition '{}'", name),
            SemanticError::TypeMismatch { expected, found } => {
                write!(f, "Type mismatch: expected '{}', found '{}'", expected, found)
            }
            SemanticError::NotAClass { name } => write!(f, "'{}' is not a class type", name),
            SemanticError::CircularInheritance { class_name } => {
                write!(f, "Circular inheritance detected involving '{}'", class_name)
            }
            SemanticError::MissingOverride { class_name, method_name } => {
                write!(f, "Method '{}' in '{}' overrides but is not marked @override", method_name, class_name)
            }
            SemanticError::InvalidOverride { class_name, method_name, reason } => {
                write!(f, "Invalid override of '{}' in '{}': {}", method_name, class_name, reason)
            }
            SemanticError::AbstractMethodNotImplemented { class_name, method_name } => {
                write!(f, "Abstract method '{}' not implemented in '{}'", method_name, class_name)
            }
            SemanticError::FieldShadowing { class_name, field_name } => {
                write!(f, "Field '{}' in '{}' shadows parent field", field_name, class_name)
            }
            SemanticError::InvalidAccess { member, class_name } => {
                write!(f, "Cannot access '{}' from class '{}'", member, class_name)
            }
            SemanticError::InterfaceMethodNotImplemented { class_name, interface_name, method_name } => {
                write!(f, "Class '{}' does not implement method '{}' from interface '{}'", class_name, method_name, interface_name)
            }
            SemanticError::InterfaceMethodSignatureMismatch { class_name, interface_name, method_name, reason } => {
                write!(f, "Method '{}' in class '{}' does not match interface '{}' signature: {}", method_name, class_name, interface_name, reason)
            }
        }
    }
}

struct ClassInfo {
    name: String,
    parent: Option<String>,
    implements: Vec<String>,
    fields: HashMap<String, TypeRef>,
    methods: Vec<MethodDecl>,
    is_abstract: bool,
}

pub struct SemanticAnalyzer {
    classes: HashMap<String, ClassInfo>,
    interfaces: HashMap<String, InterfaceDecl>,
    functions: HashMap<String, FunctionDecl>,
    errors: Vec<SemanticError>,
    current_class: Option<String>,
    scope_stack: Vec<HashMap<String, TypeRef>>,
    warnings: Vec<String>,
}

impl SemanticAnalyzer {
    pub fn new() -> Self {
        Self {
            classes: HashMap::new(),
            interfaces: HashMap::new(),
            functions: HashMap::new(),
            errors: Vec::new(),
            current_class: None,
            scope_stack: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn analyze(&mut self, program: &Program) -> (Vec<SemanticError>, Vec<String>) {
        self.collect_declarations(program);
        self.check_inheritance();
        self.check_interface_implementation();
        self.check_members(program);
        (self.errors.clone(), self.warnings.clone())
    }

    fn collect_declarations(&mut self, program: &Program) {
        for decl in &program.declarations {
            match decl {
                Declaration::Class(class) => {
                    if self.classes.contains_key(&class.name) {
                        self.errors.push(SemanticError::DuplicateDefinition {
                            name: class.name.clone(),
                        });
                        continue;
                    }

                    let is_abstract = class.modifiers.iter().any(|m| matches!(m, ClassModifier::Abstract));

                    let mut fields = HashMap::new();
                    let mut methods = Vec::new();

                    for member in &class.members {
                        match member {
                            ClassMember::Field(field) => {
                                if fields.contains_key(&field.name) {
                                    self.errors.push(SemanticError::DuplicateDefinition {
                                        name: field.name.clone(),
                                    });
                                } else {
                                    fields.insert(field.name.clone(), field.var_type.clone());
                                }
                            }
                            ClassMember::Method(method) => {
                                let is_dup = methods.iter().any(|m: &MethodDecl| {
                                    m.name == method.name
                                        && m.params.len() == method.params.len()
                                        && m.params.iter().zip(method.params.iter()).all(|(a, b)| {
                                            self.simple_types_match(&a.param_type, &b.param_type)
                                        })
                                });
                                if is_dup {
                                    self.errors.push(SemanticError::DuplicateDefinition {
                                        name: format!("{}.{}({})", class.name, method.name,
                                            method.params.iter().map(|p| format!("{:?}", p.param_type)).collect::<Vec<_>>().join(", ")),
                                    });
                                } else {
                                    methods.push(method.clone());
                                }
                            }
                            ClassMember::Constructor(_) | ClassMember::Destructor(_) => {}
                        }
                    }

                    let parent = class.extends.as_ref().and_then(|t| {
                        if let TypeRef::Named(name, _) = t {
                            Some(name.clone())
                        } else {
                            None
                        }
                    });

                    let implements: Vec<String> = class.implements.iter().filter_map(|t| {
                        if let TypeRef::Named(name, _) = t {
                            Some(name.clone())
                        } else {
                            None
                        }
                    }).collect();

                    self.classes.insert(class.name.clone(), ClassInfo {
                        name: class.name.clone(),
                        parent,
                        implements,
                        fields,
                        methods,
                        is_abstract,
                    });
                }
                Declaration::Interface(iface) => {
                    if self.interfaces.contains_key(&iface.name) {
                        self.errors.push(SemanticError::DuplicateDefinition {
                            name: iface.name.clone(),
                        });
                    } else {
                        self.interfaces.insert(iface.name.clone(), iface.clone());
                    }
                }
                Declaration::Function(func) => {
                    if self.functions.contains_key(&func.name) {
                        self.errors.push(SemanticError::DuplicateDefinition {
                            name: func.name.clone(),
                        });
                    } else {
                        self.functions.insert(func.name.clone(), func.clone());
                    }
                }
                _ => {}
            }
        }
    }

    fn check_inheritance(&mut self) {
        let class_names: Vec<String> = self.classes.keys().cloned().collect();
        for name in &class_names {
            let mut visited = HashSet::new();
            let mut current = name.clone();
            loop {
                if visited.contains(&current) {
                    self.errors.push(SemanticError::CircularInheritance {
                        class_name: current,
                    });
                    break;
                }
                visited.insert(current.clone());
                let parent = self.classes.get(&current).and_then(|c| c.parent.clone());
                match parent {
                    Some(p) => {
                        if !self.classes.contains_key(&p) && !self.is_builtin_type(&p) {
                            self.errors.push(SemanticError::UndefinedClass { name: p.clone() });
                            break;
                        }
                        current = p;
                    }
                    None => break,
                }
            }
        }

        let override_checks: Vec<(String, String, bool, bool, usize, usize)> = {
            let mut checks = Vec::new();
            for (class_name, class_info) in &self.classes {
                if let Some(parent_name) = &class_info.parent {
                    if let Some(parent_info) = self.classes.get(parent_name) {
                        for (field_name, _) in &class_info.fields {
                            if parent_info.fields.contains_key(field_name) {
                                self.warnings.push(format!(
                                    "Field '{}' in '{}' shadows parent field",
                                    field_name, class_name
                                ));
                            }
                        }

                        for method in &class_info.methods {
                            let is_override = method.modifiers.iter().any(|m| matches!(m, MethodModifier::Override));
                            let is_virtual = method.modifiers.iter().any(|m| matches!(m, MethodModifier::Virtual));
                            let parent_match = parent_info.methods.iter().find(|pm| {
                                pm.name == method.name && self.params_match(&pm.params, &method.params)
                            });
                            let has_parent_method = parent_match.is_some();
                            let parent_param_count = parent_match.map(|m| m.params.len()).unwrap_or(0);

                            checks.push((class_name.clone(), method.name.clone(), is_override, has_parent_method, method.params.len(), parent_param_count));

                            if is_virtual && is_override {
                                self.warnings.push(format!(
                                    "Method '{}' in '{}' is both @virtual and @override, @override implies @virtual",
                                    method.name, class_name
                                ));
                            }
                        }
                    }
                }
            }
            checks
        };

        for (class_name, method_name, is_override, has_parent_method, param_count, parent_param_count) in override_checks {
            if has_parent_method {
                if !is_override {
                    self.warnings.push(format!(
                        "Method '{}' in '{}' overrides parent method but is not marked @override",
                        method_name, class_name
                    ));
                }
                if param_count != parent_param_count {
                    self.errors.push(SemanticError::InvalidOverride {
                        class_name: class_name.clone(),
                        method_name: method_name.clone(),
                        reason: "parameter count mismatch".to_string(),
                    });
                }
            } else if is_override {
                self.errors.push(SemanticError::InvalidOverride {
                    class_name: class_name.clone(),
                    method_name: method_name.clone(),
                    reason: format!("no method '{}' in parent class", method_name),
                });
            }
        }

        let abstract_checks: Vec<(String, bool)> = self.classes.iter()
            .map(|(name, info)| (name.clone(), info.is_abstract))
            .collect();

        for (class_name, is_abstract) in abstract_checks {
            if !is_abstract {
                self.check_abstract_implementation(&class_name);
            }
        }
    }

    fn check_abstract_implementation(&mut self, class_name: &str) {
        let mut abstract_methods: Vec<(String, Vec<Param>)> = Vec::new();
        let mut current = self.classes.get(class_name).and_then(|c| c.parent.clone());
        while let Some(parent_name) = current {
            if let Some(parent_info) = self.classes.get(&parent_name) {
                for method in &parent_info.methods {
                    if method.modifiers.iter().any(|m| matches!(m, MethodModifier::Abstract)) {
                        abstract_methods.push((method.name.clone(), method.params.clone()));
                    }
                }
                current = parent_info.parent.clone();
            } else {
                break;
            }
        }

        let class_info = self.classes.get(class_name);
        for (method_name, method_params) in &abstract_methods {
            let implemented = class_info.map(|info| {
                info.methods.iter().any(|m| {
                    m.name == *method_name && self.params_match(&m.params, method_params)
                })
            }).unwrap_or(false);
            if !implemented {
                self.errors.push(SemanticError::AbstractMethodNotImplemented {
                    class_name: class_name.to_string(),
                    method_name: method_name.clone(),
                });
            }
        }
    }

    fn check_interface_implementation(&mut self) {
        let checks: Vec<(String, String)> = self.classes.iter()
            .flat_map(|(class_name, class_info)| {
                class_info.implements.iter().cloned().map(move |iface_name| {
                    (class_name.clone(), iface_name)
                })
            })
            .collect();

        for (class_name, iface_name) in checks {
            if !self.interfaces.contains_key(&iface_name) {
                self.errors.push(SemanticError::UndefinedType {
                    name: iface_name.clone(),
                });
                continue;
            }

            let iface = self.interfaces.get(&iface_name).unwrap().clone();

            for member in &iface.members {
                if let InterfaceMember::MethodSignature(sig) = member {
                    if let Some(method) = self.find_method_overload_in_class_hierarchy(&class_name, &sig.name, &sig.params) {
                        if !self.types_match(&method.return_type, &sig.return_type) {
                            self.errors.push(SemanticError::InterfaceMethodSignatureMismatch {
                                class_name: class_name.clone(),
                                interface_name: iface_name.clone(),
                                method_name: sig.name.clone(),
                                reason: "return type mismatch".to_string(),
                            });
                        }
                        if method.params.len() != sig.params.len() {
                            self.errors.push(SemanticError::InterfaceMethodSignatureMismatch {
                                class_name: class_name.clone(),
                                interface_name: iface_name.clone(),
                                method_name: sig.name.clone(),
                                reason: format!("parameter count mismatch: expected {}, found {}",
                                    sig.params.len(), method.params.len()),
                            });
                        } else {
                            for (i, (mp, sp)) in method.params.iter().zip(sig.params.iter()).enumerate() {
                                if !self.types_match(&mp.param_type, &sp.param_type) {
                                    self.errors.push(SemanticError::InterfaceMethodSignatureMismatch {
                                        class_name: class_name.clone(),
                                        interface_name: iface_name.clone(),
                                        method_name: sig.name.clone(),
                                        reason: format!("parameter {} type mismatch", i),
                                    });
                                }
                            }
                        }
                    } else {
                        self.errors.push(SemanticError::InterfaceMethodNotImplemented {
                            class_name: class_name.clone(),
                            interface_name: iface_name.clone(),
                            method_name: sig.name.clone(),
                        });
                    }
                }
            }
        }
    }

    fn find_method_in_class_hierarchy(&self, class_name: &str, method_name: &str) -> Option<MethodDecl> {
        if let Some(class_info) = self.classes.get(class_name) {
            if let Some(method) = class_info.methods.iter().find(|m| m.name == method_name) {
                return Some(method.clone());
            }
            if let Some(parent) = &class_info.parent {
                return self.find_method_in_class_hierarchy(parent, method_name);
            }
        }
        None
    }

    fn find_method_overload_in_class_hierarchy(&self, class_name: &str, method_name: &str, params: &[Param]) -> Option<MethodDecl> {
        if let Some(class_info) = self.classes.get(class_name) {
            if let Some(method) = class_info.methods.iter().find(|m| {
                m.name == method_name && self.params_match(&m.params, params)
            }) {
                return Some(method.clone());
            }
            if let Some(parent) = &class_info.parent {
                return self.find_method_overload_in_class_hierarchy(parent, method_name, params);
            }
        }
        None
    }

    fn types_match(&self, a: &TypeRef, b: &TypeRef) -> bool {
        match (a, b) {
            (TypeRef::Primitive(pa), TypeRef::Primitive(pb)) => pa == pb,
            (TypeRef::Named(na, _), TypeRef::Named(nb, _)) => na == nb,
            (TypeRef::Array(ia), TypeRef::Array(ib)) => self.types_match(ia, ib),
            _ => false,
        }
    }

    fn params_match(&self, a: &[Param], b: &[Param]) -> bool {
        if a.len() != b.len() { return false; }
        for (pa, pb) in a.iter().zip(b.iter()) {
            if !self.types_match(&pa.param_type, &pb.param_type) { return false; }
        }
        true
    }

    fn simple_types_match(&self, a: &TypeRef, b: &TypeRef) -> bool {
        match (a, b) {
            (TypeRef::Primitive(pa), TypeRef::Primitive(pb)) => pa == pb,
            (TypeRef::Named(na, taa), TypeRef::Named(nb, tab)) => {
                na == nb && taa.len() == tab.len()
            },
            (TypeRef::Array(ia), TypeRef::Array(ib)) => self.simple_types_match(ia, ib),
            _ => false,
        }
    }

    fn check_members(&mut self, program: &Program) {
        for decl in &program.declarations {
            match decl {
                Declaration::Class(class) => {
                    self.current_class = Some(class.name.clone());
                    self.scope_stack.clear();
                    self.push_scope();

                    let class_fields: Vec<(String, TypeRef)> = self.classes.get(&class.name)
                        .map(|info| info.fields.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                        .unwrap_or_default();

                    for (field_name, field_type) in &class_fields {
                        self.define_var(field_name.clone(), field_type.clone());
                    }

                    self.define_var("this".to_string(), TypeRef::Named(class.name.clone(), vec![]));
                    self.define_var("super".to_string(), TypeRef::Named(class.name.clone(), vec![]));

                    for member in &class.members {
                        match member {
                            ClassMember::Method(method) => {
                                if let Some(body) = &method.body {
                                    self.push_scope();
                                    for param in &method.params {
                                        self.define_var(param.name.clone(), param.param_type.clone());
                                    }
                                    self.check_block(body);
                                    self.pop_scope();
                                }
                            }
                            ClassMember::Constructor(ctor) => {
                                self.push_scope();
                                for param in &ctor.params {
                                    self.define_var(param.name.clone(), param.param_type.clone());
                                }
                                self.check_block(&ctor.body);
                                self.pop_scope();
                            }
                            ClassMember::Destructor(dtor) => {
                                self.check_block(&dtor.body);
                            }
                            ClassMember::Field(field) => {
                                if let Some(init) = &field.initializer {
                                    self.check_expr(init);
                                }
                            }
                        }
                    }

                    self.pop_scope();
                    self.current_class = None;
                }
                Declaration::Function(func) => {
                    self.scope_stack.clear();
                    self.push_scope();
                    for param in &func.params {
                        self.define_var(param.name.clone(), param.param_type.clone());
                    }
                    self.check_block(&func.body);
                    self.pop_scope();
                }
                _ => {}
            }
        }
    }

    fn check_block(&mut self, block: &Block) {
        self.push_scope();
        for stmt in &block.statements {
            self.check_stmt(stmt);
        }
        self.pop_scope();
    }

    fn check_stmt(&mut self, stmt: &Stmt) {
        match stmt {
            Stmt::VarDecl(var) => {
                self.check_type_ref(&var.var_type);
                if let Some(init) = &var.initializer {
                    self.check_expr(init);
                }
                self.define_var(var.name.clone(), var.var_type.clone());
            }
            Stmt::Return(expr) => {
                if let Some(e) = expr {
                    self.check_expr(e);
                }
            }
            Stmt::Expr(expr) => {
                self.check_expr(expr);
            }
            Stmt::If(cond, then, else_) => {
                self.check_expr(cond);
                self.check_stmt(then);
                if let Some(else_stmt) = else_ {
                    self.check_stmt(else_stmt);
                }
            }
            Stmt::While(cond, body) => {
                self.check_expr(cond);
                self.check_stmt(body);
            }
            Stmt::For(init, cond, update, body) => {
                if let Some(e) = init {
                    self.check_expr(e);
                }
                if let Some(e) = cond {
                    self.check_expr(e);
                }
                if let Some(e) = update {
                    self.check_expr(e);
                }
                self.check_stmt(body);
            }
            Stmt::Block(block) => {
                self.check_block(block);
            }
            Stmt::Try(try_block, catches, finally) => {
                self.check_block(try_block);
                for catch in catches {
                    self.check_type_ref(&catch.var_type);
                    self.check_block(&catch.body);
                }
                if let Some(finally_block) = finally {
                    self.check_block(finally_block);
                }
            }
            Stmt::Break | Stmt::Continue => {}
        }
    }

    fn check_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Variable(name) => {
                if !self.lookup_var(name) && !self.is_builtin_func(name) {
                    self.errors.push(SemanticError::UndefinedVariable {
                        name: name.clone(),
                    });
                }
            }
            Expr::BinaryOp(_, left, right) => {
                self.check_expr(left);
                self.check_expr(right);
            }
            Expr::UnaryOp(_, operand) => {
                self.check_expr(operand);
            }
            Expr::Assignment(target, value) => {
                self.check_expr(target);
                self.check_expr(value);
            }
            Expr::Call(callee, args) => {
                self.check_expr(callee);
                for arg in args {
                    self.check_expr(arg);
                }
            }
            Expr::MethodCall(obj, method_name, args) => {
                self.check_expr(obj);
                for arg in args {
                    self.check_expr(arg);
                }
                if let Expr::Variable(var_name) = obj.as_ref() {
                    if let Some(class_name) = self.resolve_var_class(var_name) {
                        if !self.class_has_method(&class_name, method_name) {
                            self.errors.push(SemanticError::UndefinedMethod {
                                class_name,
                                method_name: method_name.clone(),
                            });
                        }
                    }
                }
            }
            Expr::FieldAccess(obj, field_name) => {
                self.check_expr(obj);
                if let Expr::Variable(var_name) = obj.as_ref() {
                    if let Some(class_name) = self.resolve_var_class(var_name) {
                        if !self.class_has_field(&class_name, field_name) &&
                           !self.class_has_method(&class_name, field_name) {
                            self.warnings.push(format!(
                                "Class '{}' may not have field or method '{}'",
                                class_name, field_name
                            ));
                        }
                    }
                }
            }
            Expr::New(class_name, type_args, args) => {
                if !self.classes.contains_key(class_name) && !self.is_builtin_type(class_name) {
                    self.errors.push(SemanticError::UndefinedClass {
                        name: class_name.clone(),
                    });
                }
                for ta in type_args {
                    self.check_type_ref(ta);
                }
                for arg in args {
                    self.check_expr(arg);
                }
            }
            Expr::ArrayAccess(arr, idx) => {
                self.check_expr(arr);
                self.check_expr(idx);
            }
            Expr::Cast(target_type, e) => {
                self.check_type_ref(target_type);
                self.check_expr(e);
            }
            Expr::InstanceOf(e, type_ref) => {
                self.check_expr(e);
                self.check_type_ref(type_ref);
            }
            Expr::Delete(e) => {
                self.check_expr(e);
            }
            Expr::Ternary(cond, then, else_) => {
                self.check_expr(cond);
                self.check_expr(then);
                self.check_expr(else_);
            }
            Expr::Sizeof(type_ref) => {
                self.check_type_ref(type_ref);
            }
            Expr::TypeId(e) => {
                self.check_expr(e);
            }
            Expr::Lambda(params, _body) => {
                for param in params {
                    self.check_type_ref(&param.param_type);
                }
            }
            Expr::Throw(expr) => {
                self.check_expr(expr);
            }
            _ => {}
        }
    }

    fn check_type_ref(&mut self, type_ref: &TypeRef) {
        match type_ref {
            TypeRef::Named(name, _) => {
                if !self.classes.contains_key(name)
                    && !self.interfaces.contains_key(name)
                    && !self.is_builtin_type(name)
                {
                    self.errors.push(SemanticError::UndefinedType {
                        name: name.clone(),
                    });
                }
            }
            TypeRef::Array(inner) => {
                self.check_type_ref(inner);
            }
            TypeRef::FunctionPtr(ret, params) => {
                self.check_type_ref(ret);
                for p in params {
                    self.check_type_ref(p);
                }
            }
            TypeRef::Primitive(_) => {}
        }
    }

    fn push_scope(&mut self) {
        self.scope_stack.push(HashMap::new());
    }

    fn pop_scope(&mut self) {
        self.scope_stack.pop();
    }

    fn define_var(&mut self, name: String, type_ref: TypeRef) {
        if let Some(scope) = self.scope_stack.last_mut() {
            scope.insert(name, type_ref);
        }
    }

    fn lookup_var(&self, name: &str) -> bool {
        for scope in self.scope_stack.iter().rev() {
            if scope.contains_key(name) {
                return true;
            }
        }
        false
    }

    fn resolve_var_class(&self, var_name: &str) -> Option<String> {
        for scope in self.scope_stack.iter().rev() {
            if let Some(type_ref) = scope.get(var_name) {
                if let TypeRef::Named(class_name, _) = type_ref {
                    if self.classes.contains_key(class_name) {
                        return Some(class_name.clone());
                    }
                }
            }
        }
        None
    }

    fn class_has_method(&self, class_name: &str, method_name: &str) -> bool {
        if let Some(class_info) = self.classes.get(class_name) {
            if class_info.methods.iter().any(|m| m.name == method_name) {
                return true;
            }
            if let Some(parent) = &class_info.parent {
                return self.class_has_method(parent, method_name);
            }
        }
        false
    }

    fn class_has_field(&self, class_name: &str, field_name: &str) -> bool {
        if let Some(class_info) = self.classes.get(class_name) {
            if class_info.fields.contains_key(field_name) {
                return true;
            }
            if let Some(parent) = &class_info.parent {
                return self.class_has_field(parent, field_name);
            }
        }
        false
    }

    fn is_builtin_type(&self, name: &str) -> bool {
        matches!(
            name,
            "int" | "long" | "float" | "double" | "bool" | "void" | "byte" | "char" | "short" | "String" | "TypeInfo" | "Array" | "Map"
            | "List" | "Pair" | "Optional" | "Result" | "Set" | "Queue" | "Stack" | "HashMap" | "HashSet" | "LinkedList" | "Tuple"
        )
    }

    fn is_builtin_func(&self, name: &str) -> bool {
        matches!(
            name,
            "printf" | "malloc" | "free" | "sizeof" | "exit" | "abort" | "memcpy" | "memset" | "strlen" | "strcmp" | "strdup" | "type_of" | "gc_init" | "gc_mark" | "gc_sweep" | "gc_alloc"
            | "LemonArray_new" | "LemonArray_add" | "LemonArray_get" | "LemonArray_set" | "LemonArray_size" | "LemonArray_removeAt" | "LemonArray_ensureCapacity"
            | "LemonMap_new" | "LemonMap_put" | "LemonMap_get" | "LemonMap_size" | "LemonMap_containsKey" | "LemonMap_remove"
        )
    }
}
