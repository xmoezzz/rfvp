use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct PopGlobalInst {
    address: u32,
    idx: u32,
}

impl PopGlobalInst {
    pub fn new(address: u32, idx: u32) -> Self {
        Self {
            address,
            idx,
        }
    }

    pub fn get_idx(&self) -> u32 {
        self.idx
    }
}

impl OpcodeBase for PopGlobalInst {
    fn opcode(&self) -> Opcode {
        Opcode::PopGlobal
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "pop_global"
    }

    fn disassemble(&self) -> String {
        format!("{:8} {}", self.mnemonic(), self.idx)
    }
}
