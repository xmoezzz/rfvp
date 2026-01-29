use anyhow::{Result};

use crate::script::Variant;
use crate::subsystem::world::GameData;

use super::{get_var, Syscaller};

pub fn parts_load(game_data: &mut GameData, id: &Variant, path: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => {
            // IDA: no side effects.
            return Ok(Variant::Nil);
        }
    };

    if !(0..64).contains(&id) {
        // IDA: no side effects.
        return Ok(Variant::Nil);
    }

    // Prepare the action first (avoid holding a mutable borrow of game_data while doing VFS I/O).
    enum Action {
        Noop,
        Unload,
        Load { path: String, buff: Vec<u8> },
    }

    let action = match path {
        Variant::Nil => Action::Unload,
        Variant::String(p) | Variant::ConstString(p, _) => match game_data.vfs_load_file(p) {
            Ok(buff) => Action::Load {
                path: p.to_string(),
                buff,
            },
            Err(_) => Action::Noop,
        },
        _ => Action::Noop,
    };

    // Apply action, then always cancel/recycle any pending PartsMotion slot for this parts ID.
    {
        let pm = game_data.motion_manager.parts_manager.get_mut();
        match action {
            Action::Noop => {}
            Action::Unload => {
                pm.unload_parts_keep_name(id as u8);
            }
            Action::Load { path, buff } => {
                let _ = pm.load_parts(id as u16, &path, buff);
            }
        }
        pm.next_free_id(id as u8);
    }

    Ok(Variant::Nil)
}

pub fn parts_rgb(
    game_data: &mut GameData,
    id: &Variant,
    r: &Variant,
    g: &Variant,
    b: &Variant,
) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => return Ok(Variant::Nil),
    };

    if !(0..64).contains(&id) {
        return Ok(Variant::Nil);
    }

    // IDA: defaults to 100 for each channel, only overrides when the arg is Int and <= 200.
    let r = match r {
        Variant::Int(v) if (0..=200).contains(v) => *v,
        _ => 100,
    };
    let g = match g {
        Variant::Int(v) if (0..=200).contains(v) => *v,
        _ => 100,
    };
    let b = match b {
        Variant::Int(v) if (0..=200).contains(v) => *v,
        _ => 100,
    };

    game_data
        .motion_manager
        .parts_manager
        .borrow_mut()
        .set_rgb(id as u16, r as u8, g as u8, b as u8);
    Ok(Variant::Nil)
}

pub fn parts_select(game_data: &mut GameData, id: &Variant, entry_id: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => return Ok(Variant::Nil),
    };

    if !(0..64).contains(&id) {
        return Ok(Variant::Nil);
    }

    // IDA: only draws when entry_id is Int and < 256. Otherwise it does nothing.
    if let Variant::Int(entry_id) = entry_id {
        let entry_id_u32 = *entry_id as u32;
        if entry_id_u32 < 256 {
            if let Err(e) = game_data
                .motion_manager
                .draw_parts_to_texture(id as u8, entry_id_u32)
            {
                // IDA: draw failure is non-fatal; still cancels the motion slot.
                let _ = e;
            }
        }
    }

    let _ = game_data
        .motion_manager
        .parts_manager
        .get_mut()
        .next_free_id(id as u8);
    Ok(Variant::Nil)
}


pub fn parts_assign(
    game_data: &mut GameData,
    parts_id: &Variant,
    prim_id: &Variant,
) -> Result<Variant> {
    let parts_id = match parts_id {
        Variant::Int(parts_id) => *parts_id,
        _ => return Ok(Variant::Nil),
    };

    if !(0..64).contains(&parts_id) {
        return Ok(Variant::Nil);
    }

    // IDA: assign only when prim_id is Int and < 0x1000. Regardless, always cancels motion slot.
    if let Variant::Int(prim_id) = prim_id {
        if (0..4096).contains(prim_id) {
            game_data
                .motion_manager
                .parts_manager
                .get_mut()
                .assign_prim_id(parts_id as u8, *prim_id as u16);
        }
    }

    let _ = game_data
        .motion_manager
        .parts_manager
        .get_mut()
        .next_free_id(parts_id as u8);
    Ok(Variant::Nil)
}


pub fn parts_motion(
    game_data: &mut GameData,
    id: &Variant,
    entry_id: &Variant,
    duration: &Variant,
) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => return Ok(Variant::Nil),
    };

    if !(0..64).contains(&id) {
        return Ok(Variant::Nil);
    }

    let entry_id = match entry_id {
        Variant::Int(entry_id) => *entry_id as u32,
        _ => return Ok(Variant::Nil),
    };

    if !(0..256).contains(&entry_id) {
        return Ok(Variant::Nil);
    }

    let duration = match duration {
        Variant::Int(duration) => *duration as u32,
        _ => return Ok(Variant::Nil),
    };

    if !(1..=300000).contains(&duration) {
        return Ok(Variant::Nil);
    }

    game_data
        .motion_manager
        .set_parts_motion(id as u8, entry_id as u8, duration)?;

    Ok(Variant::Nil)
}

pub fn parts_motion_pause(game_data: &mut GameData, id: &Variant, on: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => return Ok(Variant::Nil),
    };

    if !(0..64).contains(&id) {
        return Ok(Variant::Nil);
    }

    let on = match on {
        Variant::Int(on) => *on,
        _ => return Ok(Variant::Nil),
    };

    if on != 0 && on != 1 {
        return Ok(Variant::Nil);
    }

    let parts = game_data
        .motion_manager
        .parts_manager
        .get_mut()
        .get_mut(id as u8);

    parts.set_running(on != 0);

    Ok(Variant::Nil)
}

pub fn parts_motion_stop(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => return Ok(Variant::Nil),
    };

    // IDA: valid range is 1..64 (i.e., 1..=63).
    if !(1..64).contains(&id) {
        return Ok(Variant::Nil);
    }

    game_data.motion_manager.stop_parts_motion(id as u8)?;

    Ok(Variant::Nil)
}

pub fn parts_motion_test(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => return Ok(Variant::Nil),
    };

    // IDA: valid range is 1..64 (i.e., 1..=63).
    if !(1..64).contains(&id) {
        return Ok(Variant::Nil);
    }

    Ok(Variant::Int(
        game_data.motion_manager.test_parts_motion(id as u8) as i32,
    ))
}


pub struct PartsAssign;
impl Syscaller for PartsAssign {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        parts_assign(game_data, get_var!(args, 0), get_var!(args, 1))
    }
}

unsafe impl Send for PartsAssign {}
unsafe impl Sync for PartsAssign {}

pub struct PartsLoad;
impl Syscaller for PartsLoad {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        parts_load(game_data, get_var!(args, 0), get_var!(args, 1))
    }
}

unsafe impl Send for PartsLoad {}
unsafe impl Sync for PartsLoad {}


pub struct PartsMotion;
impl Syscaller for PartsMotion {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        parts_motion(game_data, get_var!(args, 0), get_var!(args, 1), get_var!(args, 2))
    }
}

unsafe impl Send for PartsMotion {}
unsafe impl Sync for PartsMotion {}


pub struct PartsMotionPause;
impl Syscaller for PartsMotionPause {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        parts_motion_pause(game_data, get_var!(args, 0), get_var!(args, 1))
    }
}

unsafe impl Send for PartsMotionPause {}
unsafe impl Sync for PartsMotionPause {}


pub struct PartsMotionStop;
impl Syscaller for PartsMotionStop {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        parts_motion_stop(game_data, &args[0])
    }
}

unsafe impl Send for PartsMotionStop {}
unsafe impl Sync for PartsMotionStop {}


pub struct PartsMotionTest;
impl Syscaller for PartsMotionTest {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        parts_motion_test(game_data, &args[0])
    }
}

unsafe impl Send for PartsMotionTest {}
unsafe impl Sync for PartsMotionTest {}


pub struct PartsRGB;
impl Syscaller for PartsRGB {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        parts_rgb(game_data, &args[0], &args[1], &args[2], &args[3])
    }
}

unsafe impl Send for PartsRGB {}
unsafe impl Sync for PartsRGB {}


pub struct PartsSelect;
impl Syscaller for PartsSelect {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        parts_select(game_data, &args[0], &args[1])
    }
}

unsafe impl Send for PartsSelect {}
unsafe impl Sync for PartsSelect {}

