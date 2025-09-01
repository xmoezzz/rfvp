use std::{sync::Mutex, vec};

use winit::keyboard::NamedKey;

use std::sync::atomic::{AtomicBool, Ordering};
use std::cell::UnsafeCell;
use std::thread;
use std::time::Duration;

#[derive(Debug)]
pub struct CriticalSection {
    locked: AtomicBool,
}

impl CriticalSection {
    pub const fn new() -> Self {
        Self {
            locked: AtomicBool::new(false),
        }
    }

    pub fn enter(&self) -> CriticalGuard<'_> {
        while self
            .locked
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            std::hint::spin_loop();
        }
        CriticalGuard { cs: self }
    }
}

pub struct CriticalGuard<'a> {
    cs: &'a CriticalSection,
}

impl<'a> Drop for CriticalGuard<'a> {
    fn drop(&mut self) {
        self.cs.locked.store(false, Ordering::Release);
    }
}


#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct PressItem {
    keycode: u8,
    in_screen: bool,
    x: i32,
    y: i32,
}

impl PressItem {
    pub fn get_keycode(&self) -> u8 {
        self.keycode
    }

    pub fn get_in_screen(&self) -> bool {
        self.in_screen
    }

    pub fn get_x(&self) -> i32 {
        self.x
    }

    pub fn get_y(&self) -> i32 {
        self.y
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KeyCode {
    Shift = 0,
    Ctrl = 1,
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


#[derive(Debug)]
pub struct InputManager {

    pub mouse_x: i32,
    pub mouse_y: i32,

    press_items: Vec<PressItem>,
    current_index: u8,
    next_index: u8,
    // char gap2[2];
    
    new_input_state: u32,
    old_input_state: u32,
    current_event: PressItem,
    click: u32,
    down_keycode: u32,
    input_up: u32,
    input_state: u32,
    input_repeat: u32,
    cursor_in: bool,
    cursor_x: i32,
    cursor_y: i32,
    wheel_value: i32,
    control_is_masked: bool,
    control_is_pulse: bool,

    cs: CriticalSection,
}

impl Default for InputManager {
    fn default() -> Self {
        let mut s = Self::new();
        s.control_is_masked = true;
        s
    }
}

impl InputManager {
    pub fn new() -> Self {
        Self {
            mouse_x: 0,
            mouse_y: 0,
            press_items: vec![PressItem::default(); 64],
            down_keycode: 0,
            input_up: 0,
            input_state: 0,
            input_repeat: 0,
            cursor_in: false,
            cursor_x: 0,
            cursor_y: 0,
            wheel_value: 0,
            control_is_masked: false,
            control_is_pulse: false,
            current_index: 0,
            next_index: 0,
            new_input_state: 0,
            old_input_state: 0,
            current_event: PressItem::default(),
            click: 0,
            cs: CriticalSection::new(),
        }
    }

    pub fn set_flash(&mut self) {
        self.cs.enter();
        self.current_index = 0;
        self.next_index = 0;
        // this->gap2[0] = 0;
        // this->gap2[1] = 0;
        self.new_input_state = 0;
        self.input_repeat = 0;
        self.input_state = 0;
        self.down_keycode = 0;
        self.input_up = 0;
    }

    pub fn get_cursor_in(&self) -> bool {
        self.cursor_in
    }

    pub fn get_cursor_x(&self) -> i32 {
        self.cursor_x
    }

    pub fn get_cursor_y(&self) -> i32 {
        self.cursor_y
    }

    pub fn get_down_keycode(&self) -> u32 {
        self.down_keycode
    }

    pub fn get_event(&mut self) -> Option<PressItem> {
        self.cs.enter();
        if self.current_index != self.next_index {
            let event = self.press_items[self.current_index as usize].clone();
            self.next_index = (self.current_index + 1) & 0x3F;
            Some(event)
        } else {
            None
        }
    }

    pub fn all_events(&self) -> Vec<PressItem> {
        let mut events = vec![];
        for ev in &self.press_items {
            if *ev == PressItem::default() {
                continue;
            }
            events.push(ev.clone());
        }
        events
    }

    pub fn get_repeat(&self) -> u32 {
        self.input_repeat
    }

    pub fn get_input_state(&self) -> u32 {
        self.input_state
    }

    pub fn get_input_up(&self) -> u32 {
        self.input_up
    }

    pub fn get_wheel_value(&self) -> i32 {
        self.wheel_value
    }

    pub fn set_click(&mut self, clicked: u32) {
        self.cs.enter();
        self.click = clicked;
    }

    pub fn set_mouse_in(&mut self, in_screen: bool) {
        self.cs.enter();
        self.cursor_in = in_screen;
    }

    pub fn keymap(&self, key: winit::keyboard::Key) -> Option<KeyCode> {
        match key {
            winit::keyboard::Key::Named(NamedKey::Shift) => Some(KeyCode::Shift),
            winit::keyboard::Key::Named(NamedKey::Control) => Some(KeyCode::Ctrl),

            winit::keyboard::Key::Named(NamedKey::Escape) => Some(KeyCode::Esc),
            winit::keyboard::Key::Named(NamedKey::Enter) => Some(KeyCode::Enter),
            winit::keyboard::Key::Named(NamedKey::Space) => Some(KeyCode::Space),
            winit::keyboard::Key::Named(NamedKey::ArrowUp) => Some(KeyCode::UpArrow),
            winit::keyboard::Key::Named(NamedKey::ArrowDown) => Some(KeyCode::DownArrow),
            winit::keyboard::Key::Named(NamedKey::ArrowLeft) => Some(KeyCode::LeftArrow),
            winit::keyboard::Key::Named(NamedKey::ArrowRight) => Some(KeyCode::RightArrow),

            winit::keyboard::Key::Named(NamedKey::F1) => Some(KeyCode::F1),
            winit::keyboard::Key::Named(NamedKey::F2) => Some(KeyCode::F2),
            winit::keyboard::Key::Named(NamedKey::F3) => Some(KeyCode::F3),
            winit::keyboard::Key::Named(NamedKey::F4) => Some(KeyCode::F4),
            winit::keyboard::Key::Named(NamedKey::F5) => Some(KeyCode::F5),
            winit::keyboard::Key::Named(NamedKey::F6) => Some(KeyCode::F6),
            winit::keyboard::Key::Named(NamedKey::F7) => Some(KeyCode::F7),
            winit::keyboard::Key::Named(NamedKey::F8) => Some(KeyCode::F8),
            winit::keyboard::Key::Named(NamedKey::F9) => Some(KeyCode::F9),
            winit::keyboard::Key::Named(NamedKey::F10) => Some(KeyCode::F10),
            winit::keyboard::Key::Named(NamedKey::F11) => Some(KeyCode::F11),
            winit::keyboard::Key::Named(NamedKey::F12) => Some(KeyCode::F12),
            winit::keyboard::Key::Named(NamedKey::Tab) => Some(KeyCode::Tab),
            _ => None,
        }
    }

    pub fn record_keydown_or_up(&mut self, keycode: KeyCode, x: i32, y: i32) {
        self.cs.enter();
        let next_index = (self.current_index + 1) & 0x3F;
        if next_index != self.next_index {
            let event = &mut self.press_items[self.current_index as usize];
            if [KeyCode::MouseLeft, KeyCode::MouseRight].contains(&keycode) {
                if self.click == 0 {
                    event.keycode = keycode as u8;
                    event.in_screen = self.cursor_in;
                    event.x = x;
                    event.y = y;
                    self.current_index = next_index;
                }
            } else {
                event.keycode = keycode as u8;
                event.in_screen = false;
                event.x = 0;
                event.y = 0;
                self.current_index = next_index;
            }
            self.current_index = next_index;
        }
    }

    // see https://wiki.winehq.org/List_Of_Windows_Messages
    pub fn notify_keydown(&mut self, key: winit::keyboard::Key, repeat: bool) {
        if let Some(keycode) = self.keymap(key) {
            if repeat {
                self.new_input_state |= 1 << (keycode.clone() as u32);
            }
            self.input_state |= 1 << (keycode.clone() as u32);
            self.down_keycode = keycode.clone() as u32;
            if repeat {
                // winit does not provide repeat count which stored in lParam
                self.record_keydown_or_up(keycode, 0, 0);
            }
        }
    }

    pub fn notify_keyup(&mut self, key: winit::keyboard::Key) {
        if let Some(keycode) = self.keymap(key) {
            self.input_state &= !(1 << (keycode.clone() as u32));
            self.record_keydown_or_up(keycode, 0, 0);
        }
    }

    pub fn notify_mouse_down(&mut self, keycode: KeyCode) {
        self.new_input_state |= 1 << (keycode.clone() as u32);
        if self.click == 1 {
            self.record_keydown_or_up(keycode, self.cursor_x, self.cursor_y);
        }
    }

    pub fn notify_mouse_up(&mut self, keycode: KeyCode) {
        self.new_input_state &= !(1 << (keycode.clone() as u32));
        self.record_keydown_or_up(keycode, self.cursor_x, self.cursor_y);
    }

    pub fn notify_mouse_move(&mut self, x: i32, y: i32) {
        self.cursor_x = x;
        self.cursor_y = y;
    }

    pub fn notify_mouse_wheel(&mut self, value: i32) {
        self.wheel_value += value;
    }

    pub fn set_control_pulse(&mut self) {
        self.control_is_pulse = true;
    }

    // ignore both control and shift when masked
    pub fn set_control_mask(&mut self, mask: bool) {
        self.control_is_masked = mask;
    }

    // TODO: use flags to make it more clear
    pub fn refresh_input(&mut self) {
        self.cs.enter();
        self.old_input_state = self.input_state;
        let new_input_state = self.new_input_state;
        self.input_state = new_input_state;
        if (new_input_state & 0x90) != 0 {
            self.input_state = new_input_state | 4;
        }
        let mut input_state = self.input_state;
        if (input_state & 0x60) != 0 {
            self.input_state = input_state | 8;
        }
        if !self.control_is_masked {
            self.input_state &= !2u32;
        }
        input_state = self.input_state;
        let v5 = input_state & (input_state ^ self.old_input_state);
        self.input_up = (input_state ^ self.old_input_state) & !input_state;
        self.down_keycode = v5;
    }

}
