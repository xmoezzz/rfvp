use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct PopGlobalTableInst {
    address: u32,
    idx: u32,
}

impl PopGlobalTableInst {
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

impl OpcodeBase for PopGlobalTableInst {
    fn opcode(&self) -> Opcode {
        Opcode::PopGlobalTable
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "pop_global_table"
    }

    fn disassemble(&self) -> String {
        format!("{:8} {}", self.mnemonic(), self.idx)
    }
}
