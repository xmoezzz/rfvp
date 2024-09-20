use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct SetgeInst {
    address: u32,
}

impl SetgeInst {
    pub fn new(address: u32) -> Self {
        Self {
            address,
        }
    }
}

impl OpcodeBase for SetgeInst {
    fn opcode(&self) -> Opcode {
        Opcode::SetGE
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "setge"
    }

    fn disassemble(&self) -> String {
        format!("{:8}", self.mnemonic())
    }
}