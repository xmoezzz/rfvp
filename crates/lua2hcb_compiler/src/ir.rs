use std::fmt;

#[derive(Clone, Debug)]
pub struct Label {
    pub name: String,
}

impl Label {
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into() }
    }
}

#[derive(Clone, Debug)]
pub enum Item {
    Label(Label),
    Op(OpKind),
}

#[derive(Clone, Debug)]
pub enum OpKind {
    // Simple one-byte ops
    Nop,
    Ret,
    Retv,
    PushNil,
    PushTrue,
    PushTop,
    PushReturn,
    Neg,
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    BitTest,
    And,
    Or,
    SetE,
    SetNe,
    SetG,
    SetLe,
    SetL,
    SetGe,

    // Ops with immediates
    InitStack { args: i8, locals: i8 },
    CallFn { name: String },
    Syscall { id: u16 },
    JmpAbs { target: u32 },
    JzAbs { target: u32 },
    JmpLabel { label: String },
    JzLabel { label: String },

    PushI8(i8),
    PushI16(i16),
    PushI32(i32),
    PushF32(f32),
    PushString(String),

    PushGlobal(u16),
    PushStack(i8),
    PushGlobalTable(u16),
    PushLocalTable(i8),

    PopGlobal(u16),
    PopStack(i8),
    PopGlobalTable(u16),
    PopLocalTable(i8),
}

impl fmt::Display for OpKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}", self)
    }
}
