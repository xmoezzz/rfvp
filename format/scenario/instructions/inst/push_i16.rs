use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct PushI16Inst {
    address: u32,
    value: i16,
}

impl PushI16Inst {
    pub fn new(address: u32, value: i16) -> Self {
        Self {
            address,
            value,
        }
    }

    pub fn get_value(&self) -> i16 {
        self.value
    }
}

impl OpcodeBase for PushI16Inst {
    fn opcode(&self) -> Opcode {
        Opcode::PushI16
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "push_i16"
    }

    fn disassemble(&self) -> String {
        format!("{:8} {}", self.mnemonic(), self.value)
    }
}
