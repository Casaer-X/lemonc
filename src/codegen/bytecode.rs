use crate::ast::node::*;
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum BytecodeOp {
    // Stack operations
    PushConst(u16),      // Push constant from pool
    PushLocal(u16),      // Push local variable
    PopLocal(u16),       // Pop to local variable
    Pop,                 // Pop top of stack
    Dup,                 // Duplicate top of stack
    Swap,                // Swap top two elements

    // Arithmetic
    Add,                 // Add top two stack elements
    Sub,                 // Subtract
    Mul,                 // Multiply
    Div,                 // Divide
    Mod,                 // Modulo
    Neg,                 // Negate

    // Comparison
    Eq,                  // Equal
    Ne,                  // Not equal
    Lt,                  // Less than
    Le,                  // Less than or equal
    Gt,                  // Greater than
    Ge,                  // Greater than or equal

    // Bitwise
    BitAnd,              // Bitwise AND
    BitOr,               // Bitwise OR
    BitXor,              // Bitwise XOR
    BitNot,              // Bitwise NOT
    Shl,                 // Shift left
    Shr,                 // Shift right

    // Logical
    LAnd,                // Logical AND
    LOr,                 // Logical OR
    LNot,                // Logical NOT

    // Control flow
    Jmp(i32),            // Unconditional jump (offset)
    Jz(i32),             // Jump if zero
    Jnz(i32),            // Jump if not zero
    Call(u16),           // Call function by index
    CallVirt(u16),       // Call virtual method
    Ret,                 // Return
    RetVal,              // Return with value

    // Object operations
    New(u16),            // Create new object by class index
    GetField(u16),       // Get field by index
    SetField(u16),       // Set field by index
    GetStatic(u16),      // Get static field
    SetStatic(u16),      // Set static field
    LoadThis,            // Load 'this' reference
    LoadNull,            // Load null

    // Array operations
    NewArray,            // Create new array
    ArrayGet,            // Array element get
    ArraySet,            // Array element set
    ArrayLen,            // Array length

    // String operations
    StrConcat,           // String concatenation
    StrEq,               // String equality

    // Type operations
    IsInstance(u16),     // Check instance type
    Cast(u16),           // Type cast

    // External calls
    ExternCall(u16),     // Call external function (e.g., printf)

    // Debug
    Breakpoint,          // Debugger breakpoint
    Nop,                 // No operation
}

#[derive(Debug, Clone)]
pub struct BytecodeFunction {
    pub name: String,
    pub params: Vec<String>,
    pub locals: Vec<String>,
    pub code: Vec<BytecodeOp>,
    pub max_stack: u16,
}

#[derive(Debug, Clone)]
pub struct BytecodeClass {
    pub name: String,
    pub parent: Option<String>,
    pub fields: Vec<(String, TypeRef)>,
    pub methods: Vec<String>,
    pub vtable: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct BytecodeProgram {
    pub constants: Vec<Constant>,
    pub functions: Vec<BytecodeFunction>,
    pub classes: Vec<BytecodeClass>,
    pub function_map: HashMap<String, usize>,
    pub class_map: HashMap<String, usize>,
    pub entry_point: Option<String>,
}

#[derive(Debug, Clone)]
pub enum Constant {
    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),
    Null,
}

impl BytecodeProgram {
    pub fn new() -> Self {
        Self {
            constants: Vec::new(),
            functions: Vec::new(),
            classes: Vec::new(),
            function_map: HashMap::new(),
            class_map: HashMap::new(),
            entry_point: None,
        }
    }

    pub fn add_constant(&mut self, c: Constant) -> u16 {
        let idx = self.constants.len() as u16;
        self.constants.push(c);
        idx
    }

    pub fn add_function(&mut self, f: BytecodeFunction) -> u16 {
        let idx = self.functions.len() as u16;
        self.function_map.insert(f.name.clone(), idx as usize);
        self.functions.push(f);
        idx
    }

    pub fn add_class(&mut self, c: BytecodeClass) -> u16 {
        let idx = self.classes.len() as u16;
        self.class_map.insert(c.name.clone(), idx as usize);
        self.classes.push(c);
        idx
    }

    pub fn get_function_idx(&self, name: &str) -> Option<u16> {
        self.function_map.get(name).map(|&i| i as u16)
    }

    pub fn get_class_idx(&self, name: &str) -> Option<u16> {
        self.class_map.get(name).map(|&i| i as u16)
    }
}
