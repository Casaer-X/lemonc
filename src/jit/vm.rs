use crate::jit::bytecode::*;
use crate::jit::jit_compiler::{JitCompiler, JitState, JitFunction};
use std::collections::HashMap;
use std::io::Cursor;

/// VM Value types
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub enum VMValue {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Object(Box<VMObject>),
    Array(Vec<VMValue>),
    Map(Vec<(VMValue, VMValue)>),
    /// Raw pointer for native interop
    Ptr(*mut std::ffi::c_void),
}

// Allow VMValue::Ptr to be sent between threads safely
unsafe impl Send for VMValue {}
unsafe impl Sync for VMValue {}

impl VMValue {
    pub fn as_int(&self) -> i64 {
        match self {
            VMValue::Int(v) => *v,
            VMValue::Bool(b) => if *b { 1 } else { 0 },
            VMValue::Ptr(p) => *p as i64,
            _ => 0,
        }
    }

    pub fn as_float(&self) -> f64 {
        match self {
            VMValue::Float(v) => *v,
            VMValue::Int(v) => *v as f64,
            _ => 0.0,
        }
    }

    pub fn as_bool(&self) -> bool {
        match self {
            VMValue::Bool(b) => *b,
            VMValue::Int(v) => *v != 0,
            VMValue::Null => false,
            _ => true,
        }
    }

    pub fn as_string(&self) -> String {
        match self {
            VMValue::String(s) => s.clone(),
            VMValue::Int(v) => v.to_string(),
            VMValue::Float(v) => v.to_string(),
            VMValue::Bool(b) => b.to_string(),
            VMValue::Null => "null".to_string(),
            _ => format!("{:?}", self),
        }
    }

    pub fn is_truthy(&self) -> bool {
        match self {
            VMValue::Bool(b) => *b,
            VMValue::Int(v) => *v != 0,
            VMValue::Float(v) => *v != 0.0,
            VMValue::Null => false,
            VMValue::String(s) => !s.is_empty(),
            VMValue::Array(a) => !a.is_empty(),
            VMValue::Map(m) => !m.is_empty(),
            VMValue::Ptr(p) => !p.is_null(),
            VMValue::Object(_) => true,
        }
    }

    pub fn as_ptr(&self) -> *mut std::ffi::c_void {
        match self {
            VMValue::Ptr(p) => *p,
            VMValue::Int(v) => *v as *mut std::ffi::c_void,
            VMValue::Null => std::ptr::null_mut(),
            _ => std::ptr::null_mut(),
        }
    }

    pub fn from_ptr(p: *mut std::ffi::c_void) -> Self {
        VMValue::Ptr(p)
    }
}

/// VM Object
#[derive(Debug, Clone, PartialEq)]
#[repr(C)]
pub struct VMObject {
    pub class_idx: u32,
    pub fields: Vec<VMValue>,
}

/// Native function type: takes an array of VMValues, returns a VMValue
/// Uses C calling convention for FFI compatibility
pub type LeNativeFunc = extern "C" fn(args: *const VMValue, argc: usize) -> VMValue;

/// Call frame
#[derive(Debug)]
struct CallFrame {
    func_idx: u32,
    pc: usize,
    locals: Vec<VMValue>,
    stack_base: usize,
}

/// Lemon Embedded Virtual Machine
///
/// Can be created from a .lmb file or from in-memory bytecode data.
/// Supports registering native functions for AOT↔VM interop.
pub struct LeVM {
    module: BytecodeModule,
    stack: Vec<VMValue>,
    frames: Vec<CallFrame>,
    globals: Vec<VMValue>,
    halted: bool,
    /// Registered native functions (name → function pointer)
    native_functions: HashMap<String, LeNativeFunc>,
    /// JIT compilation threshold (0 = disabled)
    jit_threshold: u64,
    /// Execution counts for hot-spot detection
    execution_counts: HashMap<u32, u64>,
    /// JIT state: compiled functions and hot-spot tracking
    jit_state: JitState,
    /// JIT compiler instance
    jit_compiler: JitCompiler,
}

impl LeVM {
    /// Create a new LeVM from a BytecodeModule
    pub fn new(module: BytecodeModule) -> Self {
        let num_globals = module.globals.len();
        let mut jit_state = JitState::new();
        jit_state.hot_threshold = 100;
        Self {
            module,
            stack: Vec::new(),
            frames: Vec::new(),
            globals: vec![VMValue::Null; num_globals],
            halted: false,
            native_functions: HashMap::new(),
            jit_threshold: 100,
            execution_counts: HashMap::new(),
            jit_state,
            jit_compiler: JitCompiler::new(),
        }
    }

    /// Create a LeVM from raw .lmb bytes (for embedded use)
    pub fn from_bytes(lmb_data: &[u8]) -> Result<Self, String> {
        let mut cursor = Cursor::new(lmb_data);
        let module = crate::jit::serialize::read_module(&mut cursor)
            .map_err(|e| e.to_string())?;
        Ok(Self::new(module))
    }

    /// Get a reference to the bytecode module
    pub fn module_ref(&self) -> &BytecodeModule {
        &self.module
    }

    /// Register a native function that can be called from bytecode
    pub fn register_native(&mut self, name: &str, func: LeNativeFunc) {
        self.native_functions.insert(name.to_string(), func);
    }

    /// Check if a native function is registered
    pub fn has_native(&self, name: &str) -> bool {
        self.native_functions.contains_key(name)
    }

    /// Set the JIT compilation threshold (0 to disable)
    pub fn set_jit_threshold(&mut self, threshold: u64) {
        self.jit_threshold = threshold;
    }

    /// Run the entry point function
    pub fn run(&mut self) -> Result<VMValue, String> {
        let entry = self.module.entry_point;
        self.call_function(entry, Vec::new())?;

        while !self.halted && !self.frames.is_empty() {
            self.step()?;
        }

        Ok(self.stack.pop().unwrap_or(VMValue::Null))
    }

    /// Call a specific function by index with arguments
    ///
    /// This is the primary interface for native code to call into the VM.
    pub fn call(&mut self, func_idx: u32, args: Vec<VMValue>) -> Result<VMValue, String> {
        self.halted = false;
        self.call_function(func_idx, args)?;

        while !self.halted && !self.frames.is_empty() {
            self.step()?;
        }

        Ok(self.stack.pop().unwrap_or(VMValue::Null))
    }

    /// Call a function by name
    pub fn call_by_name(&mut self, name: &str, args: Vec<VMValue>) -> Result<VMValue, String> {
        for (idx, func) in self.module.functions.iter().enumerate() {
            if func.name == name {
                return self.call(idx as u32, args);
            }
        }
        Err(format!("Function '{}' not found", name))
    }

    /// Find a function index by name
    pub fn find_function(&self, name: &str) -> Option<u32> {
        for (idx, func) in self.module.functions.iter().enumerate() {
            if func.name == name {
                return Some(idx as u32);
            }
        }
        None
    }

    /// Reset the VM state (clear stack, frames, globals)
    pub fn reset(&mut self) {
        self.stack.clear();
        self.frames.clear();
        for g in self.globals.iter_mut() {
            *g = VMValue::Null;
        }
        self.halted = false;
    }

    fn step(&mut self) -> Result<(), String> {
        if self.frames.is_empty() {
            return Ok(());
        }

        let frame_idx = self.frames.len() - 1;
        let func_idx = self.frames[frame_idx].func_idx;
        let pc = self.frames[frame_idx].pc;

        if func_idx as usize >= self.module.functions.len() {
            return Err("Invalid function index".to_string());
        }

        let func = &self.module.functions[func_idx as usize];

        if pc >= func.code.len() {
            self.frames.pop();
            return Ok(());
        }

        let instr = func.code[pc].clone();
        self.frames[frame_idx].pc = pc + 1;

        // Hot-spot detection
        if self.jit_threshold > 0 {
            let count = self.execution_counts.entry(func_idx).or_insert(0);
            *count += 1;
            // Future: trigger JIT compilation when count >= threshold
        }

        match instr {
            Bytecode::PushConst(v) => self.stack.push(VMValue::Int(v)),
            Bytecode::PushFloat(v) => self.stack.push(VMValue::Float(v)),
            Bytecode::PushString(idx) => {
                if let Some(s) = self.module.string_pool.get(idx as usize) {
                    self.stack.push(VMValue::String(s.clone()));
                }
            }
            Bytecode::PushBool(b) => self.stack.push(VMValue::Bool(b)),
            Bytecode::PushNull => self.stack.push(VMValue::Null),
            Bytecode::Pop => { self.stack.pop(); }
            Bytecode::Dup => {
                if let Some(v) = self.stack.last() {
                    self.stack.push(v.clone());
                }
            }
            Bytecode::Swap => {
                let len = self.stack.len();
                if len >= 2 {
                    self.stack.swap(len - 1, len - 2);
                }
            }
            Bytecode::LoadLocal(idx) => {
                if let Some(frame) = self.frames.last() {
                    if let Some(v) = frame.locals.get(idx as usize) {
                        self.stack.push(v.clone());
                    }
                }
            }
            Bytecode::StoreLocal(idx) => {
                if let Some(v) = self.stack.pop() {
                    if let Some(frame) = self.frames.last_mut() {
                        while frame.locals.len() <= idx as usize {
                            frame.locals.push(VMValue::Null);
                        }
                        frame.locals[idx as usize] = v;
                    }
                }
            }
            Bytecode::LoadGlobal(idx) => {
                if let Some(v) = self.globals.get(idx as usize) {
                    self.stack.push(v.clone());
                }
            }
            Bytecode::StoreGlobal(idx) => {
                if let Some(v) = self.stack.pop() {
                    while self.globals.len() <= idx as usize {
                        self.globals.push(VMValue::Null);
                    }
                    self.globals[idx as usize] = v;
                }
            }
            Bytecode::LoadField(idx) => {
                if let Some(VMValue::Object(obj)) = self.stack.pop() {
                    if let Some(v) = obj.fields.get(idx as usize) {
                        self.stack.push(v.clone());
                    } else {
                        self.stack.push(VMValue::Null);
                    }
                }
            }
            Bytecode::StoreField(idx) => {
                let value = self.stack.pop().unwrap_or(VMValue::Null);
                if let Some(VMValue::Object(mut obj)) = self.stack.pop() {
                    while obj.fields.len() <= idx as usize {
                        obj.fields.push(VMValue::Null);
                    }
                    obj.fields[idx as usize] = value;
                    self.stack.push(VMValue::Object(obj));
                }
            }
            Bytecode::Add => self.binop(|a, b| {
                match (&a, &b) {
                    (VMValue::Int(x), VMValue::Int(y)) => VMValue::Int(x + y),
                    (VMValue::Float(x), VMValue::Float(y)) => VMValue::Float(x + y),
                    (VMValue::Int(x), VMValue::Float(y)) => VMValue::Float(*x as f64 + y),
                    (VMValue::Float(x), VMValue::Int(y)) => VMValue::Float(x + *y as f64),
                    (VMValue::String(x), VMValue::String(y)) => VMValue::String(x.clone() + y),
                    _ => VMValue::Int(a.as_int() + b.as_int()),
                }
            })?,
            Bytecode::Sub => self.binop(|a, b| {
                match (&a, &b) {
                    (VMValue::Int(x), VMValue::Int(y)) => VMValue::Int(x - y),
                    (VMValue::Float(x), VMValue::Float(y)) => VMValue::Float(x - y),
                    (VMValue::Int(x), VMValue::Float(y)) => VMValue::Float(*x as f64 - y),
                    (VMValue::Float(x), VMValue::Int(y)) => VMValue::Float(x - *y as f64),
                    _ => VMValue::Int(a.as_int() - b.as_int()),
                }
            })?,
            Bytecode::Mul => self.binop(|a, b| {
                match (&a, &b) {
                    (VMValue::Int(x), VMValue::Int(y)) => VMValue::Int(x * y),
                    (VMValue::Float(x), VMValue::Float(y)) => VMValue::Float(x * y),
                    (VMValue::Int(x), VMValue::Float(y)) => VMValue::Float(*x as f64 * y),
                    (VMValue::Float(x), VMValue::Int(y)) => VMValue::Float(x * *y as f64),
                    _ => VMValue::Int(a.as_int() * b.as_int()),
                }
            })?,
            Bytecode::Div => self.binop(|a, b| {
                match (&a, &b) {
                    (VMValue::Int(x), VMValue::Int(y)) => if *y != 0 { VMValue::Int(x / y) } else { VMValue::Int(0) },
                    (VMValue::Float(x), VMValue::Float(y)) => if *y != 0.0 { VMValue::Float(x / y) } else { VMValue::Float(0.0) },
                    _ => VMValue::Int(a.as_int() / b.as_int().max(1)),
                }
            })?,
            Bytecode::Mod => self.binop(|a, b| {
                match (&a, &b) {
                    (VMValue::Int(x), VMValue::Int(y)) => if *y != 0 { VMValue::Int(x % y) } else { VMValue::Int(0) },
                    _ => VMValue::Int(a.as_int() % b.as_int().max(1)),
                }
            })?,
            Bytecode::Neg => {
                if let Some(v) = self.stack.pop() {
                    match v {
                        VMValue::Int(x) => self.stack.push(VMValue::Int(-x)),
                        VMValue::Float(x) => self.stack.push(VMValue::Float(-x)),
                        _ => self.stack.push(VMValue::Int(-v.as_int())),
                    }
                }
            }
            Bytecode::BitAnd => self.binop(|a, b| VMValue::Int(a.as_int() & b.as_int()))?,
            Bytecode::BitOr => self.binop(|a, b| VMValue::Int(a.as_int() | b.as_int()))?,
            Bytecode::BitXor => self.binop(|a, b| VMValue::Int(a.as_int() ^ b.as_int()))?,
            Bytecode::BitNot => {
                if let Some(v) = self.stack.pop() {
                    self.stack.push(VMValue::Int(!v.as_int()));
                }
            }
            Bytecode::Shl => self.binop(|a, b| VMValue::Int(a.as_int() << b.as_int()))?,
            Bytecode::Shr => self.binop(|a, b| VMValue::Int(a.as_int() >> b.as_int()))?,
            Bytecode::Eq => self.binop(|a, b| VMValue::Bool(a == b))?,
            Bytecode::Ne => self.binop(|a, b| VMValue::Bool(a != b))?,
            Bytecode::Lt => self.binop(|a, b| {
                let result = match (&a, &b) {
                    (VMValue::Float(fa), VMValue::Float(fb)) => *fa < *fb,
                    (VMValue::Int(ia), VMValue::Float(fb)) => (*ia as f64) < *fb,
                    (VMValue::Float(fa), VMValue::Int(ib)) => *fa < (*ib as f64),
                    _ => a.as_int() < b.as_int(),
                };
                VMValue::Bool(result)
            })?,
            Bytecode::Gt => self.binop(|a, b| {
                let result = match (&a, &b) {
                    (VMValue::Float(fa), VMValue::Float(fb)) => *fa > *fb,
                    (VMValue::Int(ia), VMValue::Float(fb)) => (*ia as f64) > *fb,
                    (VMValue::Float(fa), VMValue::Int(ib)) => *fa > (*ib as f64),
                    _ => a.as_int() > b.as_int(),
                };
                VMValue::Bool(result)
            })?,
            Bytecode::Le => self.binop(|a, b| {
                let result = match (&a, &b) {
                    (VMValue::Float(fa), VMValue::Float(fb)) => *fa <= *fb,
                    (VMValue::Int(ia), VMValue::Float(fb)) => (*ia as f64) <= *fb,
                    (VMValue::Float(fa), VMValue::Int(ib)) => *fa <= (*ib as f64),
                    _ => a.as_int() <= b.as_int(),
                };
                VMValue::Bool(result)
            })?,
            Bytecode::Ge => self.binop(|a, b| {
                let result = match (&a, &b) {
                    (VMValue::Float(fa), VMValue::Float(fb)) => *fa >= *fb,
                    (VMValue::Int(ia), VMValue::Float(fb)) => (*ia as f64) >= *fb,
                    (VMValue::Float(fa), VMValue::Int(ib)) => *fa >= (*ib as f64),
                    _ => a.as_int() >= b.as_int(),
                };
                VMValue::Bool(result)
            })?,
            Bytecode::And => {
                let b = self.stack.pop().unwrap_or(VMValue::Bool(false));
                let a = self.stack.pop().unwrap_or(VMValue::Bool(false));
                self.stack.push(VMValue::Bool(a.is_truthy() && b.is_truthy()));
            }
            Bytecode::Or => {
                let b = self.stack.pop().unwrap_or(VMValue::Bool(false));
                let a = self.stack.pop().unwrap_or(VMValue::Bool(false));
                self.stack.push(VMValue::Bool(a.is_truthy() || b.is_truthy()));
            }
            Bytecode::Not => {
                if let Some(v) = self.stack.pop() {
                    self.stack.push(VMValue::Bool(!v.is_truthy()));
                }
            }
            Bytecode::Jump(offset) => {
                if let Some(frame) = self.frames.last_mut() {
                    frame.pc = offset as usize;
                }
            }
            Bytecode::JumpIf(offset) => {
                if let Some(v) = self.stack.pop() {
                    if v.is_truthy() {
                        if let Some(frame) = self.frames.last_mut() {
                            frame.pc = offset as usize;
                        }
                    }
                }
            }
            Bytecode::JumpIfNot(offset) => {
                if let Some(v) = self.stack.pop() {
                    if !v.is_truthy() {
                        if let Some(frame) = self.frames.last_mut() {
                            frame.pc = offset as usize;
                        }
                    }
                }
            }
            Bytecode::Call(func_idx, argc) => {
                let mut args = Vec::new();
                for _ in 0..argc {
                    args.push(self.stack.pop().unwrap_or(VMValue::Null));
                }
                args.reverse();
                self.call_function(func_idx, args)?;
            }
            Bytecode::CallMethod(func_idx, argc) => {
                let mut args = Vec::new();
                for _ in 0..argc {
                    args.push(self.stack.pop().unwrap_or(VMValue::Null));
                }
                args.reverse();
                // 'self' should be on stack - pop it and add as first arg
                if let Some(self_obj) = self.stack.pop() {
                    args.insert(0, self_obj);
                }
                self.call_function(func_idx, args)?;
            }
            Bytecode::CallNative(name_idx) => {
                // Look up the native function name from the string pool
                let name = self.module.string_pool.get(name_idx as usize)
                    .cloned()
                    .unwrap_or_default();
                if let Some(&native_func) = self.native_functions.get(&name) {
                    // Pop argc from stack, then pop that many args
                    let argc = self.stack.pop()
                        .and_then(|v| if let VMValue::Int(n) = v { Some(n as usize) } else { None })
                        .unwrap_or(0);
                    let mut args = Vec::with_capacity(argc);
                    for _ in 0..argc {
                        args.push(self.stack.pop().unwrap_or(VMValue::Null));
                    }
                    args.reverse();
                    let result = native_func(args.as_ptr(), args.len());
                    self.stack.push(result);
                } else {
                    return Err(format!("Native function '{}' not registered", name));
                }
            }
            Bytecode::Return => {
                let ret_val = self.stack.pop().unwrap_or(VMValue::Null);
                self.frames.pop();
                self.stack.push(ret_val);
            }
            Bytecode::New(class_idx) => {
                if let Some(class) = self.module.classes.get(class_idx as usize) {
                    let obj = VMObject {
                        class_idx,
                        fields: vec![VMValue::Null; class.fields.len()],
                    };
                    self.stack.push(VMValue::Object(Box::new(obj)));
                }
            }
            Bytecode::NewArray => {
                if let Some(VMValue::Int(len)) = self.stack.pop() {
                    self.stack.push(VMValue::Array(vec![VMValue::Null; len as usize]));
                }
            }
            Bytecode::ArrayGet => {
                let idx = self.stack.pop().unwrap_or(VMValue::Int(0));
                if let Some(VMValue::Array(arr)) = self.stack.pop() {
                    let i = idx.as_int() as usize;
                    if i < arr.len() {
                        self.stack.push(arr[i].clone());
                    } else {
                        self.stack.push(VMValue::Null);
                    }
                }
            }
            Bytecode::ArraySet => {
                let value = self.stack.pop().unwrap_or(VMValue::Null);
                let idx = self.stack.pop().unwrap_or(VMValue::Int(0));
                if let Some(VMValue::Array(mut arr)) = self.stack.pop() {
                    let i = idx.as_int() as usize;
                    if i < arr.len() {
                        arr[i] = value;
                    }
                    self.stack.push(VMValue::Array(arr));
                }
            }
            Bytecode::ArrayLen => {
                if let Some(VMValue::Array(arr)) = self.stack.pop() {
                    self.stack.push(VMValue::Int(arr.len() as i64));
                }
            }
            Bytecode::Delete => {
                self.stack.pop(); // Just pop for now
            }
            Bytecode::ArrayPush => {
                let value = self.stack.pop().unwrap_or(VMValue::Null);
                if let Some(VMValue::Array(mut arr)) = self.stack.pop() {
                    arr.push(value);
                    self.stack.push(VMValue::Array(arr));
                }
            }
            Bytecode::MapNew => {
                self.stack.push(VMValue::Map(Vec::new()));
            }
            Bytecode::MapGet => {
                let key = self.stack.pop().unwrap_or(VMValue::Null);
                if let Some(VMValue::Map(map)) = self.stack.pop() {
                    let result = map.iter().find(|(k, _)| *k == key).map(|(_, v)| v.clone()).unwrap_or(VMValue::Null);
                    self.stack.push(result);
                }
            }
            Bytecode::MapPut => {
                let value = self.stack.pop().unwrap_or(VMValue::Null);
                let key = self.stack.pop().unwrap_or(VMValue::Null);
                if let Some(VMValue::Map(mut map)) = self.stack.pop() {
                    if let Some(entry) = map.iter_mut().find(|(k, _)| *k == key) {
                        entry.1 = value;
                    } else {
                        map.push((key, value));
                    }
                    self.stack.push(VMValue::Map(map));
                }
            }
            Bytecode::MapContains => {
                let key = self.stack.pop().unwrap_or(VMValue::Null);
                if let Some(VMValue::Map(map)) = self.stack.pop() {
                    self.stack.push(VMValue::Bool(map.iter().any(|(k, _)| *k == key)));
                }
            }
            Bytecode::MapLen => {
                if let Some(VMValue::Map(map)) = self.stack.pop() {
                    self.stack.push(VMValue::Int(map.len() as i64));
                }
            }
            Bytecode::MapRemove => {
                let key = self.stack.pop().unwrap_or(VMValue::Null);
                if let Some(VMValue::Map(mut map)) = self.stack.pop() {
                    map.retain(|(k, _)| *k != key);
                    self.stack.push(VMValue::Map(map));
                }
            }
            Bytecode::MapKeys => {
                if let Some(VMValue::Map(map)) = self.stack.pop() {
                    let keys: Vec<VMValue> = map.iter().map(|(k, _)| k.clone()).collect();
                    self.stack.push(VMValue::Array(keys));
                }
            }
            Bytecode::StringConcat => {
                let b = self.stack.pop().unwrap_or(VMValue::Null);
                let a = self.stack.pop().unwrap_or(VMValue::Null);
                self.stack.push(VMValue::String(format!("{}{}", a.as_string(), b.as_string())));
            }
            Bytecode::StringLen => {
                if let Some(VMValue::String(s)) = self.stack.pop() {
                    self.stack.push(VMValue::Int(s.len() as i64));
                }
            }
            Bytecode::StringEquals => {
                let b = self.stack.pop().unwrap_or(VMValue::Null);
                let a = self.stack.pop().unwrap_or(VMValue::Null);
                self.stack.push(VMValue::Bool(a.as_string() == b.as_string()));
            }
            Bytecode::CheckNotNull => {
                // Assert top of stack is not null - just peek, don't pop
                if let Some(v) = self.stack.last() {
                    if *v == VMValue::Null {
                        return Err("NullPointerException".to_string());
                    }
                }
            }
            Bytecode::FAdd => self.binop(|a, b| VMValue::Float(a.as_float() + b.as_float()))?,
            Bytecode::FSub => self.binop(|a, b| VMValue::Float(a.as_float() - b.as_float()))?,
            Bytecode::FMul => self.binop(|a, b| VMValue::Float(a.as_float() * b.as_float()))?,
            Bytecode::FDiv => self.binop(|a, b| {
                let divisor = b.as_float();
                if divisor != 0.0 { VMValue::Float(a.as_float() / divisor) } else { VMValue::Float(0.0) }
            })?,
            Bytecode::FCmp => {
                let b = self.stack.pop().ok_or("Stack underflow")?;
                let a = self.stack.pop().ok_or("Stack underflow")?;
                let cmp = a.as_float().partial_cmp(&b.as_float()).unwrap_or(std::cmp::Ordering::Equal);
                self.stack.push(VMValue::Int(cmp as i64));
            }
            Bytecode::IncLocal(idx, delta) => {
                if let Some(frame) = self.frames.last_mut() {
                    while frame.locals.len() <= idx as usize {
                        frame.locals.push(VMValue::Null);
                    }
                    let current = frame.locals[idx as usize].as_int();
                    frame.locals[idx as usize] = VMValue::Int(current + delta as i64);
                }
            }
            Bytecode::InvokeVirtual(vtable_idx, argc) => {
                let mut args = Vec::new();
                for _ in 0..argc {
                    args.push(self.stack.pop().unwrap_or(VMValue::Null));
                }
                args.reverse();
                // 'self' should be on stack - pop it
                let self_obj = self.stack.pop().unwrap_or(VMValue::Null);
                // Look up method from vtable
                if let VMValue::Object(ref obj) = self_obj {
                    if let Some(class) = self.module.classes.get(obj.class_idx as usize) {
                        if let Some(&func_idx) = class.vtable.get(vtable_idx as usize) {
                            let mut call_args = vec![self_obj];
                            call_args.extend(args);
                            self.call_function(func_idx, call_args)?;
                            return Ok(());
                        }
                    }
                }
                self.stack.push(VMValue::Null);
            }
            Bytecode::Cast(type_idx) => {
                // Type casting: for numeric types, perform conversion; for objects, pass through
                if let Some(v) = self.stack.pop() {
                    let type_name = self.module.string_pool.get(type_idx as usize)
                        .cloned()
                        .unwrap_or_default();
                    let result = match type_name.as_str() {
                        "Int" | "int" | "i64" => VMValue::Int(v.as_int()),
                        "Float" | "float" | "f64" | "Double" | "double" => {
                            match v {
                                VMValue::Int(n) => VMValue::Float(n as f64),
                                VMValue::Float(f) => VMValue::Float(f),
                                VMValue::Bool(b) => VMValue::Float(if b { 1.0 } else { 0.0 }),
                                _ => VMValue::Float(0.0),
                            }
                        }
                        "Bool" | "bool" => VMValue::Bool(v.is_truthy()),
                        "String" | "string" => VMValue::String(v.as_string()),
                        _ => v, // Object casts: pass through (runtime type checking would go here)
                    };
                    self.stack.push(result);
                }
            }
            Bytecode::InstanceOf(type_idx) => {
                // Check if value is an instance of the given type
                if let Some(v) = self.stack.pop() {
                    let type_name = self.module.string_pool.get(type_idx as usize)
                        .cloned()
                        .unwrap_or_default();
                    let result = match type_name.as_str() {
                        "Int" | "int" | "i64" => matches!(v, VMValue::Int(_)),
                        "Float" | "float" | "f64" | "Double" | "double" => matches!(v, VMValue::Float(_)),
                        "Bool" | "bool" => matches!(v, VMValue::Bool(_)),
                        "String" | "string" => matches!(v, VMValue::String(_)),
                        "Null" => matches!(v, VMValue::Null),
                        _ => {
                            // For class types, check Object's class_idx against module classes
                            match &v {
                                VMValue::Object(obj) => {
                                    // Look up the class name from the module
                                    self.module.classes.get(obj.class_idx as usize)
                                        .map(|c| c.name == type_name)
                                        .unwrap_or(false)
                                }
                                _ => false,
                            }
                        }
                    };
                    self.stack.push(VMValue::Bool(result));
                }
            }
            Bytecode::TypeId => {
                // Return a type identifier for the top value
                if let Some(v) = self.stack.pop() {
                    let type_id = match v {
                        VMValue::Null => 0,
                        VMValue::Int(_) => 1,
                        VMValue::Float(_) => 2,
                        VMValue::Bool(_) => 3,
                        VMValue::String(_) => 4,
                        VMValue::Ptr(_) => 5,
                        VMValue::Object(ref obj) => {
                            // Use class_idx as type ID (offset by 100 to distinguish from primitives)
                            100 + obj.class_idx as i64
                        }
                        VMValue::Array(_) => 6,
                        VMValue::Map(_) => 7,
                    };
                    self.stack.push(VMValue::Int(type_id));
                }
            }
            Bytecode::Print => {
                if let Some(v) = self.stack.pop() {
                    print!("{}", v.as_string());
                }
            }
            Bytecode::Println => {
                if let Some(v) = self.stack.pop() {
                    println!("{}", v.as_string());
                }
            }
            Bytecode::Printf(argc) => {
                let mut args = Vec::new();
                for _ in 0..argc {
                    if let Some(v) = self.stack.pop() {
                        args.push(v);
                    }
                }
                args.reverse();
                if let Some(format_val) = args.get(0) {
                    let format_str = format_val.as_string();
                    let mut arg_idx = 1;
                    let mut result = String::new();
                    let mut chars = format_str.chars().peekable();
                    while let Some(c) = chars.next() {
                        if c == '%' {
                            if let Some(next) = chars.next() {
                                match next {
                                    'd' | 'i' => {
                                        if let Some(arg) = args.get(arg_idx) {
                                            result.push_str(&arg.as_int().to_string());
                                            arg_idx += 1;
                                        }
                                    }
                                    's' => {
                                        if let Some(arg) = args.get(arg_idx) {
                                            result.push_str(&arg.as_string());
                                            arg_idx += 1;
                                        }
                                    }
                                    'f' => {
                                        if let Some(arg) = args.get(arg_idx) {
                                            result.push_str(&arg.as_float().to_string());
                                            arg_idx += 1;
                                        }
                                    }
                                    '%' => result.push('%'),
                                    _ => {
                                        result.push('%');
                                        result.push(next);
                                    }
                                }
                            }
                        } else if c == '\\' {
                            if let Some(next) = chars.next() {
                                match next {
                                    'n' => result.push('\n'),
                                    't' => result.push('\t'),
                                    'r' => result.push('\r'),
                                    '\\' => result.push('\\'),
                                    _ => {
                                        result.push('\\');
                                        result.push(next);
                                    }
                                }
                            }
                        } else {
                            result.push(c);
                        }
                    }
                    print!("{}", result);
                    use std::io::Write;
                    let _ = std::io::stdout().flush();
                }
            }
            Bytecode::Halt => {
                self.halted = true;
            }
            Bytecode::Nop => {}
        }

        Ok(())
    }

    fn binop<F>(&mut self, op: F) -> Result<(), String>
    where
        F: FnOnce(&VMValue, &VMValue) -> VMValue,
    {
        let b = self.stack.pop().ok_or("Stack underflow")?;
        let a = self.stack.pop().ok_or("Stack underflow")?;
        self.stack.push(op(&a, &b));
        Ok(())
    }

    fn call_function(&mut self, func_idx: u32, args: Vec<VMValue>) -> Result<(), String> {
        if func_idx as usize >= self.module.functions.len() {
            return Err(format!("Function {} not found", func_idx));
        }

        // Hot-spot detection: increment execution count and trigger JIT if threshold reached
        if self.jit_threshold > 0 {
            let count = self.execution_counts.entry(func_idx).or_insert(0);
            *count += 1;

            if *count >= self.jit_threshold && !self.jit_state.is_compiled(func_idx) {
                // Trigger JIT compilation for this hot function
                let func = &self.module.functions[func_idx as usize];
                if let Some(jit_func) = self.jit_compiler.compile_function(func, &self.module) {
                    eprintln!("[JIT] Compiled hot function #{}: {} (executed {} times)",
                        func_idx, func.name, count);
                    self.jit_state.register_compiled(func_idx, jit_func);
                }
            }
        }

        let func = &self.module.functions[func_idx as usize];
        let mut locals = vec![VMValue::Null; func.locals as usize];

        // Copy args to locals
        for (i, arg) in args.iter().enumerate() {
            if i < locals.len() {
                locals[i] = arg.clone();
            }
        }

        self.frames.push(CallFrame {
            func_idx,
            pc: 0,
            locals,
            stack_base: self.stack.len(),
        });

        Ok(())
    }

    /// Check if a function has been JIT compiled
    pub fn is_jit_compiled(&self, func_idx: u32) -> bool {
        self.jit_state.is_compiled(func_idx)
    }

    /// Get the JIT compiled function info
    pub fn get_jit_function(&self, func_idx: u32) -> Option<&JitFunction> {
        self.jit_state.get_compiled(func_idx)
    }

    /// Get execution count for a function
    pub fn get_execution_count(&self, func_idx: u32) -> u64 {
        *self.execution_counts.get(&func_idx).unwrap_or(&0)
    }

    /// Force JIT compilation of a specific function
    pub fn force_jit_compile(&mut self, func_idx: u32) -> Result<(), String> {
        if func_idx as usize >= self.module.functions.len() {
            return Err(format!("Function {} not found", func_idx));
        }
        if self.jit_state.is_compiled(func_idx) {
            return Ok(()); // Already compiled
        }
        let func = &self.module.functions[func_idx as usize];
        match self.jit_compiler.compile_function(func, &self.module) {
            Some(jit_func) => {
                self.jit_state.register_compiled(func_idx, jit_func);
                Ok(())
            }
            None => Err(format!("Failed to JIT compile function {}", func.name)),
        }
    }
}

/// Backward compatibility: VM is now an alias for LeVM
pub type VM = LeVM;
