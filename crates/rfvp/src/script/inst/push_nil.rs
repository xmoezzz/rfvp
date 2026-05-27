use crate::script::opcode::Opcode;
use crate::script::opcode::OpcodeBase;
use alloc::format;
use alloc::string::String;

pub struct PushNilInst {
    address: u32,
}

impl PushNilInst {
    pub fn new(address: u32) -> Self {
        Self { address }
    }
}

impl OpcodeBase for PushNilInst {
    fn opcode(&self) -> Opcode {
        Opcode::PushNil
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "push_nil"
    }

    fn disassemble(&self) -> String {
        format!("{:8}", self.mnemonic())
    }
}
