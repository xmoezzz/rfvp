use std::sync::Mutex;

use rfvp_events::{EventQueue, Notify};

use crate::{ButtonState, InputEvent, InputSnapshot, KeyCode};

#[derive(Default)]
struct Inner {
    pressed: u32,
    down: u32,
    up: u32,
    repeat: u32,

    cursor_in: bool,
    cursor_x: i32,
    cursor_y: i32,
    wheel_value: i32,

    last_text: Option<String>,
}

pub struct InputHub {
    inner: Mutex<Inner>,
    events: EventQueue<InputEvent>,
    notify: Notify,
}

impl Default for InputHub {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            events: EventQueue::new(),
            notify: Notify::new(),
        }
    }
}

impl InputHub {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn events(&self) -> &EventQueue<InputEvent> {
        &self.events
    }

    pub fn notifier(&self) -> &Notify {
        &self.notify
    }

    pub fn push(&self, ev: InputEvent) {
        self.apply_event(&ev);
        self.events.push(ev);
        self.notify.notify_waiters();
    }

    pub fn push_many<I: IntoIterator<Item = InputEvent>>(&self, it: I) {
        for ev in it {
            self.push(ev);
        }
    }

    pub fn snapshot(&self) -> InputSnapshot {
        let inner = self.inner.lock().unwrap();
        InputSnapshot {
            input_down: inner.down,
            input_up: inner.up,
            input_state: inner.pressed,
            input_repeat: inner.repeat,
            cursor_in: inner.cursor_in,
            cursor_x: inner.cursor_x,
            cursor_y: inner.cursor_y,
            wheel_value: inner.wheel_value,
        }
    }

    pub fn clear_transients(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.down = 0;
        inner.up = 0;
        inner.repeat = 0;
        inner.wheel_value = 0;
    }

    pub fn take_last_text(&self) -> Option<String> {
        self.inner.lock().unwrap().last_text.take()
    }

    pub fn is_down(&self, code: KeyCode) -> bool {
        (self.inner.lock().unwrap().pressed & code.bit()) != 0
    }

    pub async fn wait_any_input(&self) {
        self.notify.notified().await;
    }

    pub async fn wait_key_down(&self, code: KeyCode) {
        loop {
            if self.is_down(code) {
                return;
            }
            self.wait_any_input().await;
        }
    }

    fn apply_event(&self, ev: &InputEvent) {
        let mut inner = self.inner.lock().unwrap();
        match ev {
            InputEvent::Key { code, state, repeat } => {
                let bit = code.bit();
                match state {
                    ButtonState::Pressed => {
                        if (inner.pressed & bit) == 0 {
                            inner.down |= bit;
                        }
                        inner.pressed |= bit;
                        if *repeat {
                            inner.repeat |= bit;
                        }
                    }
                    ButtonState::Released => {
                        if (inner.pressed & bit) != 0 {
                            inner.up |= bit;
                        }
                        inner.pressed &= !bit;
                    }
                }
                Self::recompute_virtuals(&mut inner);
            }
            InputEvent::CursorMove { x, y } => {
                inner.cursor_x = *x;
                inner.cursor_y = *y;
            }
            InputEvent::Wheel { delta } => {
                inner.wheel_value += *delta;
            }
            InputEvent::CursorIn(v) => {
                inner.cursor_in = *v;
            }
            InputEvent::Focused(v) => {
                if !*v {
                    inner.pressed = 0;
                    inner.down = 0;
                    inner.up = 0;
                    inner.repeat = 0;
                }
                inner.cursor_in = *v;
            }
            InputEvent::Text { utf8 } => {
                inner.last_text = Some(utf8.clone());
            }
        }
    }

    fn recompute_virtuals(inner: &mut Inner) {
        let left = (inner.pressed & KeyCode::MouseLeft.bit()) != 0;
        let enter = (inner.pressed & KeyCode::Enter.bit()) != 0;
        let left_click = left || enter;
        if left_click {
            inner.pressed |= KeyCode::LeftClick.bit();
        } else {
            inner.pressed &= !KeyCode::LeftClick.bit();
        }

        let right = (inner.pressed & KeyCode::MouseRight.bit()) != 0;
        let esc = (inner.pressed & KeyCode::Esc.bit()) != 0;
        let right_click = right || esc;
        if right_click {
            inner.pressed |= KeyCode::RightClick.bit();
        } else {
            inner.pressed &= !KeyCode::RightClick.bit();
        }
    }
}
