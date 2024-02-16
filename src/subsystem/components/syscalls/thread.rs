use anyhow::Result;

use crate::script::Variant;
use crate::subsystem::world::GameData;

use super::Syscaller;

/// exit thread by id
pub fn thread_exit(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = if let Variant::Int(id) = id {
        *id
    } else {
        log::error!("thread_exit: invalid id type");
        return Ok(Variant::Nil);
    };

    if !(0..32).contains(&id) {
        log::error!("thread_exit: invalid id");
        return Ok(Variant::Nil);
    }
    game_data
        .script_scheduler
        .borrow_mut()
        .thread_exit(id as u32)?;
    Ok(Variant::Nil)
}

/// yield to the next thread
pub fn thread_next(game_data: &mut GameData) -> Result<Variant> {
    game_data.script_scheduler.borrow_mut().thread_yield()?;
    Ok(Variant::Nil)
}

/// raise the thread
pub fn thread_raise(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = if let Variant::Int(id) = id {
        *id
    } else {
        log::error!("thread_raise: invalid id type");
        return Ok(Variant::Nil);
    };

    if !(0..32).contains(&id) {
        log::error!("thread_raise: invalid id");
        return Ok(Variant::Nil);
    }
    log::warn!("thread_raise is not implemented");
    Ok(Variant::Nil)
}

/// sleep the thread
pub fn thread_sleep(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = if let Variant::Int(id) = id {
        *id
    } else {
        log::error!("thread_sleep: invalid id type");
        return Ok(Variant::Nil);
    };

    if !(0..32).contains(&id) {
        log::error!("thread_sleep: invalid id");
        return Ok(Variant::Nil);
    }
    log::warn!("thread_sleep is not implemented");
    Ok(Variant::Nil)
}

/// start a new thread
pub fn thread_start(game_data: &mut GameData, id: &Variant, addr: &Variant) -> Result<Variant> {
    let id = if let Variant::Int(id) = id {
        *id
    } else {
        log::error!("thread_start: invalid id type");
        return Ok(Variant::Nil);
    };

    let addr = if let Variant::Int(addr) = addr {
        *addr
    } else {
        log::error!("thread_start: invalid addr type");
        return Ok(Variant::Nil);
    };

    if !(0..32).contains(&id) {
        log::error!("thread_exit: invalid id");
        return Ok(Variant::Nil);
    }
    game_data
        .script_scheduler
        .borrow_mut()
        .thread_start(id as u32, addr as u32)?;
    Ok(Variant::Nil)
}

/// thread sleep
pub fn thread_wait(game_data: &mut GameData, time: &Variant) -> Result<Variant> {
    let time = if let Variant::Int(time) = time {
        *time
    } else {
        log::error!("thread_wait: invalid time type");
        return Ok(Variant::Nil);
    };

    if time < 0 {
        log::error!("thread_wait: invalid time");
        return Ok(Variant::Nil);
    }

    game_data
        .script_scheduler
        .borrow_mut()
        .thread_wait(time as u32)?;
    Ok(Variant::Nil)
}

pub struct ThreadExit;
impl Syscaller for ThreadExit {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        thread_exit(game_data, super::get_var!(args, 0))
    }
}

unsafe impl Send for ThreadExit {}
unsafe impl Sync for ThreadExit {}

pub struct ThreadNext;
impl Syscaller for ThreadNext {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        thread_next(game_data)
    }
}

unsafe impl Send for ThreadNext {}
unsafe impl Sync for ThreadNext {}

pub struct ThreadRaise;
impl Syscaller for ThreadRaise {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        thread_raise(game_data, super::get_var!(args, 0))
    }
}

unsafe impl Send for ThreadRaise {}
unsafe impl Sync for ThreadRaise {}

pub struct ThreadSleep;
impl Syscaller for ThreadSleep {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        thread_sleep(game_data, super::get_var!(args, 0))
    }
}

unsafe impl Send for ThreadSleep {}
unsafe impl Sync for ThreadSleep {}

pub struct ThreadStart;
impl Syscaller for ThreadStart {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        thread_start(
            game_data,
            super::get_var!(args, 0),
            super::get_var!(args, 1),
        )
    }
}

unsafe impl Send for ThreadStart {}
unsafe impl Sync for ThreadStart {}

pub struct ThreadWait;
impl Syscaller for ThreadWait {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        thread_wait(game_data, super::get_var!(args, 0))
    }
}

unsafe impl Send for ThreadWait {}
unsafe impl Sync for ThreadWait {}
