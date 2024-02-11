use std::collections::HashMap;
use serde::{Serialize, Deserialize};
use twofloat::TwoFloat;

pub mod context;
pub mod parser;
pub mod global;
pub mod opcode;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SavedStackInfo {
    stack_base: usize,
    stack_pos: usize,
    return_addr: usize,
}

/// Represents a value that can be stored in the VM
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum Variant {
    #[default]
    Nil,
    True,
    Int(i32),
    Float(f32),
    String(String),
    Table(HashMap<i32, Variant>),

    /// used to store the stack info when calling a function
    /// for internal use only
    _SavedStackInfo(SavedStackInfo),
}

impl Variant {
    pub fn is_nil(&self) -> bool {
        matches!(self, Variant::Nil)
    }

    pub fn is_true(&self) -> bool {
        matches!(self, Variant::True)
    }

    pub fn is_int(&self) -> bool {
        matches!(self, Variant::Int(_))
    }

    pub fn is_float(&self) -> bool {
        matches!(self, Variant::Float(_))
    }

    pub fn is_string(&self) -> bool {
        matches!(self, Variant::String(_))
    }

    pub fn is_table(&self) -> bool {
        matches!(self, Variant::Table(_))
    }

    pub fn is_saved_stack_info(&self) -> bool {
        matches!(self, Variant::_SavedStackInfo(_))
    }

    pub fn canbe_true(&self) -> bool {
        !matches!(self, Variant::Nil)
    }

    pub fn as_int(&self) -> Option<i32> {
        match self {
            Variant::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f32> {
        match self {
            Variant::Float(f) => Some(*f),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&String> {
        match self {
            Variant::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_table(&self) -> Option<&HashMap<i32, Variant>> {
        match self {
            Variant::Table(t) => Some(t),
            _ => None,
        }
    }

    pub fn as_table_mut(&mut self) -> Option<&mut HashMap<i32, Variant>> {
        match self {
            Variant::Table(t) => Some(t),
            _ => None,
        }
    }

    pub fn as_saved_stack_info(&self) -> Option<&SavedStackInfo> {
        match self {
            Variant::_SavedStackInfo(info) => Some(info),
            _ => None,
        }
    }

    pub fn set_nil(&mut self) {
        *self = Variant::Nil;
    }

    pub fn vadd(&mut self, other: &Variant) {
        *self = vm_add(self.clone(), other.clone());
    }

    pub fn vsub(&mut self, other: &Variant) {
        *self = vm_sub(self.clone(), other.clone());
    }

    pub fn vmul(&mut self, other: &Variant) {
        *self = vm_mul(self.clone(), other.clone());
    }

    pub fn vdiv(&mut self, other: &Variant) {
        *self = vm_div(self.clone(), other.clone());
    }

    pub fn vmod(&mut self, other: &Variant) {
        *self = vm_mod(self.clone(), other.clone());
    }

    pub fn neg(&mut self) {
        match self {
            Variant::Int(i) => *i = -*i,
            Variant::Float(f) => *f = -*f,
            _ => {},
        }
    }

    pub fn and(&mut self, other: &Variant) {
        let result = match (self.clone(), other) {
            (Variant::Nil, Variant::Nil) => Variant::Nil,
            (Variant::Nil, _) => Variant::Nil,
            (_, Variant::Nil) => Variant::Nil,
            _ => Variant::True,
        };

        *self = result;
    }

    pub fn or(&mut self, other: &Variant) {
        let result = match (self.clone(), other) {
            (Variant::Nil, Variant::Nil) => Variant::Nil,
            _ => Variant::True,
        };

        *self = result;
    }

    pub fn equal(&mut self, other: &Variant) {
        let result = match (self.clone(), other) {
            (Variant::Int(a), Variant::Int(b)) => {
                if a == *b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::Float(a), Variant::Float(b)) => {
                let wrapped_a = TwoFloat::from(a);
                let wrapped_b = TwoFloat::from(*b);

                if wrapped_a == wrapped_b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::Int(a), Variant::Float(b)) => {
                let wrapped_a = TwoFloat::from(a);
                let wrapped_b = TwoFloat::from(*b);

                if wrapped_a == wrapped_b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::Float(a), Variant::Int(b)) => {
                let wrapped_a = TwoFloat::from(a);
                let wrapped_b = TwoFloat::from(*b);

                if wrapped_a == wrapped_b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::String(a), Variant::String(b)) => {
                if a == *b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            _ => Variant::Nil,
        };

        *self = result;
    }

    pub fn not_equal(&mut self, other: &Variant) {
        let result = match (self.clone(), other) {
            (Variant::Int(a), Variant::Int(b)) => {
                if a != *b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::Float(a), Variant::Float(b)) => {
                let wrapped_a = TwoFloat::from(a);
                let wrapped_b = TwoFloat::from(*b);

                if wrapped_a != wrapped_b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::Int(a), Variant::Float(b)) => {
                let wrapped_a = TwoFloat::from(a);
                let wrapped_b = TwoFloat::from(*b);

                if wrapped_a != wrapped_b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::Float(a), Variant::Int(b)) => {
                let wrapped_a = TwoFloat::from(a);
                let wrapped_b = TwoFloat::from(*b);

                if wrapped_a != wrapped_b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::String(a), Variant::String(b)) => {
                if a != *b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            _ => Variant::Nil,
        };

        *self = result;
    }

    pub fn greater(&mut self, other: &Variant) {
        let result = match (self.clone(), other) {
            (Variant::Int(a), Variant::Int(b)) => {
                if a > *b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::Float(a), Variant::Float(b)) => {
                let wrapped_a = TwoFloat::from(a);
                let wrapped_b = TwoFloat::from(*b);

                if wrapped_a > wrapped_b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::Int(a), Variant::Float(b)) => {
                let wrapped_a = TwoFloat::from(a);
                let wrapped_b = TwoFloat::from(*b);

                if wrapped_a > wrapped_b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::Float(a), Variant::Int(b)) => {
                let wrapped_a = TwoFloat::from(a);
                let wrapped_b = TwoFloat::from(*b);

                if wrapped_a > wrapped_b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::String(a), Variant::String(b)) => {
                // TODO:
                // the original implementation of the VM uses lstrcmpA to compare strings
                // which is heavily dependent on the current locale (NLS)
                // we can reimplment this by rewriting the lstrcmpA function in Rust (from leaked winxp source code, very complex)
                // I tried to sumbit a PR to the wine project many years ago... but it was rejected
                //
                // In fact, the VM seems never use the partial comparison (less than, greater than, etc) for strings
                // so we can just use the default string comparison for now
                if a > *b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            _ => Variant::Nil,
        };

        *self = result;
    }

    pub fn less(&mut self, other: &Variant) {
        let _result = match (self.clone(), other) {
            (Variant::Int(a), Variant::Int(b)) => {
                if a < *b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::Float(a), Variant::Float(b)) => {
                let wrapped_a = TwoFloat::from(a);
                let wrapped_b = TwoFloat::from(*b);

                if wrapped_a < wrapped_b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::Int(a), Variant::Float(b)) => {
                let wrapped_a = TwoFloat::from(a);
                let wrapped_b = TwoFloat::from(*b);

                if wrapped_a < wrapped_b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::Float(a), Variant::Int(b)) => {
                let wrapped_a = TwoFloat::from(a);
                let wrapped_b = TwoFloat::from(*b);

                if wrapped_a < wrapped_b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::String(a), Variant::String(b)) => {
                if a < *b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            _ => Variant::Nil,
        };
    }

    pub fn greater_equal(&mut self, other: &Variant) {
        let mut lhs1 = self.clone();
        let mut lhs2 = self.clone();
        lhs1.greater(other);
        if lhs1.is_nil() {
            lhs2.equal(other);
            lhs1 = lhs2;
        }
        
        *self = lhs1;
    }

    pub fn less_equal(&mut self, other: &Variant) {
        let mut lhs1 = self.clone();
        let mut lhs2 = self.clone();
        lhs1.less(other);
        if lhs1.is_nil() {
            lhs2.equal(other);
            lhs1 = lhs2;
        }
        
        *self = lhs1;
    }
}

pub fn vm_add(a: Variant, b: Variant) -> Variant {
    match (a, b) {
        (Variant::Int(a), Variant::Int(b)) => Variant::Int(a + b),
        (Variant::Float(a), Variant::Float(b)) => {
            let wrapped_a = TwoFloat::from(a);
            let wrapped_b = TwoFloat::from(b);
            let result = wrapped_a + wrapped_b;
            Variant::Float(result.into())
        },
        (Variant::Int(a), Variant::Float(b)) => {
            let wrapped_a = TwoFloat::from(a);
            let wrapped_b = TwoFloat::from(b);
            let result = wrapped_a + wrapped_b;
            Variant::Float(result.into())
        },
        (Variant::Float(a), Variant::Int(b)) => {
            let wrapped_a = TwoFloat::from(a);
            let wrapped_b = TwoFloat::from(b as f32);
            let result = wrapped_a + wrapped_b;
            Variant::Float(result.into())
        },
        (Variant::String(a), Variant::String(b)) => Variant::String(a + &b),
        _ => Variant::Nil,
    }
}

pub fn vm_sub(a: Variant, b: Variant) -> Variant {
    match (a, b) {
        (Variant::Int(a), Variant::Int(b)) => Variant::Int(a - b),
        (Variant::Float(a), Variant::Float(b)) => {
            let wrapped_a = TwoFloat::from(a);
            let wrapped_b = TwoFloat::from(b);
            let result = wrapped_a - wrapped_b;
            Variant::Float(result.into())
        },
        (Variant::Int(a), Variant::Float(b)) => {
            let wrapped_a = TwoFloat::from(a);
            let wrapped_b = TwoFloat::from(b);
            let result = wrapped_a - wrapped_b;
            Variant::Float(result.into())
        },
        (Variant::Float(a), Variant::Int(b)) => {
            let wrapped_a = TwoFloat::from(a);
            let wrapped_b = TwoFloat::from(b);
            let result = wrapped_a - wrapped_b;
            Variant::Float(result.into())
        },
        _ => Variant::Nil,
    }
}

pub fn vm_mul(a: Variant, b: Variant) -> Variant {
    match (a, b) {
        (Variant::Int(a), Variant::Int(b)) => {
            let wrapped_a = TwoFloat::from(a);
            let wrapped_b = TwoFloat::from(b);
            let result = wrapped_a * wrapped_b;
            Variant::Float(result.into())
        },
        (Variant::Float(a), Variant::Float(b)) => {
            let wrapped_a = TwoFloat::from(a);
            let wrapped_b = TwoFloat::from(b);
            let result = wrapped_a * wrapped_b;
            Variant::Float(result.into())
        },
        (Variant::Int(a), Variant::Float(b)) => {
            let wrapped_a = TwoFloat::from(a);
            let wrapped_b = TwoFloat::from(b);
            let result = wrapped_a * wrapped_b;
            Variant::Float(result.into())
        },
        (Variant::Float(a), Variant::Int(b)) => {
            let wrapped_a = TwoFloat::from(a);
            let wrapped_b = TwoFloat::from(b);
            let result = wrapped_a * wrapped_b;
            Variant::Float(result.into())
        },
        _ => Variant::Nil,
    }
}

pub fn vm_div(a: Variant, b: Variant) -> Variant {
    match (a, b) {
        (Variant::Int(a), Variant::Int(b)) => Variant::Int(a / b),
        (Variant::Float(a), Variant::Float(b)) => {
            let wrapped_a = TwoFloat::from(a);
            let wrapped_b = TwoFloat::from(b);
            let result = wrapped_a / wrapped_b;
            if result.is_valid() {
                Variant::Float(result.into())
            }
            else {
                Variant::Nil
            }
        
        },
        (Variant::Int(a), Variant::Float(b)) => {
            let wrapped_a = TwoFloat::from(a);
            let wrapped_b = TwoFloat::from(b);
            let result = wrapped_a / wrapped_b;
            if result.is_valid() {
                Variant::Float(result.into())
            }
            else {
                Variant::Nil
            }
        },
        (Variant::Float(a), Variant::Int(b)) => {
            let wrapped_a = TwoFloat::from(a);
            let wrapped_b = TwoFloat::from(b);
            let result = wrapped_a / wrapped_b;
            Variant::Float(result.into())
        
        },
        _ => Variant::Nil,
    }
}

fn vm_mod(a: Variant, b: Variant) -> Variant {
    match (a, b) {
        (Variant::Int(a), Variant::Int(b)) => Variant::Int(a % b),
        _ => Variant::Nil,
    }
}

pub trait VmSyscall {
    fn do_syscall(&self, name: &str, args: Vec<Variant>) -> anyhow::Result<Variant>;
}
