use crate::ir::function::IRFunction;
use crate::ir::instruction::IRInstruction;
use crate::ir::module::IRModule;
use crate::ir::regalloc::LinearScanAllocator;
use std::collections::HashMap;
use std::io::Write;

pub struct X86_64CodeGen {
    output: Vec<u8>,
    labels: HashMap<String, u32>,
    stack_size: u32,
    reg_alloc: LinearScanAllocator,
}

impl X86_64CodeGen {
    pub fn new() -> Self {
        Self {
            output: Vec::new(),
            labels: HashMap::new(),
            stack_size: 0,
            reg_alloc: LinearScanAllocator::new(),
        }
    }

    pub fn generate(&mut self, module: &IRModule) -> String {
        self.emit_line("; Lemon Compiler - x86-64 Code Generation");
        self.emit_line("");
        self.emit_section("text");

        for func in &module.functions {
            self.generate_function(func);
        }

        self.emit_line("");
        self.emit_section("data");
        for global in &module.globals {
            self.emit_line(&format!("{}: dq 0", global.name));
        }

        self.emit_line("");
        self.emit_section("rodata");
        for meta in &module.metadata {
            self.emit_line(&format!("{}: db {}", meta.name, meta.data.iter().map(|b| format!("{}", b)).collect::<Vec<_>>().join(", ")));
        }

        String::from_utf8_lossy(&self.output).to_string()
    }

    fn generate_function(&mut self, func: &IRFunction) {
        self.emit_label(&func.name);
        self.emit_prologue();

        for bb in &func.basic_blocks {
            if !bb.name.is_empty() && bb.name != "entry" {
                self.emit_label(&format!(".{}", bb.name));
            }
            for inst in &bb.instructions {
                self.generate_instruction(inst);
            }
        }

        self.emit_epilogue();
    }

    fn generate_instruction(&mut self, inst: &IRInstruction) {
        match inst {
            IRInstruction::Alloca(typ) => {
                let size = typ.size_bytes();
                self.stack_size += size;
                self.stack_size = (self.stack_size + 15) & !15;
                self.emit_line(&format!("    lea rax, [rbp - {}]", self.stack_size));
            }
            IRInstruction::Load(src) => {
                self.emit_line(&format!("    mov rax, [{}]", self.value_name(src)));
            }
            IRInstruction::Store(val, ptr) => {
                self.emit_line(&format!("    mov [{}], {}", self.value_name(ptr), self.value_name(val)));
            }
            IRInstruction::Add(l, r) => {
                self.emit_line(&format!("    mov rax, {}", self.value_name(l)));
                self.emit_line(&format!("    add rax, {}", self.value_name(r)));
            }
            IRInstruction::Sub(l, r) => {
                self.emit_line(&format!("    mov rax, {}", self.value_name(l)));
                self.emit_line(&format!("    sub rax, {}", self.value_name(r)));
            }
            IRInstruction::Mul(l, r) => {
                self.emit_line(&format!("    mov rax, {}", self.value_name(l)));
                self.emit_line(&format!("    imul rax, {}", self.value_name(r)));
            }
            IRInstruction::Div(l, r) => {
                self.emit_line(&format!("    mov rax, {}", self.value_name(l)));
                self.emit_line("    cqo");
                self.emit_line(&format!("    idiv {}", self.value_name(r)));
            }
            IRInstruction::Call(func, args) => {
                let arg_regs = ["rdi", "rsi", "rdx", "rcx", "r8", "r9"];
                for (i, arg) in args.iter().enumerate() {
                    if i < arg_regs.len() {
                        self.emit_line(&format!("    mov {}, {}", arg_regs[i], self.value_name(arg)));
                    } else {
                        self.emit_line(&format!("    push {}", self.value_name(arg)));
                    }
                }
                self.emit_line(&format!("    call {}", func));
                if args.len() > 6 {
                    self.emit_line(&format!("    add rsp, {}", (args.len() - 6) * 8));
                }
            }
            IRInstruction::GcAlloc(typ) => {
                let size = typ.size_bytes();
                self.emit_line(&format!("    mov rdi, {}", size));
                self.emit_line("    call gc_alloc");
            }
            IRInstruction::Retain(obj) => {
                self.emit_line(&format!("    mov rdi, {}", self.value_name(obj)));
                self.emit_line("    call retain");
            }
            IRInstruction::Release(obj) => {
                self.emit_line(&format!("    mov rdi, {}", self.value_name(obj)));
                self.emit_line("    call release");
            }
            IRInstruction::Ret(value) => {
                if let Some(val) = value {
                    self.emit_line(&format!("    mov rax, {}", self.value_name(val)));
                }
                self.emit_line("    jmp .return");
            }
            IRInstruction::CondBr(cond, true_label, false_label) => {
                self.emit_line(&format!("    test {}, {}", self.value_name(cond), self.value_name(cond)));
                self.emit_line(&format!("    jnz .{}", true_label));
                self.emit_line(&format!("    jmp .{}", false_label));
            }
            IRInstruction::Br(label) => {
                self.emit_line(&format!("    jmp .{}", label));
            }
            IRInstruction::VCall(obj, index, args) => {
                self.emit_line(&format!("    mov rax, [{}]", self.value_name(obj)));
                self.emit_line(&format!("    mov rax, [rax + {}]", index * 8));
                let arg_regs = ["rdi", "rsi", "rdx"];
                for (i, arg) in args.iter().enumerate() {
                    if i < arg_regs.len() {
                        self.emit_line(&format!("    mov {}, {}", arg_regs[i], self.value_name(arg)));
                    } else {
                        self.emit_line(&format!("    push {}", self.value_name(arg)));
                    }
                }
                self.emit_line("    call rax");
            }
            _ => {
                self.emit_line(&format!("    ; unimplemented: {:?}", inst));
            }
        }
    }

    fn value_name(&self, val: &crate::ir::instruction::IRValue) -> String {
        if let Some(preg) = self.reg_alloc.get_assignment(val.id) {
            format!("r{}", preg)
        } else if let Some(slot) = self.reg_alloc.get_spill_slot(val.id) {
            format!("[rbp - {}]", (slot + 1) * 8)
        } else {
            format!("v{}", val.id)
        }
    }

    fn emit_line(&mut self, line: &str) {
        self.output.write_all(line.as_bytes()).unwrap();
        self.output.write_all(b"\n").unwrap();
    }

    fn emit_label(&mut self, name: &str) {
        self.emit_line(&format!("{}:", name));
    }

    fn emit_section(&mut self, name: &str) {
        self.emit_line(&format!("section .{}", name));
    }

    fn emit_prologue(&mut self) {
        self.emit_line("    push rbp");
        self.emit_line("    mov rbp, rsp");
        if self.stack_size > 0 {
            self.emit_line(&format!("    sub rsp, {}", self.stack_size));
        }
        self.emit_line("    push rbx");
        self.emit_line("    push r12");
        self.emit_line("    push r13");
        self.emit_line("    push r14");
        self.emit_line("    push r15");
    }

    fn emit_epilogue(&mut self) {
        self.emit_label(".return");
        self.emit_line("    pop r15");
        self.emit_line("    pop r14");
        self.emit_line("    pop r13");
        self.emit_line("    pop r12");
        self.emit_line("    pop rbx");
        self.emit_line("    mov rsp, rbp");
        self.emit_line("    pop rbp");
        self.emit_line("    ret");
    }
}
