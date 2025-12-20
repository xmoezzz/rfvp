use std::fmt;

/// A VM value.
///
/// Type mapping (from your RE notes):
/// - 0: nil
/// - 1: true (boolean)
/// - 2: i32
/// - 3: f32 (stored as raw u32 in bytecode)
/// - 4: const string (offset into script buffer, with length prefix in bytecode)
/// - 5: dynamic string (engine string pool)
/// - 6: table (engine table pool)
#[derive(Clone, Debug, PartialEq)]
pub enum Variant {
    Nil,
    Bool(bool),
    Int(i32),
    Float(f32),

    /// A constant string embedded in the script buffer.
    /// `off` points to the first byte of the string bytes (not including length prefix).
    /// `len` is the byte length.
    ConstStr { off: u32, len: u8 },

    /// A heap string owned by the VM/runtime.
    DynStr(String),

    /// A table identifier. The actual table storage lives in the runtime.
    Table(u32),

    /// Internal: used by the interpreter when it needs a placeholder.
    /// Not part of the original engine's Variant space.
    #[doc(hidden)]
    _Poison,
}

impl Variant {
    pub fn truthy(&self) -> bool {
        match self {
            Variant::Nil => false,
            Variant::Bool(b) => *b,
            Variant::Int(v) => *v != 0,
            Variant::Float(v) => *v != 0.0,
            Variant::ConstStr { .. } => true,
            Variant::DynStr(s) => !s.is_empty(),
            Variant::Table(_) => true,
            Variant::_Poison => false,
        }
    }

    pub fn type_tag_u8(&self) -> u8 {
        match self {
            Variant::Nil => 0,
            Variant::Bool(true) => 1,
            Variant::Bool(false) => 0, // engine uses Type==0 as falsey; false is not a first-class distinct tag
            Variant::Int(_) => 2,
            Variant::Float(_) => 3,
            Variant::ConstStr { .. } => 4,
            Variant::DynStr(_) => 5,
            Variant::Table(_) => 6,
            Variant::_Poison => 0xFF,
        }
    }
}

impl fmt::Display for Variant {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Variant::Nil => write!(f, "nil"),
            Variant::Bool(b) => write!(f, "{}", b),
            Variant::Int(v) => write!(f, "{}", v),
            Variant::Float(v) => write!(f, "{}", v),
            Variant::ConstStr { off, len } => write!(f, "const_str(off=0x{off:X}, len={len})"),
            Variant::DynStr(s) => write!(f, "{s:?}"),
            Variant::Table(id) => write!(f, "table({id})"),
            Variant::_Poison => write!(f, "<poison>"),
        }
    }
}
