#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PointerButton {
    Left,
    Right,
    Middle,
    Other(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyCode {
    Escape,
    Return,
    Space,
    Backspace,
    Tab,
    Left,
    Right,
    Up,
    Down,
    PageUp,
    PageDown,
    Home,
    End,
    Insert,
    Delete,
    Shift,
    Control,
    Alt,
    Character(char),
    Function(u8),
    Unknown(u32),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputModifiers {
    bits: u32,
}

impl InputModifiers {
    pub const SHIFT: Self = Self { bits: 1 << 0 };
    pub const CONTROL: Self = Self { bits: 1 << 1 };
    pub const ALT: Self = Self { bits: 1 << 2 };
    pub const SUPER: Self = Self { bits: 1 << 3 };

    pub const fn empty() -> Self {
        Self { bits: 0 }
    }

    pub const fn from_bits(bits: u32) -> Self {
        Self { bits }
    }

    pub const fn bits(self) -> u32 {
        self.bits
    }

    pub const fn contains(self, other: Self) -> bool {
        (self.bits & other.bits) == other.bits
    }

    pub const fn union(self, other: Self) -> Self {
        Self {
            bits: self.bits | other.bits,
        }
    }
}

impl Default for InputModifiers {
    fn default() -> Self {
        Self::empty()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RfvpEvent {
    Quit,
    FocusGained,
    FocusLost,
    KeyDown {
        key: KeyCode,
        repeat: bool,
        modifiers: InputModifiers,
    },
    KeyUp {
        key: KeyCode,
        modifiers: InputModifiers,
    },
    TextInput {
        ch: char,
    },
    PointerMove {
        x: i32,
        y: i32,
        in_screen: bool,
    },
    PointerDown {
        button: PointerButton,
        x: i32,
        y: i32,
    },
    PointerUp {
        button: PointerButton,
        x: i32,
        y: i32,
    },
    Wheel {
        delta_x: i32,
        delta_y: i32,
    },
    TouchDown {
        id: u64,
        x: i32,
        y: i32,
    },
    TouchMove {
        id: u64,
        x: i32,
        y: i32,
    },
    TouchUp {
        id: u64,
        x: i32,
        y: i32,
    },
}
