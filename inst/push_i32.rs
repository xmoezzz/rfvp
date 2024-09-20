use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct PushI32Inst {
    address: u32,
    value: i32,
}

impl PushI32Inst {
    pub fn new(address: u32, value: i32) -> Self {
        Self {
            address,
            value,
        }
    }

    pub fn get_value(&self) -> i32 {
        self.value
    }
}

impl OpcodeBase for PushI32Inst {
    fn opcode(&self) -> Opcode {
        Opcode::PushI32
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "push_i32"
    }

    fn disassemble(&self) -> String {
        format!("{:8} {}", self.mnemonic(), self.value)
    }
}
