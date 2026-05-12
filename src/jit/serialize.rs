use crate::jit::bytecode::*;
use std::io::{Read, Write, Result};

/// Lemon Bytecode File Format (.lmb)
/// 
/// File structure:
/// - Magic: "LMB\0" (4 bytes)
/// - Version: u32 (4 bytes)
/// - String pool section
/// - Function section
/// - Class section
/// - Entry point: u32

const MAGIC: &[u8] = b"LMB\0";
const VERSION: u32 = 1;

pub fn write_module<W: Write>(writer: &mut W, module: &BytecodeModule) -> Result<()> {
    // Write magic
    writer.write_all(MAGIC)?;
    // Write version
    writer.write_all(&VERSION.to_le_bytes())?;
    
    // Write string pool
    writer.write_all(&(module.string_pool.len() as u32).to_le_bytes())?;
    for s in &module.string_pool {
        let bytes = s.as_bytes();
        writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
        writer.write_all(bytes)?;
    }
    
    // Write functions
    writer.write_all(&(module.functions.len() as u32).to_le_bytes())?;
    for func in &module.functions {
        write_function(writer, func)?;
    }
    
    // Write classes
    writer.write_all(&(module.classes.len() as u32).to_le_bytes())?;
    for class in &module.classes {
        write_class(writer, class)?;
    }
    
    // Write globals
    writer.write_all(&(module.globals.len() as u32).to_le_bytes())?;
    for g in &module.globals {
        let bytes = g.as_bytes();
        writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
        writer.write_all(bytes)?;
    }
    
    // Write entry point
    writer.write_all(&module.entry_point.to_le_bytes())?;
    
    Ok(())
}

fn write_function<W: Write>(writer: &mut W, func: &BytecodeFunction) -> Result<()> {
    // Name
    let name_bytes = func.name.as_bytes();
    writer.write_all(&(name_bytes.len() as u32).to_le_bytes())?;
    writer.write_all(name_bytes)?;
    
    // Params
    writer.write_all(&(func.params.len() as u32).to_le_bytes())?;
    for p in &func.params {
        let bytes = p.as_bytes();
        writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
        writer.write_all(bytes)?;
    }
    
    // Locals
    writer.write_all(&func.locals.to_le_bytes())?;
    
    // Is static
    writer.write_all(&[if func.is_static { 1 } else { 0 }])?;
    
    // Class name
    let class_name = func.class_name.as_ref().map(|s| s.as_str()).unwrap_or("");
    let class_name_bytes = class_name.as_bytes();
    writer.write_all(&(class_name_bytes.len() as u32).to_le_bytes())?;
    writer.write_all(class_name_bytes)?;
    
    // Code
    writer.write_all(&(func.code.len() as u32).to_le_bytes())?;
    for instr in &func.code {
        write_instruction(writer, instr)?;
    }
    
    Ok(())
}

fn write_instruction<W: Write>(writer: &mut W, instr: &Bytecode) -> Result<()> {
    let opcode = match instr {
        Bytecode::PushConst(_) => 0x01,
        Bytecode::PushFloat(_) => 0x02,
        Bytecode::PushString(_) => 0x03,
        Bytecode::PushBool(_) => 0x04,
        Bytecode::PushNull => 0x05,
        Bytecode::Pop => 0x06,
        Bytecode::Dup => 0x07,
        Bytecode::Swap => 0x08,
        Bytecode::LoadLocal(_) => 0x10,
        Bytecode::StoreLocal(_) => 0x11,
        Bytecode::LoadGlobal(_) => 0x12,
        Bytecode::StoreGlobal(_) => 0x13,
        Bytecode::LoadField(_) => 0x14,
        Bytecode::StoreField(_) => 0x15,
        Bytecode::Add => 0x20,
        Bytecode::Sub => 0x21,
        Bytecode::Mul => 0x22,
        Bytecode::Div => 0x23,
        Bytecode::Mod => 0x24,
        Bytecode::Neg => 0x25,
        Bytecode::BitAnd => 0x26,
        Bytecode::BitOr => 0x27,
        Bytecode::BitXor => 0x28,
        Bytecode::BitNot => 0x29,
        Bytecode::Shl => 0x2A,
        Bytecode::Shr => 0x2B,
        Bytecode::Eq => 0x30,
        Bytecode::Ne => 0x31,
        Bytecode::Lt => 0x32,
        Bytecode::Gt => 0x33,
        Bytecode::Le => 0x34,
        Bytecode::Ge => 0x35,
        Bytecode::And => 0x36,
        Bytecode::Or => 0x37,
        Bytecode::Not => 0x38,
        Bytecode::Jump(_) => 0x40,
        Bytecode::JumpIf(_) => 0x41,
        Bytecode::JumpIfNot(_) => 0x42,
        Bytecode::Call(_, _) => 0x43,
        Bytecode::CallMethod(_, _) => 0x44,
        Bytecode::Return => 0x45,
        Bytecode::New(_) => 0x50,
        Bytecode::NewArray => 0x51,
        Bytecode::ArrayGet => 0x52,
        Bytecode::ArraySet => 0x53,
        Bytecode::ArrayLen => 0x54,
        Bytecode::Delete => 0x55,
        Bytecode::Cast(_) => 0x60,
        Bytecode::InstanceOf(_) => 0x61,
        Bytecode::TypeId => 0x62,
        Bytecode::Print => 0x70,
        Bytecode::Println => 0x71,
        Bytecode::Printf(_) => 0x72,
        Bytecode::Halt => 0x73,
        Bytecode::Nop => 0x74,
    };
    
    writer.write_all(&[opcode])?;
    
    match instr {
        Bytecode::PushConst(v) => writer.write_all(&v.to_le_bytes())?,
        Bytecode::PushFloat(v) => writer.write_all(&v.to_le_bytes())?,
        Bytecode::PushString(idx) => writer.write_all(&idx.to_le_bytes())?,
        Bytecode::PushBool(b) => writer.write_all(&[*b as u8])?,
        Bytecode::LoadLocal(idx) => writer.write_all(&idx.to_le_bytes())?,
        Bytecode::StoreLocal(idx) => writer.write_all(&idx.to_le_bytes())?,
        Bytecode::LoadGlobal(idx) => writer.write_all(&idx.to_le_bytes())?,
        Bytecode::StoreGlobal(idx) => writer.write_all(&idx.to_le_bytes())?,
        Bytecode::LoadField(idx) => writer.write_all(&idx.to_le_bytes())?,
        Bytecode::StoreField(idx) => writer.write_all(&idx.to_le_bytes())?,
        Bytecode::Jump(target) => writer.write_all(&target.to_le_bytes())?,
        Bytecode::JumpIf(target) => writer.write_all(&target.to_le_bytes())?,
        Bytecode::JumpIfNot(target) => writer.write_all(&target.to_le_bytes())?,
        Bytecode::Call(func_idx, argc) => {
            writer.write_all(&func_idx.to_le_bytes())?;
            writer.write_all(&argc.to_le_bytes())?;
        }
        Bytecode::CallMethod(func_idx, argc) => {
            writer.write_all(&func_idx.to_le_bytes())?;
            writer.write_all(&argc.to_le_bytes())?;
        }
        Bytecode::New(class_idx) => writer.write_all(&class_idx.to_le_bytes())?,
        Bytecode::Cast(type_idx) => writer.write_all(&type_idx.to_le_bytes())?,
        Bytecode::InstanceOf(class_idx) => writer.write_all(&class_idx.to_le_bytes())?,
        Bytecode::Printf(argc) => writer.write_all(&argc.to_le_bytes())?,
        _ => {}
    }
    
    Ok(())
}

fn write_class<W: Write>(writer: &mut W, class: &BytecodeClass) -> Result<()> {
    // Name
    let name_bytes = class.name.as_bytes();
    writer.write_all(&(name_bytes.len() as u32).to_le_bytes())?;
    writer.write_all(name_bytes)?;
    
    // Parent
    let parent = class.parent.as_ref().map(|s| s.as_str()).unwrap_or("");
    let parent_bytes = parent.as_bytes();
    writer.write_all(&(parent_bytes.len() as u32).to_le_bytes())?;
    writer.write_all(parent_bytes)?;
    
    // Fields
    writer.write_all(&(class.fields.len() as u32).to_le_bytes())?;
    for f in &class.fields {
        let bytes = f.as_bytes();
        writer.write_all(&(bytes.len() as u32).to_le_bytes())?;
        writer.write_all(bytes)?;
    }
    
    // Methods
    writer.write_all(&(class.methods.len() as u32).to_le_bytes())?;
    for m in &class.methods {
        writer.write_all(&m.to_le_bytes())?;
    }
    
    // Vtable
    writer.write_all(&(class.vtable.len() as u32).to_le_bytes())?;
    for v in &class.vtable {
        writer.write_all(&v.to_le_bytes())?;
    }
    
    Ok(())
}

pub fn read_module<R: Read>(reader: &mut R) -> Result<BytecodeModule> {
    // Read magic
    let mut magic = [0u8; 4];
    reader.read_exact(&mut magic)?;
    if &magic != MAGIC {
        return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, "Invalid magic number"));
    }
    
    // Read version
    let mut version = [0u8; 4];
    reader.read_exact(&mut version)?;
    let _version = u32::from_le_bytes(version);
    
    let mut module = BytecodeModule::new();
    
    // Read string pool
    let mut len_buf = [0u8; 4];
    reader.read_exact(&mut len_buf)?;
    let string_count = u32::from_le_bytes(len_buf);
    for _ in 0..string_count {
        reader.read_exact(&mut len_buf)?;
        let len = u32::from_le_bytes(len_buf) as usize;
        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf)?;
        module.string_pool.push(String::from_utf8(buf).unwrap_or_default());
    }
    
    // Read functions
    reader.read_exact(&mut len_buf)?;
    let func_count = u32::from_le_bytes(len_buf);
    for _ in 0..func_count {
        module.functions.push(read_function(reader)?);
    }
    
    // Read classes
    reader.read_exact(&mut len_buf)?;
    let class_count = u32::from_le_bytes(len_buf);
    for _ in 0..class_count {
        module.classes.push(read_class(reader)?);
    }
    
    // Read globals
    reader.read_exact(&mut len_buf)?;
    let global_count = u32::from_le_bytes(len_buf);
    for _ in 0..global_count {
        reader.read_exact(&mut len_buf)?;
        let len = u32::from_le_bytes(len_buf) as usize;
        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf)?;
        module.globals.push(String::from_utf8(buf).unwrap_or_default());
    }
    
    // Read entry point
    let mut entry_buf = [0u8; 4];
    reader.read_exact(&mut entry_buf)?;
    module.entry_point = u32::from_le_bytes(entry_buf);
    
    Ok(module)
}

fn read_function<R: Read>(reader: &mut R) -> Result<BytecodeFunction> {
    let mut len_buf = [0u8; 4];
    
    // Name
    reader.read_exact(&mut len_buf)?;
    let name_len = u32::from_le_bytes(len_buf) as usize;
    let mut name_buf = vec![0u8; name_len];
    reader.read_exact(&mut name_buf)?;
    let name = String::from_utf8(name_buf).unwrap_or_default();
    
    // Params
    reader.read_exact(&mut len_buf)?;
    let param_count = u32::from_le_bytes(len_buf);
    let mut params = Vec::new();
    for _ in 0..param_count {
        reader.read_exact(&mut len_buf)?;
        let len = u32::from_le_bytes(len_buf) as usize;
        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf)?;
        params.push(String::from_utf8(buf).unwrap_or_default());
    }
    
    // Locals
    let mut locals_buf = [0u8; 4];
    reader.read_exact(&mut locals_buf)?;
    let locals = u32::from_le_bytes(locals_buf);
    
    // Is static
    let mut is_static_buf = [0u8; 1];
    reader.read_exact(&mut is_static_buf)?;
    let is_static = is_static_buf[0] != 0;
    
    // Class name
    reader.read_exact(&mut len_buf)?;
    let class_name_len = u32::from_le_bytes(len_buf) as usize;
    let mut class_name_buf = vec![0u8; class_name_len];
    reader.read_exact(&mut class_name_buf)?;
    let class_name_str = String::from_utf8(class_name_buf).unwrap_or_default();
    let class_name = if class_name_str.is_empty() { None } else { Some(class_name_str) };
    
    // Code
    reader.read_exact(&mut len_buf)?;
    let code_count = u32::from_le_bytes(len_buf);
    let mut code = Vec::new();
    for _ in 0..code_count {
        code.push(read_instruction(reader)?);
    }
    
    Ok(BytecodeFunction {
        name,
        params,
        locals,
        code,
        is_static,
        class_name,
    })
}

fn read_instruction<R: Read>(reader: &mut R) -> Result<Bytecode> {
    let mut opcode_buf = [0u8; 1];
    reader.read_exact(&mut opcode_buf)?;
    let opcode = opcode_buf[0];
    
    let mut u32_buf = [0u8; 4];
    let mut u8_buf = [0u8; 1];
    let mut i64_buf = [0u8; 8];
    let mut f64_buf = [0u8; 8];
    
    let instr = match opcode {
        0x01 => {
            reader.read_exact(&mut i64_buf)?;
            Bytecode::PushConst(i64::from_le_bytes(i64_buf))
        }
        0x02 => {
            reader.read_exact(&mut f64_buf)?;
            Bytecode::PushFloat(f64::from_le_bytes(f64_buf))
        }
        0x03 => {
            reader.read_exact(&mut u32_buf)?;
            Bytecode::PushString(u32::from_le_bytes(u32_buf))
        }
        0x04 => {
            reader.read_exact(&mut u8_buf)?;
            Bytecode::PushBool(u8_buf[0] != 0)
        }
        0x05 => Bytecode::PushNull,
        0x06 => Bytecode::Pop,
        0x07 => Bytecode::Dup,
        0x08 => Bytecode::Swap,
        0x10 => {
            reader.read_exact(&mut u32_buf)?;
            Bytecode::LoadLocal(u32::from_le_bytes(u32_buf))
        }
        0x11 => {
            reader.read_exact(&mut u32_buf)?;
            Bytecode::StoreLocal(u32::from_le_bytes(u32_buf))
        }
        0x12 => {
            reader.read_exact(&mut u32_buf)?;
            Bytecode::LoadGlobal(u32::from_le_bytes(u32_buf))
        }
        0x13 => {
            reader.read_exact(&mut u32_buf)?;
            Bytecode::StoreGlobal(u32::from_le_bytes(u32_buf))
        }
        0x14 => {
            reader.read_exact(&mut u32_buf)?;
            Bytecode::LoadField(u32::from_le_bytes(u32_buf))
        }
        0x15 => {
            reader.read_exact(&mut u32_buf)?;
            Bytecode::StoreField(u32::from_le_bytes(u32_buf))
        }
        0x20 => Bytecode::Add,
        0x21 => Bytecode::Sub,
        0x22 => Bytecode::Mul,
        0x23 => Bytecode::Div,
        0x24 => Bytecode::Mod,
        0x25 => Bytecode::Neg,
        0x26 => Bytecode::BitAnd,
        0x27 => Bytecode::BitOr,
        0x28 => Bytecode::BitXor,
        0x29 => Bytecode::BitNot,
        0x2A => Bytecode::Shl,
        0x2B => Bytecode::Shr,
        0x30 => Bytecode::Eq,
        0x31 => Bytecode::Ne,
        0x32 => Bytecode::Lt,
        0x33 => Bytecode::Gt,
        0x34 => Bytecode::Le,
        0x35 => Bytecode::Ge,
        0x36 => Bytecode::And,
        0x37 => Bytecode::Or,
        0x38 => Bytecode::Not,
        0x40 => {
            reader.read_exact(&mut u32_buf)?;
            Bytecode::Jump(u32::from_le_bytes(u32_buf))
        }
        0x41 => {
            reader.read_exact(&mut u32_buf)?;
            Bytecode::JumpIf(u32::from_le_bytes(u32_buf))
        }
        0x42 => {
            reader.read_exact(&mut u32_buf)?;
            Bytecode::JumpIfNot(u32::from_le_bytes(u32_buf))
        }
        0x43 => {
            reader.read_exact(&mut u32_buf)?;
            let func_idx = u32::from_le_bytes(u32_buf);
            reader.read_exact(&mut u32_buf)?;
            let argc = u32::from_le_bytes(u32_buf);
            Bytecode::Call(func_idx, argc)
        }
        0x44 => {
            reader.read_exact(&mut u32_buf)?;
            let func_idx = u32::from_le_bytes(u32_buf);
            reader.read_exact(&mut u32_buf)?;
            let argc = u32::from_le_bytes(u32_buf);
            Bytecode::CallMethod(func_idx, argc)
        }
        0x45 => Bytecode::Return,
        0x50 => {
            reader.read_exact(&mut u32_buf)?;
            Bytecode::New(u32::from_le_bytes(u32_buf))
        }
        0x51 => Bytecode::NewArray,
        0x52 => Bytecode::ArrayGet,
        0x53 => Bytecode::ArraySet,
        0x54 => Bytecode::ArrayLen,
        0x55 => Bytecode::Delete,
        0x60 => {
            reader.read_exact(&mut u32_buf)?;
            Bytecode::Cast(u32::from_le_bytes(u32_buf))
        }
        0x61 => {
            reader.read_exact(&mut u32_buf)?;
            Bytecode::InstanceOf(u32::from_le_bytes(u32_buf))
        }
        0x62 => Bytecode::TypeId,
        0x70 => Bytecode::Print,
        0x71 => Bytecode::Println,
        0x72 => {
            reader.read_exact(&mut u32_buf)?;
            Bytecode::Printf(u32::from_le_bytes(u32_buf))
        }
        0x73 => Bytecode::Halt,
        0x74 => Bytecode::Nop,
        _ => return Err(std::io::Error::new(std::io::ErrorKind::InvalidData, format!("Unknown opcode: 0x{:02x}", opcode))),
    };
    
    Ok(instr)
}

fn read_class<R: Read>(reader: &mut R) -> Result<BytecodeClass> {
    let mut len_buf = [0u8; 4];
    
    // Name
    reader.read_exact(&mut len_buf)?;
    let name_len = u32::from_le_bytes(len_buf) as usize;
    let mut name_buf = vec![0u8; name_len];
    reader.read_exact(&mut name_buf)?;
    let name = String::from_utf8(name_buf).unwrap_or_default();
    
    // Parent
    reader.read_exact(&mut len_buf)?;
    let parent_len = u32::from_le_bytes(len_buf) as usize;
    let mut parent_buf = vec![0u8; parent_len];
    reader.read_exact(&mut parent_buf)?;
    let parent_str = String::from_utf8(parent_buf).unwrap_or_default();
    let parent = if parent_str.is_empty() { None } else { Some(parent_str) };
    
    // Fields
    reader.read_exact(&mut len_buf)?;
    let field_count = u32::from_le_bytes(len_buf);
    let mut fields = Vec::new();
    for _ in 0..field_count {
        reader.read_exact(&mut len_buf)?;
        let len = u32::from_le_bytes(len_buf) as usize;
        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf)?;
        fields.push(String::from_utf8(buf).unwrap_or_default());
    }
    
    // Methods
    reader.read_exact(&mut len_buf)?;
    let method_count = u32::from_le_bytes(len_buf);
    let mut methods = Vec::new();
    for _ in 0..method_count {
        reader.read_exact(&mut len_buf)?;
        methods.push(u32::from_le_bytes(len_buf));
    }
    
    // Vtable
    reader.read_exact(&mut len_buf)?;
    let vtable_count = u32::from_le_bytes(len_buf);
    let mut vtable = Vec::new();
    for _ in 0..vtable_count {
        reader.read_exact(&mut len_buf)?;
        vtable.push(u32::from_le_bytes(len_buf));
    }
    
    Ok(BytecodeClass {
        name,
        parent,
        fields,
        methods,
        vtable,
    })
}
