use crate::jit::bytecode::*;
use std::collections::HashMap;

/// VM Value types
#[derive(Debug, Clone, PartialEq)]
pub enum VMValue {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Object(Box<VMObject>),
    Array(Vec<VMValue>),
}

impl VMValue {
    pub fn as_int(&self) -> i64 {
        match self {
            VMValue::Int(v) => *v,
            VMValue::Bool(b) => if *b { 1 } else { 0 },
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
            _ => true,
        }
    }
}

/// VM Object
#[derive(Debug, Clone, PartialEq)]
pub struct VMObject {
    pub class_idx: u32,
    pub fields: Vec<VMValue>,
}

/// Call frame
#[derive(Debug)]
struct CallFrame {
    func_idx: u32,
    pc: usize,
    locals: Vec<VMValue>,
    stack_base: usize,
}

/// Virtual Machine
pub struct VM {
    module: BytecodeModule,
    stack: Vec<VMValue>,
    frames: Vec<CallFrame>,
    globals: Vec<VMValue>,
    halted: bool,
}

impl VM {
    pub fn new(module: BytecodeModule) -> Self {
        let num_globals = module.globals.len();
        Self {
            module,
            stack: Vec::new(),
            frames: Vec::new(),
            globals: vec![VMValue::Null; num_globals],
            halted: false,
        }
    }

    /// 获取模块的不可变引用
    pub fn module_ref(&self) -> &BytecodeModule {
        &self.module
    }

    pub fn run(&mut self) -> Result<VMValue, String> {
        let entry = self.module.entry_point;
        self.call_function(entry, Vec::new())?;

        while !self.halted && !self.frames.is_empty() {
            self.step()?;
        }

        Ok(self.stack.pop().unwrap_or(VMValue::Null))
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

        // Debug: print instruction and stack
        // eprintln!("  [{:3}] {:?} | stack: {:?}", pc, instr, self.stack);

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
            Bytecode::Lt => self.binop(|a, b| VMValue::Bool(a.as_int() < b.as_int()))?,
            Bytecode::Gt => self.binop(|a, b| VMValue::Bool(a.as_int() > b.as_int()))?,
            Bytecode::Le => self.binop(|a, b| VMValue::Bool(a.as_int() <= b.as_int()))?,
            Bytecode::Ge => self.binop(|a, b| VMValue::Bool(a.as_int() >= b.as_int()))?,
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
            Bytecode::Cast(_) => {
                // TODO: Type casting
            }
            Bytecode::InstanceOf(_) => {
                self.stack.push(VMValue::Bool(true));
            }
            Bytecode::TypeId => {
                self.stack.push(VMValue::Int(0));
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
                // printf(format, ...args) - format string with %d, %s, etc.
                let mut args = Vec::new();
                for _ in 0..argc {
                    if let Some(v) = self.stack.pop() {
                        args.push(v);
                    }
                }
                args.reverse();
                if let Some(format_val) = args.get(0) {
                    let mut format_str = format_val.as_string();
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
}
