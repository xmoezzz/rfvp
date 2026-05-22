use crate::script::opcode::Opcode;
use crate::script::opcode::OpcodeBase;

pub struct RetInst {
    address: u32,
}

impl RetInst {
    pub fn new(address: u32) -> Self {
        Self { address }
    }
}

impl OpcodeBase for RetInst {
    fn opcode(&self) -> Opcode {
        Opcode::Ret
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "ret"
    }

    fn disassemble(&self) -> String {
        format!("{:8}", self.mnemonic())
    }
}
