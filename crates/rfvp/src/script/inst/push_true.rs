use crate::script::opcode::Opcode;
use crate::script::opcode::OpcodeBase;

pub struct PushTrueInst {
    address: u32,
}

impl PushTrueInst {
    pub fn new(address: u32) -> Self {
        Self { address }
    }
}

impl OpcodeBase for PushTrueInst {
    fn opcode(&self) -> Opcode {
        Opcode::PushTrue
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "push_true"
    }

    fn disassemble(&self) -> String {
        format!("{:8}", self.mnemonic())
    }
}
