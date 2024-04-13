use crate::script::opcode::OpcodeBase;
use crate::script::opcode::Opcode;

pub struct SyscallInst {
    address: u32,
    syscall_name: String,
}


impl SyscallInst {
    pub fn new(address: u32, syscall_name: String) -> Self {
        Self {
            address,
            syscall_name,
        }
    }
}

impl OpcodeBase for SyscallInst {
    fn opcode(&self) -> Opcode {
        Opcode::Syscall
    }

    fn address(&self) -> u32 {
        self.address
    }

    fn mnemonic(&self) -> &'static str {
        "syscall"
    }

    fn disassemble(&self) -> String {
        format!("{:8} {}", self.mnemonic(), self.syscall_name)
    }
}



