//! The Wii Remote and the GameCube controller, read each frame by the runtime (WPAD and PAD in
//! main.c, which merges them into one set of bits), turned into engine events.
//!
//! Pointing the Wii Remote at the screen moves the cursor (the engine draws none on consoles),
//! and A and B are then left and right clicks at it; the GameCube stick moves the cursor the same
//! way. With the cursor hidden (pointing away, or after the D-pad), A is Enter and B Escape, for
//! screens that only take the keyboard. D-pad = arrow keys, 1 = Space, 2 = Enter, + and − =
//! Escape (the menu), holding the skip button (GameCube L, R or Z) = Ctrl, HOME = quit.

use alloc::vec::Vec;

use rfvp::host_api::{InputModifiers, KeyCode, PointerButton, RfvpEvent};

/// The space the cursor moves in: the 640x480 picture.
pub const SCREEN_WIDTH: i32 = 640;
pub const SCREEN_HEIGHT: i32 = 480;

/// `struct RfvpWiiInput` in main.c: one reading of the controllers.
#[repr(C)]
pub struct WiiInput {
    /// The bits below, set while held.
    pub buttons: u32,
    /// GameCube stick, 0..=255 with 128 at rest (up is low).
    pub stick_x: u8,
    pub stick_y: u8,
    /// Whether the Wii Remote points at the screen, and where (screen pixels).
    pub pointer_valid: u8,
    pub _pad: u8,
    pub pointer_x: i16,
    pub pointer_y: i16,
}

// main.c's merged buttons.
pub const A: u32 = 1 << 0;
pub const B: u32 = 1 << 1;
pub const ONE: u32 = 1 << 2;
pub const TWO: u32 = 1 << 3;
pub const PLUS: u32 = 1 << 4;
pub const MINUS: u32 = 1 << 5;
pub const HOME: u32 = 1 << 6;
pub const UP: u32 = 1 << 7;
pub const DOWN: u32 = 1 << 8;
pub const LEFT: u32 = 1 << 9;
pub const RIGHT: u32 = 1 << 10;
pub const SKIP: u32 = 1 << 11;

const DPAD: u32 = UP | RIGHT | DOWN | LEFT;

const KEYS: [(u32, KeyCode); 8] = [
    (UP, KeyCode::Up),
    (DOWN, KeyCode::Down),
    (LEFT, KeyCode::Left),
    (RIGHT, KeyCode::Right),
    (PLUS | MINUS, KeyCode::Escape),
    (ONE, KeyCode::Space),
    (TWO, KeyCode::Return),
    // Holding the skip button skips text, as Ctrl does on a keyboard.
    (SKIP, KeyCode::Control),
];

/// A and B: a key without the cursor, a click at it with it.
const ACTION_BUTTONS: [(u32, KeyCode, PointerButton); 2] = [
    (A, KeyCode::Return, PointerButton::Left),
    (B, KeyCode::Escape, PointerButton::Right),
];

/// The stick reads 0..=255 with 128 at rest; a worn stick drifts a little around it.
const STICK_CENTER: i32 = 128;
const STICK_DEADZONE: i32 = 40;
/// Cursor speed: at full tilt about 10 pixels a frame.
const STICK_SPEED_DIVISOR: i32 = 9;
/// Cursor positions are kept in 1/16 pixels, so a slight tilt still moves it smoothly.
const SUBPIXEL: i32 = 16;

/// What a held A or B was sent as, so its release matches.
#[derive(Clone, Copy, PartialEq)]
enum Held {
    None,
    Key,
    Click,
}

pub struct PadInput {
    /// In 1/16 screen pixels.
    cursor_x: i32,
    cursor_y: i32,
    /// The pointer or the stick moved the cursor since the D-pad was last pressed: it is shown,
    /// and A and B click at it.
    cursor_shown: bool,
    previous_buttons: u32,
    held: [Held; ACTION_BUTTONS.len()],
}

impl Default for PadInput {
    fn default() -> Self {
        Self::new()
    }
}

impl PadInput {
    pub fn new() -> Self {
        Self {
            cursor_x: SCREEN_WIDTH / 2 * SUBPIXEL,
            cursor_y: SCREEN_HEIGHT / 2 * SUBPIXEL,
            cursor_shown: false,
            previous_buttons: 0,
            held: [Held::None; ACTION_BUTTONS.len()],
        }
    }

    /// The cursor, in screen pixels.
    pub fn cursor(&self) -> (i32, i32) {
        (self.cursor_x / SUBPIXEL, self.cursor_y / SUBPIXEL)
    }

    /// Whether the cursor should be drawn.
    pub fn cursor_shown(&self) -> bool {
        self.cursor_shown
    }

    /// Turn one controller reading into events. Pointer positions are in screen pixels.
    pub fn poll(&mut self, input: &WiiInput, out: &mut Vec<RfvpEvent>) {
        let buttons = input.buttons;
        let held_before = self.previous_buttons;
        let down = !held_before & buttons;
        self.previous_buttons = buttons;

        if down & HOME != 0 {
            out.push(RfvpEvent::Quit);
        }
        if down & DPAD != 0 {
            self.cursor_shown = false;
        }
        for (mask, key) in KEYS {
            // Several buttons can share a key: it goes down with the first and up with the last.
            let was = held_before & mask != 0;
            let is = buttons & mask != 0;
            if is && !was {
                out.push(key_down(key));
            } else if was && !is {
                out.push(key_up(key));
            }
        }

        if input.pointer_valid != 0 {
            self.point_at(input.pointer_x as i32, input.pointer_y as i32, out);
        } else {
            self.move_cursor(input.stick_x, input.stick_y, out);
        }
        let (x, y) = self.cursor();
        for (index, (mask, key, button)) in ACTION_BUTTONS.into_iter().enumerate() {
            let is = buttons & mask != 0;
            match (self.held[index], is) {
                (Held::None, true) if self.cursor_shown => {
                    out.push(RfvpEvent::PointerDown { button, x, y });
                    self.held[index] = Held::Click;
                }
                (Held::None, true) => {
                    out.push(key_down(key));
                    self.held[index] = Held::Key;
                }
                (Held::Click, false) => {
                    out.push(RfvpEvent::PointerUp { button, x, y });
                    self.held[index] = Held::None;
                }
                (Held::Key, false) => {
                    out.push(key_up(key));
                    self.held[index] = Held::None;
                }
                _ => {}
            }
        }
    }

    /// The Wii Remote points at (x, y).
    fn point_at(&mut self, x: i32, y: i32, out: &mut Vec<RfvpEvent>) {
        let x = x.clamp(0, SCREEN_WIDTH - 1);
        let y = y.clamp(0, SCREEN_HEIGHT - 1);
        let moved = (x, y) != self.cursor();
        let was_shown = self.cursor_shown;
        // Holding still keeps the D-pad's choice: only moving the pointer takes over again.
        if !moved && !was_shown {
            return;
        }
        self.cursor_shown = true;
        self.cursor_x = x * SUBPIXEL;
        self.cursor_y = y * SUBPIXEL;
        if moved || !was_shown {
            out.push(RfvpEvent::PointerMove { x, y, in_screen: true });
        }
    }

    fn move_cursor(&mut self, stick_x: u8, stick_y: u8, out: &mut Vec<RfvpEvent>) {
        let dx = stick_step(stick_x);
        let dy = stick_step(stick_y);
        if dx == 0 && dy == 0 {
            return;
        }
        let was_shown = self.cursor_shown;
        self.cursor_shown = true;
        let before = self.cursor();
        self.cursor_x = (self.cursor_x + dx).clamp(0, (SCREEN_WIDTH - 1) * SUBPIXEL);
        self.cursor_y = (self.cursor_y + dy).clamp(0, (SCREEN_HEIGHT - 1) * SUBPIXEL);
        let (x, y) = self.cursor();
        if (x, y) != before || !was_shown {
            out.push(RfvpEvent::PointerMove { x, y, in_screen: true });
        }
    }
}

fn key_down(key: KeyCode) -> RfvpEvent {
    RfvpEvent::KeyDown {
        key,
        repeat: false,
        modifiers: InputModifiers::empty(),
    }
}

fn key_up(key: KeyCode) -> RfvpEvent {
    RfvpEvent::KeyUp {
        key,
        modifiers: InputModifiers::empty(),
    }
}

/// How far one axis of the stick moves the cursor this frame, in 1/16 pixels.
fn stick_step(value: u8) -> i32 {
    let offset = value as i32 - STICK_CENTER;
    if offset.abs() < STICK_DEADZONE {
        return 0;
    }
    // Start from zero at the edge of the dead zone, so the cursor does not jump.
    let past = offset - offset.signum() * STICK_DEADZONE;
    past * SUBPIXEL / STICK_SPEED_DIVISOR
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(buttons: u32, pointer: Option<(i16, i16)>) -> WiiInput {
        WiiInput {
            buttons,
            stick_x: 128,
            stick_y: 128,
            pointer_valid: pointer.is_some() as u8,
            _pad: 0,
            pointer_x: pointer.map_or(0, |p| p.0),
            pointer_y: pointer.map_or(0, |p| p.1),
        }
    }

    fn poll(pad: &mut PadInput, buttons: u32, pointer: Option<(i16, i16)>) -> Vec<RfvpEvent> {
        let mut out = Vec::new();
        pad.poll(&input(buttons, pointer), &mut out);
        out
    }

    #[test]
    fn a_is_enter_without_the_pointer() {
        let mut pad = PadInput::new();
        let events = poll(&mut pad, A, None);
        assert!(matches!(events[..], [RfvpEvent::KeyDown { key: KeyCode::Return, .. }]));
    }

    #[test]
    fn pointing_moves_the_cursor_and_a_clicks_there() {
        let mut pad = PadInput::new();
        let events = poll(&mut pad, 0, Some((100, 200)));
        assert_eq!(events, [RfvpEvent::PointerMove { x: 100, y: 200, in_screen: true }]);
        let events = poll(&mut pad, A, Some((100, 200)));
        assert_eq!(events, [RfvpEvent::PointerDown { button: PointerButton::Left, x: 100, y: 200 }]);
    }

    #[test]
    fn the_dpad_takes_over_until_the_pointer_moves() {
        let mut pad = PadInput::new();
        poll(&mut pad, 0, Some((100, 200)));
        poll(&mut pad, DOWN, Some((100, 200)));
        assert!(!pad.cursor_shown());
        assert!(matches!(
            poll(&mut pad, A, Some((100, 200)))[..],
            [RfvpEvent::KeyDown { key: KeyCode::Return, .. }]
        ));
        poll(&mut pad, 0, Some((101, 200)));
        assert!(pad.cursor_shown());
    }

    #[test]
    fn home_quits() {
        let mut pad = PadInput::new();
        assert_eq!(poll(&mut pad, HOME, None), [RfvpEvent::Quit]);
    }
}
