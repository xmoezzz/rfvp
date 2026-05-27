use crate::script::opcode::Opcode;
use crate::script::opcode::OpcodeBase;
use alloc::format;
use alloc::string::String;

pub struct OrInst {
    address: u32,
}

impl OrInst {
    pub fn new(address: u32) -> Self {
        Self { address }
    }
}

impl OpcodeBase for OrInst {
    fn opcode(&self) -> Opcode {
        Opcode::Or
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "or"
    }

    fn disassemble(&self) -> String {
        format!("{:8}", self.mnemonic())
    }
}
