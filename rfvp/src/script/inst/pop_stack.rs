use crate::script::opcode::OpcodeBase;
use crate::script::opcode::Opcode;

pub struct PopStackInst {
    address: u32,
    idx: i8,
}

impl PopStackInst {
    pub fn new(address: u32, idx: i8) -> Self {
        Self {
            address,
            idx,
        }
    }
}

impl OpcodeBase for PopStackInst {
    fn opcode(&self) -> Opcode {
        Opcode::PopStack
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "pop_stack"
    }

    fn disassemble(&self) -> String {
        format!("{:8} {}", self.mnemonic(), self.idx)
    }
}
