use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct SetleInst {
    address: u32,
}

impl SetleInst {
    pub fn new(address: u32) -> Self {
        Self {
            address,
        }
    }
}

impl OpcodeBase for SetleInst {
    fn opcode(&self) -> Opcode {
        Opcode::SetLE
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "setle"
    }

    fn disassemble(&self) -> String {
        format!("{:8}", self.mnemonic())
    }
}