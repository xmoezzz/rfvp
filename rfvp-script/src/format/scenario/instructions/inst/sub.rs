use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct SubInst {
    address: u32,
}

impl SubInst {
    pub fn new(address: u32) -> Self {
        Self {
            address,
        }
    }
}

impl OpcodeBase for SubInst {
    fn opcode(&self) -> Opcode {
        Opcode::Sub
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "sub"
    }

    fn disassemble(&self) -> String {
        format!("{:8}", self.mnemonic())
    }
}
