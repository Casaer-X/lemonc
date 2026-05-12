use super::type_system::IRType;

#[derive(Debug, Clone)]
pub struct IRValue {
    pub name: String,
    pub typ: IRType,
    pub id: u32,
}

#[derive(Debug, Clone)]
pub enum CmpOp {
    Eq, Ne, Lt, Gt, Le, Ge,
}

#[derive(Debug)]
pub enum IRInstruction {
    Alloca(IRType),
    Load(IRValue),
    Store(IRValue, IRValue),
    GEP(IRValue, Vec<IRValue>),

    Add(IRValue, IRValue),
    Sub(IRValue, IRValue),
    Mul(IRValue, IRValue),
    Div(IRValue, IRValue),
    Mod(IRValue, IRValue),

    ICmp(CmpOp, IRValue, IRValue),
    FCmp(CmpOp, IRValue, IRValue),

    Cast(IRValue, IRType),
    Bitcast(IRValue, IRType),

    CondBr(IRValue, String, String),
    Br(String),
    Ret(Option<IRValue>),

    Call(String, Vec<IRValue>),

    GcAlloc(IRType),
    Retain(IRValue),
    Release(IRValue),
    WeakRefCreate(IRValue),
    WeakRefGet(IRValue),

    VCall(IRValue, u32, Vec<IRValue>),
    ICall(IRValue, u32, Vec<IRValue>),

    Phi(Vec<(IRValue, String)>),
    Unreachable,
    Debug(String),
}
