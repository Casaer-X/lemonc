use crate::ast::node::*;
use std::collections::HashMap;

pub struct AstOptimizer {
    constant_env: HashMap<String, ConstValue>,
    stats: OptimizerStats,
}

#[derive(Debug, Clone)]
enum ConstValue {
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
}

#[derive(Debug, Default)]
pub struct OptimizerStats {
    pub constants_folded: u32,
    pub dead_code_removed: u32,
    pub constants_propagated: u32,
}

impl AstOptimizer {
    pub fn new() -> Self {
        Self {
            constant_env: HashMap::new(),
            stats: OptimizerStats::default(),
        }
    }

    pub fn optimize(&mut self, program: &mut Program) -> &OptimizerStats {
        for decl in &mut program.declarations {
            match decl {
                Declaration::Class(class) => self.optimize_class(class),
                Declaration::Function(func) => self.optimize_function(func),
                Declaration::Enum(_) => {}
                Declaration::Interface(_) => {}
                Declaration::Variable(_) => {}
                Declaration::Import(_) => {}
                Declaration::Package(_) => {}
            }
        }
        &self.stats
    }

    fn optimize_class(&mut self, class: &mut ClassDecl) {
        for member in &mut class.members {
            match member {
                ClassMember::Method(method) => {
                    if let Some(body) = &mut method.body {
                        self.optimize_block(body);
                    }
                }
                ClassMember::Constructor(ctor) => {
                    self.optimize_block(&mut ctor.body);
                }
                ClassMember::Destructor(dtor) => {
                    self.optimize_block(&mut dtor.body);
                }
                ClassMember::Field(field) => {
                    if let Some(init) = &mut field.initializer {
                        self.optimize_expr(init);
                    }
                }
            }
        }
    }

    fn optimize_function(&mut self, func: &mut FunctionDecl) {
        self.optimize_block(&mut func.body);
    }

    fn optimize_block(&mut self, block: &mut Block) {
        let mut i = 0;
        while i < block.statements.len() {
            let removed = {
                let stmt = &mut block.statements[i];
                self.optimize_stmt(stmt);
                self.is_dead_stmt(stmt)
            };
            if removed {
                block.statements.remove(i);
                self.stats.dead_code_removed += 1;
            } else {
                i += 1;
            }
        }
    }

    fn optimize_stmt(&mut self, stmt: &mut Stmt) {
        match stmt {
            Stmt::VarDecl(var) => {
                if let Some(init) = &mut var.initializer {
                    self.optimize_expr(init);
                }
                if let Some(init) = &var.initializer {
                    if let Some(val) = self.eval_const(init) {
                        self.constant_env.insert(var.name.clone(), val);
                    }
                }
            }
            Stmt::Return(expr) => {
                if let Some(e) = expr {
                    self.optimize_expr(e);
                }
            }
            Stmt::Expr(expr) => {
                self.optimize_expr(expr);
            }
            Stmt::If(cond, then, else_) => {
                self.optimize_expr(cond);
                if let Expr::BoolLiteral(b) = cond {
                    self.stats.dead_code_removed += 1;
                    if *b {
                        let then_stmt = std::mem::replace(then.as_mut(), Stmt::Block(Block { statements: vec![] }));
                        *stmt = then_stmt;
                    } else if let Some(else_stmt) = else_ {
                        let else_stmt = std::mem::replace(else_stmt.as_mut(), Stmt::Block(Block { statements: vec![] }));
                        *stmt = else_stmt;
                    } else {
                        *stmt = Stmt::Block(Block { statements: vec![] });
                    }
                } else {
                    self.optimize_stmt(then);
                    if let Some(else_stmt) = else_ {
                        self.optimize_stmt(else_stmt);
                    }
                }
            }
            Stmt::While(cond, body) => {
                self.optimize_expr(cond);
                if let Expr::BoolLiteral(false) = cond {
                    self.stats.dead_code_removed += 1;
                    *stmt = Stmt::Block(Block { statements: vec![] });
                } else {
                    self.optimize_stmt(body);
                }
            }
            Stmt::For(init, cond, update, body) => {
                if let Some(stmt) = init {
                    self.optimize_stmt(&mut stmt.as_ref().clone());
                }
                if let Some(e) = cond {
                    self.optimize_expr(e);
                }
                if let Some(e) = update {
                    self.optimize_expr(e);
                }
                self.optimize_stmt(body);
            }
            Stmt::Block(block) => {
                self.optimize_block(block);
            }
            Stmt::Try(try_block, catches, finally) => {
                self.optimize_block(try_block);
                for catch in catches {
                    self.optimize_block(&mut catch.body);
                }
                if let Some(finally_block) = finally {
                    self.optimize_block(finally_block);
                }
            }
            Stmt::Break | Stmt::Continue => {}
            Stmt::Switch(expr, cases, default) => {
                self.optimize_expr(expr);
                for case in cases {
                    for pattern in &mut case.patterns {
                        self.optimize_expr(pattern);
                    }
                    self.optimize_block(&mut case.body);
                }
                if let Some(default_block) = default {
                    self.optimize_block(default_block);
                }
            }
            Stmt::ForEach(_type_ref, _name, iterable, body) => {
                self.optimize_expr(iterable);
                self.optimize_stmt(body);
            }
        }
    }

    fn optimize_expr(&mut self, expr: &mut Expr) {
        match expr {
            Expr::BinaryOp(op, left, right) => {
                self.optimize_expr(left);
                self.optimize_expr(right);
                let folded = self.try_fold_binary(*op, left, right);
                if let Some(result) = folded {
                    self.stats.constants_folded += 1;
                    *expr = result;
                }
            }
            Expr::UnaryOp(op, operand) => {
                self.optimize_expr(operand);
                let folded = self.try_fold_unary(*op, operand);
                if let Some(result) = folded {
                    self.stats.constants_folded += 1;
                    *expr = result;
                }
            }
            Expr::Ternary(cond, then, else_) => {
                self.optimize_expr(cond);
                if let Expr::BoolLiteral(b) = cond.as_ref() {
                    self.stats.constants_folded += 1;
                    if *b {
                        let t = std::mem::replace(then.as_mut(), Expr::Null);
                        *expr = t;
                    } else {
                        let e = std::mem::replace(else_.as_mut(), Expr::Null);
                        *expr = e;
                    }
                } else {
                    self.optimize_expr(then);
                    self.optimize_expr(else_);
                }
            }
            Expr::Assignment(target, value) => {
                self.optimize_expr(target);
                self.optimize_expr(value);
            }
            Expr::Call(callee, args) => {
                self.optimize_expr(callee);
                for arg in args {
                    self.optimize_expr(arg);
                }
            }
            Expr::MethodCall(obj, _method, args) => {
                self.optimize_expr(obj);
                for arg in args {
                    self.optimize_expr(arg);
                }
            }
            Expr::FieldAccess(obj, _field) => {
                self.optimize_expr(obj);
            }
            Expr::ArrayAccess(arr, idx) => {
                self.optimize_expr(arr);
                self.optimize_expr(idx);
            }
            Expr::New(_class_name, _type_args, args) => {
                for arg in args {
                    self.optimize_expr(arg);
                }
            }
            Expr::Delete(e) => {
                self.optimize_expr(e);
            }
            Expr::Cast(_target_type, e) => {
                self.optimize_expr(e);
            }
            Expr::InstanceOf(e, _type_ref) => {
                self.optimize_expr(e);
            }
            Expr::Variable(name) => {
                if let Some(val) = self.constant_env.get(name) {
                    match val {
                        ConstValue::Int(v) => {
                            self.stats.constants_propagated += 1;
                            *expr = Expr::IntegerLiteral(*v);
                        }
                        ConstValue::Float(v) => {
                            self.stats.constants_propagated += 1;
                            *expr = Expr::FloatLiteral(*v);
                        }
                        ConstValue::Bool(v) => {
                            self.stats.constants_propagated += 1;
                            *expr = Expr::BoolLiteral(*v);
                        }
                        ConstValue::Null => {
                            self.stats.constants_propagated += 1;
                            *expr = Expr::Null;
                        }
                    }
                }
            }
            Expr::IntegerLiteral(_)
            | Expr::FloatLiteral(_)
            | Expr::StringLiteral(_)
            | Expr::CharLiteral(_)
            | Expr::BoolLiteral(_)
            | Expr::Null
            | Expr::This
            | Expr::Super
            | Expr::Sizeof(_)
            | Expr::Match(_, _) => {}
            Expr::TypeId(e) => {
                self.optimize_expr(e);
            }
            Expr::Lambda(_, body) => {
                match body.as_mut() {
                    LambdaBody::Expr(e) => self.optimize_expr(e),
                    LambdaBody::Block(b) => self.optimize_block(b),
                }
            }
            Expr::Throw(e) => {
                self.optimize_expr(e);
            }
        }
    }

    fn try_fold_binary(&self, op: BinaryOp, left: &Expr, right: &Expr) -> Option<Expr> {
        match (left, right) {
            (Expr::IntegerLiteral(l), Expr::IntegerLiteral(r)) => {
                self.eval_int_binop(&op, *l, *r).map(Expr::IntegerLiteral)
            }
            (Expr::FloatLiteral(l), Expr::FloatLiteral(r)) => {
                self.eval_float_binop(&op, *l, *r).map(Expr::FloatLiteral)
            }
            (Expr::BoolLiteral(l), Expr::BoolLiteral(r)) => {
                match op {
                    BinaryOp::And => Some(Expr::BoolLiteral(*l && *r)),
                    BinaryOp::Or => Some(Expr::BoolLiteral(*l || *r)),
                    BinaryOp::Eq => Some(Expr::BoolLiteral(*l == *r)),
                    BinaryOp::Ne => Some(Expr::BoolLiteral(*l != *r)),
                    _ => None,
                }
            }
            (Expr::Null, Expr::Null) => {
                match op {
                    BinaryOp::Eq => Some(Expr::BoolLiteral(true)),
                    BinaryOp::Ne => Some(Expr::BoolLiteral(false)),
                    _ => None,
                }
            }
            (Expr::StringLiteral(l), Expr::StringLiteral(r)) => {
                match op {
                    BinaryOp::Eq => Some(Expr::BoolLiteral(l == r)),
                    BinaryOp::Ne => Some(Expr::BoolLiteral(l != r)),
                    BinaryOp::Add => Some(Expr::StringLiteral(format!("{}{}", l, r))),
                    _ => None,
                }
            }
            (Expr::IntegerLiteral(0), _) if op == BinaryOp::Add => {
                Some(right.clone())
            }
            (Expr::IntegerLiteral(1), _) if op == BinaryOp::Mul => {
                Some(right.clone())
            }
            (_, Expr::IntegerLiteral(0)) if op == BinaryOp::Add => {
                Some(left.clone())
            }
            (_, Expr::IntegerLiteral(1)) if op == BinaryOp::Mul => {
                Some(left.clone())
            }
            (Expr::BoolLiteral(false), _) if op == BinaryOp::And => {
                Some(Expr::BoolLiteral(false))
            }
            (Expr::BoolLiteral(true), _) if op == BinaryOp::Or => {
                Some(Expr::BoolLiteral(true))
            }
            _ => None,
        }
    }

    fn try_fold_unary(&self, op: UnaryOp, operand: &Expr) -> Option<Expr> {
        match (&op, operand) {
            (UnaryOp::Minus, Expr::IntegerLiteral(v)) => Some(Expr::IntegerLiteral(-v)),
            (UnaryOp::Minus, Expr::FloatLiteral(v)) => Some(Expr::FloatLiteral(-v)),
            (UnaryOp::Not, Expr::BoolLiteral(v)) => Some(Expr::BoolLiteral(!v)),
            (UnaryOp::Not, Expr::IntegerLiteral(v)) => Some(Expr::IntegerLiteral(!v)),
            (UnaryOp::BitNot, Expr::IntegerLiteral(v)) => Some(Expr::IntegerLiteral(!v)),
            _ => None,
        }
    }

    fn eval_int_binop(&self, op: &BinaryOp, l: i64, r: i64) -> Option<i64> {
        match op {
            BinaryOp::Add => Some(l.wrapping_add(r)),
            BinaryOp::Sub => Some(l.wrapping_sub(r)),
            BinaryOp::Mul => Some(l.wrapping_mul(r)),
            BinaryOp::Div => if r != 0 { Some(l / r) } else { None },
            BinaryOp::Mod => if r != 0 { Some(l % r) } else { None },
            BinaryOp::Shl => Some(l.wrapping_shl(r as u32)),
            BinaryOp::Shr => Some(l.wrapping_shr(r as u32)),
            BinaryOp::BitAnd => Some(l & r),
            BinaryOp::BitOr => Some(l | r),
            BinaryOp::BitXor => Some(l ^ r),
            BinaryOp::Eq => Some(if l == r { 1 } else { 0 }),
            BinaryOp::Ne => Some(if l != r { 1 } else { 0 }),
            BinaryOp::Lt => Some(if l < r { 1 } else { 0 }),
            BinaryOp::Gt => Some(if l > r { 1 } else { 0 }),
            BinaryOp::Le => Some(if l <= r { 1 } else { 0 }),
            BinaryOp::Ge => Some(if l >= r { 1 } else { 0 }),
            _ => None,
        }
    }

    fn eval_float_binop(&self, op: &BinaryOp, l: f64, r: f64) -> Option<f64> {
        match op {
            BinaryOp::Add => Some(l + r),
            BinaryOp::Sub => Some(l - r),
            BinaryOp::Mul => Some(l * r),
            BinaryOp::Div => if r != 0.0 { Some(l / r) } else { None },
            _ => None,
        }
    }

    fn eval_const(&self, expr: &Expr) -> Option<ConstValue> {
        match expr {
            Expr::IntegerLiteral(v) => Some(ConstValue::Int(*v)),
            Expr::FloatLiteral(v) => Some(ConstValue::Float(*v)),
            Expr::BoolLiteral(v) => Some(ConstValue::Bool(*v)),
            Expr::Null => Some(ConstValue::Null),
            _ => None,
        }
    }

    fn is_dead_stmt(&self, stmt: &Stmt) -> bool {
        match stmt {
            Stmt::Expr(expr) => self.is_pure_expr(expr),
            Stmt::Block(block) => block.statements.is_empty(),
            Stmt::VarDecl(_)
            | Stmt::Return(_)
            | Stmt::If(_, _, _)
            | Stmt::While(_, _)
            | Stmt::For(_, _, _, _)
            | Stmt::ForEach(_, _, _, _)
            | Stmt::Try(_, _, _)
            | Stmt::Switch(_, _, _)
            | Stmt::Break
            | Stmt::Continue => false,
        }
    }

    fn is_pure_expr(&self, expr: &Expr) -> bool {
        match expr {
            Expr::IntegerLiteral(_)
            | Expr::FloatLiteral(_)
            | Expr::StringLiteral(_)
            | Expr::CharLiteral(_)
            | Expr::BoolLiteral(_)
            | Expr::Null
            | Expr::This
            | Expr::Super
            | Expr::Variable(_) => true,
            Expr::BinaryOp(_, l, r) => self.is_pure_expr(l) && self.is_pure_expr(r),
            Expr::UnaryOp(_, o) => self.is_pure_expr(o),
            Expr::Ternary(c, t, e) => self.is_pure_expr(c) && self.is_pure_expr(t) && self.is_pure_expr(e),
            Expr::Assignment(_, _)
            | Expr::Call(_, _)
            | Expr::MethodCall(_, _, _)
            | Expr::FieldAccess(_, _)
            | Expr::ArrayAccess(_, _)
            | Expr::New(_, _, _)
            | Expr::Delete(_)
            | Expr::Cast(_, _)
            | Expr::InstanceOf(_, _)
            | Expr::Sizeof(_)
            | Expr::TypeId(_)
            | Expr::Lambda(_, _)
            | Expr::Throw(_)
            | Expr::Match(_, _) => false,
        }
    }
}
