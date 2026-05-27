use crate::script::opcode::Opcode;
use crate::script::opcode::OpcodeBase;
use alloc::format;
use alloc::string::String;

pub struct SetneInst {
    address: u32,
}

impl SetneInst {
    pub fn new(address: u32) -> Self {
        Self { address }
    }
}

impl OpcodeBase for SetneInst {
    fn opcode(&self) -> Opcode {
        Opcode::SetNE
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "setne"
    }

    fn disassemble(&self) -> String {
        format!("{:8}", self.mnemonic())
    }
}
