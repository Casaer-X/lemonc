#[derive(Debug, Clone, PartialEq)]
pub enum IRType {
    Void,
    Integer { bits: u32, signed: bool },
    Float { bits: u32 },
    Pointer(Box<IRType>),
    Struct(String, Vec<IRType>),
    Array(Box<IRType>, u64),
    Function(Vec<IRType>, Box<IRType>),
    VTable(String),
    ITable(String),
    Object,
}

impl IRType {
    pub fn i8() -> Self { IRType::Integer { bits: 8, signed: true } }
    pub fn i16() -> Self { IRType::Integer { bits: 16, signed: true } }
    pub fn i32() -> Self { IRType::Integer { bits: 32, signed: true } }
    pub fn i64() -> Self { IRType::Integer { bits: 64, signed: true } }
    pub fn u8() -> Self { IRType::Integer { bits: 8, signed: false } }
    pub fn u32() -> Self { IRType::Integer { bits: 32, signed: false } }
    pub fn f32() -> Self { IRType::Float { bits: 32 } }
    pub fn f64() -> Self { IRType::Float { bits: 64 } }
    pub fn ptr(inner: IRType) -> Self { IRType::Pointer(Box::new(inner)) }

    pub fn size_bytes(&self) -> u32 {
        match self {
            IRType::Void => 0,
            IRType::Integer { bits, .. } => (*bits + 7) / 8,
            IRType::Float { bits } => *bits / 8,
            IRType::Pointer(_) => 8,
            IRType::Struct(_, fields) => {
                let mut size = 0u32;
                for f in fields {
                    size += f.size_bytes();
                    size = (size + 7) & !7;
                }
                size
            }
            IRType::Array(elem, count) => elem.size_bytes() * (*count as u32),
            IRType::Function(_, _) => 8,
            IRType::VTable(_) => 8,
            IRType::ITable(_) => 8,
            IRType::Object => 16,
        }
    }
}
