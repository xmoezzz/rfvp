use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct InitStackInst {
    address: u32,
    arg_count: u8,
    local_count: u8,
}

impl InitStackInst {
    pub fn new(address: u32, arg_count: u8, local_count: u8) -> Self {
        Self {
            address,
            arg_count,
            local_count,
        }
    }

    pub fn get_arg_count(&self) -> u8 {
        self.arg_count
    }

    pub fn get_local_count(&self) -> u8 {
        self.local_count
    }
}

impl OpcodeBase for InitStackInst {
    fn opcode(&self) -> Opcode {
        Opcode::InitStack
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "initstack"
    }

    fn disassemble(&self) -> String {
        format!("{:8} {:2} {:2}", self.mnemonic(), self.arg_count, self.local_count)
    }
}