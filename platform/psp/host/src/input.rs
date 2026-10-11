//! The PSP's buttons and analog stick, read each frame, turned into engine events.
//!
//! Buttons work as on a keyboard: Cross = Enter (advance, confirm), Circle = Escape (cancel,
//! menu), D-pad = arrow keys, Triangle = Space, Select = Escape, L/R = Ctrl (hold to skip
//! text), Start = quit.
//!
//! Moving the analog stick shows a cursor (the engine draws none on consoles) and makes Cross
//! and Circle left and right clicks at it, for screens that only take the mouse. Pressing the
//! D-pad hides it again, so the cursor never fights the arrow keys over which item is chosen.

use alloc::vec::Vec;

use rfvp::host_api::{InputModifiers, KeyCode, PointerButton, RfvpEvent};

pub const SCREEN_WIDTH: i32 = 480;
pub const SCREEN_HEIGHT: i32 = 272;

// `SceCtrlData::buttons` bits.
pub const SELECT: u32 = 0x0001;
pub const START: u32 = 0x0008;
pub const UP: u32 = 0x0010;
pub const RIGHT: u32 = 0x0020;
pub const DOWN: u32 = 0x0040;
pub const LEFT: u32 = 0x0080;
pub const LTRIGGER: u32 = 0x0100;
pub const RTRIGGER: u32 = 0x0200;
pub const TRIANGLE: u32 = 0x1000;
pub const CIRCLE: u32 = 0x2000;
pub const CROSS: u32 = 0x4000;
pub const SQUARE: u32 = 0x8000;

const DPAD: u32 = UP | RIGHT | DOWN | LEFT;

const KEYS: [(u32, KeyCode); 7] = [
    (UP, KeyCode::Up),
    (DOWN, KeyCode::Down),
    (LEFT, KeyCode::Left),
    (RIGHT, KeyCode::Right),
    (SELECT, KeyCode::Escape),
    (TRIANGLE, KeyCode::Space),
    // Holding a shoulder button skips text, as Ctrl does on a keyboard.
    (LTRIGGER | RTRIGGER, KeyCode::Control),
];

/// Cross and Circle: a key without the cursor, a click at it with it.
const ACTION_BUTTONS: [(u32, KeyCode, PointerButton); 2] = [
    (CROSS, KeyCode::Return, PointerButton::Left),
    (CIRCLE, KeyCode::Escape, PointerButton::Right),
];

/// The stick reads 0..=255 with 128 at rest; a worn stick drifts a little around it.
const STICK_CENTER: i32 = 128;
const STICK_DEADZONE: i32 = 40;
/// Cursor speed: at full tilt about 7 pixels a frame (a screen's width in about a second).
const STICK_SPEED_DIVISOR: i32 = 12;
/// Cursor positions are kept in 1/16 pixels, so a slight tilt still moves it smoothly.
const SUBPIXEL: i32 = 16;

/// What a held Cross or Circle was sent as, so its release matches.
#[derive(Clone, Copy, PartialEq)]
enum Held {
    None,
    Key,
    Click,
}

pub struct PspInput {
    /// In 1/16 screen pixels.
    cursor_x: i32,
    cursor_y: i32,
    /// The stick moved the cursor since the D-pad was last pressed: it is shown, and Cross and
    /// Circle click at it.
    cursor_shown: bool,
    previous_buttons: u32,
    held: [Held; ACTION_BUTTONS.len()],
}

impl Default for PspInput {
    fn default() -> Self {
        Self::new()
    }
}

impl PspInput {
    pub fn new() -> Self {
        Self::new_const()
    }

    /// [`Self::new`], usable in a `static`.
    pub const fn new_const() -> Self {
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
    pub fn poll(&mut self, buttons: u32, stick_x: u8, stick_y: u8, out: &mut Vec<RfvpEvent>) {
        let held_before = self.previous_buttons;
        let down = !held_before & buttons;
        self.previous_buttons = buttons;

        if down & START != 0 {
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

        self.move_cursor(stick_x, stick_y, out);
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
            out.push(RfvpEvent::PointerMove {
                x,
                y,
                in_screen: true,
            });
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

    fn poll(input: &mut PspInput, buttons: u32, x: u8, y: u8) -> Vec<RfvpEvent> {
        let mut out = Vec::new();
        input.poll(buttons, x, y, &mut out);
        out
    }

    fn has_key_down(events: &[RfvpEvent], want: KeyCode) -> bool {
        events
            .iter()
            .any(|e| matches!(e, RfvpEvent::KeyDown { key, .. } if *key == want))
    }

    fn has_key_up(events: &[RfvpEvent], want: KeyCode) -> bool {
        events
            .iter()
            .any(|e| matches!(e, RfvpEvent::KeyUp { key, .. } if *key == want))
    }

    #[test]
    fn cross_is_enter_and_circle_escape_without_the_cursor() {
        let mut input = PspInput::new();
        let events = poll(&mut input, CROSS, 128, 128);
        assert!(has_key_down(&events, KeyCode::Return));
        assert!(!input.cursor_shown());
        assert!(has_key_up(&poll(&mut input, 0, 128, 128), KeyCode::Return));
        assert!(has_key_down(&poll(&mut input, CIRCLE, 128, 128), KeyCode::Escape));
    }

    #[test]
    fn the_stick_shows_the_cursor_and_cross_clicks_at_it() {
        let mut input = PspInput::new();
        let events = poll(&mut input, 0, 255, 128);
        assert!(matches!(events[..], [RfvpEvent::PointerMove { x, y: 136, .. }] if x > 240));
        assert!(input.cursor_shown());
        let (x, y) = input.cursor();
        let events = poll(&mut input, CROSS, 128, 128);
        assert_eq!(
            events,
            [RfvpEvent::PointerDown {
                button: PointerButton::Left,
                x,
                y
            }]
        );
        let events = poll(&mut input, 0, 128, 128);
        assert_eq!(
            events,
            [RfvpEvent::PointerUp {
                button: PointerButton::Left,
                x,
                y
            }]
        );
    }

    #[test]
    fn the_dpad_hides_the_cursor_and_sends_arrows_only() {
        let mut input = PspInput::new();
        poll(&mut input, 0, 255, 128);
        let at = input.cursor();
        let events = poll(&mut input, RIGHT, 128, 128);
        assert!(has_key_down(&events, KeyCode::Right));
        assert!(!events
            .iter()
            .any(|e| matches!(e, RfvpEvent::PointerMove { .. })));
        assert_eq!(input.cursor(), at);
        assert!(!input.cursor_shown());
        assert!(has_key_down(&poll(&mut input, RIGHT | CROSS, 128, 128), KeyCode::Return));
    }

    #[test]
    fn a_button_held_across_a_mode_change_releases_as_it_was_pressed() {
        let mut input = PspInput::new();
        assert!(has_key_down(&poll(&mut input, CROSS, 128, 128), KeyCode::Return));
        poll(&mut input, CROSS, 255, 128);
        assert!(has_key_up(&poll(&mut input, 0, 128, 128), KeyCode::Return));
    }

    #[test]
    fn a_resting_or_drifting_stick_leaves_the_cursor() {
        let mut input = PspInput::new();
        assert!(poll(&mut input, 0, 128 + 30, 128 - 30).is_empty());
        assert_eq!(input.cursor(), (240, 136));
        assert!(!input.cursor_shown());
    }

    #[test]
    fn the_cursor_stops_at_the_edge() {
        let mut input = PspInput::new();
        for _ in 0..200 {
            poll(&mut input, 0, 255, 0);
        }
        assert_eq!(input.cursor(), (479, 0));
    }

    #[test]
    fn both_shoulders_hold_one_ctrl() {
        let mut input = PspInput::new();
        let count = |events: &[RfvpEvent], down: bool| {
            events
                .iter()
                .filter(|e| match e {
                    RfvpEvent::KeyDown { key: KeyCode::Control, .. } => down,
                    RfvpEvent::KeyUp { key: KeyCode::Control, .. } => !down,
                    _ => false,
                })
                .count()
        };
        assert_eq!(count(&poll(&mut input, LTRIGGER, 128, 128), true), 1);
        assert_eq!(count(&poll(&mut input, LTRIGGER | RTRIGGER, 128, 128), true), 0);
        assert_eq!(count(&poll(&mut input, RTRIGGER, 128, 128), false), 0);
        assert_eq!(count(&poll(&mut input, 0, 128, 128), false), 1);
    }

    #[test]
    fn start_quits_and_triangle_is_space() {
        let mut input = PspInput::new();
        assert_eq!(poll(&mut input, START, 128, 128), [RfvpEvent::Quit]);
        assert!(has_key_down(&poll(&mut input, TRIANGLE, 128, 128), KeyCode::Space));
    }
}
