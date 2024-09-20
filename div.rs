use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct DivInst {
    address: u32,
}

impl DivInst {
    pub fn new(address: u32) -> Self {
        Self {
            address,
        }
    }
}

impl OpcodeBase for DivInst {
    fn opcode(&self) -> Opcode {
        Opcode::Div
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "div"
    }

    fn disassemble(&self) -> String {
        format!("{:8}", self.mnemonic())
    }
}
