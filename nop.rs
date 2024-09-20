use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct NopInst {
    address: u32,
}

impl NopInst {
    pub fn new(address: u32) -> Self {
        Self { address }
    }
}

impl OpcodeBase for NopInst {
    fn opcode(&self) -> Opcode {
        Opcode::Nop
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "nop"
    }

    fn disassemble(&self) -> String {
        format!("{:8}", self.mnemonic())
    }
}


