use crate::script::opcode::Opcode;
use crate::script::opcode::OpcodeBase;
use alloc::format;
use alloc::string::String;

pub struct AndInst {
    address: u32,
}

impl AndInst {
    pub fn new(address: u32) -> Self {
        Self { address }
    }
}

impl OpcodeBase for AndInst {
    fn opcode(&self) -> Opcode {
        Opcode::And
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "and"
    }

    fn disassemble(&self) -> String {
        format!("{:8}", self.mnemonic())
    }
}
