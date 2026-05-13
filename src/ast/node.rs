use crate::lexer::token::TokenKind;

#[derive(Debug, Clone)]
pub struct Program {
    pub declarations: Vec<Declaration>,
}

#[derive(Debug, Clone)]
pub enum Declaration {
    Class(ClassDecl),
    Interface(InterfaceDecl),
    Function(FunctionDecl),
    Variable(VarDecl),
    Import(ImportDecl),
    Package(PackageDecl),
}

#[derive(Debug, Clone)]
pub struct ClassDecl {
    pub name: String,
    pub modifiers: Vec<ClassModifier>,
    pub type_params: Vec<TypeParam>,
    pub extends: Option<TypeRef>,
    pub implements: Vec<TypeRef>,
    pub members: Vec<ClassMember>,
}

#[derive(Debug, Clone)]
pub enum ClassModifier {
    Public,
    Private,
    Abstract,
    Final,
    Reflectable,
}

#[derive(Debug, Clone)]
pub struct TypeParam {
    pub name: String,
    pub bound: Option<TypeRef>,
}

#[derive(Debug, Clone)]
pub struct InterfaceDecl {
    pub name: String,
    pub type_params: Vec<TypeParam>,
    pub extends: Vec<TypeRef>,
    pub members: Vec<InterfaceMember>,
}

#[derive(Debug, Clone)]
pub enum InterfaceMember {
    MethodSignature(MethodSignature),
    Field(VarDecl),
}

#[derive(Debug, Clone)]
pub struct MethodSignature {
    pub return_type: TypeRef,
    pub name: String,
    pub params: Vec<Param>,
}

#[derive(Debug, Clone)]
pub struct FunctionDecl {
    pub return_type: TypeRef,
    pub name: String,
    pub params: Vec<Param>,
    pub body: Block,
    pub modifiers: Vec<MethodModifier>,
}

#[derive(Debug, Clone)]
pub enum ClassMember {
    Field(VarDecl),
    Method(MethodDecl),
    Constructor(ConstructorDecl),
    Destructor(DestructorDecl),
}

#[derive(Debug, Clone)]
pub enum MethodModifier {
    Public,
    Private,
    Static,
    Virtual,
    Override,
    Final,
    Abstract,
}

#[derive(Debug, Clone)]
pub struct MethodDecl {
    pub return_type: TypeRef,
    pub name: String,
    pub params: Vec<Param>,
    pub body: Option<Block>,
    pub modifiers: Vec<MethodModifier>,
}

#[derive(Debug, Clone)]
pub struct ConstructorDecl {
    pub name: String,
    pub params: Vec<Param>,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct DestructorDecl {
    pub name: String,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub struct VarDecl {
    pub var_type: TypeRef,
    pub name: String,
    pub initializer: Option<Expr>,
    pub modifiers: Vec<VarModifier>,
}

#[derive(Debug, Clone)]
pub enum VarModifier {
    Public,
    Private,
    Static,
    Final,
    Virtual,
    Override,
    Abstract,
}

#[derive(Debug, Clone)]
pub struct Param {
    pub param_type: TypeRef,
    pub name: String,
    pub default_value: Option<Expr>,
}

#[derive(Debug, Clone)]
pub enum TypeRef {
    Primitive(PrimitiveType),
    Named(String, Vec<TypeRef>),
    Array(Box<TypeRef>),
    FunctionPtr(Box<TypeRef>, Vec<TypeRef>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PrimitiveType {
    Void,
    Bool,
    Byte,
    Char,
    Short,
    Int,
    Long,
    Float,
    Double,
}

#[derive(Debug, Clone)]
pub struct ImportDecl {
    pub path: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PackageDecl {
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct Block {
    pub statements: Vec<Stmt>,
}

#[derive(Debug, Clone)]
pub enum Stmt {
    Block(Block),
    Expr(Expr),
    If(Expr, Box<Stmt>, Option<Box<Stmt>>),
    For(Option<Box<Stmt>>, Option<Expr>, Option<Expr>, Box<Stmt>),
    While(Expr, Box<Stmt>),
    Return(Option<Expr>),
    Break,
    Continue,
    Try(Block, Vec<CatchClause>, Option<Block>),
    VarDecl(VarDecl),
}

#[derive(Debug, Clone)]
pub struct CatchClause {
    pub var_type: TypeRef,
    pub name: String,
    pub body: Block,
}

#[derive(Debug, Clone)]
pub enum Expr {
    IntegerLiteral(i64),
    FloatLiteral(f64),
    StringLiteral(String),
    CharLiteral(char),
    BoolLiteral(bool),
    Null,
    This,
    Super,
    Variable(String),
    BinaryOp(BinaryOp, Box<Expr>, Box<Expr>),
    UnaryOp(UnaryOp, Box<Expr>),
    Assignment(Box<Expr>, Box<Expr>),
    Call(Box<Expr>, Vec<Expr>),
    MethodCall(Box<Expr>, String, Vec<Expr>),
    FieldAccess(Box<Expr>, String),
    ArrayAccess(Box<Expr>, Box<Expr>),
    New(String, Vec<TypeRef>, Vec<Expr>),
    Delete(Box<Expr>),
    Cast(TypeRef, Box<Expr>),
    InstanceOf(Box<Expr>, TypeRef),
    Sizeof(TypeRef),
    TypeId(Box<Expr>),
    Lambda(Vec<Param>, Box<LambdaBody>),
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),
    Throw(Box<Expr>),
}

#[derive(Debug, Clone)]
pub enum LambdaBody {
    Expr(Expr),
    Block(Block),
}

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum BinaryOp {
    Add, Sub, Mul, Div, Mod,
    And, Or, BitAnd, BitOr, BitXor,
    Shl, Shr,
    Eq, Ne, Lt, Gt, Le, Ge,
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Plus, Minus, Not, BitNot,
    Deref, AddressOf,
    PreInc, PreDec,
    PostInc, PostDec,
}

impl TryFrom<TokenKind> for BinaryOp {
    type Error = ();

    fn try_from(kind: TokenKind) -> Result<Self, ()> {
        match kind {
            TokenKind::Plus => Ok(BinaryOp::Add),
            TokenKind::Minus => Ok(BinaryOp::Sub),
            TokenKind::Star => Ok(BinaryOp::Mul),
            TokenKind::Slash => Ok(BinaryOp::Div),
            TokenKind::Percent => Ok(BinaryOp::Mod),
            TokenKind::And => Ok(BinaryOp::And),
            TokenKind::Or => Ok(BinaryOp::Or),
            TokenKind::Amp => Ok(BinaryOp::BitAnd),
            TokenKind::Pipe => Ok(BinaryOp::BitOr),
            TokenKind::Caret => Ok(BinaryOp::BitXor),
            TokenKind::Shl => Ok(BinaryOp::Shl),
            TokenKind::Shr => Ok(BinaryOp::Shr),
            TokenKind::Eq => Ok(BinaryOp::Eq),
            TokenKind::Ne => Ok(BinaryOp::Ne),
            TokenKind::Lt => Ok(BinaryOp::Lt),
            TokenKind::Gt => Ok(BinaryOp::Gt),
            TokenKind::Le => Ok(BinaryOp::Le),
            TokenKind::Ge => Ok(BinaryOp::Ge),
            _ => Err(()),
        }
    }
}

pub fn mangle_method_name(class_name: &str, method_name: &str, params: &[Param]) -> String {
    let mut mangled = format!("{}_{}", class_name, method_name);
    for p in params {
        mangled.push('_');
        mangled.push_str(&type_ref_mangle(&p.param_type));
    }
    mangled
}

pub fn type_ref_mangle(tr: &TypeRef) -> String {
    match tr {
        TypeRef::Primitive(p) => match p {
            PrimitiveType::Void => "v",
            PrimitiveType::Bool => "b",
            PrimitiveType::Byte => "u8",
            PrimitiveType::Char => "c",
            PrimitiveType::Short => "s",
            PrimitiveType::Int => "i",
            PrimitiveType::Long => "l",
            PrimitiveType::Float => "f",
            PrimitiveType::Double => "d",
        }.to_string(),
        TypeRef::Named(name, type_args) => {
            let mut s = name.to_lowercase();
            for ta in type_args {
                s.push('_');
                s.push_str(&type_ref_mangle(ta));
            }
            s
        }
        TypeRef::Array(inner) => format!("a{}", type_ref_mangle(inner)),
        TypeRef::FunctionPtr(ret, params) => {
            let mut s = format!("fp_{}", type_ref_mangle(ret));
            for p in params {
                s.push('_');
                s.push_str(&type_ref_mangle(p));
            }
            s
        }
    }
}
