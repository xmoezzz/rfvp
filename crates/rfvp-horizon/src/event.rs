use alloc::vec::Vec;

use nx::input;
use nx::service::hid;
use rfvp::host_api::{InputModifiers, KeyCode, PointerButton, RfvpError, RfvpEvent, RfvpResult};

const SCREEN_WIDTH: i32 = 1280;
const SCREEN_HEIGHT: i32 = 720;
const STICK_DEADZONE: i32 = 8_000;
const STICK_CURSOR_DIVISOR: i32 = 18_000;
const DPAD_CURSOR_STEP: i32 = 12;

pub struct HorizonEventQueue {
    events: Vec<RfvpEvent>,
    capacity: usize,
}

impl HorizonEventQueue {
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

pub struct HorizonInput {
    context: input::Context,
    cursor_x: i32,
    cursor_y: i32,
    previous_buttons: hid::NpadButton,
    touch_active: bool,
    touch_id: u64,
    touch_x: i32,
    touch_y: i32,
}

impl HorizonInput {
    pub fn new() -> RfvpResult<Self> {
        let supported_style_tags = hid::NpadStyleTag::FullKey()
            | hid::NpadStyleTag::Handheld()
            | hid::NpadStyleTag::JoyDual()
            | hid::NpadStyleTag::JoyLeft()
            | hid::NpadStyleTag::JoyRight();
        let context =
            input::Context::new(supported_style_tags, 1).map_err(|_| RfvpError::Backend)?;
        Ok(Self {
            context,
            cursor_x: SCREEN_WIDTH / 2,
            cursor_y: SCREEN_HEIGHT / 2,
            previous_buttons: hid::NpadButton::default(),
            touch_active: false,
            touch_id: 0,
            touch_x: 0,
            touch_y: 0,
        })
    }

    pub fn cursor(&self) -> (i32, i32) {
        (self.cursor_x, self.cursor_y)
    }

    pub fn poll(&mut self, queue: &mut HorizonEventQueue) -> RfvpResult<()> {
        let mut buttons = hid::NpadButton::default();
        let mut left_stick = hid::AnalogStickState::default();

        for controller in [hid::NpadIdType::Handheld, hid::NpadIdType::No1] {
            let mut player = self.context.get_player(controller);
            buttons |= player.get_buttons();
            let reported_style = player.get_reported_style_tag();
            let (stick_l, _) = player.get_stick_status(reported_style);
            if stick_l.x.abs() > left_stick.x.abs() || stick_l.y.abs() > left_stick.y.abs() {
                left_stick = stick_l;
            }
        }

        let down = (!self.previous_buttons) & buttons;
        let up = self.previous_buttons & (!buttons);
        self.previous_buttons = buttons;

        self.emit_button_events(queue, down, true)?;
        self.emit_button_events(queue, up, false)?;
        self.update_cursor_from_buttons_and_stick(queue, buttons, left_stick)?;
        self.update_touch(queue)?;

        Ok(())
    }

    fn emit_button_events(
        &self,
        queue: &mut HorizonEventQueue,
        buttons: hid::NpadButton,
        pressed: bool,
    ) -> RfvpResult<()> {
        if buttons.contains(hid::NpadButton::Plus()) {
            queue.push(RfvpEvent::Quit)?;
        }

        self.emit_key(queue, buttons, hid::NpadButton::Up(), KeyCode::Up, pressed)?;
        self.emit_key(
            queue,
            buttons,
            hid::NpadButton::Down(),
            KeyCode::Down,
            pressed,
        )?;
        self.emit_key(
            queue,
            buttons,
            hid::NpadButton::Left(),
            KeyCode::Left,
            pressed,
        )?;
        self.emit_key(
            queue,
            buttons,
            hid::NpadButton::Right(),
            KeyCode::Right,
            pressed,
        )?;
        self.emit_key(
            queue,
            buttons,
            hid::NpadButton::Minus(),
            KeyCode::Escape,
            pressed,
        )?;
        self.emit_key(
            queue,
            buttons,
            hid::NpadButton::X(),
            KeyCode::Return,
            pressed,
        )?;

        if buttons.contains(hid::NpadButton::A()) {
            self.emit_pointer(queue, PointerButton::Left, pressed)?;
        }
        if buttons.contains(hid::NpadButton::B()) {
            self.emit_pointer(queue, PointerButton::Right, pressed)?;
        }

        Ok(())
    }

    fn emit_key(
        &self,
        queue: &mut HorizonEventQueue,
        buttons: hid::NpadButton,
        button: hid::NpadButton,
        key: KeyCode,
        pressed: bool,
    ) -> RfvpResult<()> {
        if buttons.contains(button) {
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
        queue: &mut HorizonEventQueue,
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
        queue: &mut HorizonEventQueue,
        buttons: hid::NpadButton,
        stick: hid::AnalogStickState,
    ) -> RfvpResult<()> {
        let mut dx = 0;
        let mut dy = 0;

        if buttons.contains(hid::NpadButton::Left()) {
            dx -= DPAD_CURSOR_STEP;
        }
        if buttons.contains(hid::NpadButton::Right()) {
            dx += DPAD_CURSOR_STEP;
        }
        if buttons.contains(hid::NpadButton::Up()) {
            dy -= DPAD_CURSOR_STEP;
        }
        if buttons.contains(hid::NpadButton::Down()) {
            dy += DPAD_CURSOR_STEP;
        }

        if stick.x.abs() >= STICK_DEADZONE {
            dx += stick.x / STICK_CURSOR_DIVISOR;
        }
        if stick.y.abs() >= STICK_DEADZONE {
            dy -= stick.y / STICK_CURSOR_DIVISOR;
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

    fn update_touch(&mut self, queue: &mut HorizonEventQueue) -> RfvpResult<()> {
        let mut touches = [hid::TouchState::default(); 16];
        let count = self.context.get_touches(&mut touches);

        if count == 0 {
            if self.touch_active {
                queue.push(RfvpEvent::TouchUp {
                    id: self.touch_id,
                    x: self.touch_x,
                    y: self.touch_y,
                })?;
                queue.push(RfvpEvent::PointerUp {
                    button: PointerButton::Left,
                    x: self.touch_x,
                    y: self.touch_y,
                })?;
                self.touch_active = false;
            }
            return Ok(());
        }

        let touch = touches[0];
        let id = touch.finger_id as u64;
        let x = (touch.x as i32).clamp(0, SCREEN_WIDTH - 1);
        let y = (touch.y as i32).clamp(0, SCREEN_HEIGHT - 1);

        self.cursor_x = x;
        self.cursor_y = y;
        queue.push(RfvpEvent::PointerMove {
            x,
            y,
            in_screen: true,
        })?;

        if !self.touch_active || self.touch_id != id {
            self.touch_active = true;
            self.touch_id = id;
            self.touch_x = x;
            self.touch_y = y;
            queue.push(RfvpEvent::TouchDown { id, x, y })?;
            queue.push(RfvpEvent::PointerDown {
                button: PointerButton::Left,
                x,
                y,
            })?;
        } else if self.touch_x != x || self.touch_y != y {
            self.touch_x = x;
            self.touch_y = y;
            queue.push(RfvpEvent::TouchMove { id, x, y })?;
        }

        Ok(())
    }
}
