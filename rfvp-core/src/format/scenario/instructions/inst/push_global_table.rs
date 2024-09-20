use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct PushGlobalTableInst {
    address: u32,
    idx: u32,
}

impl PushGlobalTableInst {
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

impl OpcodeBase for PushGlobalTableInst {
    fn opcode(&self) -> Opcode {
        Opcode::PushGlobalTable
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "push_global_table"
    }

    fn disassemble(&self) -> String {
        format!("{:8} {}", self.mnemonic(), self.idx)
    }
}

