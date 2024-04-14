use super::variant::Expression;



pub enum Opcode {
    Mov = 0,
    Call = 1,
    Syscall = 2,
    Ret = 3,
    Jmp = 4,
    Jz = 5,
}

pub trait ReisaInst {
    fn address(&self) -> u32;
}

pub struct Mov {
    address: u32,
    left: Expression,
    right: Expression,
}

impl Mov {
    pub fn new(address: u32, left: Expression, right: Expression) -> Self {
        Self {
            address,
            left,
            right,
        }
    }

    pub fn get_left(&self) -> &Expression {
        &self.left
    }

    pub fn get_right(&self) -> &Expression {
        &self.right
    }
}

impl ReisaInst for Mov {
    fn address(&self) -> u32 {
        self.address
    }
}

pub struct Call {
    address: u32,
    target: u32,
    args: Vec<Expression>,
    return_value: Option<Expression>,
}

impl Call {
    pub fn new(address: u32, target: u32, args: Vec<Expression>, return_value: Option<Expression>) -> Self {
        Self {
            address,
            target,
            args,
            return_value,
        }
    }

    pub fn get_target(&self) -> u32 {
        self.target
    }

    pub fn get_args(&self) -> &Vec<Expression> {
        &self.args
    }

    pub fn get_return_value(&self) -> Option<&Expression> {
        self.return_value.as_ref()
    }
}

impl ReisaInst for Call {
    fn address(&self) -> u32 {
        self.address
    }
}

// Bad design, we should not have return value in syscall
// because syscall will set return value within itself, 
// we don't need to return it
pub struct Syscall {
    address: u32,
    syscall_name: String,
    args: Vec<Expression>,
}

impl Syscall {
    pub fn new(address: u32, syscall_name: String, args: Vec<Expression>) -> Self {
        Self {
            address,
            syscall_name,
            args,
        }
    }

    pub fn get_syscall_name(&self) -> String {
        self.syscall_name.clone()
    }

    pub fn get_args(&self) -> &Vec<Expression> {
        &self.args
    }
}

impl ReisaInst for Syscall {
    fn address(&self) -> u32 {
        self.address
    }
}

pub struct Ret {
    address: u32,
    return_value: Option<Expression>,
}

impl Ret {
    pub fn new(address: u32, return_value: Option<Expression>) -> Self {
        Self {
            address,
            return_value,
        }
    }

    pub fn get_return_value(&self) -> Option<&Expression> {
        self.return_value.as_ref()
    }
}

impl ReisaInst for Ret {
    fn address(&self) -> u32 {
        self.address
    }
}

pub struct Jmp {
    address: u32,
    target: u32,
}

impl Jmp {
    pub fn new(address: u32, target: u32) -> Self {
        Self {
            address,
            target,
        }
    }

    pub fn get_target(&self) -> u32 {
        self.target
    }
}

impl ReisaInst for Jmp {
    fn address(&self) -> u32 {
        self.address
    }
}

pub struct Jz {
    address: u32,
    target: u32,
    condition: Expression,
}

impl Jz {
    pub fn new(address: u32, target: u32, condition: Expression) -> Self {
        Self {
            address,
            target,
            condition,
        }
    }

    pub fn get_target(&self) -> u32 {
        self.target
    }

    pub fn get_condition(&self) -> &Expression {
        &self.condition
    }
}

impl ReisaInst for Jz {
    fn address(&self) -> u32 {
        self.address
    }
}




