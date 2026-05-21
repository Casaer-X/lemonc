use crate::ast::node::BinaryOp;

/// Lemon JIT Bytecode Instructions
/// Stack-based VM with accumulator register for efficiency
#[derive(Debug, Clone, PartialEq)]
pub enum Bytecode {
    // Stack operations
    PushConst(i64),           // Push constant integer
    PushFloat(f64),           // Push constant float
    PushString(u32),          // Push string from constant pool (index)
    PushBool(bool),           // Push boolean
    PushNull,                 // Push null
    Pop,                      // Pop top of stack
    Dup,                      // Duplicate top of stack
    Swap,                     // Swap top two stack elements

    // Variable operations
    LoadLocal(u32),           // Load local variable by index
    StoreLocal(u32),          // Store to local variable
    LoadGlobal(u32),          // Load global variable by index
    StoreGlobal(u32),         // Store to global variable
    LoadField(u32),           // Load object field
    StoreField(u32),          // Store to object field

    // Arithmetic operations (pop two, push result)
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Neg,                      // Negate top of stack

    // Bitwise operations
    BitAnd,
    BitOr,
    BitXor,
    BitNot,
    Shl,
    Shr,

    // Comparison operations
    Eq,
    Ne,
    Lt,
    Gt,
    Le,
    Ge,

    // Logical operations
    And,
    Or,
    Not,

    // Control flow
    Jump(u32),                // Unconditional jump (offset)
    JumpIf(u32),              // Jump if top of stack is true
    JumpIfNot(u32),           // Jump if top of stack is false
    Call(u32, u32),           // Call function (func_idx, argc)
    CallMethod(u32, u32),     // Call method (method_idx, argc)
    Return,                   // Return from function

    // Object operations
    New(u32),                 // Create new object (class_idx)
    NewArray,                 // Create new array
    ArrayGet,                 // Get array element
    ArraySet,                 // Set array element
    ArrayLen,                 // Get array length
    Delete,                   // Delete object

    // Type operations
    Cast(u32),                // Cast to type
    InstanceOf(u32),          // Check instance of class
    TypeId,                   // Get type id

    // Special
    Print,                    // Print top of stack
    Println,                  // Print with newline
    Printf(u32),              // Print formatted string (argc)
    Halt,                     // Stop execution
    Nop,                      // No operation
}

/// Bytecode function metadata
#[derive(Debug, Clone)]
pub struct BytecodeFunction {
    pub name: String,
    pub params: Vec<String>,
    pub locals: u32,           // Number of local variables
    pub code: Vec<Bytecode>,
    pub is_static: bool,
    pub class_name: Option<String>, // For methods
}

/// Bytecode class metadata
#[derive(Debug, Clone)]
pub struct BytecodeClass {
    pub name: String,
    pub parent: Option<String>,
    pub fields: Vec<String>,
    pub methods: Vec<u32>,     // Indices into function table
    pub vtable: Vec<u32>,      // Virtual method indices
}

/// Bytecode module - complete compiled program
#[derive(Debug, Clone)]
pub struct BytecodeModule {
    pub string_pool: Vec<String>,
    pub functions: Vec<BytecodeFunction>,
    pub classes: Vec<BytecodeClass>,
    pub globals: Vec<String>,
    pub entry_point: u32,      // Index of main function
}

impl BytecodeModule {
    pub fn new() -> Self {
        Self {
            string_pool: Vec::new(),
            functions: Vec::new(),
            classes: Vec::new(),
            globals: Vec::new(),
            entry_point: 0,
        }
    }

    pub fn add_string(&mut self, s: &str) -> u32 {
        if let Some(idx) = self.string_pool.iter().position(|x| x == s) {
            idx as u32
        } else {
            let idx = self.string_pool.len() as u32;
            self.string_pool.push(s.to_string());
            idx
        }
    }

    pub fn add_function(&mut self, func: BytecodeFunction) -> u32 {
        let idx = self.functions.len() as u32;
        self.functions.push(func);
        idx
    }

    pub fn add_class(&mut self, class: BytecodeClass) -> u32 {
        let idx = self.classes.len() as u32;
        self.classes.push(class);
        idx
    }
}

impl Default for BytecodeModule {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert AST BinaryOp to bytecode operation
pub fn binop_to_bytecode(op: &BinaryOp) -> Bytecode {
    match op {
        BinaryOp::Add => Bytecode::Add,
        BinaryOp::Sub => Bytecode::Sub,
        BinaryOp::Mul => Bytecode::Mul,
        BinaryOp::Div => Bytecode::Div,
        BinaryOp::Mod => Bytecode::Mod,
        BinaryOp::Eq => Bytecode::Eq,
        BinaryOp::Ne => Bytecode::Ne,
        BinaryOp::Lt => Bytecode::Lt,
        BinaryOp::Gt => Bytecode::Gt,
        BinaryOp::Le => Bytecode::Le,
        BinaryOp::Ge => Bytecode::Ge,
        BinaryOp::And => Bytecode::And,
        BinaryOp::Or => Bytecode::Or,
        BinaryOp::BitAnd => Bytecode::BitAnd,
        BinaryOp::BitOr => Bytecode::BitOr,
        BinaryOp::BitXor => Bytecode::BitXor,
        BinaryOp::Shl => Bytecode::Shl,
        BinaryOp::Shr => Bytecode::Shr,
        BinaryOp::NullCoalesce => Bytecode::Shr, // fallback: not directly supported
    }
}
