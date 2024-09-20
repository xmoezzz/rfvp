use crate::format::scenario::instructions::OpcodeBase;
use crate::format::scenario::instructions::Opcode;

pub struct PushStringInst {
    address: u32,
    value: String,
}

impl PushStringInst {
    pub fn new(address: u32, value: String) -> Self {
        Self {
            address,
            value,
        }
    }

    pub fn get_value(&self) -> &String {
        &self.value
    }
}

impl OpcodeBase for PushStringInst {
    fn opcode(&self) -> Opcode {
        Opcode::PushString
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "push_string"
    }

    fn disassemble(&self) -> String {
        format!("{:8} {}", self.mnemonic(), self.value)
    }
}
