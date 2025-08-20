use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct ModInst {
    address: u32,
}

impl ModInst {
    pub fn new(address: u32) -> Self {
        Self {
            address,
        }
    }
}

impl OpcodeBase for ModInst {
    fn opcode(&self) -> Opcode {
        Opcode::Mod
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "mod"
    }

    fn disassemble(&self) -> String {
        format!("{:8}", self.mnemonic())
    }
}
