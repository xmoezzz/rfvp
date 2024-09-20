use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct PushLocalTableInst {
    address: u32,
    idx: i8,
}

impl PushLocalTableInst {
    pub fn new(address: u32, idx: i8) -> Self {
        Self {
            address,
            idx,
        }
    }

    pub fn get_idx(&self) -> i8 {
        self.idx
    }
}

impl OpcodeBase for PushLocalTableInst {
    fn opcode(&self) -> Opcode {
        Opcode::PushLocalTable
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "push_local_table"
    }

    fn disassemble(&self) -> String {
        format!("{:8} {}", self.mnemonic(), self.idx)
    }
}
