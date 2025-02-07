pub mod inst;


pub enum Opcode {
    Nop = 0,
    InitStack = 1,
    Call = 2,
    Syscall,
    Ret,
    RetV,
    Jmp,
    Jz,
    PushNil,
    PushTrue,
    PushI32,
    PushI16,
    PushI8,
    PushF32,
    PushString,
    PushGlobal,
    PushStack,
    PushGlobalTable,
    PushLocalTable,
    PushTop,
    PushReturn,
    PopGlobal,
    PopStack,
    PopGlobalTable,
    PopLocalTable,
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
    SetNE,
    SetG,
    SetLE,
    SetL,
    SetGE,
}

impl TryFrom<i32> for Opcode {
    type Error = ();

    fn try_from(v: i32) -> core::result::Result<Self, Self::Error> {
        match v {
            x if x == Opcode::Nop as i32 => Ok(Opcode::Nop),
            x if x == Opcode::InitStack as i32 => Ok(Opcode::InitStack),
            x if x == Opcode::Call as i32 => Ok(Opcode::Call),
            x if x == Opcode::Syscall as i32 => Ok(Opcode::Syscall),
            x if x == Opcode::Ret as i32 => Ok(Opcode::Ret),
            x if x == Opcode::RetV as i32 => Ok(Opcode::RetV),
            x if x == Opcode::Jmp as i32 => Ok(Opcode::Jmp),
            x if x == Opcode::Jz as i32 => Ok(Opcode::Jz),
            x if x == Opcode::PushNil as i32 => Ok(Opcode::PushNil),
            x if x == Opcode::PushTrue as i32 => Ok(Opcode::PushTrue),
            x if x == Opcode::PushI32 as i32 => Ok(Opcode::PushI32),
            x if x == Opcode::PushI16 as i32 => Ok(Opcode::PushI16),
            x if x == Opcode::PushI8 as i32 => Ok(Opcode::PushI8),
            x if x == Opcode::PushF32 as i32 => Ok(Opcode::PushF32),
            x if x == Opcode::PushString as i32 => Ok(Opcode::PushString),
            x if x == Opcode::PushGlobal as i32 => Ok(Opcode::PushGlobal),
            x if x == Opcode::PushStack as i32 => Ok(Opcode::PushStack),
            x if x == Opcode::PushGlobalTable as i32 => Ok(Opcode::PushGlobalTable),
            x if x == Opcode::PushLocalTable as i32 => Ok(Opcode::PushLocalTable),
            x if x == Opcode::PushTop as i32 => Ok(Opcode::PushTop),
            x if x == Opcode::PushReturn as i32 => Ok(Opcode::PushReturn),
            x if x == Opcode::PopGlobal as i32 => Ok(Opcode::PopGlobal),
            x if x == Opcode::PopStack as i32 => Ok(Opcode::PopStack),
            x if x == Opcode::PopGlobalTable as i32 => Ok(Opcode::PopGlobalTable),
            x if x == Opcode::PopLocalTable as i32 => Ok(Opcode::PopLocalTable),
            x if x == Opcode::Neg as i32 => Ok(Opcode::Neg),
            x if x == Opcode::Add as i32 => Ok(Opcode::Add),
            x if x == Opcode::Sub as i32 => Ok(Opcode::Sub),
            x if x == Opcode::Mul as i32 => Ok(Opcode::Mul),
            x if x == Opcode::Div as i32 => Ok(Opcode::Div),
            x if x == Opcode::Mod as i32 => Ok(Opcode::Mod),
            x if x == Opcode::BitTest as i32 => Ok(Opcode::BitTest),
            x if x == Opcode::And as i32 => Ok(Opcode::And),
            x if x == Opcode::Or as i32 => Ok(Opcode::Or),
            x if x == Opcode::SetE as i32 => Ok(Opcode::SetE),
            x if x == Opcode::SetNE as i32 => Ok(Opcode::SetNE),
            x if x == Opcode::SetG as i32 => Ok(Opcode::SetG),
            x if x == Opcode::SetLE as i32 => Ok(Opcode::SetLE),
            x if x == Opcode::SetL as i32 => Ok(Opcode::SetL),
            x if x == Opcode::SetGE as i32 => Ok(Opcode::SetGE),
            _ => Err(()),
        }
    }
}

impl TryFrom<&str> for Opcode {
    type Error = ();

    fn try_from(v: &str) -> core::result::Result<Self, Self::Error> {
        match v {
            x if x == "nop" => Ok(Opcode::Nop),
            x if x == "init_stack" => Ok(Opcode::InitStack),
            x if x == "call" => Ok(Opcode::Call),
            x if x == "syscall" => Ok(Opcode::Syscall),
            x if x == "ret" => Ok(Opcode::Ret),
            x if x == "retv" => Ok(Opcode::RetV),
            x if x == "jmp" => Ok(Opcode::Jmp),
            x if x == "jz" => Ok(Opcode::Jz),
            x if x == "push_nil" => Ok(Opcode::PushNil),
            x if x == "push_true" => Ok(Opcode::PushTrue),
            x if x == "push_i32" => Ok(Opcode::PushI32),
            x if x == "push_i16" => Ok(Opcode::PushI16),
            x if x == "push_i8" => Ok(Opcode::PushI8),
            x if x == "push_f32" => Ok(Opcode::PushF32),
            x if x == "push_string" => Ok(Opcode::PushString),
            x if x == "push_global" => Ok(Opcode::PushGlobal),
            x if x == "push_stack" => Ok(Opcode::PushStack),
            x if x == "push_global_table" => Ok(Opcode::PushGlobalTable),
            x if x == "push_local_table" => Ok(Opcode::PushLocalTable),
            x if x == "push_top" => Ok(Opcode::PushTop),
            x if x == "push_return" => Ok(Opcode::PushReturn),
            x if x == "pop_global" => Ok(Opcode::PopGlobal),
            x if x == "pop_stack" => Ok(Opcode::PopStack),
            x if x == "pop_global_table" => Ok(Opcode::PopGlobalTable),
            x if x == "pop_local_table" => Ok(Opcode::PopLocalTable),
            x if x == "neg" => Ok(Opcode::Neg),
            x if x == "add" => Ok(Opcode::Add),
            x if x == "sub" => Ok(Opcode::Sub),
            x if x == "mul" => Ok(Opcode::Mul),
            x if x == "div" => Ok(Opcode::Div),
            x if x == "mod" => Ok(Opcode::Mod),
            x if x == "bit_test" => Ok(Opcode::BitTest),
            x if x == "and" => Ok(Opcode::And),
            x if x == "or" => Ok(Opcode::Or),
            x if x == "set_e" => Ok(Opcode::SetE),
            x if x == "set_ne" => Ok(Opcode::SetNE),
            x if x == "set_g" => Ok(Opcode::SetG),
            x if x == "set_le" => Ok(Opcode::SetLE),
            x if x == "set_l" => Ok(Opcode::SetL),
            x if x == "set_ge" => Ok(Opcode::SetGE),
            _ => Err(()),
        }
    }
}

impl ToString for Opcode {
    fn to_string(&self) -> String {
        match self {
            Opcode::Nop => "nop",
            Opcode::InitStack => "init_stack",
            Opcode::Call => "call",
            Opcode::Syscall => "syscall",
            Opcode::Ret => "ret",
            Opcode::RetV => "retv",
            Opcode::Jmp => "jmp",
            Opcode::Jz => "jz",
            Opcode::PushNil => "push_nil",
            Opcode::PushTrue => "push_true",
            Opcode::PushI32 => "push_i32",
            Opcode::PushI16 => "push_i16",
            Opcode::PushI8 => "push_i8",
            Opcode::PushF32 => "push_f32",
            Opcode::PushString => "push_string",
            Opcode::PushGlobal => "push_global",
            Opcode::PushStack => "push_stack",
            Opcode::PushGlobalTable => "push_global_table",
            Opcode::PushLocalTable => "push_local_table",
            Opcode::PushTop => "push_top",
            Opcode::PushReturn => "push_return",
            Opcode::PopGlobal => "pop_global",
            Opcode::PopStack => "pop_stack",
            Opcode::PopGlobalTable => "pop_global_table",
            Opcode::PopLocalTable => "pop_local_table",
            Opcode::Neg => "neg",
            Opcode::Add => "add",
            Opcode::Sub => "sub",
            Opcode::Mul => "mul",
            Opcode::Div => "div",
            Opcode::Mod => "mod",
            Opcode::BitTest => "bit_test",
            Opcode::And => "and",
            Opcode::Or => "or",
            Opcode::SetE => "set_e",
            Opcode::SetNE => "set_ne",
            Opcode::SetG => "set_g",
            Opcode::SetLE => "set_le",
            Opcode::SetL => "set_l",
            Opcode::SetGE => "set_ge",
        }.to_string()
    }
}

pub trait OpcodeBase {
    fn opcode(&self) -> Opcode;
    fn address(&self) -> u32;
    fn mnemonic(&self) -> &'static str;
    fn disassemble(&self) -> String;
}
