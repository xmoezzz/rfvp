

pub trait Inst {
    fn address(&self) -> u32;
    fn set_address(&mut self, address: u32);
    fn serialize_to_binary(&self) -> Vec<u8>;
    fn size(&self) -> u32;
}


pub struct NopInst {
    address: u32,
}

impl NopInst {
    pub fn new() -> Self {
        Self {
            address: 0,
        }
    }
}

impl Inst for NopInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x00]
    }

    fn size(&self) -> u32 {
        1
    }
}

pub struct InitStackInst {
    address: u32,
    arg_count: u8,
    locals_count: u8,
}

impl InitStackInst {
    pub fn new(arg_count: u8, locals_count: u8) -> Self {
        Self {
            address: 0,
            arg_count,
            locals_count,
        }
    }
}

impl Inst for InitStackInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x01, self.arg_count, self.locals_count]
    }

    fn size(&self) -> u32 {
        3
    }
}


pub struct CallInst<'a> {
    address: u32,
    func_address: u32,
    // reference to the instruction that is being called
    func_target: Option<&'a dyn Inst>,
}

impl<'a> CallInst<'a> {
    pub fn new(func_address: u32) -> Self {
        Self {
            address: 0,
            func_address,
            func_target: None,
        }
    }

    fn set_func_target(&mut self, target: &'a dyn Inst) {
        self.func_target = Some(target);
    }

    fn get_func_target(&self) -> Option<&'a dyn Inst> {
        self.func_target
    }
}

impl<'a> Inst for CallInst<'a> {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x02, (self.func_address >> 24) as u8, (self.func_address >> 16) as u8, (self.func_address >> 8) as u8, self.func_address as u8]
    }

    fn size(&self) -> u32 {
        5
    }
}


pub struct SyscallInst {
    address: u32,
    syscall_id: u16,
}

impl SyscallInst {
    pub fn new(syscall_id: u16) -> Self {
        Self {
            address: 0,
            syscall_id,
        }
    }
}

impl Inst for SyscallInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x03, (self.syscall_id >> 8) as u8, self.syscall_id as u8]
    }

    fn size(&self) -> u32 {
        3
    }
}


pub struct RetInst {
    address: u32,
}

impl RetInst {
    pub fn new() -> Self {
        Self {
            address: 0,
        }
    }
}

impl Inst for RetInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x04]
    }

    fn size(&self) -> u32 {
        1
    }
}


pub struct RetVInst {
    address: u32,
}

impl RetVInst {
    pub fn new() -> Self {
        Self {
            address: 0,
        }
    }
}


impl Inst for RetVInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x05]
    }

    fn size(&self) -> u32 {
        1
    }
}


pub struct JmpInst<'a> {
    address: u32,
    target_address: u32,
    target: Option<&'a dyn Inst>,
}

impl<'a> JmpInst<'a> {
    pub fn new(target_address: u32) -> Self {
        Self {
            address: 0,
            target_address,
            target: None,
        }
    }

    fn set_target(&mut self, target: &'a dyn Inst) {
        self.target = Some(target);
    }

    fn get_target(&self) -> Option<&'a dyn Inst> {
        self.target
    }
}


impl<'a> Inst for JmpInst<'a> {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x06, (self.target_address >> 24) as u8, (self.target_address >> 16) as u8, (self.target_address >> 8) as u8, self.target_address as u8]
    }

    fn size(&self) -> u32 {
        5
    }
}


pub struct JzInst<'a> {
    address: u32,
    target_address: u32,
    target: Option<&'a dyn Inst>,
}

impl<'a> JzInst<'a> {
    pub fn new(target_address: u32) -> Self {
        Self {
            address: 0,
            target_address,
            target: None,
        }
    }

    fn set_target(&mut self, target: &'a dyn Inst) {
        self.target = Some(target);
    }

    fn get_target(&self) -> Option<&'a dyn Inst> {
        self.target
    }
}

impl<'a> Inst for JzInst<'a> {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x07, (self.target_address >> 24) as u8, (self.target_address >> 16) as u8, (self.target_address >> 8) as u8, self.target_address as u8]
    }

    fn size(&self) -> u32 {
        5
    }
}


pub struct PushNilInst {
    address: u32,
}

impl PushNilInst {
    pub fn new() -> Self {
        Self {
            address: 0,
        }
    }
}

impl Inst for PushNilInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x08]
    }

    fn size(&self) -> u32 {
        1
    }
}


pub struct PushTrueInst {
    address: u32,
}

impl PushTrueInst {
    pub fn new() -> Self {
        Self {
            address: 0,
        }
    }
}

impl Inst for PushTrueInst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x09]
    }

    fn size(&self) -> u32 {
        1
    }
}


pub struct PushI32Inst {
    address: u32,
    value: i32,
}

impl PushI32Inst {
    pub fn new(value: i32) -> Self {
        Self {
            address: 0,
            value,
        }
    }
}

impl Inst for PushI32Inst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        let mut bytes = vec![0x0A];
        bytes.extend_from_slice(&self.value.to_be_bytes());
        bytes
    }

    fn size(&self) -> u32 {
        5
    }
}


pub struct PushI16Inst {
    address: u32,
    value: i16,
}

impl PushI16Inst {
    pub fn new(value: i16) -> Self {
        Self {
            address: 0,
            value,
        }
    }
}

impl Inst for PushI16Inst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        let mut bytes = vec![0x0B];
        bytes.extend_from_slice(&self.value.to_be_bytes());
        bytes
    }

    fn size(&self) -> u32 {
        3
    }
}

pub struct PushI8Inst {
    address: u32,
    value: i8,
}

impl PushI8Inst {
    pub fn new(value: i8) -> Self {
        Self {
            address: 0,
            value,
        }
    }
}

impl Inst for PushI8Inst {
    fn address(&self) -> u32 {
        self.address
    }

    fn set_address(&mut self, address: u32) {
        self.address = address;
    }

    fn serialize_to_binary(&self) -> Vec<u8> {
        vec![0x0C, self.value as u8]
    }

    fn size(&self) -> u32 {
        2
    }
}




// pub enum Opcode {
//     Nop = 0,
//     InitStack = 1,
//     Call = 2,
//     Syscall,
//     Ret,
//     RetV,
//     Jmp,
//     Jz,
//     PushNil,
//     PushTrue,
//     PushI32,
//     PushI16,
//     PushI8,
//     PushF32,
//     PushString,
//     PushGlobal,
//     PushStack,
//     PushGlobalTable,
//     PushLocalTable,
//     PushTop,
//     PushReturn,
//     PopGlobal,
//     PopStack,
//     PopGlobalTable,
//     PopLocalTable,
//     Neg,
//     Add,
//     Sub,
//     Mul,
//     Div,
//     Mod,
//     BitTest,
//     And,
//     Or,
//     SetE,
//     SetNE,
//     SetG,
//     SetLE,
//     SetL,
//     SetGE,
// }


