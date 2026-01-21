use anyhow::Result;

use crate::script::Variant;
use crate::subsystem::world::GameData;

use super::Syscaller;

pub fn timer_set(game_data: &mut GameData, id: &Variant, resolution: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("timer_set: invalid id type");
            return Ok(Variant::Nil);
        },
    };

    if !(0..16).contains(&id) {
        log::error!("timer_set: id should be in range 0..16");
        return Ok(Variant::Nil);
    }

    let resolution = match resolution {
        Variant::Int(resolution) => *resolution,
        _ => {
            log::error!("timer_set: invalid resolution type");
            return Ok(Variant::Nil);
        },
    };

    if resolution <= 0 || resolution > 100000 {
        log::error!("timer_set: resolution should be in range 1..100000");
        return Ok(Variant::Nil);
    }

    game_data.timer_manager.set_elapsed(id as usize, 0);
    game_data
        .timer_manager
        .set_resolution(id as usize, resolution as u32);
    game_data.timer_manager.set_enabled(id as usize, true);
    Ok(Variant::Nil)
}

pub fn timer_get(game_data: &GameData, id: &Variant, default_value: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            log::error!("timer_get: invalid id type");
            return Ok(Variant::Nil);
        },
    };

    if !(0..16).contains(&id) {
        log::error!("timer_get: id should be in range 0..16");
        return Ok(Variant::Nil);
    }

    if default_value.is_int() {
        let default_value = default_value.as_int().unwrap();
        if (1..=10000).contains(&default_value) {
            if game_data.timer_manager.get_enabled(id as usize) {
                let result = default_value as u32
                    * game_data.timer_manager.get_elapsed(id as usize)
                    / game_data.timer_manager.get_resolution(id as usize);
                return Ok(Variant::Int(result as i32));
            } else {
                return Ok(Variant::Int(default_value));
            }
        }
    } else if game_data.timer_manager.get_enabled(id as usize) {
        let result = game_data.timer_manager.get_elapsed(id as usize);
        return Ok(Variant::Int(result as i32));
    } else {
        return Ok(Variant::Int(0));
    }

    Ok(Variant::Nil)
}


pub fn timer_suspend(game_data: &mut GameData, on: &Variant) -> Result<Variant> {
    // IDA (original engine): boolean args are evaluated as (Type != 0).
    game_data.timer_manager.set_suspend(!on.canbe_true());

    Ok(Variant::Nil)
}


pub struct TimerSet;
impl Syscaller for TimerSet {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        if args.len() != 2 {
            log::error!("timer_set: invalid number of arguments");
            return Ok(Variant::Nil);
        }

        timer_set(game_data, &args[0], &args[1])
    }
}

unsafe impl Send for TimerSet {}
unsafe impl Sync for TimerSet {}


pub struct TimerGet;
impl Syscaller for TimerGet {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        if args.len() != 2 {
            log::error!("timer_get: invalid number of arguments");
            return Ok(Variant::Nil);
        }

        timer_get(game_data, &args[0], &args[1])
    }
}

unsafe impl Send for TimerGet {}
unsafe impl Sync for TimerGet {}


pub struct TimerSuspend;
impl Syscaller for TimerSuspend {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        if args.len() != 1 {
            log::error!("timer_suspend: invalid number of arguments");
            return Ok(Variant::Nil);
        }

        timer_suspend(game_data, &args[0])
    }
}

unsafe impl Send for TimerSuspend {}
unsafe impl Sync for TimerSuspend {}

