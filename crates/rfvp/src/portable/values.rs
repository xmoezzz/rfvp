use alloc::string::String;
use alloc::vec::Vec;

#[derive(Debug, Clone, Default, PartialEq)]
pub enum Variant {
    #[default]
    Nil,
    True,
    Int(i32),
    Float(f32),
    String(String),
    ConstString(String, u32),
    Table(Table),
    SavedStackInfo(SavedStackInfo),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Table {
    entries: Vec<(u32, Variant)>,
    next_index: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SavedStackInfo {
    pub stack_base: usize,
    pub stack_pos: usize,
    pub return_addr: usize,
    pub args: usize,
}

impl Table {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, value: Variant) {
        let key = self.next_index;
        self.insert(key, value);
        self.next_index = key.saturating_add(1);
    }

    pub fn insert(&mut self, key: u32, value: Variant) {
        if let Some((_, existing)) = self.entries.iter_mut().find(|(k, _)| *k == key) {
            *existing = value;
        } else {
            self.entries.push((key, value));
        }
        if key >= self.next_index {
            self.next_index = key.saturating_add(1);
        }
    }

    pub fn get(&self, key: u32) -> Option<&Variant> {
        self.entries.iter().find(|(k, _)| *k == key).map(|(_, v)| v)
    }

    pub fn get_mut(&mut self, key: u32) -> Option<&mut Variant> {
        self.entries
            .iter_mut()
            .find(|(k, _)| *k == key)
            .map(|(_, v)| v)
    }
}

impl Variant {
    pub fn is_nil(&self) -> bool {
        matches!(self, Self::Nil)
    }

    pub fn canbe_true(&self) -> bool {
        !self.is_nil()
    }

    pub fn as_int(&self) -> Option<i32> {
        match self {
            Self::Int(v) => Some(*v),
            _ => None,
        }
    }

    pub fn as_saved_stack_info(&self) -> Option<&SavedStackInfo> {
        match self {
            Self::SavedStackInfo(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_saved_stack_info_mut(&mut self) -> Option<&mut SavedStackInfo> {
        match self {
            Self::SavedStackInfo(v) => Some(v),
            _ => None,
        }
    }

    pub fn as_table_mut(&mut self) -> Option<&mut Table> {
        match self {
            Self::Table(table) => Some(table),
            _ => None,
        }
    }

    pub fn set_nil(&mut self) {
        *self = Self::Nil;
    }

    pub fn cast_table(&mut self) {
        *self = Self::Table(Table::new());
    }

    pub fn neg(&mut self) {
        match self {
            Self::Int(v) => *v = v.wrapping_neg(),
            Self::Float(v) => *v = -*v,
            _ => {}
        }
    }

    pub fn add(&mut self, rhs: &Self) {
        *self = match (self.clone(), rhs.clone()) {
            (Self::Int(a), Self::Int(b)) => Self::Int(a.wrapping_add(b)),
            (Self::Float(a), Self::Float(b)) => Self::Float(a + b),
            (Self::Int(a), Self::Float(b)) => Self::Float(a as f32 + b),
            (Self::Float(a), Self::Int(b)) => Self::Float(a + b as f32),
            (Self::String(a), Self::String(b)) => Self::String(a + &b),
            (Self::String(a), Self::ConstString(b, _)) => Self::String(a + &b),
            (Self::ConstString(a, _), Self::String(b)) => Self::String(a + &b),
            (Self::ConstString(a, _), Self::ConstString(b, _)) => Self::String(a + &b),
            _ => Self::Nil,
        };
    }

    pub fn sub(&mut self, rhs: &Self) {
        *self = match (self.clone(), rhs) {
            (Self::Int(a), Self::Int(b)) => Self::Int(a.wrapping_sub(*b)),
            (Self::Float(a), Self::Float(b)) => Self::Float(a - *b),
            (Self::Int(a), Self::Float(b)) => Self::Float(a as f32 - *b),
            (Self::Float(a), Self::Int(b)) => Self::Float(a - *b as f32),
            _ => Self::Nil,
        };
    }

    pub fn mul(&mut self, rhs: &Self) {
        *self = match (self.clone(), rhs) {
            (Self::Int(a), Self::Int(b)) => Self::Int(a.wrapping_mul(*b)),
            (Self::Float(a), Self::Float(b)) => Self::Float(a * *b),
            (Self::Int(a), Self::Float(b)) => Self::Float(a as f32 * *b),
            (Self::Float(a), Self::Int(b)) => Self::Float(a * *b as f32),
            _ => Self::Nil,
        };
    }

    pub fn div(&mut self, rhs: &Self) {
        *self = match (self.clone(), rhs) {
            (_, Self::Int(0)) | (_, Self::Float(0.0)) => Self::Nil,
            (Self::Int(a), Self::Int(b)) => Self::Int(a.wrapping_div(*b)),
            (Self::Float(a), Self::Float(b)) => Self::Float(a / *b),
            (Self::Int(a), Self::Float(b)) => Self::Float(a as f32 / *b),
            (Self::Float(a), Self::Int(b)) => Self::Float(a / *b as f32),
            _ => Self::Nil,
        };
    }

    pub fn modulo(&mut self, rhs: &Self) {
        *self = match (self.clone(), rhs) {
            (_, Self::Int(0)) => Self::Nil,
            (Self::Int(a), Self::Int(b)) => Self::Int(a.wrapping_rem(*b)),
            _ => Self::Nil,
        };
    }

    pub fn bit_test(&mut self, rhs: &Self) {
        *self = match (self.as_int(), rhs.as_int()) {
            (Some(a), Some(b)) if b >= 0 && b < 32 && (a & (1 << b)) != 0 => Self::True,
            _ => Self::Nil,
        };
    }

    pub fn and(&mut self, rhs: &Self) {
        *self = if self.canbe_true() && rhs.canbe_true() {
            Self::True
        } else {
            Self::Nil
        };
    }

    pub fn or(&mut self, rhs: &Self) {
        *self = if self.canbe_true() || rhs.canbe_true() {
            Self::True
        } else {
            Self::Nil
        };
    }

    pub fn equal(&mut self, rhs: &Self) {
        *self = if values_equal(self, rhs) {
            Self::True
        } else {
            Self::Nil
        };
    }

    pub fn not_equal(&mut self, rhs: &Self) {
        let mut tmp = self.clone();
        tmp.equal(rhs);
        *self = if tmp.is_nil() { Self::True } else { Self::Nil };
    }

    pub fn greater(&mut self, rhs: &Self) {
        *self = if compare_values(self, rhs).map(|v| v > 0).unwrap_or(false) {
            Self::True
        } else {
            Self::Nil
        };
    }

    pub fn less(&mut self, rhs: &Self) {
        *self = if compare_values(self, rhs).map(|v| v < 0).unwrap_or(false) {
            Self::True
        } else {
            Self::Nil
        };
    }

    pub fn greater_equal(&mut self, rhs: &Self) {
        let mut tmp = self.clone();
        tmp.less(rhs);
        *self = if tmp.is_nil() { Self::True } else { Self::Nil };
    }

    pub fn less_equal(&mut self, rhs: &Self) {
        let mut tmp = self.clone();
        tmp.greater(rhs);
        *self = if tmp.is_nil() { Self::True } else { Self::Nil };
    }
}

fn values_equal(lhs: &Variant, rhs: &Variant) -> bool {
    match (lhs, rhs) {
        (Variant::Nil, Variant::Nil) | (Variant::True, Variant::True) => true,
        (Variant::Int(a), Variant::Int(b)) => a == b,
        (Variant::Float(a), Variant::Float(b)) => a == b,
        (Variant::String(a), Variant::String(b))
        | (Variant::String(a), Variant::ConstString(b, _))
        | (Variant::ConstString(a, _), Variant::String(b))
        | (Variant::ConstString(a, _), Variant::ConstString(b, _)) => a == b,
        _ => false,
    }
}

fn compare_values(lhs: &Variant, rhs: &Variant) -> Option<i8> {
    match (lhs, rhs) {
        (Variant::Int(a), Variant::Int(b)) => Some(a.cmp(b) as i8),
        (Variant::Int(a), Variant::Float(b)) => compare_f32(*a as f32, *b),
        (Variant::Float(a), Variant::Int(b)) => compare_f32(*a, *b as f32),
        (Variant::Float(a), Variant::Float(b)) => compare_f32(*a, *b),
        (Variant::String(a), Variant::String(b))
        | (Variant::String(a), Variant::ConstString(b, _))
        | (Variant::ConstString(a, _), Variant::String(b))
        | (Variant::ConstString(a, _), Variant::ConstString(b, _)) => Some(a.cmp(b) as i8),
        _ => None,
    }
}

fn compare_f32(lhs: f32, rhs: f32) -> Option<i8> {
    if lhs < rhs {
        Some(-1)
    } else if lhs > rhs {
        Some(1)
    } else if lhs == rhs {
        Some(0)
    } else {
        None
    }
}
