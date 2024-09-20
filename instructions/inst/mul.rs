use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct MulInst {
    address: u32,
}

impl MulInst {
    pub fn new(address: u32) -> Self {
        Self {
            address,
        }
    }
}

impl OpcodeBase for MulInst {
    fn opcode(&self) -> Opcode {
        Opcode::Mul
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "mul"
    }

    fn disassemble(&self) -> String {
        format!("{:8}", self.mnemonic())
    }
}

