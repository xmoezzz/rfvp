use crate::script::opcode::OpcodeBase;
use crate::script::opcode::Opcode;

pub struct PushF32Inst {
    address: u32,
    value: f32,
}

impl PushF32Inst {
    pub fn new(address: u32, value: f32) -> Self {
        Self {
            address,
            value,
        }
    }
}

impl OpcodeBase for PushF32Inst {
    fn opcode(&self) -> Opcode {
        Opcode::PushF32
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "push_f32"
    }

    fn disassemble(&self) -> String {
        format!("{:8} {}", self.mnemonic(), self.value)
    }
}


