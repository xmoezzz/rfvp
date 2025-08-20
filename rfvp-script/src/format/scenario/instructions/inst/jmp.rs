use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct JmpInst {
    address: u32,
    target: u32,
}

impl JmpInst {
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

impl OpcodeBase for JmpInst {
    fn opcode(&self) -> Opcode {
        Opcode::Jmp
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "jmp"
    }

    fn disassemble(&self) -> String {
        format!("{:8} 0x{:08x}", self.mnemonic(), self.target)
    }
}
