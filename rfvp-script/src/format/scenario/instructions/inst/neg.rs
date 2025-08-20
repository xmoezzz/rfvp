use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct NegInst {
    address: u32,
}

impl NegInst {
    pub fn new(address: u32) -> Self {
        Self {
            address,
        }
    }
}

impl OpcodeBase for NegInst {
    fn opcode(&self) -> Opcode {
        Opcode::Neg
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "neg"
    }

    fn disassemble(&self) -> String {
        format!("{:8}", self.mnemonic())
    }
}
