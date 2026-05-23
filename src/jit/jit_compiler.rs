use crate::jit::bytecode::*;
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

    /// 获取函数执行计数
    pub fn get_execution_count(&self, func_idx: u32) -> u64 {
        *self.execution_counts.get(&func_idx).unwrap_or(&0)
    }

    /// 重置所有执行计数
    pub fn reset_counts(&mut self) {
        self.execution_counts.clear();
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
    /// 栈帧大小（字节）
    pub frame_size: usize,
    /// 需要的参数数量
    pub param_count: u32,
}

/// x86-64 register allocation for JIT
/// Uses Windows x64 calling convention:
/// - Args: rcx, rdx, r8, r9 (then stack)
/// - Callee-saved: rbx, rbp, rsi, rdi, r12-r15
/// - Caller-saved: rax, rcx, rdx, r8, r9, r10, r11
/// - Scratch: rax, r10, r11
struct RegAlloc {
    /// Which local variable is in which register
    locals_in_reg: HashMap<u32, u8>,  // local_idx → register code
    /// Next available register for allocation
    next_reg: u8,
}

impl RegAlloc {
    fn new() -> Self {
        Self {
            locals_in_reg: HashMap::new(),
            next_reg: 0,
        }
    }

    /// Allocate a register for a local variable
    fn alloc_local(&mut self, local_idx: u32) -> u8 {
        if let Some(&reg) = self.locals_in_reg.get(&local_idx) {
            return reg;
        }
        // Use registers r8-r15 for locals (reg codes 8-15)
        let reg = 8 + (self.next_reg % 8);
        self.next_reg += 1;
        self.locals_in_reg.insert(local_idx, reg);
        reg
    }

    /// Get the register for a local, or None
    fn get_reg(&self, local_idx: u32) -> Option<u8> {
        self.locals_in_reg.get(&local_idx).copied()
    }
}

/// Jump target for patching
struct PendingJump {
    /// Offset in code where the jump offset should be written
    patch_offset: usize,
    /// The label/target offset
    target_label: u32,
}

/// JIT Compiler - 将 BytecodeModule 编译为原生 x86-64 机器码
///
/// Stack-based model: values are pushed/popped from the x86 stack.
/// Local variables are stored in stack slots relative to rbp.
/// The JIT compiler uses a simple stack-based approach with
/// rbp-relative addressing for locals.
pub struct JitCompiler {
    /// Register allocator
    reg_alloc: RegAlloc,
    /// Label positions: label_id → code offset
    label_positions: HashMap<u32, usize>,
    /// Pending jumps that need patching
    pending_jumps: Vec<PendingJump>,
    /// Next label ID
    next_label: u32,
}

impl JitCompiler {
    pub fn new() -> Self {
        Self {
            reg_alloc: RegAlloc::new(),
            label_positions: HashMap::new(),
            pending_jumps: Vec::new(),
            next_label: 0,
        }
    }

    fn new_label(&mut self) -> u32 {
        let label = self.next_label;
        self.next_label += 1;
        label
    }

    fn emit_label(&mut self, code: &mut Vec<u8>, label: u32) {
        self.label_positions.insert(label, code.len());
    }

    fn patch_jumps(&mut self, code: &mut Vec<u8>) {
        for jump in self.pending_jumps.drain(..) {
            if let Some(&target) = self.label_positions.get(&jump.target_label) {
                let offset = target as i32 - jump.patch_offset as i32 - 4;
                let bytes = offset.to_le_bytes();
                code[jump.patch_offset] = bytes[0];
                code[jump.patch_offset + 1] = bytes[1];
                code[jump.patch_offset + 2] = bytes[2];
                code[jump.patch_offset + 3] = bytes[3];
            }
        }
    }

    /// Emit a conditional jump (32-bit relative offset)
    /// Returns the patch offset for later fixup
    fn emit_jcc(&mut self, code: &mut Vec<u8>, cc: u8, label: u32) {
        // 0F 8x xx xx xx xx  (6 bytes, 32-bit offset)
        code.push(0x0F);
        code.push(0x80 | cc);
        let patch_offset = code.len();
        code.extend_from_slice(&0i32.to_le_bytes()); // placeholder
        self.pending_jumps.push(PendingJump {
            patch_offset,
            target_label: label,
        });
    }

    /// Emit an unconditional jump (32-bit relative offset)
    fn emit_jmp(&mut self, code: &mut Vec<u8>, label: u32) {
        // E9 xx xx xx xx  (5 bytes, 32-bit offset)
        code.push(0xE9);
        let patch_offset = code.len();
        code.extend_from_slice(&0i32.to_le_bytes()); // placeholder
        self.pending_jumps.push(PendingJump {
            patch_offset,
            target_label: label,
        });
    }

    /// Get the stack offset for a local variable
    /// Locals are stored at [rbp - (local_idx+1)*8]
    fn local_offset(local_idx: u32) -> i32 {
        -((local_idx as i32 + 1) * 8)
    }

    /// 编译单个函数为原生代码
    pub fn compile_function(&mut self, func: &BytecodeFunction, _module: &BytecodeModule) -> Option<JitFunction> {
        let mut code = Vec::new();

        self.reg_alloc = RegAlloc::new();
        self.label_positions.clear();
        self.pending_jumps.clear();
        self.next_label = 0;

        // === Function Prologue ===
        // push rbp
        emit_push_r64(&mut code, 5); // rbp = reg 5
        // mov rbp, rsp
        emit_mov_r64_r64(&mut code, 5, 4); // rbp←rsp
        // push rbx (callee-saved)
        emit_push_r64(&mut code, 3);
        // push rsi (callee-saved)
        emit_push_r64(&mut code, 6);
        // push rdi (callee-saved)
        emit_push_r64(&mut code, 7);

        // 为局部变量分配栈空间 (aligned to 16 bytes)
        let locals_size = ((func.locals as u32 * 8) + 15) & !15;
        if locals_size > 0 {
            // sub rsp, locals_size
            code.push(0x48);
            code.push(0x81);
            code.push(0xEC);
            code.extend_from_slice(&locals_size.to_le_bytes());
        }

        // Copy register args to local slots
        // Windows x64: rcx, rdx, r8, r9 → local 0, 1, 2, 3
        let arg_regs: &[u8] = &[1, 2, 8, 9]; // rcx, rdx, r8, r9
        for (i, &reg) in arg_regs.iter().enumerate() {
            if i < func.params.len() {
                let offset = Self::local_offset(i as u32);
                emit_mov_m64_r64(&mut code, offset, reg); // [rbp+offset] ← reg
            }
        }

        // Compile bytecode instructions
        for instr in &func.code {
            self.compile_instruction(instr, &mut code);
        }

        // === Function Epilogue (default return) ===
        // xor rax, rax (return 0 if no explicit return)
        code.push(0x48);
        code.push(0x31);
        code.push(0xC0);

        // Restore stack and callee-saved registers
        if locals_size > 0 {
            // add rsp, locals_size
            code.push(0x48);
            code.push(0x81);
            code.push(0xC4);
            code.extend_from_slice(&locals_size.to_le_bytes());
        }
        emit_pop_r64(&mut code, 7); // pop rdi
        emit_pop_r64(&mut code, 6); // pop rsi
        emit_pop_r64(&mut code, 3); // pop rbx
        emit_pop_r64(&mut code, 5); // pop rbp
        code.push(0xC3); // ret

        // Patch all jump targets
        self.patch_jumps(&mut code);

        let code_size = code.len();
        Some(JitFunction {
            func_idx: 0,
            name: func.name.clone(),
            code,
            code_size,
            frame_size: locals_size as usize,
            param_count: func.params.len() as u32,
        })
    }

    /// 编译单条字节码指令为 x86-64 机器码
    fn compile_instruction(&mut self, instr: &Bytecode, code: &mut Vec<u8>) {
        match instr {
            // === Stack Operations ===
            Bytecode::PushConst(v) => {
                // mov rax, imm64; push rax
                emit_mov_r64_imm64(code, 0, *v); // rax = imm
                emit_push_r64(code, 0); // push rax
            }
            Bytecode::PushFloat(v) => {
                let bits = v.to_bits() as i64;
                emit_mov_r64_imm64(code, 0, bits);
                emit_push_r64(code, 0);
            }
            Bytecode::PushBool(b) => {
                let v = if *b { 1i64 } else { 0i64 };
                emit_mov_r64_imm64(code, 0, v);
                emit_push_r64(code, 0);
            }
            Bytecode::PushString(idx) => {
                // Push string index as a tagged value (high bit set)
                let tagged = (*idx as i64) | (1i64 << 48); // tag: bit 48 = string
                emit_mov_r64_imm64(code, 0, tagged);
                emit_push_r64(code, 0);
            }
            Bytecode::PushNull => {
                // push 0
                code.push(0x6A);
                code.push(0x00);
            }
            Bytecode::Pop => {
                // add rsp, 8
                code.push(0x48);
                code.push(0x83);
                code.push(0xC4);
                code.push(0x08);
            }
            Bytecode::Dup => {
                // mov rax, [rsp]; push rax
                emit_mov_r64_m64(code, 0, 0); // rax = [rsp]
                emit_push_r64(code, 0);
            }
            Bytecode::Swap => {
                // pop rax; pop rbx; push rax; push rbx
                emit_pop_r64(code, 0); // rax
                emit_pop_r64(code, 3); // rbx
                emit_push_r64(code, 0); // push rax
                emit_push_r64(code, 3); // push rbx
            }

            // === Local Variable Access ===
            Bytecode::LoadLocal(idx) => {
                let offset = Self::local_offset(*idx);
                // mov rax, [rbp+offset]; push rax
                emit_mov_r64_m64_rbp(code, 0, offset);
                emit_push_r64(code, 0);
            }
            Bytecode::StoreLocal(idx) => {
                let offset = Self::local_offset(*idx);
                // pop rax; mov [rbp+offset], rax
                emit_pop_r64(code, 0);
                emit_mov_r64_m64_rbp_store(code, offset, 0);
            }
            Bytecode::LoadGlobal(idx) => {
                // Global: load from globals array (simplified: use large local index)
                let offset = Self::local_offset(1000 + idx);
                emit_mov_r64_m64_rbp(code, 0, offset);
                emit_push_r64(code, 0);
            }
            Bytecode::StoreGlobal(idx) => {
                let offset = Self::local_offset(1000 + idx);
                emit_pop_r64(code, 0);
                emit_mov_r64_m64_rbp_store(code, offset, 0);
            }

            // === Field Access ===
            Bytecode::LoadField(idx) => {
                // pop obj (in rax); mov rax, [rax + idx*8]; push rax
                emit_pop_r64(code, 0); // rax = obj
                let field_offset = *idx as i32 * 8;
                emit_mov_r64_m64_rax(code, 0, field_offset);
                emit_push_r64(code, 0);
            }
            Bytecode::StoreField(idx) => {
                // pop value (rax); pop obj (rcx); mov [rcx + idx*8], rax
                emit_pop_r64(code, 0); // rax = value
                emit_pop_r64(code, 1); // rcx = obj
                let field_offset = *idx as i32 * 8;
                emit_mov_m64_r64_rax(code, field_offset, 0);
            }

            // === Arithmetic ===
            Bytecode::Add => {
                emit_pop_r64(code, 3); // rbx
                emit_pop_r64(code, 0); // rax
                // add rax, rbx
                code.push(0x48);
                code.push(0x01);
                code.push(0xD8);
                emit_push_r64(code, 0);
            }
            Bytecode::Sub => {
                emit_pop_r64(code, 3); // rbx
                emit_pop_r64(code, 0); // rax
                // sub rax, rbx
                code.push(0x48);
                code.push(0x29);
                code.push(0xD8);
                emit_push_r64(code, 0);
            }
            Bytecode::Mul => {
                emit_pop_r64(code, 3); // rbx
                emit_pop_r64(code, 0); // rax
                // imul rax, rbx
                code.push(0x48);
                code.push(0x0F);
                code.push(0xAF);
                code.push(0xC3);
                emit_push_r64(code, 0);
            }
            Bytecode::Div => {
                emit_pop_r64(code, 3); // rbx (divisor)
                emit_pop_r64(code, 0); // rax (dividend)
                // cqo; idiv rbx
                code.push(0x48); code.push(0x99); // cqo (sign-extend rax into rdx)
                code.push(0x48); code.push(0xF7); code.push(0xFB); // idiv rbx
                emit_push_r64(code, 0);
            }
            Bytecode::Mod => {
                emit_pop_r64(code, 3); // rbx
                emit_pop_r64(code, 0); // rax
                code.push(0x48); code.push(0x99); // cqo
                code.push(0x48); code.push(0xF7); code.push(0xFB); // idiv rbx
                // rdx = remainder
                emit_mov_r64_r64(code, 0, 2); // rax ← rdx
                emit_push_r64(code, 0);
            }
            Bytecode::Neg => {
                emit_pop_r64(code, 0);
                // neg rax
                code.push(0x48);
                code.push(0xF7);
                code.push(0xD8);
                emit_push_r64(code, 0);
            }

            // === Float Arithmetic ===
            Bytecode::FAdd => {
                // Pop two values, add as doubles using SSE2
                emit_pop_r64(code, 3); // rbx
                emit_pop_r64(code, 0); // rax
                // movq xmm0, rax; movq xmm1, rbx; addsd xmm0, xmm1; movq rax, xmm0
                code.push(0x66); code.push(0x48); code.push(0x0F); code.push(0x6E); code.push(0xC0); // movq xmm0, rax
                code.push(0x66); code.push(0x48); code.push(0x0F); code.push(0x6E); code.push(0xCB); // movq xmm1, rbx
                code.push(0xF2); code.push(0x0F); code.push(0x58); code.push(0xC1); // addsd xmm0, xmm1
                code.push(0x66); code.push(0x48); code.push(0x0F); code.push(0x7E); code.push(0xC0); // movq rax, xmm0
                emit_push_r64(code, 0);
            }
            Bytecode::FSub => {
                emit_pop_r64(code, 3);
                emit_pop_r64(code, 0);
                code.push(0x66); code.push(0x48); code.push(0x0F); code.push(0x6E); code.push(0xC0);
                code.push(0x66); code.push(0x48); code.push(0x0F); code.push(0x6E); code.push(0xCB);
                code.push(0xF2); code.push(0x0F); code.push(0x5C); code.push(0xC1); // subsd xmm0, xmm1
                code.push(0x66); code.push(0x48); code.push(0x0F); code.push(0x7E); code.push(0xC0);
                emit_push_r64(code, 0);
            }
            Bytecode::FMul => {
                emit_pop_r64(code, 3);
                emit_pop_r64(code, 0);
                code.push(0x66); code.push(0x48); code.push(0x0F); code.push(0x6E); code.push(0xC0);
                code.push(0x66); code.push(0x48); code.push(0x0F); code.push(0x6E); code.push(0xCB);
                code.push(0xF2); code.push(0x0F); code.push(0x59); code.push(0xC1); // mulsd xmm0, xmm1
                code.push(0x66); code.push(0x48); code.push(0x0F); code.push(0x7E); code.push(0xC0);
                emit_push_r64(code, 0);
            }
            Bytecode::FDiv => {
                emit_pop_r64(code, 3);
                emit_pop_r64(code, 0);
                code.push(0x66); code.push(0x48); code.push(0x0F); code.push(0x6E); code.push(0xC0);
                code.push(0x66); code.push(0x48); code.push(0x0F); code.push(0x6E); code.push(0xCB);
                code.push(0xF2); code.push(0x0F); code.push(0x5E); code.push(0xC1); // divsd xmm0, xmm1
                code.push(0x66); code.push(0x48); code.push(0x0F); code.push(0x7E); code.push(0xC0);
                emit_push_r64(code, 0);
            }
            Bytecode::FCmp => {
                emit_pop_r64(code, 3);
                emit_pop_r64(code, 0);
                code.push(0x66); code.push(0x48); code.push(0x0F); code.push(0x6E); code.push(0xC0);
                code.push(0x66); code.push(0x48); code.push(0x0F); code.push(0x6E); code.push(0xCB);
                code.push(0x66); code.push(0x0F); code.push(0x2F); code.push(0xC1); // comisd xmm0, xmm1
                // set result: -1/0/1
                code.push(0x48); code.push(0x31); code.push(0xC0); // xor rax, rax
                code.push(0x0F); code.push(0x97); code.push(0xC0); // seta al (above = 1)
                code.push(0x48); code.push(0x0F); code.push(0xB6); code.push(0xC0); // movzx rax, al
                // If not above, check below
                code.push(0x48); code.push(0x89); code.push(0xC2); // mov rdx, rax
                code.push(0x48); code.push(0x31); code.push(0xC0); // xor rax, rax
                code.push(0x0F); code.push(0x92); code.push(0xC0); // setb al (below = 1)
                code.push(0x48); code.push(0x0F); code.push(0xB6); code.push(0xC0);
                // rax = below ? -1 : (above ? 1 : 0)
                code.push(0x48); code.push(0x83); code.push(0xF8); code.push(0x01); // cmp rax, 1
                code.push(0x48); code.push(0x0F); code.push(0x44); code.push(0xC2); // cmove rax, rdx
                // Simplified: just push 0 for now (FCmp is complex)
                emit_push_r64(code, 0);
            }

            // === Bitwise ===
            Bytecode::BitAnd => {
                emit_pop_r64(code, 3);
                emit_pop_r64(code, 0);
                code.push(0x48); code.push(0x21); code.push(0xD8); // and rax, rbx
                emit_push_r64(code, 0);
            }
            Bytecode::BitOr => {
                emit_pop_r64(code, 3);
                emit_pop_r64(code, 0);
                code.push(0x48); code.push(0x09); code.push(0xD8); // or rax, rbx
                emit_push_r64(code, 0);
            }
            Bytecode::BitXor => {
                emit_pop_r64(code, 3);
                emit_pop_r64(code, 0);
                code.push(0x48); code.push(0x31); code.push(0xD8); // xor rax, rbx
                emit_push_r64(code, 0);
            }
            Bytecode::BitNot => {
                emit_pop_r64(code, 0);
                code.push(0x48); code.push(0xF7); code.push(0xD0); // not rax
                emit_push_r64(code, 0);
            }
            Bytecode::Shl => {
                emit_pop_r64(code, 3); // shift count in rbx
                emit_pop_r64(code, 0); // value in rax
                // mov cl, bl; shl rax, cl
                code.push(0x88); code.push(0xD9); // mov cl, bl
                code.push(0x48); code.push(0xD3); code.push(0xE0); // shl rax, cl
                emit_push_r64(code, 0);
            }
            Bytecode::Shr => {
                emit_pop_r64(code, 3);
                emit_pop_r64(code, 0);
                code.push(0x88); code.push(0xD9);
                code.push(0x48); code.push(0xD3); code.push(0xE8); // shr rax, cl
                emit_push_r64(code, 0);
            }

            // === Comparison ===
            Bytecode::Eq => {
                emit_pop_r64(code, 3);
                emit_pop_r64(code, 0);
                code.push(0x48); code.push(0x39); code.push(0xD8); // cmp rax, rbx
                code.push(0x0F); code.push(0x94); code.push(0xC0); // sete al
                code.push(0x48); code.push(0x0F); code.push(0xB6); code.push(0xC0); // movzx rax, al
                emit_push_r64(code, 0);
            }
            Bytecode::Ne => {
                emit_pop_r64(code, 3);
                emit_pop_r64(code, 0);
                code.push(0x48); code.push(0x39); code.push(0xD8);
                code.push(0x0F); code.push(0x95); code.push(0xC0); // setne al
                code.push(0x48); code.push(0x0F); code.push(0xB6); code.push(0xC0);
                emit_push_r64(code, 0);
            }
            Bytecode::Lt => {
                emit_pop_r64(code, 3);
                emit_pop_r64(code, 0);
                code.push(0x48); code.push(0x39); code.push(0xD8);
                code.push(0x0F); code.push(0x9C); code.push(0xC0); // setl al
                code.push(0x48); code.push(0x0F); code.push(0xB6); code.push(0xC0);
                emit_push_r64(code, 0);
            }
            Bytecode::Gt => {
                emit_pop_r64(code, 3);
                emit_pop_r64(code, 0);
                code.push(0x48); code.push(0x39); code.push(0xD8);
                code.push(0x0F); code.push(0x9F); code.push(0xC0); // setg al
                code.push(0x48); code.push(0x0F); code.push(0xB6); code.push(0xC0);
                emit_push_r64(code, 0);
            }
            Bytecode::Le => {
                emit_pop_r64(code, 3);
                emit_pop_r64(code, 0);
                code.push(0x48); code.push(0x39); code.push(0xD8);
                code.push(0x0F); code.push(0x9E); code.push(0xC0); // setle al
                code.push(0x48); code.push(0x0F); code.push(0xB6); code.push(0xC0);
                emit_push_r64(code, 0);
            }
            Bytecode::Ge => {
                emit_pop_r64(code, 3);
                emit_pop_r64(code, 0);
                code.push(0x48); code.push(0x39); code.push(0xD8);
                code.push(0x0F); code.push(0x9D); code.push(0xC0); // setge al
                code.push(0x48); code.push(0x0F); code.push(0xB6); code.push(0xC0);
                emit_push_r64(code, 0);
            }

            // === Logical ===
            Bytecode::And => {
                emit_pop_r64(code, 3);
                emit_pop_r64(code, 0);
                code.push(0x48); code.push(0x21); code.push(0xD8);
                emit_push_r64(code, 0);
            }
            Bytecode::Or => {
                emit_pop_r64(code, 3);
                emit_pop_r64(code, 0);
                code.push(0x48); code.push(0x09); code.push(0xD8);
                emit_push_r64(code, 0);
            }
            Bytecode::Not => {
                emit_pop_r64(code, 0);
                // test rax, rax; sete al; movzx rax, al
                code.push(0x48); code.push(0x85); code.push(0xC0);
                code.push(0x0F); code.push(0x94); code.push(0xC0);
                code.push(0x48); code.push(0x0F); code.push(0xB6); code.push(0xC0);
                emit_push_r64(code, 0);
            }

            // === Control Flow ===
            Bytecode::Jump(label) => {
                self.emit_jmp(code, *label);
            }
            Bytecode::JumpIf(label) => {
                // pop rax; test rax, rax; jnz label (jump if truthy)
                emit_pop_r64(code, 0);
                code.push(0x48); code.push(0x85); code.push(0xC0); // test rax, rax
                self.emit_jcc(code, 0x5, *label); // JNZ (cc=5)
            }
            Bytecode::JumpIfNot(label) => {
                emit_pop_r64(code, 0);
                code.push(0x48); code.push(0x85); code.push(0xC0);
                self.emit_jcc(code, 0x4, *label); // JZ (cc=4)
            }
            Bytecode::Call(func_idx, argc) => {
                // Set up arguments and call function
                // For simplicity, args are already on the stack in the right order
                // We need to call the function at func_idx
                // This is a placeholder - real implementation needs a function table
                let _ = (func_idx, argc);
                // For now, emit a call to a runtime helper
                // mov rax, 0; call rax (placeholder)
                emit_mov_r64_imm64(code, 0, 0); // rax = 0 (null function pointer)
                code.push(0xFF); code.push(0xD0); // call rax
                emit_push_r64(code, 0); // push return value
            }
            Bytecode::CallMethod(func_idx, argc) => {
                let _ = (func_idx, argc);
                emit_mov_r64_imm64(code, 0, 0);
                code.push(0xFF); code.push(0xD0);
                emit_push_r64(code, 0);
            }
            Bytecode::CallNative(name_idx) => {
                let _ = name_idx;
                emit_mov_r64_imm64(code, 0, 0);
                code.push(0xFF); code.push(0xD0);
                emit_push_r64(code, 0);
            }
            Bytecode::InvokeVirtual(vtable_idx, argc) => {
                let _ = (vtable_idx, argc);
                emit_mov_r64_imm64(code, 0, 0);
                code.push(0xFF); code.push(0xD0);
                emit_push_r64(code, 0);
            }
            Bytecode::Return => {
                // pop return value into rax
                emit_pop_r64(code, 0); // rax = return value
                // Epilogue: restore stack and return
                // This will be handled by the epilogue at the end
                // For now, jump to epilogue
                // Actually, we emit the full epilogue here
                code.push(0x48); code.push(0x89); code.push(0xEC); // mov rsp, rbp
                emit_pop_r64(code, 5); // pop rbp
                code.push(0xC3); // ret
            }

            // === Object Operations ===
            Bytecode::New(class_idx) => {
                // Allocate object: call runtime allocator
                let _ = class_idx;
                emit_mov_r64_imm64(code, 0, 0); // placeholder
                emit_push_r64(code, 0);
            }
            Bytecode::NewArray => {
                // pop size; allocate array
                emit_pop_r64(code, 0);
                // placeholder
                emit_push_r64(code, 0);
            }
            Bytecode::ArrayGet => {
                // pop idx; pop arr; push arr[idx]
                emit_pop_r64(code, 3); // idx
                emit_pop_r64(code, 0); // arr
                // mov rax, [rax + rbx*8 + 16] (skip length field)
                code.push(0x48); code.push(0x8B); code.push(0x44); code.push(0xD8);
                code.push(0x10); // mov rax, [rax + rbx*8 + 16]
                emit_push_r64(code, 0);
            }
            Bytecode::ArraySet => {
                // pop value; pop idx; pop arr; arr[idx] = value
                emit_pop_r64(code, 2); // rdx = value
                emit_pop_r64(code, 3); // rbx = idx
                emit_pop_r64(code, 0); // rax = arr
                // mov [rax + rbx*8 + 16], rdx
                code.push(0x48); code.push(0x89); code.push(0x54); code.push(0xD8);
                code.push(0x10);
            }
            Bytecode::ArrayLen => {
                // pop arr; push arr.length (at offset 0)
                emit_pop_r64(code, 0);
                // mov rax, [rax]
                code.push(0x48); code.push(0x8B); code.push(0x00);
                emit_push_r64(code, 0);
            }
            Bytecode::ArrayPush => {
                // pop value; pop arr; arr.push(value)
                emit_pop_r64(code, 3); // value
                emit_pop_r64(code, 0); // arr
                // placeholder
                emit_push_r64(code, 0);
            }
            Bytecode::Delete => {
                emit_pop_r64(code, 0);
                // placeholder - free memory
            }

            // === Type Operations ===
            Bytecode::Cast(type_idx) => {
                // Cast is a no-op at the machine code level (type checking is runtime)
                let _ = type_idx;
            }
            Bytecode::InstanceOf(type_idx) => {
                let _ = type_idx;
                emit_pop_r64(code, 0);
                // placeholder: push true
                emit_mov_r64_imm64(code, 0, 1);
                emit_push_r64(code, 0);
            }
            Bytecode::TypeId => {
                // placeholder
                emit_mov_r64_imm64(code, 0, 0);
                emit_push_r64(code, 0);
            }
            Bytecode::CheckNotNull => {
                // pop value; assert non-null; push back
                emit_pop_r64(code, 0);
                // test rax, rax; jnz ok; int3 (breakpoint on null)
                code.push(0x48); code.push(0x85); code.push(0xC0);
                code.push(0x75); code.push(0x01); // jnz +1
                code.push(0xCC); // int3
                emit_push_r64(code, 0);
            }

            // === Optimized ===
            Bytecode::IncLocal(idx, delta) => {
                let offset = Self::local_offset(*idx);
                // add [rbp+offset], delta
                code.push(0x48); code.push(0x81); code.push(0x45);
                code.extend_from_slice(&offset.to_le_bytes());
                code.extend_from_slice(&(*delta as i64).to_le_bytes());
            }

            // === Map/String/Other ===
            // These require runtime calls, emit placeholders
            Bytecode::MapNew | Bytecode::MapGet | Bytecode::MapPut
            | Bytecode::MapContains | Bytecode::MapLen | Bytecode::MapRemove
            | Bytecode::MapKeys | Bytecode::StringConcat | Bytecode::StringLen
            | Bytecode::StringEquals => {
                // Runtime call placeholder
                emit_mov_r64_imm64(code, 0, 0);
                emit_push_r64(code, 0);
            }

            Bytecode::Printf(argc) => {
                // Pop argc values, discard them
                for _ in 0..*argc {
                    code.push(0x48);
                    code.push(0x83);
                    code.push(0xC4);
                    code.push(0x08); // add rsp, 8
                }
                emit_mov_r64_imm64(code, 0, 0);
                emit_push_r64(code, 0);
            }
            Bytecode::Print | Bytecode::Println => {
                // pop value (discard for JIT)
                code.push(0x48);
                code.push(0x83);
                code.push(0xC4);
                code.push(0x08);
            }
            Bytecode::Halt => {
                // Exit: mov rax, 0; ret
                code.push(0x48); code.push(0x31); code.push(0xC0);
                code.push(0xC3);
            }
            Bytecode::Nop => {
                code.push(0x90); // NOP
            }
        }
    }

    /// 编译整个模块（仅编译热点函数）
    pub fn compile_module(&mut self, module: &BytecodeModule, jit_state: &mut JitState) {
        for (i, func) in module.functions.iter().enumerate() {
            let idx = i as u32;
            if jit_state.record_execution(idx) {
                eprintln!("  [JIT] Compiling function #{}: {} (hot)", idx, func.name);
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

// === x86-64 Encoding Helpers ===
// Register encoding: rax=0, rcx=1, rdx=2, rbx=3, rsp=4, rbp=5, rsi=6, rdi=7
// r8=8, r9=9, r10=10, r11=11, r12=12, r13=13, r14=14, r15=15

/// push r64
fn emit_push_r64(code: &mut Vec<u8>, reg: u8) {
    if reg < 8 {
        code.push(0x50 + reg);
    } else {
        code.push(0x41); // REX.B
        code.push(0x50 + (reg - 8));
    }
}

/// pop r64
fn emit_pop_r64(code: &mut Vec<u8>, reg: u8) {
    if reg < 8 {
        code.push(0x58 + reg);
    } else {
        code.push(0x41);
        code.push(0x58 + (reg - 8));
    }
}

/// mov r64, imm64 (10 bytes)
fn emit_mov_r64_imm64(code: &mut Vec<u8>, reg: u8, imm: i64) {
    if reg < 8 {
        code.push(0x48); // REX.W
        code.push(0xB8 + reg);
    } else {
        code.push(0x49); // REX.W + REX.B
        code.push(0xB8 + (reg - 8));
    }
    code.extend_from_slice(&imm.to_le_bytes());
}

/// mov r64, r64
fn emit_mov_r64_r64(code: &mut Vec<u8>, dst: u8, src: u8) {
    let rex = 0x48 | if dst >= 8 { 0x04 } else { 0 } | if src >= 8 { 0x01 } else { 0 };
    code.push(rex);
    code.push(0x89); // mov r/m64, r64
    let modrm = 0xC0 | ((src & 7) << 3) | (dst & 7);
    code.push(modrm);
}

/// mov r64, [rsp] (load from stack top)
fn emit_mov_r64_m64(code: &mut Vec<u8>, reg: u8, _offset: i32) {
    // mov rax, [rsp] → 48 8B 04 24
    code.push(0x48);
    code.push(0x8B);
    code.push(0x04 | ((reg & 7) << 3));
    code.push(0x24);
}

/// mov r64, [rbp + offset] (load local)
fn emit_mov_r64_m64_rbp(code: &mut Vec<u8>, reg: u8, offset: i32) {
    code.push(0x48);
    code.push(0x8B);
    if offset >= -128 && offset <= 127 {
        // mov r64, [rbp + disp8]
        code.push(0x45 | ((reg & 7) << 3));
        code.push(offset as u8);
    } else {
        // mov r64, [rbp + disp32]
        code.push(0x85 | ((reg & 7) << 3));
        code.extend_from_slice(&offset.to_le_bytes());
    }
}

/// mov [rbp + offset], r64 (store local)
fn emit_mov_r64_m64_rbp_store(code: &mut Vec<u8>, offset: i32, reg: u8) {
    code.push(0x48);
    code.push(0x89);
    if offset >= -128 && offset <= 127 {
        code.push(0x45 | ((reg & 7) << 3));
        code.push(offset as u8);
    } else {
        code.push(0x85 | ((reg & 7) << 3));
        code.extend_from_slice(&offset.to_le_bytes());
    }
}

/// mov r64, [rax + offset] (load field)
fn emit_mov_r64_m64_rax(code: &mut Vec<u8>, reg: u8, offset: i32) {
    code.push(0x48);
    code.push(0x8B);
    if offset >= -128 && offset <= 127 {
        code.push(0x40 | ((reg & 7) << 3));
        code.push(offset as u8);
    } else {
        code.push(0x80 | ((reg & 7) << 3));
        code.extend_from_slice(&offset.to_le_bytes());
    }
}

/// mov [rax + offset], r64 (store field)
fn emit_mov_m64_r64_rax(code: &mut Vec<u8>, offset: i32, reg: u8) {
    code.push(0x48);
    code.push(0x89);
    if offset >= -128 && offset <= 127 {
        code.push(0x40 | ((reg & 7) << 3));
        code.push(offset as u8);
    } else {
        code.push(0x80 | ((reg & 7) << 3));
        code.extend_from_slice(&offset.to_le_bytes());
    }
}

/// mov [rbp + offset], r64 (store to memory using rbp base)
fn emit_mov_m64_r64(code: &mut Vec<u8>, offset: i32, reg: u8) {
    code.push(0x48);
    code.push(0x89);
    if offset >= -128 && offset <= 127 {
        code.push(0x45 | ((reg & 7) << 3));
        code.push(offset as u8);
    } else {
        code.push(0x85 | ((reg & 7) << 3));
        code.extend_from_slice(&offset.to_le_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jit_compile_simple() {
        let func = BytecodeFunction {
            name: "test_add".to_string(),
            params: vec!["a".to_string(), "b".to_string()],
            locals: 2,
            code: vec![
                Bytecode::LoadLocal(0),
                Bytecode::LoadLocal(1),
                Bytecode::Add,
                Bytecode::Return,
            ],
            is_static: true,
            class_name: None,
        };

        let module = BytecodeModule::new();
        let mut jit = JitCompiler::new();
        let result = jit.compile_function(&func, &module);
        assert!(result.is_some());

        let jit_func = result.unwrap();
        assert!(jit_func.code.len() > 0);
        assert_eq!(jit_func.name, "test_add");
    }

    #[test]
    fn test_jit_compile_control_flow() {
        let func = BytecodeFunction {
            name: "test_if".to_string(),
            params: vec!["x".to_string()],
            locals: 1,
            code: vec![
                Bytecode::LoadLocal(0),
                Bytecode::PushConst(0),
                Bytecode::Gt,
                Bytecode::JumpIfNot(5), // jump to else
                Bytecode::PushConst(1),
                Bytecode::Return,
                Bytecode::PushConst(0), // else
                Bytecode::Return,
            ],
            is_static: true,
            class_name: None,
        };

        let module = BytecodeModule::new();
        let mut jit = JitCompiler::new();
        let result = jit.compile_function(&func, &module);
        assert!(result.is_some());
    }

    #[test]
    fn test_jit_state_hot_detection() {
        let mut state = JitState::new();
        state.hot_threshold = 3;

        assert!(!state.record_execution(0)); // count=1
        assert!(!state.record_execution(0)); // count=2
        assert!(state.record_execution(0));  // count=3 >= threshold, not yet compiled
        assert!(!state.record_execution(0)); // already compiled
    }
}
