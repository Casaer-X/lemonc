/// Execution Strategy Decision Module
///
/// Determines whether each function/method should be compiled as:
/// - AOT (Ahead-of-Time): Native x86-64 machine code
/// - JIT (Just-in-Time): Bytecode interpreted by LeVM, with optional JIT compilation
/// - Hybrid: AOT with JIT fallback for dynamic features

use crate::ast::node::*;

/// Execution strategy for a class member
#[derive(Debug, Clone, PartialEq)]
pub enum ExecutionStrategy {
    /// Compile to native machine code (default for most code)
    Aot,
    /// Compile to bytecode for VM execution (for dynamic features)
    Jit,
    /// AOT compiled but with runtime profiling for potential JIT optimization
    AotWithProfile,
}

/// Strategy decision configuration
#[derive(Debug, Clone)]
pub struct StrategyConfig {
    /// Force all code to AOT (for --aot-only mode)
    pub aot_only: bool,
    /// Force all code to JIT (for --jit-only mode)
    pub jit_only: bool,
    /// Enable profiling for AOT code
    pub enable_profiling: bool,
}

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            aot_only: false,
            jit_only: false,
            enable_profiling: false,
        }
    }
}

/// Decide the execution strategy for a class member
pub fn decide_strategy(member: &ClassMember, class: &ClassDecl, config: &StrategyConfig) -> ExecutionStrategy {
    // Rule 0: Command-line overrides
    if config.aot_only {
        return ExecutionStrategy::Aot;
    }
    if config.jit_only {
        return ExecutionStrategy::Jit;
    }

    match member {
        ClassMember::Method(method) => {
            // Rule 1: @reflectable annotation → JIT
            // Note: MethodModifier doesn't have Annotation variant yet
            // When annotations are added, check for @reflectable here
            // For now, check method name prefix as a convention
            if method.name.starts_with("reflect_") {
                return ExecutionStrategy::Jit;
            }

            // Rule 2: Virtual methods → AOT (vtable needs deterministic addresses)
            if method.modifiers.iter().any(|m| matches!(m, MethodModifier::Virtual)) {
                return if config.enable_profiling {
                    ExecutionStrategy::AotWithProfile
                } else {
                    ExecutionStrategy::Aot
                };
            }

            // Rule 3: Methods with dynamic type operations → JIT
            if has_dynamic_operations(&method.body) {
                return ExecutionStrategy::Jit;
            }

            // Rule 4: Default → AOT
            if config.enable_profiling {
                ExecutionStrategy::AotWithProfile
            } else {
                ExecutionStrategy::Aot
            }
        }
        ClassMember::Constructor(_) => {
            // Constructors always AOT (deterministic lifecycle management)
            ExecutionStrategy::Aot
        }
        ClassMember::Field(_) => {
            // Fields don't have execution strategy
            ExecutionStrategy::Aot
        }
        ClassMember::Destructor(_) => {
            // Destructors always AOT
            ExecutionStrategy::Aot
        }
    }
}

/// Decide strategy for a top-level function
pub fn decide_function_strategy(func: &FunctionDecl, config: &StrategyConfig) -> ExecutionStrategy {
    if config.aot_only {
        return ExecutionStrategy::Aot;
    }
    if config.jit_only {
        return ExecutionStrategy::Jit;
    }

    // Check for dynamic operations in function body
    if has_dynamic_operations_block(&func.body) {
        return ExecutionStrategy::Jit;
    }

    if config.enable_profiling {
        ExecutionStrategy::AotWithProfile
    } else {
        ExecutionStrategy::Aot
    }
}

/// Check if a method body contains dynamic type operations
fn has_dynamic_operations(body: &Option<Block>) -> bool {
    if let Some(block) = body {
        has_dynamic_operations_block(block)
    } else {
        false
    }
}

/// Check if a block contains dynamic type operations (instanceof, dynamic casts, match on types)
fn has_dynamic_operations_block(block: &Block) -> bool {
    for stmt in &block.statements {
        if has_dynamic_operations_stmt(stmt) {
            return true;
        }
    }
    false
}

fn has_dynamic_operations_stmt(stmt: &Stmt) -> bool {
    match stmt {
        Stmt::Expr(expr) => has_dynamic_operations_expr(expr),
        Stmt::Return(expr) => expr.as_ref().map_or(false, |e| has_dynamic_operations_expr(e)),
        Stmt::If(cond, then, else_) => {
            has_dynamic_operations_expr(cond)
                || has_dynamic_operations_stmt(then)
                || else_.as_ref().map_or(false, |s| has_dynamic_operations_stmt(s))
        }
        Stmt::While(cond, body) => {
            has_dynamic_operations_expr(cond) || has_dynamic_operations_stmt(body)
        }
        Stmt::For(init, cond, update, body) => {
            init.as_ref().map_or(false, |s| has_dynamic_operations_stmt(s))
                || cond.as_ref().map_or(false, |e| has_dynamic_operations_expr(e))
                || update.as_ref().map_or(false, |e| has_dynamic_operations_expr(e))
                || has_dynamic_operations_stmt(body)
        }
        Stmt::ForEach(_, _, iter, body) => {
            has_dynamic_operations_expr(iter) || has_dynamic_operations_stmt(body)
        }
        Stmt::Block(block) => has_dynamic_operations_block(block),
        Stmt::VarDecl(var) => {
            var.initializer.as_ref().map_or(false, |e| has_dynamic_operations_expr(e))
        }
        Stmt::Switch(scrutinee, cases, _) => {
            has_dynamic_operations_expr(scrutinee)
                || cases.iter().any(|c| c.patterns.iter().any(|p| has_dynamic_operations_expr(p)))
        }
        Stmt::Try(block, catches, finally) => {
            // Try/catch implies dynamic dispatch → JIT
            true
        }
        Stmt::Break | Stmt::Continue => false,
    }
}

fn has_dynamic_operations_expr(expr: &Expr) -> bool {
    match expr {
        Expr::InstanceOf(_, _) => true,
        Expr::Match(_, _) => true, // Match on types is dynamic
        Expr::Throw(_) => true,    // Exception handling is dynamic
        Expr::BinaryOp(_, left, right) => {
            has_dynamic_operations_expr(left) || has_dynamic_operations_expr(right)
        }
        Expr::UnaryOp(_, operand) => has_dynamic_operations_expr(operand),
        Expr::Call(callee, args) => {
            has_dynamic_operations_expr(callee) || args.iter().any(|a| has_dynamic_operations_expr(a))
        }
        Expr::MethodCall(obj, _, args) => {
            has_dynamic_operations_expr(obj) || args.iter().any(|a| has_dynamic_operations_expr(a))
        }
        Expr::FieldAccess(obj, _) => has_dynamic_operations_expr(obj),
        Expr::ArrayAccess(arr, idx) => {
            has_dynamic_operations_expr(arr) || has_dynamic_operations_expr(idx)
        }
        Expr::Assignment(target, value) => {
            has_dynamic_operations_expr(target) || has_dynamic_operations_expr(value)
        }
        Expr::Ternary(cond, then, else_) => {
            has_dynamic_operations_expr(cond)
                || has_dynamic_operations_expr(then)
                || has_dynamic_operations_expr(else_)
        }
        Expr::New(_, _, args) => args.iter().any(|a| has_dynamic_operations_expr(a)),
        Expr::Delete(obj) => has_dynamic_operations_expr(obj),
        Expr::Cast(_, inner) => has_dynamic_operations_expr(inner),
        Expr::Lambda(_, body) => {
            match body.as_ref() {
                LambdaBody::Expr(e) => has_dynamic_operations_expr(e),
                LambdaBody::Block(b) => has_dynamic_operations_block(b),
            }
        }
        _ => false,
    }
}

/// Strategy map: maps function/method names to their execution strategy
pub type StrategyMap = HashMap<String, ExecutionStrategy>;

use std::collections::HashMap;

/// Analyze a program and produce a strategy map
pub fn analyze_program(program: &Program, config: &StrategyConfig) -> StrategyMap {
    let mut strategies = StrategyMap::new();

    for decl in &program.declarations {
        match decl {
            Declaration::Class(class_) => {
                for member in &class_.members {
                    let strategy = decide_strategy(member, class_, config);
                    match member {
                        ClassMember::Method(method) => {
                            let name = format!("{}_{}", class_.name, method.name);
                            strategies.insert(name, strategy);
                        }
                        ClassMember::Constructor(_) => {
                            let name = format!("{}_ctor", class_.name);
                            strategies.insert(name, strategy);
                        }
                        ClassMember::Destructor(_) => {
                            let name = format!("{}_dtor", class_.name);
                            strategies.insert(name, strategy);
                        }
                        ClassMember::Field(_) => {}
                    }
                }
            }
            Declaration::Function(func) => {
                let strategy = decide_function_strategy(func, config);
                strategies.insert(func.name.clone(), strategy);
            }
            _ => {}
        }
    }

    strategies
}
