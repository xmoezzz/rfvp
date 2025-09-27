use serde::{Serialize, Deserialize};
use twofloat::TwoFloat;
use std::collections::HashMap;

pub mod context;
pub mod parser;
pub mod global;
pub mod opcode;
pub mod inst;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct SavedStackInfo {
    stack_base: usize,
    stack_pos: usize,
    return_addr: usize,
    args: usize,
}


#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Table {
    table: HashMap<u32, Variant>,
    count: u32,
    next_index: u32,
}

impl Table {
    pub fn new() -> Self {
        Table {
            table: HashMap::new(),
            count: 0,
            next_index: 0,
        }
    }

    pub fn push(&mut self, value: Variant) {
        self.table.insert(self.next_index, value);
        self.count += 1;
        self.next_index += 1;
    }

    pub fn insert(&mut self, key: u32, value: Variant) {
        self.table.insert(key, value);
        self.count += 1;
        self.next_index += 1;
    }


    pub fn get(&self, key: u32) -> Option<&Variant> {
        self.table.get(&key)
    }
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
    ConstString(String, u32),
    Table(Table),

    /// used to store the stack info when calling a function
    /// for internal use only
    SavedStackInfo(SavedStackInfo),
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

    #[allow(dead_code)]
    pub fn is_float(&self) -> bool {
        matches!(self, Variant::Float(_))
    }

    #[allow(dead_code)]
    pub fn is_string(&self) -> bool {
        matches!(self, Variant::String(_)) || matches!(self, Variant::ConstString(_, _))
    }

    #[allow(dead_code)]
    pub fn is_const_string(&self) -> bool {
        matches!(self, Variant::ConstString(_, _))
    }

    pub fn is_table(&self) -> bool {
        matches!(self, Variant::Table(_))
    }

    #[allow(dead_code)]
    pub fn is_saved_stack_info(&self) -> bool {
        matches!(self, Variant::SavedStackInfo(_))
    }

    pub fn canbe_true(&self) -> bool {
        !matches!(self, Variant::Nil)
    }

    pub fn cast_table(&mut self) {
        *self = Variant::Table(Table::new());
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
            Variant::ConstString(s, _) => Some(s),
            _ => None,
        }
    }

    pub fn as_table(&mut self) -> Option<&mut Table> {
        match self {
            Variant::Table(t) => Some(t),
            _ => None,
        }
    }

    #[allow(dead_code)]
    fn as_saved_stack_info(&self) -> Option<&SavedStackInfo> {
        match self {
            Variant::SavedStackInfo(info) => Some(info),
            _ => None,
        }
    }

    #[allow(dead_code)]
    pub fn as_saved_stack_info_mut(&mut self) -> Option<&mut SavedStackInfo> {
        match self {
            Variant::SavedStackInfo(info) => Some(info),
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
            (Variant::Nil, Variant::Nil) => Variant::True, // 0
            (Variant::True, Variant::True) => Variant::True, // 8 = 7 + 1
            (Variant::True, Variant::Int(_)) => Variant::True, // 9 = 7 + 2
            (Variant::True, Variant::Float(_)) => Variant::True, // 10 = 7 + 3
            (Variant::True, Variant::String(_)) => Variant::True, // 11 = 7 + 4
            (Variant::True, Variant::Table(_)) => Variant::True, // 13 = 7 + 6
            (Variant::Int(a), Variant::Int(b)) => { // 16 = 2 * 7 + 2
                if a == *b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::Float(_), Variant::True) => Variant::True, // 22 = 3 * 7 + 1
            (Variant::Float(a), Variant::Float(b)) => { // 24 = 3 * 7 + 3
                let wrapped_a = TwoFloat::from(a);
                let wrapped_b = TwoFloat::from(*b);

                if wrapped_a == wrapped_b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            // (Variant::Int(a), Variant::Float(b)) => { 
            //     let wrapped_a = TwoFloat::from(a);
            //     let wrapped_b = TwoFloat::from(*b);

            //     if wrapped_a == wrapped_b {
            //         Variant::True
            //     } else {
            //         Variant::Nil
            //     }
            // },
            // (Variant::Float(a), Variant::Int(b)) => {
            //     let wrapped_a = TwoFloat::from(a);
            //     let wrapped_b = TwoFloat::from(*b);

            //     if wrapped_a == wrapped_b {
            //         Variant::True
            //     } else {
            //         Variant::Nil
            //     }
            // },
            (Variant::String(_), Variant::True) => Variant::True, // 29 = 4 * 7 + 1
            (Variant::String(a), Variant::String(b)) => { // 32 = 4 * 7 + 4
                if a == *b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::String(a), Variant::ConstString(b, _)) => { // 33 = 4 * 7 + 5
                if a == *b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::ConstString(_, _), Variant::True) => Variant::True, // 36 = 5 * 7 + 1
            (Variant::ConstString(a, _), Variant::String(b)) => {  // 40 = 5 * 7 + 5
                if a == *b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::Table(_), Variant::True) => Variant::True, // 43 = 6 * 7 + 1
            _ => Variant::Nil,
        };

        *self = result;
    }

    pub fn not_equal(&mut self, other: &Variant) {
        let mut lhs = self.clone();
        lhs.equal(other);
        if lhs.is_nil() {
            *self = Variant::True;
            return;
        }
        
        *self = Variant::Nil;
    }

    pub fn greater(&mut self, other: &Variant) {
        let result = match (self.clone(), other) {
            (Variant::Int(a), Variant::Int(b)) => { // 16 = 2 * 7 + 2
                if a > *b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::Int(a), Variant::Float(b)) => { // 17 = 2 * 7 + 3
                let wrapped_a = TwoFloat::from(a);
                let wrapped_b = TwoFloat::from(*b);

                if wrapped_a > wrapped_b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::Float(a), Variant::Int(b)) => { // 23 = 3 * 7 + 2
                let wrapped_a = TwoFloat::from(a);
                let wrapped_b = TwoFloat::from(*b);

                if wrapped_a > wrapped_b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::Float(a), Variant::Float(b)) => { // 24 = 3 * 7 + 3
                let wrapped_a = TwoFloat::from(a);
                let wrapped_b = TwoFloat::from(*b);

                if wrapped_a > wrapped_b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::String(a), Variant::String(b)) => { //  32 = 4 * 7 + 4
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
            (Variant::String(a), Variant::ConstString(b, _)) => { // 33 = 4 * 7 + 5
                if a > *b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::ConstString(a, _), Variant::String(b)) => { // 39 = 5 * 7 + 4
                if a > *b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::ConstString(a, _), Variant::ConstString(b, _)) => { // 40 = 5 * 7 + 5
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
            (Variant::Int(a), Variant::Int(b)) => { // 16 = 2 * 7 + 2
                if a < *b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::Int(a), Variant::Float(b)) => { // 17 = 2 * 7 + 3
                let wrapped_a = TwoFloat::from(a);
                let wrapped_b = TwoFloat::from(*b);

                if wrapped_a < wrapped_b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::Float(a), Variant::Int(b)) => { // 23 = 3 * 7 + 2
                let wrapped_a = TwoFloat::from(a);
                let wrapped_b = TwoFloat::from(*b);

                if wrapped_a < wrapped_b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::Float(a), Variant::Float(b)) => { // 24 = 3 * 7 + 3
                let wrapped_a = TwoFloat::from(a);
                let wrapped_b = TwoFloat::from(*b);

                if wrapped_a < wrapped_b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::String(a), Variant::String(b)) => { //32 = 4 * 7 + 4
                if a < *b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::String(a), Variant::ConstString(b, _)) => { // 33 = 4 * 7 + 5
                if a < *b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::ConstString(a, _), Variant::String(b)) => { // 39 = 5 * 7 + 4
                if a < *b {
                    Variant::True
                } else {
                    Variant::Nil
                }
            },
            (Variant::ConstString(a, _), Variant::ConstString(b, _)) => { // 40 = 5 * 7 + 5
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
        let mut lhs = self.clone();
        lhs.less(other);
        if lhs.is_nil() {
            *self = Variant::True;
            return;
        }
        
        *self = Variant::Nil;
    }

    pub fn less_equal(&mut self, other: &Variant) {
        let mut lhs = self.clone();
        lhs.greater(other);
        if lhs.is_nil() {
            *self = Variant::True;
            return;
        }
        
        *self = Variant::Nil;
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
        (Variant::String(a), Variant::ConstString(b, _)) => Variant::String(a + &b),
        (Variant::ConstString(a, _), Variant::String(b)) => Variant::String(a + &b),
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
            let result = a * b;
            Variant::Int(result)
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
    fn do_syscall(&mut self, name: &str, args: Vec<Variant>) -> anyhow::Result<Variant>;
}
