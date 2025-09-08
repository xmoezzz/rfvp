use anyhow::{Result};

use crate::script::Variant;
use crate::subsystem::resources::motion_manager::AlphaMotionType;
use crate::subsystem::resources::motion_manager::MoveMotionType;
use crate::subsystem::resources::motion_manager::RotationMotionType;
use crate::subsystem::resources::motion_manager::ScaleMotionType;
use crate::subsystem::resources::motion_manager::V3dMotionType;
use crate::subsystem::resources::motion_manager::ZMotionType;
use crate::subsystem::world::GameData;

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
        _ => {
            log::error!("Invalid alpha motion id");
            return Ok(Variant::Nil);
        },
    };

    if !(1..4096).contains(&id) {
        log::error!("prim_id must be between 1 and 4096");
        return Ok(Variant::Nil);
    }

    let src_alpha = match src_alpha {
        Variant::Int(src_alpha) => *src_alpha as u8,
        _ => game_data
            .motion_manager
            .prim_manager
            .get_prim(id)
            .get_alpha(),
    };

    let dst_alpha = match dst_alpha {
        Variant::Int(dst_alpha) => *dst_alpha as u8,
        _ => game_data
            .motion_manager
            .prim_manager
            .get_prim(id)
            .get_alpha(),
    };

    let duration = match duration {
        Variant::Int(duration) => *duration,
        _ => {
            log::error!("Invalid duration");
            return Ok(Variant::Nil);
        },
    };

    if duration <= 0 || duration > 300000 {
        log::error!("Duration must be between 0 and 300000");
        return Ok(Variant::Nil);
    }

    let typ = match typ {
        Variant::Int(typ) => *typ,
        _ => {
            log::error!("Invalid type");
            return Ok(Variant::Nil);
        },
    };

    let typ = match typ.try_into() {
        Ok(AlphaMotionType::Linear) => AlphaMotionType::Linear,
        Ok(AlphaMotionType::Immediate) => AlphaMotionType::Immediate,
        _ => AlphaMotionType::Immediate,
    };

    game_data.motion_manager.set_alpha_motion(
        id as u32,
        src_alpha,
        dst_alpha,
        duration,
        typ,
        reverse.canbe_true(),
    )?;

    Ok(Variant::Nil)
}

pub fn motion_alpha_stop(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id as i16,
        _ => {
            log::error!("Invalid alpha motion id");
            return Ok(Variant::Nil);
        },
    };

    if !(1..4096).contains(&id) {
        log::error!("prim_id must be between 1 and 4096");
        return Ok(Variant::Nil);
    }

    game_data.motion_manager.stop_alpha_motion(id as u32)?;

    Ok(Variant::Nil)
}

pub fn motion_alpha_test(game_data: &GameData, id: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id as i16,
        _ => {
            log::error!("Invalid id");
            return Ok(Variant::Nil);
        },
    };

    if !(1..4096).contains(&id) {
        log::error!("prim_id must be between 1 and 4096");
        return Ok(Variant::Nil);
    }

    let result = game_data.motion_manager.test_alpha_motion(id as u32);

    if result {
        return Ok(Variant::True);
    }

    Ok(Variant::Nil)
}

#[allow(clippy::too_many_arguments)]
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
        _ => {
            log::error!("Invalid id");
            return Ok(Variant::Nil);
        },
    };

    if !(1..4096).contains(&id) {
        log::error!("prim_id must be between 1 and 4096");
        return Ok(Variant::Nil);
    }

    let src_x = match src_x {
        Variant::Int(x) => *x as i16,
        _ => game_data.motion_manager.prim_manager.get_prim(id).get_x(),
    };

    let src_y = match src_y {
        Variant::Int(y) => *y as i16,
        _ => game_data.motion_manager.prim_manager.get_prim(id).get_y(),
    };

    let dst_x = match dst_x {
        Variant::Int(x) => *x as i16,
        _ => game_data.motion_manager.prim_manager.get_prim(id).get_x(),
    };

    let dst_y = match dst_y {
        Variant::Int(y) => *y as i16,
        _ => game_data.motion_manager.prim_manager.get_prim(id).get_y(),
    };

    let duration = match duration {
        Variant::Int(duration) => *duration,
        _ => {
            log::error!("Invalid duration");
            return Ok(Variant::Nil);
        },
    };

    if duration <= 0 || duration > 300000 {
        log::error!("Duration must be between 0 and 300000");
        return Ok(Variant::Nil);
    }

    let typ = match typ {
        Variant::Int(typ) => *typ,
        _ => { 
            log::error!("Invalid type");
            return Ok(Variant::Nil);
        },
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

    game_data.motion_manager.set_move_motion(
        id as u32,
        src_x as u32,
        src_y as u32,
        dst_x as u32,
        dst_y as u32,
        duration,
        typ,
        reverse.canbe_true(),
    )?;

    Ok(Variant::Nil)
}

pub fn motion_move_stop(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id as i16,
        _ => {
            log::error!("Invalid id");
            return Ok(Variant::Nil);
        },
    };

    if !(1..4096).contains(&id) {
        log::error!("prim_id must be between 1 and 4096");
        return Ok(Variant::Nil);
    }

    game_data.motion_manager.stop_move_motion(id as u32)?;

    Ok(Variant::Nil)
}

pub fn motion_move_test(game_data: &GameData, id: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id as i16,
        _ => {
            log::error!("Invalid id");
            return Ok(Variant::Nil);
        },
    };

    if !(1..4096).contains(&id) {
        log::error!("prim_id must be between 1 and 4096");
        return Ok(Variant::Nil);
    }

    let result = game_data.motion_manager.test_move_motion(id as u32);

    if result {
        return Ok(Variant::True);
    }

    Ok(Variant::Nil)
}

pub fn motion_move_r(
    game_data: &mut GameData,
    id: &Variant,
    src_r: &Variant,
    dst_r: &Variant,
    duration: &Variant,
    typ: &Variant,
    reverse: &Variant,
) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id as i16,
        _ => {
            log::error!("Invalid id");
            return Ok(Variant::Nil);
        },
    };

    if !(1..4096).contains(&id) {
        log::error!("prim_id must be between 1 and 4096");
        return Ok(Variant::Nil);
    }

    let src_r = match src_r {
        Variant::Int(x) => *x as i16,
        _ => game_data.motion_manager.prim_manager.get_prim(id).get_x(),
    };

    let dst_r = match dst_r {
        Variant::Int(y) => *y as i16,
        _ => game_data.motion_manager.prim_manager.get_prim(id).get_y(),
    };

    let duration = match duration {
        Variant::Int(duration) => *duration,
        _ => {
            log::error!("Invalid duration");
            return Ok(Variant::Nil);
        },
    };

    if duration <= 0 || duration > 300000 {
        log::error!("Duration must be between 0 and 300000");
        return Ok(Variant::Nil);
    }

    let typ = match typ {
        Variant::Int(typ) => *typ,
        _ => {
            log::error!("Invalid type");
            return Ok(Variant::Nil);
        },
    };

    let typ = match typ.try_into() {
        Ok(RotationMotionType::None) => RotationMotionType::None,
        Ok(RotationMotionType::Linear) => RotationMotionType::Linear,
        Ok(RotationMotionType::Accelerate) => RotationMotionType::Accelerate,
        Ok(RotationMotionType::Decelerate) => RotationMotionType::Decelerate,
        Ok(RotationMotionType::Rebound) => RotationMotionType::Rebound,
        Ok(RotationMotionType::Bounce) => RotationMotionType::Bounce,
        _ => RotationMotionType::None,
    };

    game_data.motion_manager.set_rotation_motion(
        id as u32,
        src_r as i16,
        dst_r as i16,
        duration as i32,
        typ,
        reverse.canbe_true(),
    )?;

    Ok(Variant::Nil)
}

pub fn motion_move_r_stop(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id as i16,
        _ => {
            log::error!("Invalid id");
            return Ok(Variant::Nil);
        },
    };

    if !(1..4096).contains(&id) {
        log::error!("prim_id must be between 1 and 4096");
        return Ok(Variant::Nil);
    }

    game_data.motion_manager.stop_rotation_motion(id as u32)?;

    Ok(Variant::Nil)
}

pub fn motion_move_r_test(game_data: &GameData, id: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id as i16,
        _ => {
            log::error!("Invalid id");
            return Ok(Variant::Nil);
        },
    };

    if !(1..4096).contains(&id) {
        log::error!("prim_id must be between 1 and 4096");
        return Ok(Variant::Nil);
    }

    let result = game_data.motion_manager.test_rotation_motion(id as u32);

    if result {
        return Ok(Variant::True);
    }

    Ok(Variant::Nil)
}

#[allow(clippy::too_many_arguments)]
pub fn motion_move_s2(
    game_data: &mut GameData,
    id: &Variant,
    src_w_factor: &Variant,
    dst_w_factor: &Variant,
    src_h_factor: &Variant,
    dst_h_factor: &Variant,
    duration: &Variant,
    typ: &Variant,
    reverse: &Variant,
) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id as i16,
        _ => {
            log::error!("Invalid id");
            return Ok(Variant::Nil);
        },
    };

    if !(1..4096).contains(&id) {
        log::error!("prim_id must be between 1 and 4096");
        return Ok(Variant::Nil);
    }

    let src_w_factor = match src_w_factor {
        Variant::Int(x) => *x,
        _ => game_data
            .motion_manager
            .prim_manager
            .get_prim(id)
            .get_factor_x()
            .into(),
    };

    let src_h_factor = match src_h_factor {
        Variant::Int(y) => *y,
        _ => game_data
            .motion_manager
            .prim_manager
            .get_prim(id)
            .get_factor_y()
            .into(),
    };

    let dst_w_factor = match dst_w_factor {
        Variant::Int(x) => *x,
        _ => game_data
            .motion_manager
            .prim_manager
            .get_prim(id)
            .get_factor_x()
            .into(),
    };

    let dst_h_factor = match dst_h_factor {
        Variant::Int(y) => *y,
        _ => game_data
            .motion_manager
            .prim_manager
            .get_prim(id)
            .get_factor_y()
            .into(),
    };

    let duration = match duration {
        Variant::Int(duration) => *duration,
        _ => {
            log::error!("Invalid duration");
            return Ok(Variant::Nil);
        },
    };

    if duration <= 0 || duration > 300000 {
        log::error!("Duration must be between 0 and 300000");
        return Ok(Variant::Nil);
    }

    let typ = match typ {
        Variant::Int(typ) => *typ,
        _ => {
            log::error!("Invalid type");
            return Ok(Variant::Nil);
        },
    };

    let typ = match typ.try_into() {
        Ok(ScaleMotionType::None) => ScaleMotionType::None,
        Ok(ScaleMotionType::Linear) => ScaleMotionType::Linear,
        Ok(ScaleMotionType::Accelerate) => ScaleMotionType::Accelerate,
        Ok(ScaleMotionType::Decelerate) => ScaleMotionType::Decelerate,
        Ok(ScaleMotionType::Rebound) => ScaleMotionType::Rebound,
        Ok(ScaleMotionType::Bounce) => ScaleMotionType::Bounce,
        _ => ScaleMotionType::None,
    };

    game_data.motion_manager.set_scale_motion(
        id as u32,
        src_w_factor,
        src_h_factor,
        dst_w_factor,
        dst_h_factor,
        duration,
        typ,
        reverse.canbe_true(),
    )?;

    Ok(Variant::Nil)
}

pub fn motion_move_s2_stop(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id as i16,
        _ => {
            log::error!("Invalid id");
            return Ok(Variant::Nil);
        },
    };

    if !(1..4096).contains(&id) {
        log::error!("prim_id must be between 1 and 4096");
        return Ok(Variant::Nil);
    }

    game_data.motion_manager.stop_scale_motion(id as u32)?;

    Ok(Variant::Nil)
}

pub fn motion_move_s2_test(game_data: &GameData, id: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id as i16,
        _ => {
            log::error!("Invalid id");
            return Ok(Variant::Nil);
        },
    };

    if !(1..4096).contains(&id) {
        log::error!("prim_id must be between 1 and 4096");
        return Ok(Variant::Nil);
    }

    let result = game_data.motion_manager.test_scale_motion(id as u32);

    if result {
        return Ok(Variant::True);
    }

    Ok(Variant::Nil)
}

pub fn motion_move_z(
    game_data: &mut GameData,
    id: &Variant,
    src_z: &Variant,
    dst_z: &Variant,
    duration: &Variant,
    typ: &Variant,
    reverse: &Variant,
) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id as i16,
        _ => {
            log::error!("Invalid id");
            return Ok(Variant::Nil);
        },
    };

    if !(1..4096).contains(&id) {
        log::error!("prim_id must be between 1 and 4096");
        return Ok(Variant::Nil);
    }

    let src_z = match src_z {
        Variant::Int(x) => *x,
        _ => game_data
            .motion_manager
            .prim_manager
            .get_prim(id)
            .get_z()
            .into(),
    };

    let dst_z = match dst_z {
        Variant::Int(y) => *y,
        _ => game_data
            .motion_manager
            .prim_manager
            .get_prim(id)
            .get_z()
            .into(),
    };

    let duration = match duration {
        Variant::Int(duration) => *duration,
        _ => {
            log::error!("Invalid duration");
            return Ok(Variant::Nil);
        },
    };

    if duration <= 0 || duration > 300000 {
        log::error!("Duration must be between 0 and 300000");
        return Ok(Variant::Nil);
    }

    let typ = match typ {
        Variant::Int(typ) => *typ,
        _ => {
            log::error!("Invalid type");
            return Ok(Variant::Nil);
        },
    };

    let typ = match typ.try_into() {
        Ok(ZMotionType::None) => ZMotionType::None,
        Ok(ZMotionType::Linear) => ZMotionType::Linear,
        Ok(ZMotionType::Accelerate) => ZMotionType::Accelerate,
        Ok(ZMotionType::Decelerate) => ZMotionType::Decelerate,
        Ok(ZMotionType::Rebound) => ZMotionType::Rebound,
        Ok(ZMotionType::Bounce) => ZMotionType::Bounce,
        _ => ZMotionType::None,
    };

    game_data.motion_manager.set_z_motion(
        id as u32,
        src_z,
        dst_z,
        duration,
        typ,
        reverse.canbe_true(),
    )?;

    Ok(Variant::Nil)
}

pub fn motion_move_z_stop(game_data: &mut GameData, id: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id as i16,
        _ => {
            log::error!("Invalid id");
            return Ok(Variant::Nil);
        },
    };

    if !(1..4096).contains(&id) {
        log::error!("prim_id must be between 1 and 4096");
        return Ok(Variant::Nil);
    }

    game_data.motion_manager.stop_z_motion(id as u32)?;

    Ok(Variant::Nil)
}

pub fn motion_move_z_test(game_data: &GameData, id: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id as i16,
        _ => {
            log::error!("Invalid id");
            return Ok(Variant::Nil);;
        },
    };

    if !(1..4096).contains(&id) {
        log::error!("prim_id must be between 1 and 4096");
        return Ok(Variant::Nil);
    }

    let result = game_data.motion_manager.test_z_motion(id as u32);

    if result {
        return Ok(Variant::True);
    }

    Ok(Variant::Nil)
}

pub fn motion_pause(game_data: &mut GameData, id: &Variant, pause: &Variant) -> Result<Variant> {
    let id = match id {
        Variant::Int(id) => *id as i16,
        _ => {
            log::error!("Invalid id");
            return Ok(Variant::Nil);
        },
    };

    if !(0..=4096).contains(&id) {
        log::error!("prim_id must be between 0 and 4096");
        return Ok(Variant::Nil);
    }

    let mut prim = game_data.motion_manager.prim_manager.get_prim(id);
    match pause {
        Variant::Int(b) => {
            if *b == 0 {
                prim.set_paused(false);
            } else {
                prim.set_paused(true);
            }
        }
        Variant::Nil => {
            return Ok(Variant::Int(prim.get_paused() as i32));
        }
        _ => {
            log::error!("Invalid pause value");
            return Ok(Variant::Nil);
        },
    }

    Ok(Variant::Nil)
}

pub fn v3d_motion(
    game_data: &mut GameData,
    dest_x: &Variant,
    dest_y: &Variant,
    dest_z: &Variant,
    duration: &Variant,
    typ: &Variant,
    reverse: &Variant,
) -> Result<Variant> {
    let dest_x = match dest_x {
        Variant::Int(x) => *x,
        _ => {
            log::error!("Invalid dest_x");
            return Ok(Variant::Nil);
        },
    };

    let dest_y = match dest_y {
        Variant::Int(y) => *y,
        _ => {
            log::error!("Invalid dest_y");
            return Ok(Variant::Nil);
        },
    };

    let dest_z = match dest_z {
        Variant::Int(z) => *z,
        _ => {
            log::error!("Invalid dest_z");
            return Ok(Variant::Nil);
        },
    };

    let duration = match duration {
        Variant::Int(duration) => *duration,
        _ => {
            log::error!("Invalid duration");
            return Ok(Variant::Nil);
        },
    };

    if duration <= 0 || duration > 300000 {
        log::error!("Duration must be between 0 and 300000");
        return Ok(Variant::Nil);
    }

    let typ = match typ {
        Variant::Int(typ) => *typ,
        _ => {
            log::error!("Invalid type");
            return Ok(Variant::Nil);
        },
    };

    let typ = match typ.try_into() {
        Ok(V3dMotionType::None) => V3dMotionType::None,
        Ok(V3dMotionType::Linear) => V3dMotionType::Linear,
        _ => V3dMotionType::None,
    };

    game_data.motion_manager.set_v3d_motion(
        dest_x,
        dest_y,
        dest_z,
        duration,
        typ,
        reverse.canbe_true(),
    )?;

    Ok(Variant::Nil)
}

pub fn v3d_motion_pause(game_data: &mut GameData, pause: &Variant) -> Result<Variant> {
    match pause {
        Variant::Int(b) => {
            if *b == 0 {
                game_data.motion_manager.set_v3d_motion_paused(false);
            } else {
                game_data.motion_manager.set_v3d_motion_paused(true);
            }
        }
        Variant::Nil => {
            return Ok(Variant::Int(
                game_data.motion_manager.get_v3d_motion_paused() as i32,
            ));
        }
        _ => {
            log::error!("Invalid pause value");
            return Ok(Variant::Nil);
        },
    };

    Ok(Variant::Nil)
}

pub fn v3d_motion_stop(game_data: &mut GameData) -> Result<Variant> {
    game_data.motion_manager.stop_v3d_motion()?;

    Ok(Variant::Nil)
}

pub fn v3d_motion_test(game_data: &GameData) -> Result<Variant> {
    let result = game_data.motion_manager.test_v3d_motion();

    if result {
        return Ok(Variant::True);
    }

    Ok(Variant::Nil)
}

pub fn v3d_set(game_data: &mut GameData, x: &Variant, y: &Variant, z: &Variant) -> Result<Variant> {
    let x = match x {
        Variant::Int(x) => *x,
        _ => game_data.motion_manager.get_v3d_x(),
    };

    let y = match y {
        Variant::Int(y) => *y,
        _ => game_data.motion_manager.get_v3d_y(),
    };

    let z = match z {
        Variant::Int(z) => *z,
        _ => game_data.motion_manager.get_v3d_z(),
    };

    game_data.motion_manager.set_v3d(x, y, z);

    Ok(Variant::Nil)
}

/// Start an alpha animation on a primitive (fade/tween).
///
/// Arg1: primitive index (1–4095)
/// Arg2: src_alpha (0–255, optional)
///       - starting alpha; if omitted or invalid, uses current prim.m_Alpha
/// Arg3: dest_alpha (0–255, optional)
///       - target alpha; if omitted or invalid, uses current prim.m_Alpha
/// Arg4: duration (ms) (1–300000)
///       - animation length in milliseconds
/// Arg5: type (0 or 1, optional)
///       - 0 = Linear interpolation (default)
///       - 1 = Immediate (set to src_alpha immediately)
/// Arg6: reverse (optional)
///       - treated as true if the argument exists (i.e. args[5].Type != 0)
///       - when reverse is enabled, negative elapsed passed into the updater is handled (elapsed sign may be flipped)
///
/// Behavior:
/// - Validates parameters; src/dest alpha fallback to the primitive's current alpha when not provided or out of range (<0x100).
/// - Registers an alpha animation record (set_alpha_anm) with the engine's alpha motion container.
/// - During each frame update the animation:
///     - marks the primitive dirty (`m_Attribute |= 0x40`)
///     - accumulates elapsed time (if reverse=true and elapsed < 0, elapsed is negated)
///     - if elapsed >= duration: sets `m_Alpha = dest_alpha` and ends the animation
///     - otherwise for type=0: linearly interpolates
///       `m_Alpha = src + elapsed * (dest - src) / duration`
///       for type=1: sets `m_Alpha = src` (immediate)
///
/// Notes:
/// - Alpha values are 0..255 (1 byte). Time is in milliseconds.
/// - `reverse` is detected by the presence/type of the 6th argument (not by its numeric value).
/// - The updater skips running if the primitive or any ancestor is paused, or if scene root constraints apply.
/// - The animation sets the primitive's alpha progressively on the `m_Alpha` field used by the renderer.
///
/// Example usage (script):
///   MotionAlpha(12, 0,   255, 1000, 0)      // fade in prim 12 over 1 second (linear)
///   MotionAlpha(8,  128, 128,  500, 1)      // immediately set prim 8 alpha to 128
///   MotionAlpha(20, 255, 0,  2000, 0, true) // fade out prim 20 over 2s with reverse allowed
///
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

/// Stop the alpha animation with a primitive index
/// 
/// Arg1: primitive index (1–4095)
/// Behavior:
/// - The function has no return value
pub struct MotionAlphaStop;
impl Syscaller for MotionAlphaStop {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let id = get_var!(args, 0);

        motion_alpha_stop(game_data, id)
    }
}

unsafe impl Send for MotionAlphaStop {}
unsafe impl Sync for MotionAlphaStop {}

/// Test whether a primitive currently has a running alpha animation.
///
/// Arg1: primitive index (1–4095)
///
/// Behavior:
/// - Queries the alpha motion container (`alpha_motion_is_running`) for the specified prim.
/// - Sets the script return value to:
///     - True if an alpha animation is currently active
///     - Nil/False if no alpha animation is running
///
/// Notes:
/// - This does not return details about the animation (duration, type, etc.), only a boolean.
/// - Works in tandem with `MotionAlpha`; useful for checking whether a fade/tween is still ongoing before triggering new actions.
///
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

        motion_move(
            game_data, id, src_x, src_y, dst_x, dst_y, duration, typ, reverse,
        )
    }
}

unsafe impl Send for MotionMove {}
unsafe impl Sync for MotionMove {}

pub struct MotionMoveStop;
impl Syscaller for MotionMoveStop {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let id = get_var!(args, 0);

        motion_move_stop(game_data, id)
    }
}

unsafe impl Send for MotionMoveStop {}
unsafe impl Sync for MotionMoveStop {}

pub struct MotionMoveTest;
impl Syscaller for MotionMoveTest {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let id = get_var!(args, 0);

        motion_move_test(game_data, id)
    }
}

unsafe impl Send for MotionMoveTest {}
unsafe impl Sync for MotionMoveTest {}

pub struct MotionMoveR;
impl Syscaller for MotionMoveR {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let id = get_var!(args, 0);
        let src_r = get_var!(args, 1);
        let dst_r = get_var!(args, 2);
        let duration = get_var!(args, 3);
        let typ = get_var!(args, 4);
        let reverse = get_var!(args, 5);

        motion_move_r(game_data, id, src_r, dst_r, duration, typ, reverse)
    }
}

unsafe impl Send for MotionMoveR {}
unsafe impl Sync for MotionMoveR {}

pub struct MotionMoveRStop;
impl Syscaller for MotionMoveRStop {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let id = get_var!(args, 0);

        motion_move_r_stop(game_data, id)
    }
}

unsafe impl Send for MotionMoveRStop {}
unsafe impl Sync for MotionMoveRStop {}

pub struct MotionMoveRTest;
impl Syscaller for MotionMoveRTest {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let id = get_var!(args, 0);

        motion_move_r_test(game_data, id)
    }
}

unsafe impl Send for MotionMoveRTest {}
unsafe impl Sync for MotionMoveRTest {}

pub struct MotionMoveS2;
impl Syscaller for MotionMoveS2 {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let id = get_var!(args, 0);
        let src_w_factor = get_var!(args, 1);
        let dst_w_factor = get_var!(args, 2);
        let src_h_factor = get_var!(args, 3);
        let dst_h_factor = get_var!(args, 4);
        let duration = get_var!(args, 5);
        let typ = get_var!(args, 6);
        let reverse = get_var!(args, 7);

        motion_move_s2(
            game_data,
            id,
            src_w_factor,
            dst_w_factor,
            src_h_factor,
            dst_h_factor,
            duration,
            typ,
            reverse,
        )
    }
}

unsafe impl Send for MotionMoveS2 {}
unsafe impl Sync for MotionMoveS2 {}

pub struct MotionMoveS2Stop;
impl Syscaller for MotionMoveS2Stop {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let id = get_var!(args, 0);

        motion_move_s2_stop(game_data, id)
    }
}

unsafe impl Send for MotionMoveS2Stop {}
unsafe impl Sync for MotionMoveS2Stop {}

pub struct MotionMoveS2Test;
impl Syscaller for MotionMoveS2Test {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let id = get_var!(args, 0);

        motion_move_s2_test(game_data, id)
    }
}

unsafe impl Send for MotionMoveS2Test {}
unsafe impl Sync for MotionMoveS2Test {}

pub struct MotionMoveZ;
impl Syscaller for MotionMoveZ {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let id = get_var!(args, 0);
        let src_z = get_var!(args, 1);
        let dst_z = get_var!(args, 2);
        let duration = get_var!(args, 3);
        let typ = get_var!(args, 4);
        let reverse = get_var!(args, 5);

        motion_move_z(game_data, id, src_z, dst_z, duration, typ, reverse)
    }
}

unsafe impl Send for MotionMoveZ {}
unsafe impl Sync for MotionMoveZ {}

pub struct MotionMoveZStop;
impl Syscaller for MotionMoveZStop {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let id = get_var!(args, 0);

        motion_move_z_stop(game_data, id)
    }
}

unsafe impl Send for MotionMoveZStop {}
unsafe impl Sync for MotionMoveZStop {}

pub struct MotionMoveZTest;
impl Syscaller for MotionMoveZTest {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let id = get_var!(args, 0);

        motion_move_z_test(game_data, id)
    }
}

unsafe impl Send for MotionMoveZTest {}
unsafe impl Sync for MotionMoveZTest {}

pub struct MotionPause;
impl Syscaller for MotionPause {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let id = get_var!(args, 0);
        let pause = get_var!(args, 1);

        motion_pause(game_data, id, pause)
    }
}

unsafe impl Send for MotionPause {}
unsafe impl Sync for MotionPause {}

pub struct V3DMotion;
impl Syscaller for V3DMotion {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let dest_x = get_var!(args, 0);
        let dest_y = get_var!(args, 1);
        let dest_z = get_var!(args, 2);
        let duration = get_var!(args, 3);
        let typ = get_var!(args, 4);
        let reverse = get_var!(args, 5);

        v3d_motion(game_data, dest_x, dest_y, dest_z, duration, typ, reverse)
    }
}

unsafe impl Send for V3DMotion {}
unsafe impl Sync for V3DMotion {}

pub struct V3DMotionPause;
impl Syscaller for V3DMotionPause {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let pause = get_var!(args, 0);

        v3d_motion_pause(game_data, pause)
    }
}

unsafe impl Send for V3DMotionPause {}
unsafe impl Sync for V3DMotionPause {}

pub struct V3DMotionStop;
impl Syscaller for V3DMotionStop {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        v3d_motion_stop(game_data)
    }
}

unsafe impl Send for V3DMotionStop {}
unsafe impl Sync for V3DMotionStop {}

pub struct V3DMotionTest;
impl Syscaller for V3DMotionTest {
    fn call(&self, game_data: &mut GameData, _args: Vec<Variant>) -> Result<Variant> {
        v3d_motion_test(game_data)
    }
}

unsafe impl Send for V3DMotionTest {}
unsafe impl Sync for V3DMotionTest {}

pub struct V3DSet;
impl Syscaller for V3DSet {
    fn call(&self, game_data: &mut GameData, args: Vec<Variant>) -> Result<Variant> {
        let x = get_var!(args, 0);
        let y = get_var!(args, 1);
        let z = get_var!(args, 2);

        v3d_set(game_data, x, y, z)
    }
}

unsafe impl Send for V3DSet {}
unsafe impl Sync for V3DSet {}
