use crate::jit::bytecode::*;
use crate::jit::vm::VMValue;
use std::collections::HashMap;

/// JIT 编译状态
#[derive(Debug, Clone)]
pub struct JitState {
    /// 已 JIT 编译的函数索引
    pub compiled_functions: HashMap<u32, JitFunction>,
    /// 函数执行计数（用于热点检测）
    pub execution_counts: HashMap<u32, u64>,
    /// JIT 编译阈值
    pub hot_threshold: u64,
    /// 是否启用 JIT
    pub enabled: bool,
}

impl JitState {
    pub fn new() -> Self {
        Self {
            compiled_functions: HashMap::new(),
            execution_counts: HashMap::new(),
            hot_threshold: 100,
            enabled: true,
        }
    }

    /// 记录函数执行，返回是否达到热点阈值
    pub fn record_execution(&mut self, func_idx: u32) -> bool {
        if !self.enabled {
            return false;
        }
        let count = self.execution_counts.entry(func_idx).or_insert(0);
        *count += 1;
        *count >= self.hot_threshold && !self.compiled_functions.contains_key(&func_idx)
    }

    /// 检查函数是否已 JIT 编译
    pub fn is_compiled(&self, func_idx: u32) -> bool {
        self.compiled_functions.contains_key(&func_idx)
    }

    /// 获取 JIT 编译后的函数
    pub fn get_compiled(&self, func_idx: u32) -> Option<&JitFunction> {
        self.compiled_functions.get(&func_idx)
    }

    /// 注册 JIT 编译后的函数
    pub fn register_compiled(&mut self, func_idx: u32, func: JitFunction) {
        self.compiled_functions.insert(func_idx, func);
    }
}

impl Default for JitState {
    fn default() -> Self {
        Self::new()
    }
}

/// JIT 编译后的函数信息
#[derive(Debug, Clone)]
pub struct JitFunction {
    /// 函数索引
    pub func_idx: u32,
    /// 函数名称
    pub name: String,
    /// 编译后的原生代码（x86-64 机器码）
    pub code: Vec<u8>,
    /// 代码大小
    pub code_size: usize,
}

/// JIT Compiler - 将 BytecodeModule 编译为原生机器码
pub struct JitCompiler;

impl JitCompiler {
    pub fn new() -> Self {
        Self
    }

    /// 编译单个函数为原生代码
    /// 
    /// 当前实现为简化版本，生成 x86-64 机器码框架
    /// 完整实现需要：
    /// 1. 寄存器分配
    /// 2. 指令选择
    /// 3. 栈帧管理
    /// 4. 调用约定处理
    pub fn compile_function(&self, func: &BytecodeFunction, _module: &BytecodeModule) -> Option<JitFunction> {
        let mut code = Vec::new();

        // x86-64 函数序言 (Function Prologue)
        // push rbp
        // mov rbp, rsp
        // sub rsp, locals_size
        code.push(0x55); // push rbp
        code.push(0x48); code.push(0x89); code.push(0xE5); // mov rbp, rsp

        // 为局部变量分配栈空间
        let locals_size = func.locals.max(16) as u32 * 8;
        if locals_size > 0 {
            code.push(0x48); code.push(0x81); code.push(0xEC);
            code.extend_from_slice(&locals_size.to_le_bytes());
        }

        // 编译字节码指令
        for instr in &func.code {
            self.compile_instruction(instr, &mut code);
        }

        // 函数尾声 (Function Epilogue)
        // mov rsp, rbp
        // pop rbp
        // ret
        code.push(0x48); code.push(0x89); code.push(0xEC); // mov rsp, rbp
        code.push(0x5D); // pop rbp
        code.push(0xC3); // ret

        let code_size = code.len();
        Some(JitFunction {
            func_idx: 0,
            name: func.name.clone(),
            code,
            code_size,
        })
    }

    /// 编译单条字节码指令为 x86-64 机器码
    fn compile_instruction(&self, instr: &Bytecode, code: &mut Vec<u8>) {
        match instr {
            Bytecode::PushConst(v) => {
                // mov rax, imm64
                // push rax
                code.push(0x48); code.push(0xB8);
                code.extend_from_slice(&v.to_le_bytes());
                code.push(0x50); // push rax
            }
            Bytecode::PushFloat(v) => {
                // 将浮点数作为整数压栈（简化处理）
                let bits = v.to_bits() as i64;
                code.push(0x48); code.push(0xB8);
                code.extend_from_slice(&bits.to_le_bytes());
                code.push(0x50); // push rax
            }
            Bytecode::PushBool(b) => {
                let v = if *b { 1i64 } else { 0i64 };
                code.push(0x48); code.push(0xB8);
                code.extend_from_slice(&v.to_le_bytes());
                code.push(0x50); // push rax
            }
            Bytecode::PushNull => {
                // push 0
                code.push(0x6A); code.push(0x00);
            }
            Bytecode::Pop => {
                // pop rax (丢弃栈顶)
                code.push(0x58);
            }
            Bytecode::Dup => {
                // mov rax, [rsp]
                // push rax
                code.push(0x48); code.push(0x8B); code.push(0x04); code.push(0x24);
                code.push(0x50);
            }
            Bytecode::Add => {
                // pop rbx
                // pop rax
                // add rax, rbx
                // push rax
                code.push(0x5B);
                code.push(0x58);
                code.push(0x48); code.push(0x01); code.push(0xD8);
                code.push(0x50);
            }
            Bytecode::Sub => {
                // pop rbx
                // pop rax
                // sub rax, rbx
                // push rax
                code.push(0x5B);
                code.push(0x58);
                code.push(0x48); code.push(0x29); code.push(0xD8);
                code.push(0x50);
            }
            Bytecode::Mul => {
                // pop rbx
                // pop rax
                // imul rax, rbx
                // push rax
                code.push(0x5B);
                code.push(0x58);
                code.push(0x48); code.push(0x0F); code.push(0xAF); code.push(0xC3);
                code.push(0x50);
            }
            Bytecode::Neg => {
                // pop rax
                // neg rax
                // push rax
                code.push(0x58);
                code.push(0x48); code.push(0xF7); code.push(0xD8);
                code.push(0x50);
            }
            Bytecode::Eq => {
                // pop rbx
                // pop rax
                // cmp rax, rbx
                // sete al
                // movzx rax, al
                // push rax
                code.push(0x5B);
                code.push(0x58);
                code.push(0x48); code.push(0x39); code.push(0xD8);
                code.push(0x0F); code.push(0x94); code.push(0xC0);
                code.push(0x48); code.push(0x0F); code.push(0xB6); code.push(0xC0);
                code.push(0x50);
            }
            Bytecode::Ne => {
                code.push(0x5B);
                code.push(0x58);
                code.push(0x48); code.push(0x39); code.push(0xD8);
                code.push(0x0F); code.push(0x95); code.push(0xC0);
                code.push(0x48); code.push(0x0F); code.push(0xB6); code.push(0xC0);
                code.push(0x50);
            }
            Bytecode::Lt => {
                code.push(0x5B);
                code.push(0x58);
                code.push(0x48); code.push(0x39); code.push(0xD8);
                code.push(0x0F); code.push(0x9C); code.push(0xC0);
                code.push(0x48); code.push(0x0F); code.push(0xB6); code.push(0xC0);
                code.push(0x50);
            }
            Bytecode::Gt => {
                code.push(0x5B);
                code.push(0x58);
                code.push(0x48); code.push(0x39); code.push(0xD8);
                code.push(0x0F); code.push(0x9F); code.push(0xC0);
                code.push(0x48); code.push(0x0F); code.push(0xB6); code.push(0xC0);
                code.push(0x50);
            }
            Bytecode::And => {
                code.push(0x5B);
                code.push(0x58);
                code.push(0x48); code.push(0x21); code.push(0xD8);
                code.push(0x50);
            }
            Bytecode::Or => {
                code.push(0x5B);
                code.push(0x58);
                code.push(0x48); code.push(0x09); code.push(0xD8);
                code.push(0x50);
            }
            Bytecode::Not => {
                // pop rax
                // test rax, rax
                // sete al
                // movzx rax, al
                // push rax
                code.push(0x58);
                code.push(0x48); code.push(0x85); code.push(0xC0);
                code.push(0x0F); code.push(0x94); code.push(0xC0);
                code.push(0x48); code.push(0x0F); code.push(0xB6); code.push(0xC0);
                code.push(0x50);
            }
            Bytecode::Return => {
                // mov rsp, rbp
                // pop rbp
                // ret
                code.push(0x48); code.push(0x89); code.push(0xEC);
                code.push(0x5D);
                code.push(0xC3);
            }
            _ => {
                // 未实现的指令：生成 NOP
                code.push(0x90);
            }
        }
    }

    /// 编译整个模块（仅编译热点函数）
    pub fn compile_module(&self, module: &BytecodeModule, jit_state: &mut JitState) {
        for (i, func) in module.functions.iter().enumerate() {
            let idx = i as u32;
            if jit_state.record_execution(idx) {
                println!("  [JIT] Compiling function #{}: {} (hot)", idx, func.name);
                if let Some(jit_func) = self.compile_function(func, module) {
                    jit_state.register_compiled(idx, jit_func);
                }
            }
        }
    }
}

impl Default for JitCompiler {
    fn default() -> Self {
        Self::new()
    }
}
