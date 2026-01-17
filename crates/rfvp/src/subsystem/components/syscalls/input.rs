

use anyhow::Result;

use crate::script::{Table, Variant};
use crate::subsystem::world::GameData;

use super::{get_var, Syscaller};

pub fn input_flash(game_data: &mut GameData) -> Result<Variant> {

    game_data.inputs_manager.set_flash();
    Ok(Variant::Nil)
}


pub fn input_get_curs_in(game_data: &GameData) -> Result<Variant> {
    let result = if game_data.inputs_manager.get_cursor_in() {
        Variant::True
    } else {
        Variant::Nil
    };

    Ok(result)
}

pub fn input_get_curs_x(game_data: &GameData) -> Result<Variant> {
    Ok(Variant::Int(game_data.inputs_manager.get_cursor_x()))
}

pub fn input_get_curs_y(game_data: &GameData) -> Result<Variant> {
    Ok(Variant::Int(game_data.inputs_manager.get_cursor_y()))
}

pub fn input_get_down(game_data: &GameData) -> Result<Variant> {
    Ok(Variant::Int(game_data.inputs_manager.get_input_down() as i32))
}

pub fn input_get_event(game_data: &mut GameData) -> Result<Variant> {
    if let Some(event) = game_data.inputs_manager.get_event() {
        let mut table = Table::new();
        table.insert(0, Variant::Int(event.get_keycode() as i32));
        table.insert(1, Variant::Int(event.get_x()));
        table.insert(2, Variant::Int(event.get_y()));

        Ok(Variant::Table(table))
    } else {
        Ok(Variant::Nil)
    }
}

pub fn input_get_repeat(game_data: &GameData) -> Result<Variant> {
    Ok(Variant::Int(game_data.inputs_manager.get_repeat() as i32))
}

pub fn input_get_state(game_data: &GameData) -> Result<Variant> {
    Ok(Variant::Int(game_data.inputs_manager.get_input_state() as i32))
}

pub fn input_get_up(game_data: &GameData) -> Result<Variant> {
    Ok(Variant::Int(game_data.inputs_manager.get_input_up() as i32))
}

pub fn input_get_wheel(game_data: &GameData) -> Result<Variant> {
    Ok(Variant::Int(game_data.inputs_manager.get_wheel_value()))
}

pub fn input_set_click(game_data: &mut GameData, clicked: &Variant) -> Result<Variant> {
    match clicked {
        Variant::Int(clicked) => {
            if [0, 1].contains(clicked) {
                game_data.inputs_manager.set_click(*clicked as u32);
            }
        },
        _ => return Err(anyhow::anyhow!("input_set_click: invalid clicked type")),
    };

    Ok(Variant::Nil)
}


/// Skip mode in AVG games
pub fn control_pulse(game_data: &mut GameData) -> Result<Variant> {
    game_data.inputs_manager.set_control_pulse();
    Ok(Variant::Nil)
}

pub fn control_mask(game_data: &mut GameData, mask: &Variant) -> Result<Variant> {
    let mask = mask.is_nil();
    game_data.inputs_manager.set_control_mask(mask);

    Ok(Variant::Nil)
}


///
/// Refresh all input states, such as which keys are pressed
/// No arguments
/// 
pub struct InputFlash;
impl Syscaller for InputFlash {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        input_flash(game_data)
    }
}

unsafe impl Send for InputFlash {}
unsafe impl Sync for InputFlash {}


///
/// Get whether the cursor is within the window
/// No arguments
/// Returns true if the cursor is inside the window, nil otherwise
/// 
pub struct InputGetCursIn;
impl Syscaller for InputGetCursIn {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        input_get_curs_in(game_data)
    }
}

unsafe impl Send for InputGetCursIn {}
unsafe impl Sync for InputGetCursIn {}


///
/// Get the X coordinate of the cursor, relative to the window
/// No arguments
/// Returns an integer value representing the X coordinate
/// 
pub struct InputGetCursX;
impl Syscaller for InputGetCursX {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        input_get_curs_x(game_data)
    }
}

unsafe impl Send for InputGetCursX {}
unsafe impl Sync for InputGetCursX {}


/// 
/// Get the Y coordinate of the cursor, relative to the window
/// No arguments
/// Returns an integer value representing the Y coordinate
/// 
pub struct InputGetCursY;
impl Syscaller for InputGetCursY {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        input_get_curs_y(game_data)
    }
}

unsafe impl Send for InputGetCursY {}
unsafe impl Sync for InputGetCursY {}

///
/// FVP's keycode:
/// #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
/// pub enum KeyCode {
///     Shift = 0,
///     Ctrl = 1,
///     LeftClick = 2, // virtual
///     RightClick = 3, // virtual
///     MouseLeft = 4,
///     MouseRight = 5,
///     Esc = 6,
///     Enter = 7,
///     Space = 8,
///     UpArrow = 9,
///     DownArrow = 10,
///     LeftArrow = 11,
///     RightArrow = 12,
///     F1 = 13,
///     F2 = 14,
///     F3 = 15,
///     F4 = 16,
///     F5 = 17,
///     F6 = 18,
///     F7 = 19,
///     F8 = 20,
///     F9 = 21,
///     F10 = 22,
///     F11 = 23,
///     F12 = 24,
///     Tab = 25,
/// }
/// 
/// Get the keycode of the most recently pressed key
/// No arguments
/// Returns an integer value representing the keycode, or nil if no key is pressed
///
pub struct InputGetDown;
impl Syscaller for InputGetDown {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        input_get_down(game_data)
    }
}

unsafe impl Send for InputGetDown {}
unsafe impl Sync for InputGetDown {}


///
/// Get the next input event from the queue
/// No arguments
/// Returns a table with the following structure if an event is available:
/// { keycode, x, y }
/// where `keycode` is an integer representing the keycode,
/// and `x`, `y` are the cursor coordinates at the time of the event.
/// Returns nil if no event is available.
///
pub struct InputGetEvent;
impl Syscaller for InputGetEvent {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        input_get_event(game_data)
    }
}

unsafe impl Send for InputGetEvent {}
unsafe impl Sync for InputGetEvent {}


///
/// Get the keys that are currently being repeated
/// Notice that both wheel movements and button repeats will be reset to 0 during each frame update.
/// No arguments
/// Returns an integer value representing the keycode, or 0 if no key is being repeated
///   
pub struct InputGetRepeat;
impl Syscaller for InputGetRepeat {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        input_get_repeat(game_data)
    }
}

unsafe impl Send for InputGetRepeat {}
unsafe impl Sync for InputGetRepeat {}


///
/// Get the current state of all keys, such as which keys are pressed
/// No arguments
/// Returns an integer value representing the bitmask of the current key states
/// 
pub struct InputGetState;
impl Syscaller for InputGetState {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        input_get_state(game_data)
    }
}

unsafe impl Send for InputGetState {}
unsafe impl Sync for InputGetState {}


/// 
/// Get the keycode of the most recently released key
/// No arguments
/// Returns an integer value representing the keycode, or nil if no key has been released
/// 
pub struct InputGetUp;
impl Syscaller for InputGetUp {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        input_get_up(game_data)
    }
}

unsafe impl Send for InputGetUp {}
unsafe impl Sync for InputGetUp {}

///
/// Get mouse wheel value (usually for scrolling)
/// Notice that the wheel value will be reset to 0 during each frame update.
/// No arguments
/// Returns an integer value representing the wheel movement
/// 
pub struct InputGetWheel;
impl Syscaller for InputGetWheel {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        input_get_wheel(game_data)
    }
}

unsafe impl Send for InputGetWheel {}
unsafe impl Sync for InputGetWheel {}


/// 
/// Set the click state from script side. 
/// Maybe this is used to simulate mouse clicks in certain scenarios.
/// Arg1: clicked (0 for not clicked, 1 for clicked)
/// 
pub struct InputSetClick;
impl Syscaller for InputSetClick {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        input_set_click(game_data, get_var!(args, 0))
    }
}

unsafe impl Send for InputSetClick {}
unsafe impl Sync for InputSetClick {}

///
/// Skip mode in AVG games
/// No arguments
/// 
pub struct ControlPulse;
impl Syscaller for ControlPulse {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        control_pulse(game_data)
    }
}

unsafe impl Send for ControlPulse {}
unsafe impl Sync for ControlPulse {}

///
/// Enable/disable control and shift keys, which are used to speed up text display in AVG games.
/// When `control` is masked, both control and shift keys are disabled.
/// Arg1: mask (nil to enable, non-nil to disable)
/// 
pub struct ControlMask;
impl Syscaller for ControlMask {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("control_mask: invalid number of arguments"));
        }

        control_mask(game_data, get_var!(args, 0))
    }
}

unsafe impl Send for ControlMask {}
unsafe impl Sync for ControlMask {}

