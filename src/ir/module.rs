use super::function::IRFunction;
use super::type_system::IRType;

#[derive(Debug)]
pub struct GlobalVar {
    pub name: String,
    pub typ: IRType,
    pub initializer: Option<String>,
}

#[derive(Debug)]
pub struct Metadata {
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub struct IRModule {
    pub name: String,
    pub functions: Vec<IRFunction>,
    pub globals: Vec<GlobalVar>,
    pub metadata: Vec<Metadata>,
    pub classes: Vec<IRClass>,
}

#[derive(Debug)]
pub struct IRClass {
    pub name: String,
    pub instance_size: u32,
    pub vtable_methods: Vec<String>,
    pub itable_interfaces: Vec<String>,
    pub fields: Vec<IRField>,
    pub constructor: String,
}

#[derive(Debug)]
pub struct IRField {
    pub name: String,
    pub typ: IRType,
    pub offset: u32,
}

impl IRModule {
    pub fn new() -> Self {
        Self {
            name: String::new(),
            functions: Vec::new(),
            globals: Vec::new(),
            metadata: Vec::new(),
            classes: Vec::new(),
        }
    }

    pub fn get_class(&self, name: &str) -> Option<&IRClass> {
        self.classes.iter().find(|c| c.name == name)
    }
}
