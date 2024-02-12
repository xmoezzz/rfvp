use anyhow::Result;

use crate::script::Variant;
use crate::subsystem::world::GameData;

/// exit thread by id
pub fn thread_exit(game_data: &mut GameData, id: i32) -> Result<Variant> {
    if !(0..32).contains(&id) {
        log::error!("thread_exit: invalid id");
        return Ok(Variant::Nil);
    }
    game_data.script_scheduler.borrow_mut().thread_exit(id as u32)?;
    Ok(Variant::Nil)
}

/// yield to the next thread
pub fn thread_next(game_data: &mut GameData) -> Result<Variant> {
    game_data.script_scheduler.borrow_mut().thread_yield()?;
    Ok(Variant::Nil)
}

/// raise the thread
pub fn thread_raise(game_data: &mut GameData, id: i32) -> Result<Variant> {
    if !(0..32).contains(&id) {
        log::error!("thread_exit: invalid id");
        return Ok(Variant::Nil);
    }
    log::warn!("thread_raise is not implemented");
    Ok(Variant::Nil)
}

/// sleep the thread
pub fn thread_sleep(game_data: &mut GameData, id: i32) -> Result<Variant> {
    if !(0..32).contains(&id) {
        log::error!("thread_exit: invalid id");
        return Ok(Variant::Nil);
    }
    log::warn!("thread_sleep is not implemented");
    Ok(Variant::Nil)
}

/// start a new thread
pub fn thread_start(game_data: &mut GameData, id: i32, addr: i32) -> Result<Variant> {
    if !(0..32).contains(&id) {
        log::error!("thread_exit: invalid id");
        return Ok(Variant::Nil);
    }
    game_data.script_scheduler.borrow_mut().thread_start(id as u32, addr as u32)?;
    Ok(Variant::Nil)
}

/// thread sleep
pub fn thread_wait(game_data: &mut GameData, time: i32) -> Result<Variant> {
    if time < 0 {
        log::error!("thread_wait: invalid time");
        return Ok(Variant::Nil);
    }
    
    game_data.script_scheduler.borrow_mut().thread_wait(time as u32)?;
    Ok(Variant::Nil)
}

