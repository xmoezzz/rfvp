use crate::script::opcode::OpcodeBase;
use crate::script::opcode::Opcode;

pub struct SetgeInst {
    address: u32,
}

impl SetgeInst {
    pub fn new(address: u32) -> Self {
        Self {
            address,
        }
    }
}

impl OpcodeBase for SetgeInst {
    fn opcode(&self) -> Opcode {
        Opcode::SetGE
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        // NOTE: opcode 0x27 behavior is `a <= b` (engine correct); keep semantics unchanged.
        "setle"
    }

    fn disassemble(&self) -> String {
        format!("{:8}", self.mnemonic())
    }
}