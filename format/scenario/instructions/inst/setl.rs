use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct SetlInst {
    address: u32,
}

impl SetlInst {
    pub fn new(address: u32) -> Self {
        Self {
            address,
        }
    }
}

impl OpcodeBase for SetlInst {
    fn opcode(&self) -> Opcode {
        Opcode::SetL
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "setl"
    }

    fn disassemble(&self) -> String {
        format!("{:8}", self.mnemonic())
    }
}