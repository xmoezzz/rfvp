use std::{sync::Mutex, vec};
use winit::keyboard::NamedKey;
use std::sync::atomic::{AtomicBool, Ordering};

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


///
/// Key codes used by FVP, auctually keycode is just the index of the bit in input_state
/// input_state |= 1 << keycode
/// if input_state is zero, then no key is pressed
/// This term is little confusing, but I keep it for compatibility
/// 
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum KeyCode {
    Shift = 0,
    Ctrl = 1,
    LeftClick = 2, // left mouse button & enter, a virtual keycode
    RightClick = 3, // right mouse button & esc, a virtual keycode
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
    input_down: u32,
    input_up: u32,
    input_state: u32,
    input_repeat: u32,
    cursor_in: bool,
    cursor_x: i32,
    cursor_y: i32,
    wheel_value: i32,
    control_is_masked: bool,
    control_is_pulse: bool,
    suppress_next_mouse: u8,

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
            input_down: 0,
            input_up: 0,
            input_state: 0,
            input_repeat: 0,
            cursor_in: false,
            cursor_x: 0,
            cursor_y: 0,
            wheel_value: 0,
            control_is_masked: false,
            control_is_pulse: false,
            suppress_next_mouse: 0,
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
        let _g = self.cs.enter();
        self.current_index = 0;
        self.next_index = 0;
        // this->gap2[0] = 0;
        // this->gap2[1] = 0;
        self.new_input_state = 0;
        self.input_repeat = 0;
        self.input_state = 0;
        self.input_down = 0;
        self.input_up = 0;
    }

    pub fn suppress_next_mouse_click(&mut self) {
        let _g = self.cs.enter();
        // Eat the activation click fully (down + up).
        self.suppress_next_mouse = 2;
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

    pub fn get_input_down(&self) -> u32 {
        self.input_down
    }

    pub fn get_event(&mut self) -> Option<PressItem> {
        let _g = self.cs.enter();
        if self.next_index != self.current_index {
            // next_index: read cursor; current_index: write cursor
            let event = self.press_items[self.next_index as usize].clone();
            self.next_index = (self.next_index + 1) & 0x3F; // wrap 64
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
        let _g = self.cs.enter();
        self.click = clicked;
    }

    pub fn set_mouse_in(&mut self, in_screen: bool) {
        let _g = self.cs.enter();
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
    #[inline]
    fn bit_for(k: KeyCode) -> u32 {
        1u32 << (k as u32)
    }

    #[inline]
    fn virtual_left_active(bits: u32) -> bool {
        (bits & (Self::bit_for(KeyCode::MouseLeft) | Self::bit_for(KeyCode::Enter))) != 0
    }

    #[inline]
    fn virtual_right_active(bits: u32) -> bool {
        (bits & (Self::bit_for(KeyCode::MouseRight) | Self::bit_for(KeyCode::Esc))) != 0
    }

    #[inline]
    fn apply_virtual_click_state(bits: &mut u32) {
        // Clear first to avoid stale virtual bits.
        *bits &= !(Self::bit_for(KeyCode::LeftClick) | Self::bit_for(KeyCode::RightClick));
        if Self::virtual_left_active(*bits) {
            *bits |= Self::bit_for(KeyCode::LeftClick);
        }
        if Self::virtual_right_active(*bits) {
            *bits |= Self::bit_for(KeyCode::RightClick);
        }
    }

    #[inline]
    fn latch_virtual_click_edges(&mut self, prev_bits: u32, new_bits: u32) {
        let prev_left = Self::virtual_left_active(prev_bits);
        let new_left = Self::virtual_left_active(new_bits);
        if !prev_left && new_left {
            self.input_down |= Self::bit_for(KeyCode::LeftClick);
        }
        if prev_left && !new_left {
            self.input_up |= Self::bit_for(KeyCode::LeftClick);
        }

        let prev_right = Self::virtual_right_active(prev_bits);
        let new_right = Self::virtual_right_active(new_bits);
        if !prev_right && new_right {
            self.input_down |= Self::bit_for(KeyCode::RightClick);
        }
        if prev_right && !new_right {
            self.input_up |= Self::bit_for(KeyCode::RightClick);
        }
    }


    pub fn record_keydown_or_up(&mut self, keycode: KeyCode, x: i32, y: i32) {
        // Ring buffer: next_index is read cursor, current_index is write cursor.
        let next_write = (self.current_index + 1) & 0x3F;
        if next_write == self.next_index {
            // full
            return;
        }

        let event = &mut self.press_items[self.current_index as usize];
        event.keycode = keycode.clone() as u8;
        if matches!(keycode, KeyCode::MouseLeft | KeyCode::MouseRight) {
            event.in_screen = self.cursor_in;
            event.x = x;
            event.y = y;
        } else {
            event.in_screen = false;
            event.x = 0;
            event.y = 0;
        }

        self.current_index = next_write;
    }

    // see https://wiki.winehq.org/List_Of_Windows_Messages
    pub fn notify_keydown(&mut self, key: winit::keyboard::Key, repeat: bool) {
        if let Some(keycode) = self.keymap(key) {
            let mut enqueue = false;
            let mut prev_bits = 0u32;
            {
                let _g = self.cs.enter();

                // When masked, ignore both Shift and Ctrl entirely (state + edges + events).
                if self.control_is_masked && matches!(keycode, KeyCode::Shift | KeyCode::Ctrl) {
                    return;
                }

                prev_bits = self.new_input_state;
                let mask = Self::bit_for(keycode.clone());

                // Latch edge on a true 0->1 transition.
                if (self.new_input_state & mask) == 0 {
                    self.new_input_state |= mask;
                    self.input_down |= mask;
                }

                // Repeat bookkeeping is per-frame.
                self.input_repeat |= mask;

                // IDA: key events are only enqueued for keycode >= 2 (Shift/Ctrl excluded)
                // and only for non-repeat keydown.
                enqueue = !repeat && (keycode.clone() as u8) >= 2;
            }

            // Virtual click edges depend on the composite state.
            self.latch_virtual_click_edges(prev_bits, self.new_input_state);

            if enqueue {
                self.record_keydown_or_up(keycode, 0, 0);
            }
        }
    }


    pub fn notify_keyup(&mut self, key: winit::keyboard::Key) {
        if let Some(keycode) = self.keymap(key) {
            let prev_bits;
            {
                let _g = self.cs.enter();

                if self.control_is_masked && matches!(keycode, KeyCode::Shift | KeyCode::Ctrl) {
                    return;
                }

                prev_bits = self.new_input_state;
                let mask = Self::bit_for(keycode);

                if (self.new_input_state & mask) != 0 {
                    self.new_input_state &= !mask;
                    self.input_up |= mask;
                }
            }

            self.latch_virtual_click_edges(prev_bits, self.new_input_state);
        }
    }

    pub fn notify_mouse_down(&mut self, keycode: KeyCode) {
        // Snapshot cursor position outside the guard.
        let (x, y) = (self.cursor_x, self.cursor_y);

        let mut should_record = false;
        let mut prev_bits = 0u32;
        {
            let _g = self.cs.enter();

            if self.suppress_next_mouse != 0 {
                // Eat the activation click (down + up).
                self.suppress_next_mouse = self.suppress_next_mouse.saturating_sub(1);
                return;
            }

            prev_bits = self.new_input_state;
            let mask = Self::bit_for(keycode.clone());

            if (self.new_input_state & mask) == 0 {
                self.new_input_state |= mask;
                self.input_down |= mask;
            }

            // IDA: mouse events are enqueued depending on click mode.
            should_record = self.click == 1;
        }

        self.latch_virtual_click_edges(prev_bits, self.new_input_state);

        if should_record {
            self.record_keydown_or_up(keycode, x, y);
        }
    }

    pub fn notify_mouse_up(&mut self, keycode: KeyCode) {
        // Snapshot cursor position outside the guard.
        let (x, y) = (self.cursor_x, self.cursor_y);

        let mut should_record = false;
        let mut prev_bits = 0u32;
        {
            let _g = self.cs.enter();

            if self.suppress_next_mouse != 0 {
                // Eat the activation click (down + up).
                self.suppress_next_mouse = self.suppress_next_mouse.saturating_sub(1);
                return;
            }

            prev_bits = self.new_input_state;
            let mask = Self::bit_for(keycode.clone());

            if (self.new_input_state & mask) != 0 {
                self.new_input_state &= !mask;
                self.input_up |= mask;
            }

            // IDA: mouse events are enqueued depending on click mode.
            should_record = self.click == 0;
        }

        self.latch_virtual_click_edges(prev_bits, self.new_input_state);

        if should_record {
            self.record_keydown_or_up(keycode, x, y);
        }
    }

    pub fn notify_mouse_move(&mut self, x: i32, y: i32) {
        let _g = self.cs.enter();
        self.cursor_x = x;
        self.cursor_y = y;
    }

    pub fn notify_mouse_wheel(&mut self, value: i32) {
        let _g = self.cs.enter();
        self.wheel_value += value;
    }

    pub fn set_control_pulse(&mut self) {
        self.control_is_pulse = true;
    }

    /// Consume the one-frame ControlPulse flag.
    ///
    /// In the original engine, `ControlPulse` sets a scene flag that is checked during
    /// the next frame update and then cleared immediately. That makes it a *pulse*,
    /// not a persistent mode toggle.
    pub fn take_control_pulse(&mut self) -> bool {
        let v = self.control_is_pulse;
        self.control_is_pulse = false;
        v
    }

    // ignore both control and shift when masked
    pub fn set_control_mask(&mut self, mask: bool) {
        self.control_is_masked = mask;
    }

    pub fn frame_reset(&mut self) {
        // Per-frame transient signals.
        // NOTE: input_state/new_input_state are NOT cleared here.
        self.input_repeat = 0;
        self.wheel_value = 0;
        self.input_down = 0;
        self.input_up = 0;
    }

    // TODO: use flags to make it more clear
    pub fn refresh_input(&mut self) {
        let _g = self.cs.enter();

        self.old_input_state = self.input_state;
        self.input_state = self.new_input_state;

        // Synthesize virtual click keys for InputGetState.
        Self::apply_virtual_click_state(&mut self.input_state);

        // When masked, ignore both Shift and Ctrl.
        if self.control_is_masked {
            self.input_state &= !Self::bit_for(KeyCode::Shift);
            self.input_state &= !Self::bit_for(KeyCode::Ctrl);
        }

        // NOTE: input_down/input_up are edge-latched directly from event delivery
        // (keydown/keyup/mouse down/up) so we do NOT derive them from state diffs here.
    }

}
