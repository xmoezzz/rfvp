use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct BitTestInst {
    address: u32,
}

impl BitTestInst {
    pub fn new(address: u32) -> Self {
        Self {
            address,
        }
    }
}

impl OpcodeBase for BitTestInst {
    fn opcode(&self) -> Opcode {
        Opcode::BitTest
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "bittest"
    }

    fn disassemble(&self) -> String {
        format!("{:8}", self.mnemonic())
    }
}
