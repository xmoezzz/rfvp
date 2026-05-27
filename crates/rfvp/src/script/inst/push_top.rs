use crate::script::opcode::Opcode;
use crate::script::opcode::OpcodeBase;
use alloc::format;
use alloc::string::String;

pub struct PushTopInst {
    address: u32,
}

impl PushTopInst {
    pub fn new(address: u32) -> Self {
        Self { address }
    }
}

impl OpcodeBase for PushTopInst {
    fn opcode(&self) -> Opcode {
        Opcode::PushTop
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "push_top"
    }

    fn disassemble(&self) -> String {
        format!("{:8}", self.mnemonic())
    }
}
