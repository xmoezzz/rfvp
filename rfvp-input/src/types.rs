use bitflags::bitflags;

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KeyCode {
    Shift = 0,
    Ctrl = 1,
    LeftClick = 2,
    RightClick = 3,
    MouseLeft = 4,
    MouseRight = 5,
    Esc = 6,
    Enter = 7,
    Space = 8,
    UpArrow = 9,
    DownArrow = 10,
    LeftArrow = 11,
    RightArrow = 12,
    F1 = 13,
    F2 = 14,
    F3 = 15,
    F4 = 16,
    F5 = 17,
    F6 = 18,
    F7 = 19,
    F8 = 20,
    F9 = 21,
    F10 = 22,
    F11 = 23,
    F12 = 24,
    Tab = 25,
}

impl KeyCode {
    #[inline]
    pub const fn bit(self) -> u32 {
        1u32 << (self as u32)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonState {
    Pressed,
    Released,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InputEvent {
    Key { code: KeyCode, state: ButtonState, repeat: bool },
    Text { utf8: String },

    CursorMove { x: i32, y: i32 },
    Wheel { delta: i32 },

    CursorIn(bool),
    Focused(bool),
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct KeyMask: u32 {
        const NONE = 0;
    }
}

#[derive(Debug, Clone, Copy)]
pub struct InputSnapshot {
    pub input_down: u32,
    pub input_up: u32,
    pub input_state: u32,
    pub input_repeat: u32,
    pub cursor_in: bool,
    pub cursor_x: i32,
    pub cursor_y: i32,
    pub wheel_value: i32,
}
