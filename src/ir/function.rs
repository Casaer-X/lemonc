use super::instruction::IRInstruction;
use super::type_system::IRType;

#[derive(Debug)]
pub struct BasicBlock {
    pub name: String,
    pub instructions: Vec<IRInstruction>,
    pub terminated: bool,
}

impl BasicBlock {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            instructions: Vec::new(),
            terminated: false,
        }
    }
}

#[derive(Debug)]
pub struct IRFunction {
    pub name: String,
    pub params: Vec<(String, IRType)>,
    pub return_type: IRType,
    pub basic_blocks: Vec<BasicBlock>,
    pub local_vars: u32,
}

impl IRFunction {
    pub fn new(name: &str, return_type: IRType) -> Self {
        Self {
            name: name.to_string(),
            params: Vec::new(),
            return_type,
            basic_blocks: vec![BasicBlock::new("entry")],
            local_vars: 0,
        }
    }

    pub fn add_param(&mut self, name: String, typ: IRType) {
        self.params.push((name, typ));
    }

    pub fn new_temp(&mut self) -> u32 {
        let id = self.local_vars;
        self.local_vars += 1;
        id
    }

    pub fn current_block(&mut self) -> &mut BasicBlock {
        self.basic_blocks.last_mut().unwrap()
    }

    pub fn new_block(&mut self, name: &str) -> usize {
        self.basic_blocks.push(BasicBlock::new(name));
        self.basic_blocks.len() - 1
    }

    pub fn push_instruction(&mut self, inst: IRInstruction) {
        if !self.current_block().terminated {
            self.current_block().instructions.push(inst);
        }
    }
}
