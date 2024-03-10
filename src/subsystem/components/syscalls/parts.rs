use anyhow::{bail, Result};

use crate::script::Variant;
use crate::subsystem::world::GameData;

use super::{get_var, Syscaller};

pub fn parts_load(game_data: &mut GameData, id: &Variant, path: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => bail!("parts_load: invalid id type"),
    };

    if !(0..64).contains(&id) {
        bail!("parts_load: id should be in range 0..64");
    }

    let path = match path {
        Variant::String(path) | Variant::ConstString(path, _) => path,
        _ => bail!("parts_load: invalid path type"),
    };

    let buff = match game_data.vfs_load_file(path) {
        Ok(buff) => buff,
        Err(e) => bail!("parts_load: failed to load file: {}", e),
    };

    game_data
        .motion_manager
        .parts_manager
        .borrow_mut()
        .load_parts(id as u16, path, buff)?;

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
        _ => bail!("parts_rgb: invalid id type"),
    };

    if !(0..64).contains(&id) {
        bail!("parts_rgb: id should be in range 0..64");
    }

    let r = match r {
        Variant::Int(r) => {
            if *r >= 0 && *r <= 200 {
                *r
            } else {
                100
            }
        }
        _ => bail!("parts_rgb: invalid r type"),
    };

    let g = match g {
        Variant::Int(g) => {
            if *g >= 0 && *g <= 200 {
                *g
            } else {
                100
            }
        }
        _ => bail!("parts_rgb: invalid g type"),
    };

    let b = match b {
        Variant::Int(b) => {
            if *b >= 0 && *b <= 200 {
                *b
            } else {
                100
            }
        }
        _ => bail!("parts_rgb: invalid b type"),
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
        _ => bail!("parts_select: invalid id type"),
    };

    if !(0..64).contains(&id) {
        bail!("parts_select: id should be in range 0..64");
    }

    let entry_id = match entry_id {
        Variant::Int(entry_id) => *entry_id as u32,
        _ => bail!("parts_select: invalid entry_id type"),
    };

    if !(0..256).contains(&entry_id) {
        let _ = game_data
            .motion_manager
            .parts_manager
            .get_mut()
            .next_free_id(id as u8);
        bail!("parts_select: entry_id should be in range 0..256");
    }

    if let Err(e) = game_data
        .motion_manager
        .draw_parts_to_texture(id as u8, entry_id)
    {
        log::error!("failed to draw parts to primitive: {:?}", e);
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
        _ => bail!("parts_assign: invalid parts_id type"),
    };

    if !(0..64).contains(&parts_id) {
        bail!("parts_assign: parts_id should be in range 0..64");
    }

    let prim_id = match prim_id {
        Variant::Int(prim_id) => *prim_id as u16,
        _ => bail!("parts_assign: invalid prim_id type"),
    };

    if !(0..4096).contains(&prim_id) {
        bail!("parts_assign: prim_id should be in range 0..256");
    }

    game_data
        .motion_manager
        .parts_manager
        .get_mut()
        .assign_prim_id(parts_id as u8, prim_id);

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
        _ => bail!("parts_motion: invalid id type"),
    };

    if !(0..64).contains(&id) {
        bail!("parts_motion: id should be in range 0..64");
    }

    let entry_id = match entry_id {
        Variant::Int(entry_id) => *entry_id as u32,
        _ => bail!("parts_motion: invalid entry_id type"),
    };

    if !(0..256).contains(&entry_id) {
        bail!("parts_motion: entry_id should be in range 0..256");
    }

    let duration = match duration {
        Variant::Int(duration) => *duration as u32,
        _ => bail!("parts_motion: invalid duration type"),
    };

    if !(1..=300000).contains(&duration) {
        bail!("parts_motion: duration should be in range 1..300000");
    }

    game_data
        .motion_manager
        .set_parts_motion(id as u8, entry_id as u8, duration)?;

    Ok(Variant::Nil)
}

pub fn parts_motion_pause(game_data: &mut GameData, id: &Variant, on: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => bail!("parts_motion_pause: invalid id type"),
    };

    if !(0..64).contains(&id) {
        bail!("parts_motion_pause: id should be in range 0..64");
    }

    let on = match on {
        Variant::Int(on) => *on,
        _ => bail!("parts_motion_pause: invalid on type"),
    };

    if on != 0 && on != 1 {
        bail!("parts_motion_pause: on should be 0 or 1");
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
        _ => bail!("parts_motion_stop: invalid id type"),
    };

    if !(0..64).contains(&id) {
        bail!("parts_motion_stop: id should be in range 0..64");
    }

    game_data.motion_manager.stop_parts_motion(id as u8)?;

    Ok(Variant::Nil)
}

pub fn parts_motion_test(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id,
        _ => bail!("parts_motion_test: invalid id type"),
    };

    if !(0..64).contains(&id) {
        bail!("parts_motion_test: id should be in range 0..64");
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

