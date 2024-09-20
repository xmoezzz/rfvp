use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct PushI8Inst {
    address: u32,
    value: i8,
}

impl PushI8Inst {
    pub fn new(address: u32, value: i8) -> Self {
        Self {
            address,
            value,
        }
    }

    pub fn get_value(&self) -> i8 {
        self.value
    }
}

impl OpcodeBase for PushI8Inst {
    fn opcode(&self) -> Opcode {
        Opcode::PushI8
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "push_i8"
    }

    fn disassemble(&self) -> String {
        format!("{:8} {}", self.mnemonic(), self.value)
    }
}
