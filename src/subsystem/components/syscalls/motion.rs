use anyhow::{bail, Result};

use crate::subsystem::resources::motion_manager::AlphaMotionType;
use crate::subsystem::resources::motion_manager::MoveMotionType;
use crate::subsystem::world::GameData;
use crate::{script::Variant, subsystem::resources::motion_manager::AlphaMotion};

use super::{get_var, Syscaller};

pub fn motion_alpha(
    game_data: &mut GameData,
    id: &Variant,
    src_alpha: &Variant,
    dst_alpha: &Variant,
    duration: &Variant,
    typ: &Variant,
    reverse: &Variant,
) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id as i16,
        _ => bail!("Invalid id"),
    };

    if !(1..4096).contains(&id) {
        bail!("prim_id must be between 1 and 4096");
    }

    let src_alpha = match src_alpha {
        Variant::Int(src_alpha) => *src_alpha as u8,
        _ => game_data.prim_manager.get_prim(id).get_alpha(),
    };

    let dst_alpha = match dst_alpha {
        Variant::Int(dst_alpha) => *dst_alpha as u8,
        _ => game_data.prim_manager.get_prim(id).get_alpha(),
    };

    let duration = match duration {
        Variant::Int(duration) => *duration,
        _ => bail!("Invalid duration"),
    };

    if duration <= 0 || duration > 300000 {
        bail!("Duration must be between 0 and 300000");
    }

    let typ = match typ {
        Variant::Int(typ) => *typ,
        _ => bail!("Invalid type"),
    };

    let typ = match typ.try_into() {
        Ok(AlphaMotionType::Linear) => AlphaMotionType::Linear,
        Ok(AlphaMotionType::Immediate) => AlphaMotionType::Immediate,
        _ => AlphaMotionType::Immediate,
    };

    if let Some(mm) = &mut game_data.motion_manager {
        mm.set_alpha_motion(
            id as u32,
            src_alpha,
            dst_alpha,
            duration,
            typ,
            reverse.canbe_true(),
        )?;
    }

    Ok(Variant::Nil)
}


pub fn motion_alpha_stop(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id as i16,
        _ => bail!("Invalid id"),
    };

    if !(1..4096).contains(&id) {
        bail!("prim_id must be between 1 and 4096");
    }

    if let Some(mm) = &mut game_data.motion_manager {
        mm.stop_alpha_motion(id as u32)?;
    }

    Ok(Variant::Nil)
}

pub fn motion_alpha_test(game_data: &GameData, id: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id as i16,
        _ => bail!("Invalid id"),
    };

    if !(1..4096).contains(&id) {
        bail!("prim_id must be between 1 and 4096");
    }

    let result = if let Some(mm) = &game_data.motion_manager {
        mm.test_alpha_motion(id as u32)
    } else {
        false
    };

    if result {
        return Ok(Variant::True);
    }

    Ok(Variant::Nil)
}

pub fn motion_move(
    game_data: &mut GameData,
    id: &Variant,
    src_x: &Variant,
    src_y: &Variant,
    dst_x: &Variant,
    dst_y: &Variant,
    duration: &Variant,
    typ: &Variant,
    reverse: &Variant,
) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id as i16,
        _ => bail!("Invalid id"),
    };

    if !(1..4096).contains(&id) {
        bail!("prim_id must be between 1 and 4096");
    }

    let src_x = match src_x {
        Variant::Int(x) => *x as i16,
        _ => game_data.prim_manager.get_prim(id).get_x(),
    };

    let src_y = match src_y {
        Variant::Int(y) => *y as i16,
        _ => game_data.prim_manager.get_prim(id).get_y(),
    };

    let dst_x = match dst_x {
        Variant::Int(x) => *x as i16,
        _ => game_data.prim_manager.get_prim(id).get_x(),
    };

    let dst_y = match dst_y {
        Variant::Int(y) => *y as i16,
        _ => game_data.prim_manager.get_prim(id).get_y(),
    };

    let duration = match duration {
        Variant::Int(duration) => *duration,
        _ => bail!("Invalid duration"),
    };

    if duration <= 0 || duration > 300000 {
        bail!("Duration must be between 0 and 300000");
    }

    let typ = match typ {
        Variant::Int(typ) => *typ,
        _ => bail!("Invalid type"),
    };

    let typ = match typ.try_into() {
        Ok(MoveMotionType::None) => MoveMotionType::None,
        Ok(MoveMotionType::Linear) => MoveMotionType::Linear,
        Ok(MoveMotionType::Accelerate) => MoveMotionType::Accelerate,
        Ok(MoveMotionType::Decelerate) => MoveMotionType::Decelerate,
        Ok(MoveMotionType::Rebound) => MoveMotionType::Rebound,
        Ok(MoveMotionType::Bounce) => MoveMotionType::Bounce,
        _ => MoveMotionType::None,
    };

    if let Some(mm) = &mut game_data.motion_manager {
        mm.set_move_motion(
            id as u32,
            src_x as u32,
            src_y as u32,
            dst_x as u32,
            dst_y as u32,
            duration,
            typ,
            reverse.canbe_true(),
        )?;
    }

    Ok(Variant::Nil)
}

pub struct MotionAlpha;
impl Syscaller for MotionAlpha {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let id = get_var!(args, 0);
        let src_alpha = get_var!(args, 1);
        let dst_alpha = get_var!(args, 2);
        let duration = get_var!(args, 3);
        let typ = get_var!(args, 4);
        let reverse = get_var!(args, 5);

        motion_alpha(game_data, id, src_alpha, dst_alpha, duration, typ, reverse)
    }
}

unsafe impl Send for MotionAlpha {}
unsafe impl Sync for MotionAlpha {}


pub struct MotionAlphaStop;
impl Syscaller for MotionAlphaStop {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let id = get_var!(args, 0);

        motion_alpha_stop(game_data, id)
    }
}

unsafe impl Send for MotionAlphaStop {}
unsafe impl Sync for MotionAlphaStop {}


pub struct MotionAlphaTest;
impl Syscaller for MotionAlphaTest {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let id = get_var!(args, 0);

        motion_alpha_test(game_data, id)
    }
}

unsafe impl Send for MotionAlphaTest {}
unsafe impl Sync for MotionAlphaTest {}


pub struct MotionMove;
impl Syscaller for MotionMove {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let id = get_var!(args, 0);
        let src_x = get_var!(args, 1);
        let src_y = get_var!(args, 2);
        let dst_x = get_var!(args, 3);
        let dst_y = get_var!(args, 4);
        let duration = get_var!(args, 5);
        let typ = get_var!(args, 6);
        let reverse = get_var!(args, 7);

        motion_move(game_data, id, src_x, src_y, dst_x, dst_y, duration, typ, reverse)
    }
}

unsafe impl Send for MotionMove {}
unsafe impl Sync for MotionMove {}


