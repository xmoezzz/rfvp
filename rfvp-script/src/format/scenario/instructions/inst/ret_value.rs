use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct RetValueInst {
    address: u32,
}

impl RetValueInst {
    pub fn new(address: u32) -> Self {
        Self {
            address,
        }
    }
}

impl OpcodeBase for RetValueInst {
    fn opcode(&self) -> Opcode {
        Opcode::RetV
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "ret_value"
    }

    fn disassemble(&self) -> String {
        format!("{:8}", self.mnemonic())
    }
}
