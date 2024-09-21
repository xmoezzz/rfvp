use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct PushReturnInst {
    address: u32,
}

impl PushReturnInst {
    pub fn new(address: u32) -> Self {
        Self {
            address,
        }
    }
}

impl OpcodeBase for PushReturnInst {
    fn opcode(&self) -> Opcode {
        Opcode::PushReturn
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "push_return"
    }

    fn disassemble(&self) -> String {
        format!("{:8}", self.mnemonic())
    }
}
