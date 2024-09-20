use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct PushStackInst {
    address: u32,
    idx: i8,
}

impl PushStackInst {
    pub fn new(address: u32, idx: i8) -> Self {
        Self {
            address,
            idx,
        }
    }

    pub fn get_idx(&self) -> i8 {
        self.idx
    }
}

impl OpcodeBase for PushStackInst {
    fn opcode(&self) -> Opcode {
        Opcode::PushStack
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "push_stack"
    }

    fn disassemble(&self) -> String {
        format!("{:8} {}", self.mnemonic(), self.idx)
    }
}
