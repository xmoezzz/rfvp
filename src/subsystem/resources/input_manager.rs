use std::vec;


#[derive(Debug, Clone, Default)]
pub struct PressItem {
    keycode: u8,
    in_screen: bool,
    x: u32,
    y: u32,
}

impl PressItem {
    pub fn get_keycode(&self) -> u8 {
        self.keycode
    }

    pub fn get_in_screen(&self) -> bool {
        self.in_screen
    }

    pub fn get_x(&self) -> u32 {
        self.x
    }

    pub fn get_y(&self) -> u32 {
        self.y
    }
}

#[derive(Debug, Clone)]
pub struct InputManager {
    pub mouse_x: i32,
    pub mouse_y: i32,

    press_items: Vec<PressItem>,
    // HANDLE input_event;
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
    cursor_x: u32,
    cursor_y: u32,
    wheel_value: u32,
    control_is_masked: bool,
}

impl Default for InputManager {
    fn default() -> Self {
        Self::new()
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
            current_index: 0,
            next_index: 0,
            new_input_state: 0,
            old_input_state: 0,
            current_event: PressItem::default(),
            click: 0,
        }
    }

    pub fn set_flash(&mut self) {
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

    pub fn get_cursor_x(&self) -> u32 {
        self.cursor_x
    }

    pub fn get_cursor_y(&self) -> u32 {
        self.cursor_y
    }

    pub fn get_down_keycode(&self) -> u32 {
        self.down_keycode
    }

    pub fn get_event(&mut self) -> Option<PressItem> {
        if self.current_index != self.next_index {
            let event = self.press_items[self.current_index as usize].clone();
            self.next_index = (self.current_index + 1) & 0x3F;
            Some(event)
        } else {
            None
        }
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

    pub fn get_wheel_value(&self) -> u32 {
        self.wheel_value
    }

    pub fn set_click(&mut self, clicked: u32) {
        self.click = clicked;
    }
}
