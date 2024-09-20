use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct SetgInst {
    address: u32,
}

impl SetgInst {
    pub fn new(address: u32) -> Self {
        Self {
            address,
        }
    }
}

impl OpcodeBase for SetgInst {
    fn opcode(&self) -> Opcode {
        Opcode::SetG
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "setg"
    }

    fn disassemble(&self) -> String {
        format!("{:8}", self.mnemonic())
    }
}