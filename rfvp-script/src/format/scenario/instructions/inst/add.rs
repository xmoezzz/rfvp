use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct AddInst {
    address: u32,
}

impl AddInst {
    pub fn new(address: u32) -> Self {
        Self {
            address,
        }
    }
}

impl OpcodeBase for AddInst {
    fn opcode(&self) -> Opcode {
        Opcode::Add
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "add"
    }

    fn disassemble(&self) -> String {
        format!("{:8}", self.mnemonic())
    }
}