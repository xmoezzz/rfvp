use crate::script::opcode::Opcode;
use crate::script::opcode::OpcodeBase;
use alloc::format;
use alloc::string::String;

pub struct PushReturnInst {
    address: u32,
}

impl PushReturnInst {
    pub fn new(address: u32) -> Self {
        Self { address }
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
