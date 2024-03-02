use std::collections::HashMap;

use anyhow::Result;

use crate::script::Variant;
use crate::subsystem::world::GameData;

use super::Syscaller;

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
    Ok(Variant::Int(game_data.inputs_manager.get_cursor_x() as i32))
}

pub fn input_get_curs_y(game_data: &GameData) -> Result<Variant> {
    Ok(Variant::Int(game_data.inputs_manager.get_cursor_y() as i32))
}

pub fn input_get_down(game_data: &GameData) -> Result<Variant> {
    Ok(Variant::Int(game_data.inputs_manager.get_down_keycode() as i32))
}

pub fn input_get_event(game_data: &mut GameData) -> Result<Variant> {
    if let Some(event) = game_data.inputs_manager.get_event() {
        let mut table = HashMap::new();
        table.insert(0i32, Variant::Int(event.get_keycode() as i32));
        table.insert(1, Variant::Int(event.get_x() as i32));
        table.insert(2, Variant::Int(event.get_y() as i32));

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


pub fn control_pulse(game_data: &mut GameData) -> Result<Variant> {
    game_data.inputs_manager.set_control_pulse();
    Ok(Variant::Nil)
}

pub fn control_mask(game_data: &mut GameData, mask: &Variant) -> Result<Variant> {
    let mask = mask.canbe_true();
    game_data.inputs_manager.set_control_mask(mask);

    Ok(Variant::Nil)
}


pub struct InputFlash;
impl Syscaller for InputFlash {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        input_flash(game_data)
    }
}

unsafe impl Send for InputFlash {}
unsafe impl Sync for InputFlash {}

pub struct InputGetCursIn;
impl Syscaller for InputGetCursIn {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        input_get_curs_in(game_data)
    }
}

unsafe impl Send for InputGetCursIn {}
unsafe impl Sync for InputGetCursIn {}

pub struct InputGetCursX;
impl Syscaller for InputGetCursX {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        input_get_curs_x(game_data)
    }
}

unsafe impl Send for InputGetCursX {}
unsafe impl Sync for InputGetCursX {}


pub struct InputGetCursY;
impl Syscaller for InputGetCursY {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        input_get_curs_y(game_data)
    }
}

unsafe impl Send for InputGetCursY {}
unsafe impl Sync for InputGetCursY {}


pub struct InputGetDown;
impl Syscaller for InputGetDown {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        input_get_down(game_data)
    }
}

unsafe impl Send for InputGetDown {}
unsafe impl Sync for InputGetDown {}


pub struct InputGetEvent;
impl Syscaller for InputGetEvent {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        input_get_event(game_data)
    }
}

unsafe impl Send for InputGetEvent {}
unsafe impl Sync for InputGetEvent {}


pub struct InputGetRepeat;
impl Syscaller for InputGetRepeat {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        input_get_repeat(game_data)
    }
}

unsafe impl Send for InputGetRepeat {}
unsafe impl Sync for InputGetRepeat {}


pub struct InputGetState;
impl Syscaller for InputGetState {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        input_get_state(game_data)
    }
}

unsafe impl Send for InputGetState {}
unsafe impl Sync for InputGetState {}


pub struct InputGetUp;
impl Syscaller for InputGetUp {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        input_get_up(game_data)
    }
}

unsafe impl Send for InputGetUp {}
unsafe impl Sync for InputGetUp {}


pub struct InputGetWheel;
impl Syscaller for InputGetWheel {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        input_get_wheel(game_data)
    }
}

unsafe impl Send for InputGetWheel {}
unsafe impl Sync for InputGetWheel {}


pub struct InputSetClick;
impl Syscaller for InputSetClick {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("input_set_click: invalid number of arguments"));
        }

        input_set_click(game_data, &args[0])
    }
}

unsafe impl Send for InputSetClick {}
unsafe impl Sync for InputSetClick {}


pub struct ControlPulse;
impl Syscaller for ControlPulse {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        control_pulse(game_data)
    }
}

unsafe impl Send for ControlPulse {}
unsafe impl Sync for ControlPulse {}


pub struct ControlMask;
impl Syscaller for ControlMask {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        if args.len() != 1 {
            return Err(anyhow::anyhow!("control_mask: invalid number of arguments"));
        }

        control_mask(game_data, &args[0])
    }
}

unsafe impl Send for ControlMask {}
unsafe impl Sync for ControlMask {}

