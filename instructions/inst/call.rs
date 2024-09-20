use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct CallInst {
    address: u32,
    target: u32,
}

impl CallInst {
    pub fn new(address: u32, target: u32) -> Self {
        Self {
            address,
            target,
        }
    }

    pub fn get_target(&self) -> u32 {
        self.target
    }
}

impl OpcodeBase for CallInst {
    fn opcode(&self) -> Opcode {
        Opcode::Call
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "call"
    }

    fn disassemble(&self) -> String {
        format!("{:8} 0x{:08x}", self.mnemonic(), self.target)
    }
}
