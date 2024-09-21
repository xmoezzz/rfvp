use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct JzInst {
    address: u32,
    target: u32,
}

impl JzInst {
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

impl OpcodeBase for JzInst {
    fn opcode(&self) -> Opcode {
        Opcode::Jz
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "jz"
    }

    fn disassemble(&self) -> String {
        format!("{:8} 0x{:08x}", self.mnemonic(), self.target)
    }
}
