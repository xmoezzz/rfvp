use alloc::vec::Vec;

use rfvp::host_api::{InputModifiers, KeyCode, PointerButton, RfvpError, RfvpEvent, RfvpResult};

const SCREEN_WIDTH: i32 = 1280;
const SCREEN_HEIGHT: i32 = 720;
const STICK_DEADZONE: i32 = 8_000;
const STICK_CURSOR_DIVISOR: i32 = 18_000;
const DPAD_CURSOR_STEP: i32 = 12;

const BUTTON_A: u32 = 1 << 0;
const BUTTON_B: u32 = 1 << 1;
const BUTTON_X: u32 = 1 << 2;
const BUTTON_Y: u32 = 1 << 3;
const BUTTON_LEFT: u32 = 1 << 4;
const BUTTON_RIGHT: u32 = 1 << 5;
const BUTTON_UP: u32 = 1 << 6;
const BUTTON_DOWN: u32 = 1 << 7;
const BUTTON_PLUS: u32 = 1 << 8;
const BUTTON_MINUS: u32 = 1 << 9;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct RawPS3InputState {
    buttons: u32,
    left_stick_x: i32,
    left_stick_y: i32,
}

unsafe extern "C" {
    fn rfvp_ps3_platform_poll_input(out_state: *mut RawPS3InputState) -> i32;
}

pub struct PS3EventQueue {
    events: Vec<RfvpEvent>,
    capacity: usize,
}

impl PS3EventQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            events: Vec::new(),
            capacity,
        }
    }

    pub fn push(&mut self, event: RfvpEvent) -> RfvpResult<()> {
        if self.events.len() >= self.capacity {
            return Err(RfvpError::CapacityExceeded);
        }
        self.events.push(event);
        Ok(())
    }

    pub fn clear(&mut self) {
        self.events.clear();
    }

    pub fn as_slice(&self) -> &[RfvpEvent] {
        &self.events
    }
}

pub struct PS3Input {
    cursor_x: i32,
    cursor_y: i32,
    previous_buttons: u32,
}

impl PS3Input {
    pub fn new() -> RfvpResult<Self> {
        Ok(Self {
            cursor_x: SCREEN_WIDTH / 2,
            cursor_y: SCREEN_HEIGHT / 2,
            previous_buttons: 0,
        })
    }

    pub fn cursor(&self) -> (i32, i32) {
        (self.cursor_x, self.cursor_y)
    }

    pub fn poll(&mut self, queue: &mut PS3EventQueue) -> RfvpResult<()> {
        let mut state = RawPS3InputState::default();
        let status = unsafe { rfvp_ps3_platform_poll_input(&mut state) };
        if status != 0 {
            return Err(crate::status::ps3_status_to_rfvp_error(status));
        }

        let down = (!self.previous_buttons) & state.buttons;
        let up = self.previous_buttons & (!state.buttons);
        self.previous_buttons = state.buttons;

        self.emit_button_events(queue, down, true)?;
        self.emit_button_events(queue, up, false)?;
        self.update_cursor_from_buttons_and_stick(
            queue,
            state.buttons,
            state.left_stick_x,
            state.left_stick_y,
        )
    }

    fn emit_button_events(
        &self,
        queue: &mut PS3EventQueue,
        buttons: u32,
        pressed: bool,
    ) -> RfvpResult<()> {
        if buttons & BUTTON_PLUS != 0 {
            queue.push(RfvpEvent::Quit)?;
        }

        self.emit_key(queue, buttons, BUTTON_UP, KeyCode::Up, pressed)?;
        self.emit_key(queue, buttons, BUTTON_DOWN, KeyCode::Down, pressed)?;
        self.emit_key(queue, buttons, BUTTON_LEFT, KeyCode::Left, pressed)?;
        self.emit_key(queue, buttons, BUTTON_RIGHT, KeyCode::Right, pressed)?;
        self.emit_key(queue, buttons, BUTTON_MINUS, KeyCode::Escape, pressed)?;
        self.emit_key(queue, buttons, BUTTON_X, KeyCode::Return, pressed)?;
        self.emit_key(queue, buttons, BUTTON_Y, KeyCode::Space, pressed)?;

        if buttons & BUTTON_A != 0 {
            self.emit_pointer(queue, PointerButton::Left, pressed)?;
        }
        if buttons & BUTTON_B != 0 {
            self.emit_pointer(queue, PointerButton::Right, pressed)?;
        }

        Ok(())
    }

    fn emit_key(
        &self,
        queue: &mut PS3EventQueue,
        buttons: u32,
        button: u32,
        key: KeyCode,
        pressed: bool,
    ) -> RfvpResult<()> {
        if buttons & button != 0 {
            if pressed {
                queue.push(RfvpEvent::KeyDown {
                    key,
                    repeat: false,
                    modifiers: InputModifiers::empty(),
                })?;
            } else {
                queue.push(RfvpEvent::KeyUp {
                    key,
                    modifiers: InputModifiers::empty(),
                })?;
            }
        }
        Ok(())
    }

    fn emit_pointer(
        &self,
        queue: &mut PS3EventQueue,
        button: PointerButton,
        pressed: bool,
    ) -> RfvpResult<()> {
        if pressed {
            queue.push(RfvpEvent::PointerDown {
                button,
                x: self.cursor_x,
                y: self.cursor_y,
            })?;
        } else {
            queue.push(RfvpEvent::PointerUp {
                button,
                x: self.cursor_x,
                y: self.cursor_y,
            })?;
        }
        Ok(())
    }

    fn update_cursor_from_buttons_and_stick(
        &mut self,
        queue: &mut PS3EventQueue,
        buttons: u32,
        stick_x: i32,
        stick_y: i32,
    ) -> RfvpResult<()> {
        let mut dx = 0;
        let mut dy = 0;

        if buttons & BUTTON_LEFT != 0 {
            dx -= DPAD_CURSOR_STEP;
        }
        if buttons & BUTTON_RIGHT != 0 {
            dx += DPAD_CURSOR_STEP;
        }
        if buttons & BUTTON_UP != 0 {
            dy -= DPAD_CURSOR_STEP;
        }
        if buttons & BUTTON_DOWN != 0 {
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
            queue.push(RfvpEvent::PointerMove {
                x: self.cursor_x,
                y: self.cursor_y,
                in_screen: true,
            })?;
        }

        Ok(())
    }
}
