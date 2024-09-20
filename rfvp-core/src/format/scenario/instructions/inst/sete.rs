use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct SeteInst {
    address: u32,
}

impl SeteInst {
    pub fn new(address: u32) -> Self {
        Self {
            address,
        }
    }
}

impl OpcodeBase for SeteInst {
    fn opcode(&self) -> Opcode {
        Opcode::SetE
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "sete"
    }

    fn disassemble(&self) -> String {
        format!("{:8}", self.mnemonic())
    }
}
