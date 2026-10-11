//! Controller and touch input, as the runtime reads it each frame, turned into engine events.
//!
//! A = left click and B = right click at a cursor moved by the left stick or the D-pad (or
//! where the screen is touched), X = Enter, Y = Space, Minus = Escape, L/R/ZL/ZR = Ctrl (skip),
//! Plus = quit.

use alloc::vec::Vec;

use rfvp::host_api::{InputModifiers, KeyCode, PointerButton, RfvpEvent};

const SCREEN_WIDTH: i32 = 1280;
const SCREEN_HEIGHT: i32 = 720;
const STICK_DEADZONE: i32 = 8_000;
const STICK_CURSOR_DIVISOR: i32 = 18_000;
const DPAD_CURSOR_STEP: i32 = 12;

/// `struct RfvpWiiuInput` in main.c.
#[repr(C)]
pub struct WiiuInput {
    /// The Switch port's `HidNpadButton` bits: main.c maps the GamePad onto them.
    pub buttons: u64,
    pub stick_x: i32,
    pub stick_y: i32,
    pub touch_count: u32,
    pub touch_id: u32,
    pub touch_x: i32,
    pub touch_y: i32,
}

// libnx `HidNpadButton` (the Switch port's layout, which main.c maps the GamePad onto).
const A: u64 = 1 << 0;
const B: u64 = 1 << 1;
const X: u64 = 1 << 2;
const Y: u64 = 1 << 3;
const L: u64 = 1 << 6;
const R: u64 = 1 << 7;
const ZL: u64 = 1 << 8;
const ZR: u64 = 1 << 9;
const PLUS: u64 = 1 << 10;
const MINUS: u64 = 1 << 11;
const LEFT: u64 = 1 << 12;
const UP: u64 = 1 << 13;
const RIGHT: u64 = 1 << 14;
const DOWN: u64 = 1 << 15;

const KEYS: [(u64, KeyCode); 8] = [
    (UP, KeyCode::Up),
    (DOWN, KeyCode::Down),
    (LEFT, KeyCode::Left),
    (RIGHT, KeyCode::Right),
    (MINUS, KeyCode::Escape),
    (X, KeyCode::Return),
    (Y, KeyCode::Space),
    // Holding a shoulder button skips text, as Ctrl does on a keyboard.
    (L | R | ZL | ZR, KeyCode::Control),
];

pub struct WiiuPad {
    cursor_x: i32,
    cursor_y: i32,
    previous_buttons: u64,
    touch_active: bool,
    touch_id: u64,
    touch_x: i32,
    touch_y: i32,
}

impl WiiuPad {
    pub fn new() -> Self {
        Self {
            cursor_x: SCREEN_WIDTH / 2,
            cursor_y: SCREEN_HEIGHT / 2,
            previous_buttons: 0,
            touch_active: false,
            touch_id: 0,
            touch_x: 0,
            touch_y: 0,
        }
    }

    pub fn poll(&mut self, input: &WiiuInput, out: &mut Vec<RfvpEvent>) {
        let buttons = input.buttons;
        let down = !self.previous_buttons & buttons;
        let up = self.previous_buttons & !buttons;
        let held_before = self.previous_buttons;
        self.previous_buttons = buttons;

        if down & PLUS != 0 {
            out.push(RfvpEvent::Quit);
        }
        for (mask, key) in KEYS {
            // Several buttons can share a key: it goes down with the first and up with the last.
            let was = held_before & mask != 0;
            let is = buttons & mask != 0;
            if is && !was {
                out.push(RfvpEvent::KeyDown {
                    key,
                    repeat: false,
                    modifiers: InputModifiers::empty(),
                });
            } else if was && !is {
                out.push(RfvpEvent::KeyUp {
                    key,
                    modifiers: InputModifiers::empty(),
                });
            }
        }

        self.move_cursor(buttons, input.stick_x, input.stick_y, out);
        for (mask, button) in [(A, PointerButton::Left), (B, PointerButton::Right)] {
            let (x, y) = (self.cursor_x, self.cursor_y);
            if down & mask != 0 {
                out.push(RfvpEvent::PointerDown { button, x, y });
            }
            if up & mask != 0 {
                out.push(RfvpEvent::PointerUp { button, x, y });
            }
        }
        self.update_touch(input, out);
    }

    fn move_cursor(&mut self, buttons: u64, stick_x: i32, stick_y: i32, out: &mut Vec<RfvpEvent>) {
        let mut dx = 0;
        let mut dy = 0;
        if buttons & LEFT != 0 {
            dx -= DPAD_CURSOR_STEP;
        }
        if buttons & RIGHT != 0 {
            dx += DPAD_CURSOR_STEP;
        }
        if buttons & UP != 0 {
            dy -= DPAD_CURSOR_STEP;
        }
        if buttons & DOWN != 0 {
            dy += DPAD_CURSOR_STEP;
        }
        if stick_x.abs() >= STICK_DEADZONE {
            dx += stick_x / STICK_CURSOR_DIVISOR;
        }
        if stick_y.abs() >= STICK_DEADZONE {
            dy -= stick_y / STICK_CURSOR_DIVISOR;
        }
        if dx != 0 || dy != 0 {
            self.cursor_x = (self.cursor_x + dx).clamp(0, SCREEN_WIDTH - 1);
            self.cursor_y = (self.cursor_y + dy).clamp(0, SCREEN_HEIGHT - 1);
            out.push(RfvpEvent::PointerMove {
                x: self.cursor_x,
                y: self.cursor_y,
                in_screen: true,
            });
        }
    }

    fn update_touch(&mut self, input: &WiiuInput, out: &mut Vec<RfvpEvent>) {
        if input.touch_count == 0 {
            if self.touch_active {
                let (id, x, y) = (self.touch_id, self.touch_x, self.touch_y);
                out.push(RfvpEvent::TouchUp { id, x, y });
                out.push(RfvpEvent::PointerUp {
                    button: PointerButton::Left,
                    x,
                    y,
                });
                self.touch_active = false;
            }
            return;
        }

        let id = input.touch_id as u64;
        let x = input.touch_x.clamp(0, SCREEN_WIDTH - 1);
        let y = input.touch_y.clamp(0, SCREEN_HEIGHT - 1);
        self.cursor_x = x;
        self.cursor_y = y;
        out.push(RfvpEvent::PointerMove {
            x,
            y,
            in_screen: true,
        });
        if !self.touch_active || self.touch_id != id {
            self.touch_active = true;
            self.touch_id = id;
            self.touch_x = x;
            self.touch_y = y;
            out.push(RfvpEvent::TouchDown { id, x, y });
            out.push(RfvpEvent::PointerDown {
                button: PointerButton::Left,
                x,
                y,
            });
        } else if self.touch_x != x || self.touch_y != y {
            self.touch_x = x;
            self.touch_y = y;
            out.push(RfvpEvent::TouchMove { id, x, y });
        }
    }
}
