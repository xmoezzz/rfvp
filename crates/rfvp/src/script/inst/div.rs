use crate::script::opcode::Opcode;
use crate::script::opcode::OpcodeBase;
use alloc::format;
use alloc::string::String;

pub struct DivInst {
    address: u32,
}

impl DivInst {
    pub fn new(address: u32) -> Self {
        Self { address }
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
