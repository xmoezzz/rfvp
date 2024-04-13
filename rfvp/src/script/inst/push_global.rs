use crate::script::opcode::OpcodeBase;
use crate::script::opcode::Opcode;

pub struct PushGlobalInst {
    address: u32,
    idx: u32,
}

impl PushGlobalInst {
    pub fn new(address: u32, idx: u32) -> Self {
        Self {
            address,
            idx,
        }
    }
}

impl OpcodeBase for PushGlobalInst {
    fn opcode(&self) -> Opcode {
        Opcode::PushGlobal
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "push_global"
    }

    fn disassemble(&self) -> String {
        format!("{:8} {}", self.mnemonic(), self.idx)
    }
}

